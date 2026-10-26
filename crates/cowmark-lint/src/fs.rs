//! [`FileLookup`] on the local filesystem, rooted at the content repository.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::FileLookup;

/// Answers from the filesystem under `root`, matching names case-sensitively even on macOS and
/// Windows, so `Photo.JPG` for `photo.jpg` is reported as on the site's Linux servers.
pub struct FsLookup {
    root: PathBuf,
    /// Directory listings, kept for the lookup's lifetime when caching.
    cache: Option<Mutex<HashMap<PathBuf, Option<HashSet<String>>>>>,
}

impl FsLookup {
    /// Reads the filesystem on every call: for the editor, where files come and go.
    pub fn new(root: impl Into<PathBuf>) -> FsLookup {
        FsLookup {
            root: root.into(),
            cache: None,
        }
    }

    /// Remembers directory listings: for linting a whole folder at once.
    pub fn cached(root: impl Into<PathBuf>) -> FsLookup {
        FsLookup {
            root: root.into(),
            cache: Some(Mutex::new(HashMap::new())),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn listing(dir: &Path) -> Option<HashSet<String>> {
        let entries = std::fs::read_dir(dir).ok()?;
        Some(
            entries
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .collect(),
        )
    }

    fn contains(&self, dir: &Path, name: &str) -> bool {
        match &self.cache {
            None => Self::listing(dir).is_some_and(|names| names.contains(name)),
            Some(cache) => {
                let mut cache = cache
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                cache
                    .entry(dir.to_path_buf())
                    .or_insert_with(|| Self::listing(dir))
                    .as_ref()
                    .is_some_and(|names| names.contains(name))
            }
        }
    }
}

impl FileLookup for FsLookup {
    fn exists(&self, path: &str) -> bool {
        let mut dir = self.root.clone();
        for part in path
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
        {
            if part == ".." || !self.contains(&dir, part) {
                return false;
            }
            dir.push(part);
        }
        true
    }
}
