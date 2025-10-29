use std::process::Command;
use miette::Diagnostic;
use owo_colors::OwoColorize as _;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum BuildError {
    #[error(
        "Swift build failed unexpectedly"
    )]
    SwiftBuildFail,
    #[error(
        "llvm-objcopy failed"
    )]
    LlvmObjcopyFail,
}

pub async fn build() -> crate::Result<()> {
    let status = Command::new("swift")
        .arg("build")
        .arg("-c")
        .arg("release")
        .arg("--triple")
        .arg("armv7-none-none-eabi")
        .arg("--toolset")
        .arg("toolset.json")
        .status()?;
    if !status.success() {
        return Err(BuildError::SwiftBuildFail.into());
    }

    let status = Command::new("llvm-objcopy")
        .arg("-O")
        .arg("binary")
        .arg("./.build/release/VexSwiftApp")
        .arg("./.build/release/VexSwiftApp.bin")
        .status()?;

    if !status.success() {
        return Err(BuildError::LlvmObjcopyFail.into());
    }

    crate::msg!("Successfully built to ./build/release/VexSwiftApp.bin", "");

    Ok(())
}
