//! Which content folder a command works on, and which paths inside it it may touch.

use std::path::{Component, Path, PathBuf};

use cowmark_core::control;

use crate::app::App;

/// Where the content folder came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `--root` (or `COWMARK_ROOT`).
    Flag,
    /// The working directory is inside a content checkout.
    WorkingDir,
    /// The running app's `ping`.
    App,
    /// The folder CowMark remembers in its data dir.
    Saved,
}

#[derive(Debug, Clone)]
pub struct ContentRoot {
    /// Canonical, so paths can be compared after symlinks resolve.
    pub path: PathBuf,
    pub source: Source,
}

/// A content checkout: a `content/` folder and `schema/collections.json`.
pub fn is_content_repo(dir: &Path) -> bool {
    dir.join("content").is_dir() && dir.join("schema").join("collections.json").is_file()
}

/// Finds the content folder, first match wins: `flag`, the working directory
/// (or a parent) when it is a content checkout, the running app's folder, the
/// folder the app remembers.
pub fn find(flag: Option<&Path>, cwd: Option<&Path>, app: Option<&App>) -> Result<ContentRoot, String> {
    let canonical = |path: &Path, source| {
        path.canonicalize()
            .ok()
            .filter(|path| path.is_dir())
            .map(|path| ContentRoot { path, source })
    };
    if let Some(flag) = flag {
        return canonical(flag, Source::Flag).ok_or_else(|| format!("{} isn't a folder", flag.display()));
    }
    if let Some(found) = cwd.and_then(|cwd| cwd.ancestors().find(|dir| is_content_repo(dir)))
        && let Some(root) = canonical(found, Source::WorkingDir)
    {
        return Ok(root);
    }
    if let Some(app) = app {
        let running = app
            .ping()
            .and_then(|ping| ping.get("folder").and_then(|folder| folder.as_str()).map(PathBuf::from));
        if let Some(root) = running.and_then(|folder| canonical(&folder, Source::App)) {
            return Ok(root);
        }
        if let Some(root) = app.saved_folder().and_then(|folder| canonical(&folder, Source::Saved)) {
            return Ok(root);
        }
    }
    Err("Couldn't find the content folder. Set up CowMark, run this inside the content folder, or pass --root <folder>.".into())
}

/// `path` (relative to the root, or absolute inside it) as an absolute path and
/// a `/`-separated root-relative one. The same rules as the app's control
/// connection: nothing outside the root, through `..` or a symlink, or in `.git`.
pub fn resolve(root: &Path, path: &str) -> Result<(PathBuf, String), String> {
    control::resolve_in_folder(root, path).map_err(|error| error.message)
}

pub const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];
pub const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"];

pub fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub fn is_markdown(path: &str) -> bool {
    MARKDOWN_EXTENSIONS.contains(&extension(path).as_str())
}

pub fn is_image(path: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&extension(path).as_str())
}

/// [`resolve`], limited to what outside tools may change: Markdown and images
/// under `content/`, no hidden files or folders, and never through a symlink.
pub fn resolve_writable(root: &Path, path: &str) -> Result<(PathBuf, String), String> {
    let (absolute, relative) = resolve(root, path)?;
    if !relative.starts_with("content/") {
        return Err(format!("{relative}: only files under content/ can be changed"));
    }
    if relative.split('/').any(|part| part.starts_with('.')) {
        return Err(format!("{relative}: hidden files and folders can't be changed"));
    }
    if !is_markdown(&relative) && !is_image(&relative) {
        return Err(format!(
            "{relative}: only Markdown (.md) and images ({}) can be changed",
            IMAGE_EXTENSIONS.join(", ")
        ));
    }
    // A symlink, even a dangling one, could send the write elsewhere.
    let mut current = root.to_path_buf();
    for part in Path::new(&relative).components() {
        if let Component::Normal(part) = part {
            current.push(part);
            if std::fs::symlink_metadata(&current).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(format!("{relative}: links can't be changed"));
            }
        }
    }
    Ok((absolute, relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cm-cli-root-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn content_repo(name: &str) -> PathBuf {
        let root = temp(name);
        std::fs::create_dir_all(root.join("content/blog")).unwrap();
        std::fs::create_dir_all(root.join("schema")).unwrap();
        std::fs::write(root.join("schema/collections.json"), r#"{"collections":[]}"#).unwrap();
        root
    }

    #[test]
    fn finds_the_root_from_the_flag_then_the_working_dir_then_the_app() {
        let repo = content_repo("find");
        let other = temp("find-other");

        let found = find(Some(&other), Some(&repo.join("content/blog")), None).unwrap();
        assert_eq!((found.path, found.source), (other.clone(), Source::Flag));
        assert!(find(Some(&other.join("missing")), None, None).is_err());

        let found = find(None, Some(&repo.join("content/blog")), None).unwrap();
        assert_eq!((found.path, found.source), (repo.clone(), Source::WorkingDir));

        // Not a checkout, no app: nothing.
        assert!(find(None, Some(&other), None).is_err());

        // The app isn't running, but remembers its folder.
        let data = temp("find-data");
        cowmark_core::access::set_folder(&data, Some(repo.to_string_lossy().into_owned())).unwrap();
        let found = find(None, Some(&other), Some(&App::at(&data))).unwrap();
        assert_eq!((found.path, found.source), (repo.clone(), Source::Saved));

        // A remembered folder that's gone isn't used.
        cowmark_core::access::set_folder(&data, Some(other.join("gone").to_string_lossy().into_owned())).unwrap();
        assert!(find(None, Some(&other), Some(&App::at(&data))).is_err());

        for dir in [repo, other, data] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn only_content_files_are_writable() {
        let root = content_repo("write");
        std::fs::create_dir_all(root.join(".git")).unwrap();

        assert_eq!(
            resolve_writable(&root, "content/blog/a.md").unwrap().1,
            "content/blog/a.md"
        );
        assert_eq!(
            resolve_writable(&root, "content/blog/a/photo.JPG").unwrap().1,
            "content/blog/a/photo.JPG"
        );
        let absolute = root.join("content/blog/b.md");
        assert_eq!(
            resolve_writable(&root, absolute.to_str().unwrap()).unwrap().1,
            "content/blog/b.md"
        );

        for bad in [
            "schema/collections.json",
            "README.md",
            ".git/config",
            "content/blog/a.txt",
            "content/.hidden/a.md",
            "content/blog/.a.md",
            "../x.md",
            "content/../../x.md",
            "/etc/hosts",
            "",
        ] {
            assert!(resolve_writable(&root, bad).is_err(), "{bad} should be refused");
        }
        // Reading is wider: anything inside the root but .git.
        assert!(resolve(&root, "schema/collections.json").is_ok());
        assert!(resolve(&root, ".git/config").is_err());

        #[cfg(unix)]
        {
            let outside = temp("write-outside");
            std::os::unix::fs::symlink(&outside, root.join("content/escape")).unwrap();
            assert!(resolve_writable(&root, "content/escape/a.md").is_err());
            // A dangling link inside the root would create its target elsewhere.
            std::os::unix::fs::symlink(outside.join("new.md"), root.join("content/blog/dangling.md")).unwrap();
            assert!(resolve_writable(&root, "content/blog/dangling.md").is_err());
            std::os::unix::fs::symlink(root.join("content/blog"), root.join("content/inner")).unwrap();
            assert!(resolve_writable(&root, "content/inner/a.md").is_err());
            let _ = std::fs::remove_dir_all(outside);
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
