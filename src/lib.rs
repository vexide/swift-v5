use std::{io, sync::LazyLock};

use directories::ProjectDirs;
use indicatif::ProgressStyle;
use miette::Diagnostic;
use thiserror::Error;

pub(crate) use fs_err::tokio as fs;
use tokio_util::sync::CancellationToken;
use trash::TrashContext;

pub mod project;
pub mod toolchain;

pub type Result<T, E = Error> = std::result::Result<T, E>;

const PROGRESS_CHARS: &str = "=> ";

pub static PROGRESS_STYLE: LazyLock<ProgressStyle> = LazyLock::new(|| {
    ProgressStyle::with_template("{percent:>3.bold}% [{bar:40.blue}] ({bytes}/{total_bytes}, {eta} remaining) {bytes_per_sec}")
    .expect("progress style valid")
    .progress_chars(PROGRESS_CHARS)
});

pub static PROGRESS_STYLE_MSG: LazyLock<ProgressStyle> = LazyLock::new(|| {
    ProgressStyle::with_template("{percent:>3.bold}% [{bar:40.green}] {msg} ({eta} remaining)")
        .expect("progress style valid")
        .progress_chars(PROGRESS_CHARS)
});

pub static PROGRESS_STYLE_SPINNER: LazyLock<ProgressStyle> = LazyLock::new(|| {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .expect("progress style valid")
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
});

pub static DIRS: LazyLock<ProjectDirs> = LazyLock::new(|| {
    ProjectDirs::from("dev", "vexide", "swift-v5").expect("home directory must be available")
});

pub static TRASH: LazyLock<TrashContext> = LazyLock::new(|| {
    #[allow(unused_mut)]
    let mut ctx = TrashContext::new();

    // Opt in to faster deletion method
    #[cfg(target_os = "macos")]
    trash::macos::TrashContextExtMacos::set_delete_method(
        &mut ctx,
        trash::macos::DeleteMethod::NsFileManager,
    );

    ctx
});

#[macro_export]
macro_rules! msg {
    ($label:expr, $($rest:tt)+) => {
        println!("{:>12} {}", $label.green().bold(), format_args!($($rest)+))
    };
}

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("Cannot determine the root of this project")]
    #[diagnostic(code(swift_v5::cannot_find_project))]
    #[diagnostic(help("navigate to a directory containing Package.swift"))]
    CannotFindProject,
    #[error("Failed to parse swift-v5 config")]
    #[diagnostic(code(swift_v5::invalid_config))]
    #[diagnostic(help("fix the errors in `v5.toml`"))]
    InvalidConfig {
        #[from]
        source: toml::de::Error,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    Toolchain(#[from] toolchain::ToolchainError),
    #[error(transparent)]
    #[diagnostic(code(swift_v5::interactive_prompt_failed))]
    Inquire(#[from] inquire::InquireError),
    #[error(transparent)]
    #[diagnostic(code(swift_v5::io_error))]
    Io(#[from] io::Error),
}

trait CheckCancellation {
    fn check_cancellation<E>(&self, error: E) -> Result<(), E>;
}

impl CheckCancellation for CancellationToken {
    fn check_cancellation<E>(&self, error: E) -> Result<(), E> {
        if self.is_cancelled() {
            Err(error)
        } else {
            Ok(())
        }
    }
}
