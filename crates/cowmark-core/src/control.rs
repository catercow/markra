//! The local control connection a `cowmark` CLI or MCP server uses to talk to the
//! running app: newline-delimited JSON-RPC 2.0 over a Unix domain socket
//! (macOS, Linux) or a named pipe (Windows). The protocol is in docs/mdc.md.
//!
//! Clients authenticate with the token in [`ControlPaths::token`], rewritten on
//! every launch. On Unix the socket and token live in an owner-only (0700)
//! directory in the app data dir; on Windows the pipe's DACL grants only its
//! owner. The app owns the UI, so focusing, opening and proposing go through
//! [`ControlUi`]; everything else (paths, git status) is answered here.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use interprocess::local_socket::{prelude::*, ListenerOptions, Stream};
use interprocess::TryClone;
use serde_json::{json, Value};

use crate::{access, git, git_tool, token};

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_LINE_BYTES: u64 = 1024 * 1024;
const MAX_MESSAGE_CHARS: usize = 5000;
const OPENABLE_EXTENSIONS: [&str; 2] = ["md", "markdown"];

// JSON-RPC error codes. -32000..-32099 are ours.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const UNAUTHORIZED: i64 = -32001;
pub const NO_FOLDER: i64 = -32002;
pub const OUTSIDE_FOLDER: i64 = -32003;
pub const NOT_CHANGED: i64 = -32004;
pub const APP_ERROR: i64 = -32005;

/// Where the control endpoint and its token are for one app data dir.
#[derive(Debug, Clone)]
pub struct ControlPaths {
    /// Owner-only directory holding the socket (Unix) and the token.
    pub dir: PathBuf,
    /// The socket file on Unix; the pipe name (`\\.\pipe\…`) on Windows.
    pub endpoint: String,
    /// The token file (0600 on Unix).
    pub token: PathBuf,
}

impl ControlPaths {
    pub fn for_data_dir(data_dir: &Path) -> Self {
        let dir = data_dir.join("control");
        let endpoint = if cfg!(windows) {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(data_dir.to_string_lossy().as_bytes());
            let id: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
            format!(r"\\.\pipe\cowmark-{id}")
        } else {
            dir.join("cowmark.sock").to_string_lossy().into_owned()
        };
        Self { token: dir.join("token"), dir, endpoint }
    }
}

/// Connects to the endpoint (used by clients and to detect a live owner).
pub fn connect(endpoint: &str) -> std::io::Result<Stream> {
    Stream::connect(endpoint_name(endpoint)?)
}

fn endpoint_name(endpoint: &str) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        endpoint.trim_start_matches(r"\\.\pipe\").to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        endpoint.to_fs_name::<GenericFilePath>()
    }
}

/// What only the app can do. Calls come from the control thread.
pub trait ControlUi: Send + Sync + 'static {
    /// Brings the main window forward.
    fn focus(&self) -> Result<(), String>;
    /// Opens `path` (absolute, inside the content folder) in the editor and preview.
    fn open(&self, path: &Path) -> Result<(), String>;
    /// Ticks exactly `paths` (relative to the content folder, `/`-separated) in
    /// the save checklist, fills in `message` and shows the save bar. Never saves.
    fn propose_publish(&self, paths: &[String], message: &str) -> Result<(), String>;
    /// Starts Get latest in the save bar, as if it were clicked. Returns once
    /// it has started; the outcome shows in the save bar and in `status`.
    fn get_latest(&self) -> Result<(), String> {
        Err("this CowMark can't get the latest changes on request".into())
    }
    /// The browser preview's address with its token, when it's running.
    fn preview_url(&self) -> Option<String> {
        None
    }
}

pub struct ControlOptions {
    pub data_dir: PathBuf,
    pub app_version: String,
    pub ui: Arc<dyn ControlUi>,
}

struct Context {
    data_dir: PathBuf,
    app_version: String,
    token: String,
    ui: Arc<dyn ControlUi>,
}

