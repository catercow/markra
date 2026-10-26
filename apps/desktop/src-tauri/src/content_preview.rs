//! Tauri commands for the CaterCow content preview. The logic is in
//! `cowmark_core::preview`.

use std::path::{Path, PathBuf};

use cowmark_core::preview::{self, PendingChanges, Poller, PreviewEvent};

pub(crate) type ContentPreviewState = Poller;

#[tauri::command]
pub(crate) async fn content_preview_pending(root: String, preview_commit: String) -> Result<PendingChanges, String> {
    tauri::async_runtime::spawn_blocking(move || preview::pending_changes(Path::new(&root), &preview_commit))
        .await
        .map_err(|error| error.to_string())
}

/// Files saved since the previous call for this root. The first call only
/// records the current state and returns nothing.
#[tauri::command]
pub(crate) async fn content_preview_poll(
    root: String,
    state: tauri::State<'_, ContentPreviewState>,
) -> Result<Vec<PreviewEvent>, String> {
    let root = PathBuf::from(root);
    let scan_root = root.clone();
    let current = tauri::async_runtime::spawn_blocking(move || preview::snapshot(&scan_root))
        .await
        .map_err(|error| error.to_string())?;
    state.advance(&root, current)
}

#[tauri::command]
pub(crate) fn content_preview_read_media(root: String, path: String) -> Result<tauri::ipc::Response, String> {
    preview::read_media(Path::new(&root), &path).map(tauri::ipc::Response::new)
}

#[tauri::command]
pub(crate) fn content_preview_stop(root: String, state: tauri::State<'_, ContentPreviewState>) -> Result<(), String> {
    state.stop(Path::new(&root))
}
