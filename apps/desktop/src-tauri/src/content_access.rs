//! Tauri commands for CaterCow content access: sign-in, session, the managed
//! folder and its git. The logic is in `cowmark_core::{access, git}`; this file
//! supplies the app data dir, the Documents dir and the system browser.

use std::path::{Path, PathBuf};

use cowmark_core::access::{self, AccessState, PendingSignIn, SignedIn, TokenCache, SIGN_IN_TIMEOUT};
use cowmark_core::git::{self, ContentGitStatus, SyncResult};
use cowmark_core::git_tool::{find_git, require_git};
use tauri::Manager;

pub(crate) type ContentAccessState = TokenCache;

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    crate::user_data::app_data_dir(app)
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn content_access_state(app: tauri::AppHandle) -> Result<AccessState, String> {
    Ok(access::access_state(&data_dir(&app)?, app.path().document_dir().ok()))
}

#[tauri::command]
pub(crate) async fn content_access_sign_in(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    site: String,
) -> Result<SignedIn, String> {
    let pending = PendingSignIn::start(site)?;
    crate::external_urls::open_system_url(pending.login_url())?;
    let code = blocking(move || pending.wait_for_code(SIGN_IN_TIMEOUT)).await?;
    code.finish(&data_dir(&app)?, &state).await
}

#[tauri::command]
pub(crate) fn content_access_sign_out(app: tauri::AppHandle, state: tauri::State<'_, ContentAccessState>) {
    match data_dir(&app) {
        Ok(dir) => access::sign_out(&dir, &state),
        Err(_) => state.clear(),
    }
}

#[tauri::command]
pub(crate) fn content_access_set_folder(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    access::set_folder(&data_dir(&app)?, path)
}

#[tauri::command]
pub(crate) async fn content_access_is_checkout(path: String) -> Result<bool, String> {
    blocking(move || Ok(find_git().is_some_and(|git| git::is_checkout(&git, Path::new(&path))))).await
}

/// Clones the content into `dest` and remembers it as the managed folder.
#[tauri::command]
pub(crate) async fn content_access_clone(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    dest: String,
) -> Result<(), String> {
    let git = require_git()?;
    let remote = access::remote(&data_dir(&app)?, &state).await?;
    let target = PathBuf::from(&dest);
    blocking(move || git::clone_into(&git, &remote, &target)).await?;
    content_access_set_folder(app, Some(dest))
}

#[tauri::command]
pub(crate) async fn content_access_status(root: String) -> Result<ContentGitStatus, String> {
    let git = require_git()?;
    blocking(move || git::status(&git, Path::new(&root))).await
}

#[tauri::command]
pub(crate) async fn content_access_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    root: String,
    message: String,
    paths: Vec<String>,
) -> Result<SyncResult, String> {
    let git = require_git()?;
    let dir = data_dir(&app)?;
    let author = access::signed_in_author(&dir)?;
    let remote = access::remote(&dir, &state).await?;
    blocking(move || git::save(&git, Path::new(&root), &remote, &author, &message, &paths)).await
}

#[tauri::command]
pub(crate) async fn content_access_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    root: String,
) -> Result<SyncResult, String> {
    let git = require_git()?;
    let dir = data_dir(&app)?;
    let author = access::signed_in_author(&dir)?;
    let remote = access::remote(&dir, &state).await?;
    blocking(move || git::update(&git, Path::new(&root), &remote, &author)).await
}
