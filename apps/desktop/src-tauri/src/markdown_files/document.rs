use std::fs;
use std::path::PathBuf;

use super::asset::allow_asset_directory;
use super::history::{
    markdown_history_root, write_markdown_file_with_history_root,
    write_markdown_file_with_optional_history_root,
};
use super::types::MarkdownFile;

#[tauri::command]
pub(crate) fn read_markdown_file(
    app: tauri::AppHandle,
    path: String,
) -> Result<MarkdownFile, String> {
    let path_buf = PathBuf::from(&path);
    let size_bytes = fs::metadata(&path_buf)
        .map_err(|error| error.to_string())?
        .len();
    let contents = fs::read_to_string(&path_buf).map_err(|error| error.to_string())?;
    if let Some(parent) = path_buf.parent() {
        allow_asset_directory(&app, parent)?;
    }
    // CowMark pages show images by site path ("/blog/x.jpg" = public/blog/x.jpg),
    // outside the page's folder; asset access isn't kept between launches.
    if let Some(public) = crate::user_data::app_data_dir(&app)
        .ok()
        .and_then(|dir| managed_public_dir(cowmark_core::access::access_state(&dir, None).folder, &path_buf))
    {
        allow_asset_directory(&app, &public)?;
    }

    Ok(MarkdownFile {
        path,
        contents,
        size_bytes,
    })
}

#[tauri::command]
pub(crate) fn write_markdown_file(
    app: tauri::AppHandle,
    path: String,
    contents: String,
    skip_history_snapshot: Option<bool>,
    history_cursor_id: Option<String>,
) -> Result<(), String> {
    match markdown_history_root(&app) {
        Ok(history_root) if skip_history_snapshot.unwrap_or(false) => {
            write_markdown_file_with_optional_history_root(
                Some(&history_root),
                path,
                contents,
                true,
                history_cursor_id,
            )
        }
        Ok(history_root) => write_markdown_file_with_history_root(&history_root, path, contents),
        Err(_) => write_markdown_file_with_optional_history_root(None, path, contents, false, None),
    }
}

/// The managed content folder's `public/` when `document` is inside that folder.
fn managed_public_dir(managed: Option<String>, document: &std::path::Path) -> Option<PathBuf> {
    let root = PathBuf::from(managed?).canonicalize().ok()?;
    document.canonicalize().ok()?.strip_prefix(&root).ok()?;
    let public = root.join("public").canonicalize().ok()?;
    public.strip_prefix(&root).ok()?;
    Some(public)
}

#[cfg(test)]
mod managed_public_tests {
    use super::managed_public_dir;
    use std::fs;

    #[test]
    fn grants_public_only_for_pages_in_the_managed_folder() {
        let base = std::env::temp_dir().join(format!("cowmark-public-{}", std::process::id()));
        let root = base.join("content-repo");
        fs::create_dir_all(root.join("content/blog")).unwrap();
        fs::create_dir_all(root.join("public/blog")).unwrap();
        fs::create_dir_all(base.join("elsewhere")).unwrap();
        let page = root.join("content/blog/a.md");
        let other = base.join("elsewhere/b.md");
        fs::write(&page, "# A").unwrap();
        fs::write(&other, "# B").unwrap();
        let managed = Some(root.to_string_lossy().into_owned());

        assert_eq!(managed_public_dir(managed.clone(), &page), Some(root.join("public").canonicalize().unwrap()));
        assert_eq!(managed_public_dir(managed, &other), None);
        assert_eq!(managed_public_dir(None, &page), None);
        fs::remove_dir_all(base).unwrap();
    }
}
