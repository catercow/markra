//! CaterCow content preview: lists the files in a marketing-content checkout that
//! differ from the commit the preview site was built from, and polls for saves.
//! The frontend posts the results to the site with the `catercow:draft*`
//! messages (js/nuxt4/modules/draft-preview in the CaterCow monorepo).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::SystemTime;

const MAX_MEDIA_BYTES: u64 = 15 * 1024 * 1024;
const WATCHED_DIRS: [&str; 2] = ["content", "public"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum PreviewEvent {
    /// A content file to render, relative to `content/`.
    Draft { fs_path: String, content: String },
    /// A content file that no longer exists, relative to `content/`.
    Remove { fs_path: String },
    /// An image under `public/`, as a site path. Without `mime` the image was
    /// deleted; otherwise read its bytes with `content_preview_read_media`.
    Media { path: String, mime: Option<String> },
    Skipped { path: String, reason: String },
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingChanges {
    events: Vec<PreviewEvent>,
    warning: Option<String>,
    base: Option<String>,
}

type Stamp = (u64, Option<SystemTime>);

#[derive(Default)]
pub(crate) struct ContentPreviewState(Mutex<HashMap<PathBuf, BTreeMap<String, Stamp>>>);

fn media_type(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn is_content_type(extension: &str) -> bool {
    matches!(extension, "md" | "yml" | "yaml" | "json")
}

/// Editors write temp and swap files next to the real one.
fn is_ignored(rel: &str) -> bool {
    rel.split('/').any(|segment| segment.starts_with('.') || segment.starts_with('~'))
        || rel.ends_with('~')
        || [".swp", ".swx", ".tmp", ".crdownload"].iter().any(|suffix| rel.ends_with(suffix))
        || rel == "4913"
        || rel.ends_with("/4913")
}

fn event_for(root: &Path, rel: &str, removed: bool) -> Option<PreviewEvent> {
    if is_ignored(rel) {
        return None;
    }
    let (top, inner) = rel.split_once('/')?;
    let extension = Path::new(inner)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let full = root.join(rel);
    let metadata = fs::metadata(&full).ok().filter(|metadata| metadata.is_file());
    let gone = removed || metadata.is_none();

    match top {
        "content" if is_content_type(&extension) => {
            if gone {
                return Some(PreviewEvent::Remove { fs_path: inner.to_string() });
            }
            let content = fs::read_to_string(&full).ok()?;
            Some(PreviewEvent::Draft { fs_path: inner.to_string(), content })
        }
        "public" => {
            let mime = media_type(&extension)?;
            let path = format!("/{inner}");
            if gone {
                return Some(PreviewEvent::Media { path, mime: None });
            }
            if metadata.map(|metadata| metadata.len()).unwrap_or(0) > MAX_MEDIA_BYTES {
                return Some(PreviewEvent::Skipped { path, reason: "larger than 15 MB".into() });
            }
            Some(PreviewEvent::Media { path, mime: Some(mime.to_string()) })
        }
        _ => None,
    }
}

// --- git -----------------------------------------------------------------------

fn git_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("git")];
    if cfg!(target_os = "macos") {
        candidates.push("/Applications/GitHub Desktop.app/Contents/Resources/app/git/bin/git".into());
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let desktop = PathBuf::from(local).join("GitHubDesktop");
            let mut versions: Vec<PathBuf> = fs::read_dir(&desktop)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("app-"))
                })
                .collect();
            versions.sort();
            versions.reverse(); // newest app-<version> first
            candidates.extend(
                versions
                    .into_iter()
                    .map(|dir| dir.join("resources/app/git/cmd/git.exe")),
            );
        }
    }
    candidates
}