pub struct ControlServer {
    paths: ControlPaths,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl ControlServer {
    /// Listens on the endpoint for `options.data_dir` and writes a fresh token.
    /// Fails with `already_running:` when another live process answers there; a
    /// socket file left by a crash is replaced.
    pub fn start(options: ControlOptions) -> Result<Self, String> {
        let paths = ControlPaths::for_data_dir(&options.data_dir);
        create_private_dir(&paths.dir)?;
        if connect(&paths.endpoint).is_ok() {
            return Err("already_running: Another CowMark is already listening for the cowmark command.".into());
        }
        #[cfg(unix)]
        let _ = std::fs::remove_file(&paths.endpoint);

        let listener = listener_options(&paths.endpoint)
            .map_err(|error| error.to_string())?
            .create_sync()
            .map_err(|error| format!("Couldn't open the control socket: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&paths.endpoint, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        let secret = token::random_token()?;
        token::write_private(&paths.token, &secret)?;

        let context = Arc::new(Context {
            data_dir: options.data_dir,
            app_version: options.app_version,
            token: secret,
            ui: options.ui,
        });
        let stop = Arc::new(AtomicBool::new(false));
        let accept_stop = stop.clone();
        let accept = thread::spawn(move || {
            for connection in listener.incoming() {
                if accept_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(connection) = connection else { continue };
                let context = context.clone();
                thread::spawn(move || serve(&context, connection));
            }
        });
        Ok(Self { paths, stop, accept: Some(accept) })
    }

    pub fn paths(&self) -> &ControlPaths {
        &self.paths
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = connect(&self.paths.endpoint);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        let _ = std::fs::remove_file(&self.paths.token);
    }
}

fn create_private_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn listener_options(endpoint: &str) -> std::io::Result<ListenerOptions<'_>> {
    let options = ListenerOptions::new().name(endpoint_name(endpoint)?);
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor};
        // Protected DACL: full access for the pipe's owner (Owner Rights) and SYSTEM only.
        let descriptor = SecurityDescriptor::deserialize(widestring::u16cstr!("D:P(A;;GA;;;OW)(A;;GA;;;SY)"))?;
        options.security_descriptor(descriptor)
    };
    Ok(options)
}

// --- client ----------------------------------------------------------------------

/// A blocking client for the control connection, as the `cowmark` command uses it.
pub struct Client {
    reader: BufReader<Stream>,
    token: String,
    next_id: u64,
}

impl Client {
    /// Connects to the running app for `paths` with the token it wrote.
    pub fn connect(paths: &ControlPaths) -> std::io::Result<Self> {
        let token = std::fs::read_to_string(&paths.token)?.trim().to_string();
        Ok(Self { reader: BufReader::new(connect(&paths.endpoint)?), token, next_id: 1 })
    }

    /// Sends one request and waits for its reply. I/O failures come back as
    /// [`APP_ERROR`] with the message prefixed `connection:`.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params, "token": self.token });
        let lost = |error: std::io::Error| rpc_error(APP_ERROR, format!("connection: {error}"));
        let mut line = request.to_string();
        line.push('\n');
        let stream = self.reader.get_mut();
        stream.write_all(line.as_bytes()).map_err(lost)?;
        stream.flush().map_err(lost)?;
        let mut reply = String::new();
        if (&mut self.reader).take(MAX_LINE_BYTES).read_line(&mut reply).map_err(lost)? == 0 {
            return Err(rpc_error(APP_ERROR, "connection: CowMark closed the connection"));
        }
        let reply: Value = serde_json::from_str(&reply).map_err(|error| rpc_error(PARSE_ERROR, error.to_string()))?;
        if let Some(error) = reply.get("error") {
            return Err(rpc_error(
                error.get("code").and_then(Value::as_i64).unwrap_or(APP_ERROR),
                error.get("message").and_then(Value::as_str).unwrap_or("unknown error"),
            ));
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }
}

// --- requests --------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}

fn rpc_error(code: i64, message: impl Into<String>) -> RpcError {
    RpcError { code, message: message.into() }
}

