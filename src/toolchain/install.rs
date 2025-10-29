use std::{io::stdout, process::{Stdio, exit}};

use inquire::Confirm;
use owo_colors::OwoColorize;
use tokio_util::sync::CancellationToken;

use crate::{msg, project::Project, toolchain::{HostArch, HostOS, ToolchainClient, ToolchainVersion}};

pub async fn install(force: bool) -> crate::Result<()> {
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