pub(crate) fn git_command(git: &Path) -> Command {
    let mut command = Command::new(git);
    command.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub(crate) fn find_git() -> Option<PathBuf> {
    git_candidates().into_iter().find(|candidate| {
        git_command(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

struct Git {
    binary: PathBuf,
    root: PathBuf,
}

impl Git {
    fn run(&self, args: &[&str]) -> Option<String> {
        let output = git_command(&self.binary)
            .args(args)
            .current_dir(&self.root)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn has_commit(&self, reference: &str) -> bool {
        self.run(&["cat-file", "-e", &format!("{reference}^{{commit}}")]).is_some()
    }

    /// The commit the preview was built from, fetched if this clone doesn't have
    /// it yet; otherwise the studio branch, with a warning.
    fn base_commit(&self, preview_commit: &str) -> (Option<String>, Option<String>) {
        let looks_like_sha = (7..=40).contains(&preview_commit.len())
            && preview_commit.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        if looks_like_sha {
            if !self.has_commit(preview_commit) {
                self.run(&["fetch", "--quiet", "origin", preview_commit]);
            }
            if self.has_commit(preview_commit) {
                return (Some(preview_commit.to_string()), None);
            }
        }
        for reference in ["origin/studio", "studio"] {
            if self.has_commit(reference) {
                let warning = if preview_commit.is_empty() {
                    format!("The preview didn't say which commit it was built from; showing changes since {reference}.")
                } else {
                    format!("Couldn't fetch the preview's commit; showing changes since {reference}.")
                };
                return (Some(reference.to_string()), Some(warning));
            }
        }
        (None, Some("No studio branch here; showing only files saved from now on.".into()))
    }

    /// Paths relative to the root that differ from `base`: committed after it,
    /// staged, unstaged or untracked. The value is true when the file was deleted.
    fn changed_since(&self, base: &str) -> Option<BTreeMap<String, bool>> {
        let mut changes = BTreeMap::new();
        let mut diff_args = vec!["diff", "--name-status", "--no-renames", "-z", base, "--"];
        diff_args.extend(WATCHED_DIRS);
        let diff = self.run(&diff_args)?;
        let parts: Vec<&str> = diff.split('\0').filter(|part| !part.is_empty()).collect();
        for pair in parts.chunks(2) {
            if let [status, path] = pair {
                changes.insert(path.to_string(), *status == "D");
            }
        }
        let mut untracked_args = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
        untracked_args.extend(WATCHED_DIRS);
        let untracked = self.run(&untracked_args)?;
        for path in untracked.split('\0').filter(|part| !part.is_empty()) {
            changes.insert(path.to_string(), false);
        }
        Some(changes)
    }
}

fn pending_changes(root: &Path, preview_commit: &str) -> PendingChanges {
    let Some(binary) = find_git() else {
        return PendingChanges {
            warning: Some("Git isn't installed (install GitHub Desktop); showing only files saved from now on.".into()),
            ..Default::default()
        };
    };
    let git = Git { binary, root: root.to_path_buf() };
    let (base, warning) = git.base_commit(preview_commit);
    let mut result = PendingChanges { warning, base: base.clone(), ..Default::default() };
    let Some(base) = base else { return result };
    match git.changed_since(&base) {
        Some(changes) => {
            result.events = changes
                .iter()
                .filter_map(|(rel, removed)| event_for(root, rel, *removed))
                .collect();
        }
        None => result.warning = Some("Couldn't list your changes with git.".into()),
    }
    result
}

// --- polling -------------------------------------------------------------------

/// Stats every file under content/ and public/. Polling sees editors'
/// save-by-rename and behaves the same on macOS and Windows.
fn snapshot(root: &Path) -> BTreeMap<String, Stamp> {
    let mut files = BTreeMap::new();
    let mut stack: Vec<PathBuf> = WATCHED_DIRS.iter().map(|dir| root.join(dir)).collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    stack.push(path);
                }
                continue;
            }
            let Ok(metadata) = entry.metadata() else { continue };
            if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                files.insert(rel, (metadata.len(), metadata.modified().ok()));
            }
        }
    }
    files
}

fn changed_paths(previous: &BTreeMap<String, Stamp>, current: &BTreeMap<String, Stamp>) -> Vec<String> {
    let mut changed: Vec<String> = current
        .iter()
        .filter(|(path, stamp)| previous.get(*path) != Some(stamp))
        .map(|(path, _)| path.clone())
        .collect();
    changed.extend(previous.keys().filter(|path| !current.contains_key(*path)).cloned());
    changed.sort();
    changed
}

#[tauri::command]
pub(crate) async fn content_preview_pending(root: String, preview_commit: String) -> Result<PendingChanges, String> {
    tauri::async_runtime::spawn_blocking(move || pending_changes(Path::new(&root), &preview_commit))
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
    let current = tauri::async_runtime::spawn_blocking(move || snapshot(&scan_root))
        .await
        .map_err(|error| error.to_string())?;
    let previous = {
        let mut snapshots = state.0.lock().map_err(|error| error.to_string())?;
        snapshots.insert(root.clone(), current.clone())
    };
    let Some(previous) = previous else { return Ok(Vec::new()) };
    Ok(changed_paths(&previous, &current)
        .iter()
        .filter_map(|rel| event_for(&root, rel, false))
        .collect())
}

/// The bytes of an image under `<root>/public`, given its site path.
#[tauri::command]
pub(crate) fn content_preview_read_media(root: String, path: String) -> Result<tauri::ipc::Response, String> {
    let relative = path.trim_start_matches('/');
    if relative.split(['/', '\\']).any(|segment| segment == ".." || segment.is_empty()) {
        return Err("invalid media path".into());
    }
    let full = Path::new(&root).join("public").join(relative);
    let extension = full.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
    if media_type(&extension).is_none() {
        return Err("not an image".into());
    }
    if fs::metadata(&full).map_err(|error| error.to_string())?.len() > MAX_MEDIA_BYTES {
        return Err("larger than 15 MB".into());
    }
    fs::read(full).map(tauri::ipc::Response::new).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn content_preview_stop(root: String, state: tauri::State<'_, ContentPreviewState>) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .remove(&PathBuf::from(root));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("markra-content-preview-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("content/blog")).unwrap();
        fs::create_dir_all(root.join("public/blog")).unwrap();
        root
    }

    #[test]
    fn builds_events_for_content_media_and_removals() {
        let root = temp_root("events");
        fs::write(root.join("content/blog/a.md"), "# A").unwrap();
        fs::write(root.join("public/blog/a.png"), [1u8, 2, 3]).unwrap();

        assert_eq!(
            event_for(&root, "content/blog/a.md", false),
            Some(PreviewEvent::Draft { fs_path: "blog/a.md".into(), content: "# A".into() })
        );
        assert_eq!(
            event_for(&root, "content/blog/gone.md", false),
            Some(PreviewEvent::Remove { fs_path: "blog/gone.md".into() })
        );
        assert_eq!(
            event_for(&root, "public/blog/a.png", false),
            Some(PreviewEvent::Media { path: "/blog/a.png".into(), mime: Some("image/png".into()) })
        );
        assert_eq!(event_for(&root, "content/blog/.a.md.swp", false), None);
        assert_eq!(event_for(&root, "content/blog/a.md~", false), None);
        assert_eq!(event_for(&root, "public/data.csv", false), None);
        assert_eq!(event_for(&root, "README.md", false), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn polling_reports_saved_and_deleted_files() {
        let root = temp_root("poll");
        fs::write(root.join("content/blog/a.md"), "one").unwrap();
        let before = snapshot(&root);
        fs::write(root.join("content/blog/a.md"), "one more").unwrap();
        fs::write(root.join("content/blog/b.md"), "two").unwrap();
        let mid = snapshot(&root);
        assert_eq!(changed_paths(&before, &mid), vec!["content/blog/a.md", "content/blog/b.md"]);
        fs::remove_file(root.join("content/blog/b.md")).unwrap();
        assert_eq!(changed_paths(&mid, &snapshot(&root)), vec!["content/blog/b.md"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_changes_since_the_preview_commit() {
        let Some(binary) = find_git() else { return };
        let root = temp_root("git");
        let git = Git { binary, root: root.clone() };
        let run = |args: &[&str]| assert!(git.run(args).is_some(), "git {args:?} failed");
        run(&["init", "--quiet", "-b", "studio"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        fs::write(root.join("content/blog/a.md"), "a").unwrap();
        fs::write(root.join("content/blog/b.md"), "b").unwrap();
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "base"]);
        let base = git.run(&["rev-parse", "HEAD"]).unwrap().trim().to_string();

        fs::write(root.join("content/blog/a.md"), "a2").unwrap();
        fs::remove_file(root.join("content/blog/b.md")).unwrap();
        fs::write(root.join("content/blog/c.md"), "c").unwrap();

        let pending = pending_changes(&root, &base);
        assert_eq!(pending.base.as_deref(), Some(base.as_str()));
        assert_eq!(pending.warning, None);
        assert_eq!(
            pending.events,
            vec![
                PreviewEvent::Draft { fs_path: "blog/a.md".into(), content: "a2".into() },
                PreviewEvent::Remove { fs_path: "blog/b.md".into() },
                PreviewEvent::Draft { fs_path: "blog/c.md".into(), content: "c".into() },
            ]
        );

        let fallback = pending_changes(&root, "");
        assert_eq!(fallback.base.as_deref(), Some("studio"));
        assert!(fallback.warning.is_some());
        let _ = fs::remove_dir_all(&root);
    }
}
