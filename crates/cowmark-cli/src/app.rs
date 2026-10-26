//! Finding and talking to the running CowMark app over its control connection,
//! and starting it when a command needs it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cowmark_core::control::{self, Client, ControlPaths, RpcError};
use serde_json::{Value, json};

/// CowMark's bundle identifier (`tauri.catercow.conf.json`); its data dir is named after it.
pub const APP_IDENTIFIER: &str = "com.catercow.cowmark";
/// Overrides the app data dir, for a CowMark started with `--user-data-dir` and for tests.
pub const DATA_DIR_ENV: &str = "COWMARK_DATA_DIR";
const START_TIMEOUT: Duration = Duration::from_secs(45);

/// The app data dir CowMark uses by default: Tauri's `app_data_dir`.
pub fn default_data_dir() -> Option<PathBuf> {
    platform_data_dir().map(|dir| dir.join(APP_IDENTIFIER))
}

fn platform_data_dir() -> Option<PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        env("HOME").map(|home| home.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")))
    }
}

/// One CowMark install, identified by its data dir.
#[derive(Debug, Clone)]
pub struct App {
    pub data_dir: PathBuf,
    /// Whether commands may start the app. Off for a data dir other than the
    /// default, since a started app would not use it.
    pub can_launch: bool,
}

impl App {
    /// `COWMARK_DATA_DIR`, else the default data dir.
    pub fn from_env() -> Option<Self> {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty()) {
            return Some(Self {
                data_dir: PathBuf::from(dir),
                can_launch: false,
            });
        }
        default_data_dir().map(|data_dir| Self {
            data_dir,
            can_launch: true,
        })
    }

    pub fn at(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            can_launch: false,
        }
    }

    pub fn paths(&self) -> ControlPaths {
        ControlPaths::for_data_dir(&self.data_dir)
    }

    /// A connection to the running app, or None when it isn't answering.
    pub fn connect_running(&self) -> Option<Connection> {
        let mut client = Client::connect(&self.paths()).ok()?;
        let ping = client.call("ping", json!({})).ok()?;
        Some(Connection { client, ping })
    }

    /// The running app's `ping`, without starting it.
    pub fn ping(&self) -> Option<Value> {
        self.connect_running().map(|connection| connection.ping)
    }

    /// The content folder CowMark remembers in its data dir, running or not.
    pub fn saved_folder(&self) -> Option<PathBuf> {
        cowmark_core::access::access_state(&self.data_dir, None)
            .folder
            .map(PathBuf::from)
    }

    /// A connection to the app, starting it first when it isn't running.
    pub fn connect(&self) -> Result<Connection, String> {
        if let Some(connection) = self.connect_running() {
            return Ok(connection);
        }
        if !self.can_launch {
            return Err(not_running());
        }
        launch()?;
        let started = Instant::now();
        while started.elapsed() < START_TIMEOUT {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(connection) = self.connect_running() {
                return Ok(connection);
            }
        }
        Err("CowMark opened but isn't answering yet. If it shows the sign-in screen, finish signing in, then try again.".into())
    }
}

fn not_running() -> String {
    "CowMark isn't running. Open CowMark (it must have its content folder set up), then try again.".into()
}

/// A live connection and the `ping` it answered.
pub struct Connection {
    client: Client,
    pub ping: Value,
}

impl Connection {
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.client.call(method, params).map_err(describe)
    }
}

/// The control error in words for a person or an agent.
pub fn describe(error: RpcError) -> String {
    match error.code {
        control::UNAUTHORIZED => "CowMark refused the request (its token changed). Try again.".into(),
        control::NO_FOLDER => "CowMark has no content folder yet. Finish setting it up in CowMark.".into(),
        _ => error.message,
    }
}

/// Starts CowMark: the app this binary is bundled in, else the installed one.
pub fn launch() -> Result<(), String> {
    let bundled = std::env::current_exe().ok().and_then(|exe| bundled_app(&exe));
    launch_platform(bundled.as_deref())
}

/// The app this `cowmark` ships in: `X.app` on macOS (we live in
/// `X.app/Contents/MacOS`), the app's .exe beside us on Windows.
pub fn bundled_app(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    if cfg!(target_os = "macos") {
        let contents = dir.parent()?;
        let app = contents.parent()?;
        let is_bundle = dir.file_name()? == "MacOS"
            && contents.file_name()? == "Contents"
            && app.extension().is_some_and(|extension| extension == "app");
        return is_bundle.then(|| app.to_path_buf());
    }
    if cfg!(windows) {
        let own = exe.file_name()?.to_string_lossy().to_ascii_lowercase();
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                name.ends_with(".exe") && name != own && !name.starts_with("uninstall")
            })
            .collect();
        found.sort();
        return found.into_iter().next();
    }
    None
}

#[cfg(target_os = "macos")]
fn launch_platform(bundled: Option<&Path>) -> Result<(), String> {
    let mut command = std::process::Command::new("/usr/bin/open");
    // -g: don't steal focus from the editor the person is typing in.
    command.arg("-g");
    match bundled {
        Some(app) => command.arg(app),
        None => command.args(["-b", APP_IDENTIFIER]),
    };
    let status = command
        .status()
        .map_err(|error| format!("Couldn't start CowMark: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Couldn't start CowMark. Is it installed in Applications?".into())
    }
}

#[cfg(windows)]
fn launch_platform(bundled: Option<&Path>) -> Result<(), String> {
    let installed = std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("CowMark").join("CowMark.exe"));
    let exe = bundled
        .map(Path::to_path_buf)
        .or_else(|| installed.filter(|path| path.is_file()))
        .ok_or("Couldn't find CowMark. Install it, or open it yourself, then try again.")?;
    std::process::Command::new(&exe)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Couldn't start CowMark ({}): {error}", exe.display()))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn launch_platform(_bundled: Option<&Path>) -> Result<(), String> {
    Err(not_running())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_named_after_the_bundle_identifier() {
        let dir = default_data_dir().unwrap();
        assert!(dir.ends_with(APP_IDENTIFIER));
        #[cfg(target_os = "macos")]
        assert!(dir.to_string_lossy().contains("Library/Application Support"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finds_the_app_bundle_around_the_sidecar() {
        assert_eq!(
            bundled_app(Path::new("/Applications/CowMark.app/Contents/MacOS/cowmark")),
            Some(PathBuf::from("/Applications/CowMark.app"))
        );
        assert_eq!(bundled_app(Path::new("/usr/local/bin/cowmark")), None);
    }

    #[test]
    fn a_missing_app_is_not_running_and_is_not_started_for_a_custom_data_dir() {
        let dir = std::env::temp_dir().join(format!("cm-cli-none-{}", std::process::id()));
        let app = App::at(&dir);
        assert!(app.ping().is_none());
        assert!(app.connect().err().unwrap().contains("isn't running"));
        assert_eq!(app.saved_folder(), None);
    }
}
