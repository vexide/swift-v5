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
    task::spawn_blocking,
};
use tracing::debug;
use zip::{read::root_dir_common_filter, result::ZipError};

use crate::{fs, toolchain::ToolchainError};

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
pub mod macos {
    use super::*;

    pub async fn extract_dmg(
        _dmg_path: PathBuf,
        _destination_folder: &Path,
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

            Ok::<_, io::Error>(reader.into_inner())
        }
    })
    .await
    .unwrap()?;

    // Find the root directory in the extracted contents and move it to the destination
    let root_dir = find_dir_contained_by(temp_destination.path()).await?;
    fs::rename(&root_dir, &destination).await?;

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
