use std::process::exit;

use clap::{Parser, Subcommand};
use human_panic::Metadata;
use inquire::Confirm;
use owo_colors::OwoColorize;
use swift_v5::{
    msg,
    project::Project,
    toolchain::{HostArch, HostOS, ToolchainClient, ToolchainVersion},
};
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
    Install {},
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
        Commands::Install {} => {
            install().await?;
        }
    }

    Ok(())
}

async fn install() -> swift_v5::Result<()> {
    let project = Project::find().await?;
    let toolchain = ToolchainClient::using_data_dir().await?;

    let toolchain_version;
    let mut toolchain_release = None;
    let installing_specific_version;

    if let Some(config) = project.config().await? {
        toolchain_version = ToolchainVersion::named(&config.llvm_version);
        installing_specific_version = true;
    } else {
        let latest = toolchain.latest_release().await?;
        toolchain_version = latest.version().to_owned();
        toolchain_release = Some(latest);
        installing_specific_version = false;
    }

    let already_installed = toolchain.install_path_for(&toolchain_version);
    if already_installed.exists() {
        println!(
            "Toolchain up-to-date: {} at {}",
            toolchain_version.to_string().bold(),
            already_installed.display().green()
        );
        return Ok(());
    }

    let confirm_message = if installing_specific_version {
        format!("Download & install LLVM toolchain {toolchain_version}?")
    } else {
        format!("Download & install latest LLVM toolchain ({toolchain_version})?")
    };

    let confirmation = Confirm::new(&confirm_message)
        .with_default(true)
        .with_help_message("Required support libraries for Embedded Swift. No = cancel")
        .prompt()?;

    if !confirmation {
        eprintln!("Cancelled.");
        exit(1);
    }

    let toolchain_release = toolchain_release.expect("todo: fetch release for specific version");
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

    Ok(())
}
