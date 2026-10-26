//! A tiny client for the control connection, driving every method end to end
//! against a real socket (named pipe on Windows) and a real git checkout.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use cowmark_core::access;
use cowmark_core::control::{self, ControlOptions, ControlPaths, ControlServer, ControlUi};
use cowmark_core::git_tool::find_git;
use serde_json::{json, Value};

#[derive(Default)]
struct RecordingUi(Mutex<Vec<Value>>);

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
        self.0.lock().unwrap().push(json!({ "propose": paths, "message": message }));
        Ok(())
    }
    fn get_latest(&self) -> Result<(), String> {
        self.0.lock().unwrap().push(json!({ "getLatest": true }));
        Ok(())
    }
    fn preview_url(&self) -> Option<String> {
        Some("http://localhost:4100/?token=t".into())
    }
}

impl RecordingUi {
    fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Short, because Unix socket paths are limited to ~100 bytes.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cm-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new(find_git().unwrap()).args(args).current_dir(root).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A content checkout on `studio` with one committed and two changed files.
fn content_folder(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(root.join("content/blog")).unwrap();
    git(&root, &["init", "--quiet", "-b", "studio"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    fs::write(root.join("content/blog/a.md"), "a").unwrap();
    fs::write(root.join("content/blog/same.md"), "same").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "base"]);
    fs::write(root.join("content/blog/a.md"), "a2").unwrap();
    fs::write(root.join("content/blog/new.md"), "new").unwrap();
    fs::write(root.join("content/notes.txt"), "not markdown").unwrap();
    root
}

struct Client {
    reader: BufReader<interprocess::local_socket::Stream>,
    token: String,
    next_id: u64,
}

impl Client {
    fn connect(paths: &ControlPaths) -> Self {
        let token = fs::read_to_string(&paths.token).unwrap();
        Self { reader: BufReader::new(control::connect(&paths.endpoint).unwrap()), token, next_id: 1 }
    }

    fn send_line(&mut self, line: &str) -> Value {
        let stream = self.reader.get_mut();
        stream.write_all(format!("{line}\n").as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut reply = String::new();
        self.reader.read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap()
    }

    fn call_with_token(&mut self, method: &str, params: Value, token: &str) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params, "token": token });
        let reply = self.send_line(&request.to_string());
        assert_eq!(reply["id"], id);
        assert_eq!(reply["jsonrpc"], "2.0");
        reply
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let token = self.token.clone();
        self.call_with_token(method, params, &token)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let reply = self.call(method, params);
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }

    fn error_code(&mut self, method: &str, params: Value) -> i64 {
        self.call(method, params)["error"]["code"].as_i64().unwrap()
    }
}

fn start(data_dir: &Path, ui: Arc<RecordingUi>) -> Result<ControlServer, String> {
    ControlServer::start(ControlOptions { data_dir: data_dir.to_path_buf(), app_version: "9.9.9".into(), ui })
}