fn serve(context: &Context, connection: Stream) {
    let mut writer = match connection.try_clone() {
        Ok(writer) => writer,
        Err(_) => return,
    };
    let mut reader = BufReader::new(connection);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE_BYTES).read_line(&mut line) {
            Ok(0) => return,
            Ok(_) if !line.ends_with('\n') && line.len() as u64 >= MAX_LINE_BYTES => {
                let _ = write_line(&mut writer, &failure(Value::Null, rpc_error(INVALID_REQUEST, "request too large")));
                return;
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
        if line.trim().is_empty() {
            continue;
        }
        let response = respond(context, &line);
        if write_line(&mut writer, &response).is_err() {
            return;
        }
    }
}

fn write_line(writer: &mut Stream, value: &Value) -> std::io::Result<()> {
    let mut text = value.to_string();
    text.push('\n');
    writer.write_all(text.as_bytes())?;
    writer.flush()
}

fn failure(id: Value, error: RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": error.code, "message": error.message } })
}

fn respond(context: &Context, line: &str) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(error) => return failure(Value::Null, rpc_error(PARSE_ERROR, error.to_string())),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return failure(id, rpc_error(INVALID_REQUEST, "missing method"));
    };
    let given = request.get("token").and_then(Value::as_str).unwrap_or_default();
    if !token::matches(&context.token, given) {
        return failure(id, rpc_error(UNAUTHORIZED, "missing or wrong token; read it from the token file"));
    }
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match dispatch(context, method, &params) {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => failure(id, error),
    }
}

fn dispatch(context: &Context, method: &str, params: &Value) -> Result<Value, RpcError> {
    match method {
        "ping" => {
            let state = access::access_state(&context.data_dir, None);
            Ok(json!({
                "app": "CowMark",
                "version": context.app_version,
                "protocol": PROTOCOL_VERSION,
                "email": state.email,
                "folder": state.folder,
                "previewUrl": context.ui.preview_url(),
            }))
        }
        "status" => {
            let folder = content_folder(context)?;
            let status = changes(&folder)?;
            serde_json::to_value(status).map_err(|error| rpc_error(APP_ERROR, error.to_string()))
        }
        "focus" => {
            context.ui.focus().map_err(|error| rpc_error(APP_ERROR, error))?;
            Ok(json!({}))
        }
        "open" => {
            let folder = content_folder(context)?;
            let path = params.get("path").and_then(Value::as_str).ok_or_else(|| rpc_error(INVALID_PARAMS, "`path` is required"))?;
            let (absolute, relative) = resolve_in_folder(&folder, path)?;
            if !absolute.is_file() {
                return Err(rpc_error(INVALID_PARAMS, format!("{relative} doesn't exist")));
            }
            let extension = absolute.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
            if !OPENABLE_EXTENSIONS.contains(&extension.as_str()) {
                return Err(rpc_error(INVALID_PARAMS, format!("{relative} isn't a Markdown file")));
            }
            context.ui.open(&absolute).map_err(|error| rpc_error(APP_ERROR, error))?;
            Ok(json!({ "path": relative }))
        }
        "get_latest" => {
            content_folder(context)?;
            context.ui.get_latest().map_err(|error| rpc_error(APP_ERROR, error))?;
            Ok(json!({ "started": true }))
        }
        "propose_publish" => {
            let folder = content_folder(context)?;
            let requested = params
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| rpc_error(INVALID_PARAMS, "`paths` must be an array of paths"))?;
            if requested.is_empty() {
                return Err(rpc_error(INVALID_PARAMS, "`paths` is empty"));
            }
            let message = params.get("message").and_then(Value::as_str).unwrap_or_default().trim().to_string();
            if message.chars().count() > MAX_MESSAGE_CHARS {
                return Err(rpc_error(INVALID_PARAMS, format!("`message` is longer than {MAX_MESSAGE_CHARS} characters")));
            }
            let mut paths = Vec::new();
            for value in requested {
                let path = value.as_str().ok_or_else(|| rpc_error(INVALID_PARAMS, "`paths` must be strings"))?;
                let (_, relative) = resolve_in_folder(&folder, path)?;
                if !paths.contains(&relative) {
                    paths.push(relative);
                }
            }
            let changed: Vec<String> = changes(&folder)?.changes.into_iter().map(|change| change.path).collect();
            let unchanged: Vec<&str> = paths.iter().filter(|path| !changed.contains(path)).map(String::as_str).collect();
            if !unchanged.is_empty() {
                return Err(rpc_error(NOT_CHANGED, format!("no changes to save in: {}", unchanged.join(", "))));
            }
            context.ui.propose_publish(&paths, &message).map_err(|error| rpc_error(APP_ERROR, error))?;
            let _ = context.ui.focus();
            Ok(json!({ "paths": paths, "message": message }))
        }
        _ => Err(rpc_error(METHOD_NOT_FOUND, format!("unknown method `{method}`"))),
    }
}

