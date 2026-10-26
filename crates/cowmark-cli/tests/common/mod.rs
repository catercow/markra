//! A content checkout, an app data dir and a control server run in-process,
//! standing in for CowMark.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use cowmark_core::control::{ControlOptions, ControlServer, ControlUi};
use cowmark_core::git_tool::find_git;
use serde_json::{Value, json};

#[derive(Default)]
pub struct RecordingUi(Mutex<Vec<Value>>);

impl ControlUi for RecordingUi {
    fn focus(&self) -> Result<(), String> {
        self.0.lock().unwrap().push(json!({ "focus": true }));
        Ok(())
    }
    fn open(&self, path: &Path) -> Result<(), String> {
        self.0.lock().unwrap().push(json!({ "open": path }));
        Ok(())
    }
    fn propose_publish(&self, paths: &[String], message: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push(json!({ "propose": paths, "message": message }));
        Ok(())
    }
    fn get_latest(&self) -> Result<(), String> {
        self.0.lock().unwrap().push(json!({ "getLatest": true }));
        Ok(())
    }
    fn preview_url(&self) -> Option<String> {
        Some("http://localhost:4100/?token=t0k".into())
    }
}

impl RecordingUi {
    pub fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Short, because Unix socket paths are limited to ~100 bytes.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cm-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

pub fn git(root: &Path, args: &[&str]) {
    let status = Command::new(find_git().unwrap())
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

pub const COLLECTIONS: &str = r#"{"collections":[{"name":"blog","folder":"content/blog","url":"/blog/<file name without .md>","fields":[
  {"name":"title","type":"string","required":true},
  {"name":"description","type":"string","required":true},
  {"name":"published_at","type":"string","required":true}]}]}"#;
pub const COMPONENTS: &str = r#"{"components":[{"name":"callout","component":"Callout","group":"Text","purpose":"A highlighted box","props":[{"name":"tone","type":"\"info\" | \"warning\"","required":true}]}]}"#;

/// A content checkout on `studio`: one committed post, schemas, nothing changed.
pub fn content_repo(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(root.join("content/blog")).unwrap();
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::write(root.join("schema/collections.json"), COLLECTIONS).unwrap();
    fs::write(root.join("schema/components.json"), COMPONENTS).unwrap();
    fs::write(
        root.join("content/blog/hello.md"),
        "---\ntitle: Hello\ndescription: First post\npublished_at: '2026-01-01'\n---\n\nHi.\n",
    )
    .unwrap();
    git(&root, &["init", "--quiet", "-b", "studio"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "base"]);
    root
}

pub struct FakeApp {
    pub data_dir: PathBuf,
    pub folder: PathBuf,
    pub ui: Arc<RecordingUi>,
    pub server: Option<ControlServer>,
}

impl FakeApp {
    /// A data dir that remembers `folder`, with a control server running on it.
    pub fn start(name: &str) -> Self {
        let folder = content_repo(&format!("{name}-c"));
        let data_dir = temp_dir(&format!("{name}-d"));
        cowmark_core::access::set_folder(&data_dir, Some(folder.to_string_lossy().into_owned())).unwrap();
        let ui = Arc::new(RecordingUi::default());
        let server = ControlServer::start(ControlOptions {
            data_dir: data_dir.clone(),
            app_version: "9.9.9".into(),
            ui: ui.clone(),
        })
        .unwrap();
        Self {
            data_dir,
            folder,
            ui,
            server: Some(server),
        }
    }

    pub fn stop(&mut self) {
        self.server.take();
    }
}

impl Drop for FakeApp {
    fn drop(&mut self) {
        self.server.take();
        let _ = fs::remove_dir_all(&self.data_dir);
        let _ = fs::remove_dir_all(&self.folder);
    }
}
