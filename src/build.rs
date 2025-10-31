use clap::Args;
use miette::Diagnostic;
use owo_colors::OwoColorize as _;
use std::process::Command;
use thiserror::Error;

use crate::{project::Project, symlink::symlink};

#[derive(Debug, Error, Diagnostic)]
pub enum BuildError {
    #[error("Build output folder is invalid UTF-8, invalid PathBuf or doesn't exist")]
    OutputFolderInvalid,
    #[error("Executable package name is invalid UTF-8 or doesn't exist")]
    ExecutableNameInvalid,
}

#[derive(Debug, Error, Clone, clap::ValueEnum)]
pub enum BuildTarget {
    Release,
    Debug,
}
impl BuildTarget {
    pub fn arg(&self) -> String {
        match self {
            BuildTarget::Release => "release".to_string(),
            BuildTarget::Debug => "debug".to_string(),
        }
    }
}
impl std::fmt::Display for BuildTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.arg())
    }
}

#[derive(Args, Debug)]
pub struct SwiftOpts {
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "SWIFT-OPTIONS"
    )]
    args: Vec<String>,
}

pub async fn build(target: &BuildTarget, opts: &SwiftOpts) -> crate::Result<()> {
    // TODO: allow custom args to be passed thru to the `swift` invocation
    // resymlink to be safe
    if !symlink().await? {
        return Ok(());
    }

    let status = Command::new("swift")
        .arg("build")
        .args(opts.args.clone())
        .arg("-c")
        .arg(target.arg())
        .arg("--triple")
        .arg("armv7-none-none-eabi")
        .arg("--toolset")
        .arg("toolset.json")
        .status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    let path = Project::output_path(target)?;
    let name = Project::executable_name()?;
    let elf = path.join(name.clone());
    let bin = path.join(format!("{}.bin", name.clone()));
    let status = Command::new("llvm-objcopy")
        .arg("-O")
        .arg("binary")
        .arg(elf)
        .arg(&bin)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    crate::msg!(format!("Successfully built to {}", &bin.display()), "");

    Ok(())
}
