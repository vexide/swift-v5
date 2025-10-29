#[cfg(unix)]
use std::path::Path;

use inquire::Confirm;
use owo_colors::OwoColorize;

use crate::{msg, project::Project, toolchain::{ToolchainClient, ToolchainVersion, install::install}};

#[cfg(unix)]
fn symlink_internal<A: AsRef<Path>, B: AsRef<Path>>(original: A, to: B) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, to)
}
#[cfg(windows)]
fn symlink_internal(original: AsRef<Path>, to: AsRef<Path>) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(original, to)
}

pub async fn symlink() -> crate::Result<bool> {
    if Path::new("./llvm-toolchain").exists() {
        return Ok(true);
    }
    let confirmation = Confirm::new("Activate toolchain?")
        .with_default(true)
        .with_help_message("Symlinks the LLVM toolchain to ./llvm-toolchain (required for building projects). Make sure you're in your project's directory for this step.")
        .prompt()?;
    if !confirmation {
        return Ok(false);
    }
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
        install(true).await?; // force since we know it doesn't exist alr
        Ok(true)
    } else {
        match symlink_internal(already_installed, String::from("./llvm-toolchain")) {
            Err(e) if e.raw_os_error() == Some(17) => {
                // The symlink already exists, which is fine.
                Ok(())
            }
            res => res
        }?;
        Ok(true)
    }
}
