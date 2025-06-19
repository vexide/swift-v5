//! Logic for extracting macOS DMG files.

use std::{
    mem, path::{Path, PathBuf}, sync::Arc, time::Duration
};

use dmg::detach;
use indicatif::ProgressBar;
use tokio::{
    runtime::Handle,
    task::{spawn_blocking, JoinSet}, time::sleep,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument, trace};
use walkdir::WalkDir;

use crate::{
    CheckCancellation, fs,
    toolchain::{
        ToolchainError,
        extract::{ExtractError, find_dir_contained_by},
    },
};

pub async fn extract_dmg(
    dmg_path: PathBuf,
    destination_folder: &Path,
    progress_bar: &ProgressBar,
    cancel_token: CancellationToken,
) -> Result<(), ToolchainError> {
    use dmg::Attach;

    let handle = spawn_blocking(|| Attach::new(dmg_path).mount_temp().attach())
        .await
        .unwrap()
        .map_err(ExtractError::Dmg)?;

    let dmg = scopeguard::guard(handle, |handle| {
        // ensure the mount point is unmounted when we exit
        handle.force_detach().expect("Failed to detach DMG");
    });

    debug!(?dmg.mount_point, "Mounted DMG");

    // First directory in the mount point is the actual contents

    cancel_token.check_cancellation(ToolchainError::Cancelled)?;
    let contents_path = find_dir_contained_by(&dmg.mount_point).await?;

    cancel_token.check_cancellation(ToolchainError::Cancelled)?;
    copy_folder(&contents_path, destination_folder.to_owned(), cancel_token.clone()).await?;

    debug!(?dmg.mount_point, "Unmounting DMG");
    progress_bar.set_message("Cleaning up...");

    let mut retries_left = 10;
    while retries_left > 0 {
        cancel_token.check_cancellation(ToolchainError::Cancelled)?;
        retries_left -= 1;

        // Attempt to cleanly unmount the DMG instead of force detaching it.
        // This helps ensure everything is flushed properly.

        match detach(&dmg.device, false) {
            Ok(_) => {
                // No need to force unmount, we can safely abort the deferred cleanup
                mem::forget(dmg);
                break;
            }
            Err(error) => {
                debug!(?error, "Failed to unmount DMG, retrying...");
                sleep(Duration::from_millis(500)).await;
            }
        }
    }

    Ok(())
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