#[test]
fn every_method_end_to_end() {
    if find_git().is_none() {
        return;
    }
    let data_dir = temp_dir("ctl-data");
    let folder = content_folder("ctl-content");
    access::set_folder(&data_dir, Some(folder.to_string_lossy().into_owned())).unwrap();
    let ui = Arc::new(RecordingUi::default());
    let server = start(&data_dir, ui.clone()).unwrap();
    let mut client = Client::connect(server.paths());

    // Authentication and framing.
    let reply = client.call_with_token("ping", json!({}), "");
    assert_eq!(reply["error"]["code"], control::UNAUTHORIZED);
    let reply = client.call_with_token("ping", json!({}), "0000");
    assert_eq!(reply["error"]["code"], control::UNAUTHORIZED);
    assert_eq!(client.send_line("{not json")["error"]["code"], control::PARSE_ERROR);
    assert_eq!(client.error_code("nope", json!({})), control::METHOD_NOT_FOUND);

    // ping
    let ping = client.ok("ping", json!({}));
    assert_eq!(ping["app"], "CowMark");
    assert_eq!(ping["version"], "9.9.9");
    assert_eq!(ping["protocol"], control::PROTOCOL_VERSION);
    assert_eq!(ping["email"], Value::Null);
    assert_eq!(ping["folder"], folder.to_string_lossy().as_ref());
    assert_eq!(ping["previewUrl"], "http://localhost:4100/?token=t");

    // status
    let status = client.ok("status", json!({}));
    assert_eq!(status["branch"], "studio");
    assert_eq!(status["ahead"], 0);
    let changed: Vec<&str> = status["changes"].as_array().unwrap().iter().map(|change| change["path"].as_str().unwrap()).collect();
    assert_eq!(changed, vec!["content/blog/a.md", "content/blog/new.md", "content/notes.txt"]);

    // focus
    assert_eq!(client.ok("focus", json!({})), json!({}));
    assert_eq!(ui.take(), vec![json!({ "focus": true })]);

    // get_latest
    assert_eq!(client.ok("get_latest", json!({})), json!({ "started": true }));
    assert_eq!(ui.take(), vec![json!({ "getLatest": true })]);

    // The library client sees the same results and errors.
    let mut library = control::Client::connect(server.paths()).unwrap();
    assert_eq!(library.call("ping", json!({})).unwrap()["app"], "CowMark");
    let error = library.call("open", json!({ "path": "../x.md" })).unwrap_err();
    assert_eq!(error.code, control::OUTSIDE_FOLDER);
    drop(library);

    // open
    assert_eq!(client.ok("open", json!({ "path": "content/blog/a.md" })), json!({ "path": "content/blog/a.md" }));
    let absolute = folder.join("content/blog/same.md");
    assert_eq!(client.ok("open", json!({ "path": absolute })), json!({ "path": "content/blog/same.md" }));
    assert_eq!(
        ui.take(),
        vec![json!({ "open": folder.join("content/blog/a.md") }), json!({ "open": absolute })]
    );
    assert_eq!(client.error_code("open", json!({ "path": "../outside.md" })), control::OUTSIDE_FOLDER);
    assert_eq!(client.error_code("open", json!({ "path": "/etc/hosts" })), control::OUTSIDE_FOLDER);
    assert_eq!(client.error_code("open", json!({ "path": "content/notes.txt" })), control::INVALID_PARAMS);
    assert_eq!(client.error_code("open", json!({ "path": "content/blog/missing.md" })), control::INVALID_PARAMS);
    assert_eq!(client.error_code("open", json!({})), control::INVALID_PARAMS);
    assert!(ui.take().is_empty());

    // propose_publish: ticks exactly the given changed paths, then focuses.
    let proposal = client.ok(
        "propose_publish",
        json!({ "paths": ["content/blog/new.md", folder.join("content/blog/a.md"), "./content/blog/new.md"], "message": "  New post  " }),
    );
    assert_eq!(proposal, json!({ "paths": ["content/blog/new.md", "content/blog/a.md"], "message": "New post" }));
    assert_eq!(
        ui.take(),
        vec![json!({ "propose": ["content/blog/new.md", "content/blog/a.md"], "message": "New post" }), json!({ "focus": true })]
    );
    assert_eq!(
        client.error_code("propose_publish", json!({ "paths": ["content/blog/same.md"], "message": "x" })),
        control::NOT_CHANGED
    );
    assert_eq!(
        client.error_code("propose_publish", json!({ "paths": ["content/../../x.md"], "message": "x" })),
        control::OUTSIDE_FOLDER
    );
    assert_eq!(client.error_code("propose_publish", json!({ "paths": [], "message": "x" })), control::INVALID_PARAMS);
    assert_eq!(client.error_code("propose_publish", json!({ "paths": "content/blog/a.md" })), control::INVALID_PARAMS);
    assert!(ui.take().is_empty());

    // Without a content folder, folder-bound methods say so.
    access::set_folder(&data_dir, None).unwrap();
    assert_eq!(client.error_code("status", json!({})), control::NO_FOLDER);
    assert_eq!(client.error_code("get_latest", json!({})), control::NO_FOLDER);
    assert_eq!(client.ok("ping", json!({}))["folder"], Value::Null);

    drop(client);
    drop(server);
    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&folder);
}

#[cfg(unix)]
#[test]
fn socket_and_token_are_owner_only() {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let data_dir = temp_dir("ctl-perm");
    let server = start(&data_dir, Arc::new(RecordingUi::default())).unwrap();
    let paths = server.paths().clone();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&paths.dir), 0o700);
    assert_eq!(mode(&paths.token), 0o600);
    assert_eq!(mode(Path::new(&paths.endpoint)), 0o600);
    assert!(fs::metadata(&paths.endpoint).unwrap().file_type().is_socket());
    assert!(Path::new(&paths.endpoint).starts_with(&data_dir));
    assert_eq!(fs::read_to_string(&paths.token).unwrap().len(), 64);
    drop(server);
    assert!(!paths.token.exists(), "the token is removed when the app quits");
    let _ = fs::remove_dir_all(&data_dir);
}

#[test]
fn one_live_owner_and_stale_sockets_are_replaced() {
    let data_dir = temp_dir("ctl-own");
    let first = start(&data_dir, Arc::new(RecordingUi::default())).unwrap();
    let first_token = fs::read_to_string(&first.paths().token).unwrap();
    let error = start(&data_dir, Arc::new(RecordingUi::default())).err().expect("second owner must fail");
    assert!(error.starts_with("already_running:"), "{error}");
    // The live server is untouched.
    assert_eq!(Client::connect(first.paths()).ok("ping", json!({}))["app"], "CowMark");
    drop(first);

    #[cfg(unix)]
    {
        // A crash leaves the socket file behind with nobody listening.
        let paths = ControlPaths::for_data_dir(&data_dir);
        drop(std::os::unix::net::UnixListener::bind(&paths.endpoint).unwrap());
        assert!(Path::new(&paths.endpoint).exists());
        assert!(control::connect(&paths.endpoint).is_err());
    }
    let second = start(&data_dir, Arc::new(RecordingUi::default())).unwrap();
    let second_token = fs::read_to_string(&second.paths().token).unwrap();
    assert_ne!(first_token, second_token, "each launch gets a new token");
    let mut client = Client::connect(second.paths());
    assert_eq!(client.ok("ping", json!({}))["app"], "CowMark");
    assert_eq!(client.call_with_token("ping", json!({}), &first_token)["error"]["code"], control::UNAUTHORIZED);
    drop(client);
    drop(second);
    let _ = fs::remove_dir_all(&data_dir);
}
