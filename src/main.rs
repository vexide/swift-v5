use std::{
    io::stdout,
    process::{Command, Stdio, exit},
    sync::LazyLock,
};

use axoupdater::AxoUpdater;
use clap::{Parser, Subcommand};
use human_panic::Metadata;
use inquire::Confirm;
use owo_colors::OwoColorize;
use swift_v5::{
    build::build, msg, project::Project, toolchain::{HostArch, HostOS, ToolchainClient, ToolchainError, ToolchainVersion}
};
use tokio::{sync::Mutex, task::block_in_place};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};

/// Create VEX V5 programs in Swift
///
/// swift-v5 can manage the Arm Toolchain for Embedded version your Swift project uses.
/// Run `swift v5 install` to download the latest version of the toolchain.
#[derive(Parser, Debug)]
#[command(bin_name = "swift v5", version, about, long_about)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Install the toolchain for this project
    Install {
        #[clap(
            long,
            help = "Force re-installation of the toolchain, even if it is already installed"
        )]
        force: bool,
    },
    /// Update swift-v5 to the latest version
    #[clap(hide = !can_update())]
    Update {},
    /// Symlink the project's toolchain to ./llvm-toolchain, needed for swift
    /// builds
    Symlink {},
    /// Builds the project using the Swift compiler. Requires the appropriate
    /// Swift version installed (`swiftly install` in your project) and the
    /// LLVM toolchain properly installed and symlinked (`swift v5 install`).
    Build {},
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .finish()
        .init();

    if cfg!(not(debug_assertions)) {
        human_panic::setup_panic!(
            Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                .homepage("https://vexide.dev")
                .support("https://discord.gg/d4uazRf2Nh")
        );
    }

    let args = Args::parse();

    match args.command {
        Commands::Install { force } => {
            install(force).await?;
        }
        Commands::Update {} => {
            update().await?;
        }
        Commands::Symlink {} => {
            symlink().await?;
        }
        Commands::Build {} => {
            build().await?;
        }
    }

    Ok(())
}

async fn install(force: bool) -> swift_v5::Result<()> {
    let project = Project::find().await?;
    let toolchain = ToolchainClient::using_data_dir().await?;

    let toolchain_release;
    let confirm_message;
    let toolchain_version;
    if let Some(config) = project.config().await? {
        toolchain_version = ToolchainVersion::named(&config.llvm_version);
        toolchain_release = toolchain.get_release(&toolchain_version).await?;
        confirm_message = format!("Download & install LLVM toolchain {toolchain_version}?");
    } else {
        toolchain_release = toolchain.latest_release().await?;
        toolchain_version = toolchain_release.version().to_owned();
        confirm_message =
            format!("Download & install latest LLVM toolchain ({toolchain_version})?");
    }

    if !force {
        let already_installed = toolchain.install_path_for(&toolchain_version);
        if already_installed.exists() {
            println!(
                "Toolchain up-to-date: {} at {}",
                toolchain_version.to_string().bold(),
                already_installed.display().green()
            );
            return Ok(());
        }
    }

    let confirmation = Confirm::new(&confirm_message)
        .with_default(true)
        .with_help_message("Required support libraries for Embedded Swift. No = cancel")
        .prompt()?;

    if !confirmation {
        eprintln!("Cancelled.");
        exit(1);
    }

    let asset = toolchain_release.asset_for(HostOS::current(), HostArch::current())?;

    msg!(
        "Downloading",
        "{} <{}>",
        asset.name.bold(),
        asset.browser_download_url.green()
    );

    let cancel_token = CancellationToken::new();

    tokio::spawn({
        let cancel_token = cancel_token.clone();
        async move {
            tokio::signal::ctrl_c().await.unwrap();
            cancel_token.cancel();
            eprintln!("Cancelled.");
        }
    });

    let destination = toolchain
        .download_and_install(&toolchain_release, asset, cancel_token)
        .await?;
    msg!("Downloaded", "to {}", destination.display());

    msg!("Creating symlink for llvm-toolchain", "");

    std::process::Command::new("ln")
        .arg("-s")
        .arg(destination)
        .arg("llvm-toolchain")
        .stdin(Stdio::null())
        .stderr(stdout())
        .output()?;

    Ok(())
}

async fn symlink() -> swift_v5::Result<()> {
    let project = Project::find().await?;
    let toolchain = ToolchainClient::using_data_dir().await?;
    let version = if let Some(config) = project.config().await? {
        ToolchainVersion::named(&config.llvm_version)
    } else {
        toolchain.latest_release().await?.version().to_owned()
    };
    let already_installed = toolchain.install_path_for(&version);
    if !already_installed.exists() {
        msg!("Selected toolchain is not installed. Installing...", "");
        // TODO: avoid recalling Project::find, ToolchainClient::using_data_dir, etc.
        install(true).await // force since we know it doesn't exist alr
    } else {
        std::process::Command::new("ln")
            .arg("-s")
            .arg(already_installed)
            .arg("llvm-toolchain")
            .stdin(Stdio::null())
            .stderr(stdout())
            .output()?;
        Ok(())
    }
}


static UPDATER: LazyLock<Mutex<AxoUpdater>> =
    LazyLock::new(|| Mutex::new(AxoUpdater::new_for("swift-v5")));

fn can_update() -> bool {
    block_in_place(|| UPDATER.blocking_lock().load_receipt().is_ok())
}

async fn update() -> swift_v5::Result<()> {
    let mut updater = UPDATER.lock().await;

    updater
        .load_receipt()
        .map_err(|_| swift_v5::Error::SelfUpdateUnavailable)?;

    eprintln!("Running self-update...");
    if let Some(update) = updater.run().await? {
        msg!(
            "Updated",
            "swift-v5 v{} -> v{}",
            update
                .old_version
                .map(|v| v.to_string())
                .unwrap_or_else(|| "[unknown]".to_string()),
            update.new_version
        );
    } else {
        eprintln!("No updates available.");
    }
    Ok(())
}
