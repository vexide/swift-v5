use std::sync::LazyLock;

use axoupdater::AxoUpdater;
use clap::{Parser, Subcommand};
use human_panic::Metadata;
use owo_colors::OwoColorize;
use swift_v5::{
    build::{BuildTarget, SwiftOpts, build},
    msg,
    symlink::symlink,
    toolchain::install::install,
};
use tokio::{sync::Mutex, task::block_in_place};
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
    Activate {},
    /// Builds the project using the Swift compiler. Requires the appropriate
    /// Swift version installed (`swiftly install` in your project) and the
    /// LLVM toolchain properly installed and symlinked (`swift v5 install`).
    Build {
        #[arg(long, value_enum, default_value_t = BuildTarget::Release)]
        target: BuildTarget,
        /// Arguments forwarded to `swift`.
        #[clap(flatten)]
        swift_opts: SwiftOpts,
    },
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
        Commands::Activate {} => {
            symlink().await?;
        }
        Commands::Build { target, swift_opts } => {
            build(&target, &swift_opts).await?;
        }
    }

    Ok(())
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
