//! This module provides functionality to extract toolchain archives in formats
//! such as DMG, ZIP, and TAR.XZ.

use std::{
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

use liblzma::read::XzDecoder;
use miette::Diagnostic;
use tempfile::tempdir;
use thiserror::Error;
use tokio::{
    io::{self},
    runtime::Handle,
    task::{JoinSet, spawn_blocking},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument, trace};
use walkdir::WalkDir;
use zip::{read::root_dir_common_filter, result::ZipError};

use crate::{CheckCancellation, fs, toolchain::ToolchainError};

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
pub mod macos {
    use indicatif::ProgressBar;
    use tokio_util::sync::CancellationToken;

    use super::*;

    pub async fn extract_dmg(
        _dmg_path: PathBuf,
        _destination_folder: &Path,
        _progress_bar: &ProgressBar,
        _cancel_token: CancellationToken,
    ) -> Result<(), ToolchainError> {
        Err(ExtractError::DmgNotSupported.into())
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum ExtractError {
    #[error("DMG extraction is not supported on this platform")]
    #[diagnostic(code(swift_v5::toolchain::extract::dmg_not_supported))]
    DmgNotSupported,

    #[error("The archive did not contain the expected contents")]
    #[diagnostic(code(swift_v5::toolchain::extract::contents_not_found))]
    ContentsNotFound,

    #[error("Failed to read directory while extracting toolchain")]
    #[diagnostic(code(swift_v5::toolchain::extract::walk_directory_failed))]
    WalkDir(#[from] walkdir::Error),

    #[error("DMG extraction failed")]
    #[diagnostic(code(swift_v5::toolchain::extract::dmg_failed))]
    Dmg(#[source] io::Error),

    #[error("ZIP extraction failed")]
    #[diagnostic(code(swift_v5::toolchain::extract::zip_failed))]
    Zip(#[from] ZipError),
}

pub async fn extract_zip(
    zip_file: fs::File,
    destination: PathBuf,
) -> Result<fs::File, ToolchainError> {
    let mut reader = BufReader::new(zip_file.into_std().await);

    let file = spawn_blocking(move || {
        let mut archive = zip::ZipArchive::new(&mut reader)?;

        archive.extract_unwrapped_root_dir(destination, root_dir_common_filter)?;

        Ok::<_, ZipError>(reader.into_inner())
    })
    .await
    .unwrap()
    .map_err(ExtractError::Zip)?;

    Ok(file.into())
}

pub async fn extract_tar_xz(
    tar_xz_file: fs::File,
    destination: PathBuf,
    cancel_token: CancellationToken,
) -> Result<fs::File, ToolchainError> {
    let mut reader = BufReader::new(tar_xz_file.into_std().await);

    let temp_destination = Arc::new(tempdir()?);

    // This behavior is necesary because the archive contains a sub-directory which we want to ignore.
    debug!(
        temp_dir = ?temp_destination.path(),
        "This tar.xz archive will be extracted to a temporary directory before being moved to the final destination"
    );

    let file = spawn_blocking({
        let temp_destination = temp_destination.clone();
        move || {
            let mut decompressor = XzDecoder::new(&mut reader);
            let mut archive = tar::Archive::new(&mut decompressor);

            archive.unpack(temp_destination.path())?;
            debug!("Done unpacking");
            Ok::<_, io::Error>(reader.into_inner())
        }
    })
    .await
    .unwrap()?;

    // Find the root directory in the extracted contents and move it to the destination
    let root_dir = find_dir_contained_by(temp_destination.path()).await?;
    debug!("mv");
    mv(&root_dir, &destination, cancel_token).await?;

    Ok(file.into())
}

async fn find_dir_contained_by(parent_dir: &Path) -> Result<PathBuf, ToolchainError> {
    let mut contents_path = None;

    let mut read_dir = fs::read_dir(parent_dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let is_dir = metadata.is_dir() && !metadata.is_symlink();
        if is_dir {
            contents_path = Some(entry.path());
            break;
        }
    }

    Ok(contents_path.ok_or(ExtractError::ContentsNotFound)?)
}

pub async fn mv(src: &Path, dst: &Path, cancel_token: CancellationToken) -> Result<(), ToolchainError> {
    match fs::rename(src, dst).await {
        Ok(()) => Ok(()),
        // Moving from /tmp/ to /anywhere-else/ isn't possible with a simple fs::rename because
        // we're moving across devices, so we'll fallback to the more complicated recursive
        // copy-and-delete method if that fails.
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            copy_folder(src, dst.to_path_buf(), cancel_token.clone()).await?;
            Ok(())
        }
        Err(e) => Err(ToolchainError::Io(e)),
    }
}

#[instrument(skip(cancel_token))]
async fn copy_folder(
    source: &Path,
    destination: PathBuf,
    cancel_token: CancellationToken,
) -> Result<(), ToolchainError> {
    debug!("Copying folder");

    let source = Arc::new(fs::canonicalize(source).await?);
    let destination = Arc::new(destination);

    let mut tasks = spawn_blocking({
        move || {
            let mut tasks = JoinSet::new();

            for entry in WalkDir::new(&*source) {
                let entry = entry.map_err(ExtractError::WalkDir)?;

                if cancel_token.is_cancelled() {
                    Handle::current().block_on(tasks.join_all());
                    return Err(ToolchainError::Cancelled);
                }

                let source = source.clone();
                let destination = destination.clone();
                let cancel_token = cancel_token.clone();

                tasks.spawn(async move {
                    if entry.file_type().is_dir() {
                        return Ok(());
                    }

                    let relative_path = entry.path().strip_prefix(&*source).unwrap();
                    let destination_path = destination.join(relative_path);

                    let destination_parent = destination_path.parent().unwrap();

                    cancel_token.check_cancellation(ToolchainError::Cancelled)?;
                    fs::create_dir_all(destination_parent).await?;

                    if entry.path_is_symlink() {
                        let target = fs::read_link(entry.path()).await?;
                        trace!(?target, ?destination_path, "Creating symlink");

                        cancel_token.check_cancellation(ToolchainError::Cancelled)?;

                        // NOTE: unix-only, but this is a macOS-specific module
                        fs::symlink(target, &destination_path).await?;
                    }

                    cancel_token.check_cancellation(ToolchainError::Cancelled)?;
                    fs::copy(entry.path(), &destination_path).await?;

                    Ok::<_, ToolchainError>(())
                });
            }

            Ok::<_, ToolchainError>(tasks)
        }
    })
    .await
    .unwrap()?;

    while let Some(result) = tasks.join_next().await {
        result.unwrap()?;
    }

    Ok(())
}