fn content_folder(context: &Context) -> Result<PathBuf, RpcError> {
    access::access_state(&context.data_dir, None)
        .folder
        .map(PathBuf::from)
        .filter(|folder| folder.is_dir())
        .ok_or_else(|| rpc_error(NO_FOLDER, "CowMark has no content folder yet"))
}

fn changes(folder: &Path) -> Result<git::ContentGitStatus, RpcError> {
    let binary = git_tool::require_git().map_err(|error| rpc_error(APP_ERROR, error))?;
    git::status(&binary, folder).map_err(|error| rpc_error(APP_ERROR, error))
}

/// Resolves `path` (absolute, or relative to `folder`) to an absolute path and
/// the `/`-separated path relative to `folder`. Rejects anything that is, or
/// through a symlink leads, outside the folder. The file need not exist.
pub fn resolve_in_folder(folder: &Path, path: &str) -> Result<(PathBuf, String), RpcError> {
    let outside = || rpc_error(OUTSIDE_FOLDER, format!("{path} is outside the content folder"));
    if path.trim().is_empty() || path.contains('\0') {
        return Err(rpc_error(INVALID_PARAMS, "empty path"));
    }
    let folder = folder.canonicalize().map_err(|_| rpc_error(NO_FOLDER, "the content folder is missing"))?;
    let given = Path::new(path);
    let relative = if given.is_absolute() {
        let absolute = given.canonicalize().unwrap_or_else(|_| given.to_path_buf());
        absolute.strip_prefix(&folder).map_err(|_| outside())?.to_path_buf()
    } else {
        given.to_path_buf()
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str().ok_or_else(outside)?.to_string()),
            Component::CurDir => {}
            _ => return Err(outside()),
        }
    }
    if parts.is_empty() || parts.first().is_some_and(|first| first == ".git") {
        return Err(outside());
    }
    let absolute = parts.iter().fold(folder.clone(), |path, part| path.join(part));
    // The nearest existing ancestor must still be inside once symlinks resolve.
    let mut existing = absolute.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or_else(outside)?;
    }
    let resolved = existing.canonicalize().map_err(|_| outside())?;
    if !resolved.starts_with(&folder) {
        return Err(outside());
    }
    Ok((absolute, parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_paths_inside_the_folder_only() {
        let root = std::env::temp_dir().join(format!("cowmark-control-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("content/blog")).unwrap();
        std::fs::write(root.join("content/blog/a.md"), "a").unwrap();
        let folder = root.canonicalize().unwrap();

        let (absolute, relative) = resolve_in_folder(&folder, "content/blog/a.md").unwrap();
        assert_eq!(absolute, folder.join("content/blog/a.md"));
        assert_eq!(relative, "content/blog/a.md");
        assert_eq!(resolve_in_folder(&folder, "./content/blog/gone.md").unwrap().1, "content/blog/gone.md");
        let inside = folder.join("content/blog/a.md");
        assert_eq!(resolve_in_folder(&folder, inside.to_str().unwrap()).unwrap().1, "content/blog/a.md");

        for bad in ["../x.md", "content/../../x.md", "/etc/passwd", ".git/config", "", "."] {
            assert!(resolve_in_folder(&folder, bad).is_err(), "{bad} should be rejected");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), folder.join("content/escape")).unwrap();
            assert_eq!(resolve_in_folder(&folder, "content/escape/x.md").err().map(|error| error.code), Some(OUTSIDE_FOLDER));
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
