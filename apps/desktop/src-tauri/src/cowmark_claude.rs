//! "Add to Claude": puts the bundled `cowmark` command into Claude Desktop's MCP
//! config, so people can use CowMark from Claude without a terminal or editing JSON
//! by hand. Only the `cowmark` server entry is written; everything else in the file
//! is kept, and the previous file is copied to `claude_desktop_config.json.bak`.
//!
//! Claude Desktop rewrites this file while it runs (it keeps its own settings there) and
//! drops entries it didn't load, so CowMark only writes while Claude is quit.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};

const SERVER_NAME: &str = "cowmark";
const CONFIG_FILE: &str = "claude_desktop_config.json";

/// Error prefix the save bar turns into its "quit Claude first" message, followed by
/// `windows` or `macos` so it can say how to quit.
const CLAUDE_RUNNING: &str = "claude_running";

/// Claude may be rewriting its file at the moment CowMark reads it (a sharing violation on
/// Windows, or half-written JSON), so a failed read is tried this many times.
const READ_ATTEMPTS: u32 = 3;
const READ_RETRY_DELAY: Duration = Duration::from_millis(150);

/// Where CowMark stands in Claude Desktop's config.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ClaudeStatus {
    /// This build has no bundled `cowmark` (stock Markra), or Claude's folder can't be found.
    Unavailable,
    Missing,
    /// There is a `cowmark` entry, but it runs a different program (CowMark moved, or an old copy).
    Outdated,
    Added,
}

/// The `cowmark` program bundled next to the app's own executable.
fn bundled_cowmark() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "cowmark.exe" } else { "cowmark" };
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

/// The config files Claude Desktop may read. The first is the usual one. On Windows, Claude
/// installed from the Microsoft Store reads a copy inside its package folder instead, so
/// any such copies are updated too. Often the "copy" is the usual file seen through a
/// second path, so paths that lead to the same file are listed once.
fn config_paths() -> Vec<PathBuf> {
    let Some(config_dir) = dirs::config_dir() else { return Vec::new() };
    let mut paths = vec![config_dir.join("Claude").join(CONFIG_FILE)];
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            paths.extend(store_config_paths(&local.join("Packages")));
        }
    }
    distinct_files(paths)
}

/// `paths` without the ones that lead to a file already listed, keeping the first.
fn distinct_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !kept.iter().any(|other| same_location(other, &path)) {
            kept.push(path);
        }
    }
    kept
}

/// Windows can show one file at two paths without a symlink (the Store package's folder
/// maps onto `%APPDATA%`), so this compares file identity, not text: the files themselves
/// when both exist, otherwise their folders.
fn same_location(a: &Path, b: &Path) -> bool {
    let same = |a: &Path, b: &Path| same_file::is_same_file(a, b).unwrap_or(false);
    a == b
        || same(a, b)
        || (a.file_name() == b.file_name()
            && matches!((a.parent(), b.parent()), (Some(x), Some(y)) if same(x, y)))
}

/// `Packages/Claude_*/LocalCache/Roaming/Claude/claude_desktop_config.json`, for each
/// installed Store package whose Claude folder exists.
fn store_config_paths(packages: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(packages) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("Claude_"))
        .map(|entry| entry.path().join("LocalCache").join("Roaming").join("Claude"))
        .filter(|dir| dir.is_dir())
        .map(|dir| dir.join(CONFIG_FILE))
        .collect();
    paths.sort();
    paths
}

/// The `cowmark` entry's state in a parsed config.
fn entry_status(config: &Value, command: &str) -> ClaudeStatus {
    match config.get("mcpServers").and_then(|servers| servers.get(SERVER_NAME)) {
        None => ClaudeStatus::Missing,
        Some(entry)
            if entry.get("command").and_then(Value::as_str) == Some(command)
                && entry.get("args") == Some(&json!(["mcp"])) =>
        {
            ClaudeStatus::Added
        }
        Some(_) => ClaudeStatus::Outdated,
    }
}

fn file_status(path: &Path, command: &str) -> Result<ClaudeStatus, String> {
    file_status_with(path, command, |path| fs::read_to_string(path), || std::thread::sleep(READ_RETRY_DELAY))
}

/// A file that isn't there is `Missing`. One that can't be read or parsed is tried again
/// (Claude may be halfway through rewriting it) and reported if it still fails, rather
/// than counted as `Missing`: that once made a correct file show "Update in Claude".
fn file_status_with(
    path: &Path,
    command: &str,
    mut read: impl FnMut(&Path) -> io::Result<String>,
    mut wait: impl FnMut(),
) -> Result<ClaudeStatus, String> {
    let mut attempt = 1;
    loop {
        let problem = match read(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(ClaudeStatus::Missing),
            Err(error) => format!("couldn't be read ({error})"),
            Ok(text) if text.trim().is_empty() => return Ok(ClaudeStatus::Missing),
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(config) => return Ok(entry_status(&config, command)),
                Err(error) => format!("isn't valid JSON ({error})"),
            },
        };
        if attempt >= READ_ATTEMPTS {
            return Err(format!("Claude's settings file {} {problem}.", path.display()));
        }
        attempt += 1;
        wait();
    }
}

/// The file's text, or `None` when it doesn't exist. Other read errors are tried again.
fn read_existing(path: &Path, mut read: impl FnMut(&Path) -> io::Result<String>, mut wait: impl FnMut()) -> Result<Option<String>, String> {
    let mut attempt = 1;
    loop {
        match read(path) {
            Ok(text) => return Ok(Some(text)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) if attempt >= READ_ATTEMPTS => return Err(format!("Couldn't read {}: {error}", path.display())),
            Err(_) => {
                attempt += 1;
                wait();
            }
        }
    }
}

/// The config text with the `cowmark` entry added or pointed at `command`. Other servers,
/// other settings and any extra fields on the entry (such as `env`) are kept.
fn merged_config(existing: Option<&str>, command: &str) -> Result<String, String> {
    let mut config = match existing.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => serde_json::from_str::<Value>(text).map_err(|error| {
            format!("Claude's settings file isn't valid JSON ({error}), so CowMark left it alone.")
        })?,
        None => Value::Object(Map::new()),
    };
    let Some(root) = config.as_object_mut() else {
        return Err("Claude's settings file isn't a JSON object, so CowMark left it alone.".to_string());
    };
    let servers = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err("\"mcpServers\" in Claude's settings file isn't an object, so CowMark left it alone.".to_string());
    };
    let entry = servers.entry(SERVER_NAME).or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(entry) = entry.as_object_mut() {
        entry.insert("command".to_string(), Value::String(command.to_string()));
        entry.insert("args".to_string(), json!(["mcp"]));
    }
    let mut text = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    text.push('\n');
    Ok(text)
}

/// Writes the merged config to `path`, backing up the old file and replacing it in one step.
fn add_to_file(path: &Path, command: &str) -> Result<(), String> {
    let existing = read_existing(path, |path| fs::read_to_string(path), || std::thread::sleep(READ_RETRY_DELAY))?;
    let text = merged_config(existing.as_deref(), command)?;
    if existing.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    let dir = path.parent().ok_or_else(|| format!("{} has no folder", path.display()))?;
    fs::create_dir_all(dir).map_err(|error| format!("Couldn't create {}: {error}", dir.display()))?;
    if existing.is_some() {
        fs::copy(path, path.with_extension("json.bak"))
            .map_err(|error| format!("Couldn't back up {}: {error}", path.display()))?;
    }
    let temp = path.with_extension("json.cowmark-tmp");
    fs::write(&temp, text).map_err(|error| format!("Couldn't write {}: {error}", temp.display()))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("Couldn't write {}: {error}", path.display())
    })
}

/// Added only when every file Claude may read has the current entry.
/// A file that can't be read is an error, not `Missing`.
fn combined_status(paths: &[PathBuf], command: &str) -> Result<ClaudeStatus, String> {
    let statuses = paths.iter().map(|path| file_status(path, command)).collect::<Result<Vec<_>, _>>()?;
    Ok(if statuses.is_empty() {
        ClaudeStatus::Unavailable
    } else if statuses.iter().all(|status| *status == ClaudeStatus::Added) {
        ClaudeStatus::Added
    } else if statuses.iter().any(|status| *status != ClaudeStatus::Missing) {
        ClaudeStatus::Outdated
    } else {
        ClaudeStatus::Missing
    })
}

fn command_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Whether a running program, given by its full path, is Claude Desktop. Claude Code's
/// `claude.exe` (which Claude Desktop keeps under `%APPDATA%\Claude\claude-code`, or a
/// standalone install) has the same name, so the folder decides.
fn is_claude_desktop(image: &str) -> bool {
    let image = image.replace('\\', "/").to_lowercase();
    // Windows: the Microsoft Store package (`WindowsApps/Claude_<version>_x64__<id>/app/Claude.exe`)
    // or the per-user installer (`AnthropicClaude/app-<version>/claude.exe`).
    (image.ends_with("/claude.exe") && (image.contains("/windowsapps/claude_") || image.contains("/anthropicclaude/")))
        // macOS: the app's main program, not its helpers.
        || image.ends_with("/claude.app/contents/macos/claude")
}

/// Full paths of the running programs that may be Claude Desktop. Programs whose path
/// can't be read are left out.
#[cfg(windows)]
fn running_programs() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut programs = Vec::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return programs;
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let name_len = entry.szExeFile.iter().position(|unit| *unit == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
        // Only `claude.exe` can be Claude Desktop, so other programs' paths aren't looked up.
        if name.eq_ignore_ascii_case("claude.exe") {
            let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
            if !process.is_null() {
                let mut buffer = vec![0u16; 32_768];
                let mut len = buffer.len() as u32;
                if unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len) } != 0 {
                    programs.push(String::from_utf16_lossy(&buffer[..len as usize]));
                }
                unsafe { CloseHandle(process) };
            }
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    programs
}

/// On macOS, `ps -o comm` prints each program's full path.
#[cfg(target_os = "macos")]
fn running_programs() -> Vec<String> {
    std::process::Command::new("/bin/ps")
        .args(["-axo", "comm="])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).lines().map(|line| line.trim().to_string()).collect())
        .unwrap_or_default()
}

/// There's no Claude Desktop for other systems.
#[cfg(not(any(windows, target_os = "macos")))]
fn running_programs() -> Vec<String> {
    Vec::new()
}

fn claude_desktop_running() -> bool {
    running_programs().iter().any(|program| is_claude_desktop(program))
}

/// Adds the entry to every file, unless Claude Desktop is running: it would put back its
/// own copy of the file and drop the entry. Closing Claude's window isn't enough, since it
/// keeps running in the system tray (Windows) or menu bar (macOS).
fn add_to_claude(paths: &[PathBuf], command: &str, claude_running: bool) -> Result<ClaudeStatus, String> {
    if paths.is_empty() {
        return Err("Couldn't find where Claude keeps its settings.".to_string());
    }
    if claude_running {
        return Err(if cfg!(target_os = "macos") {
            format!("{CLAUDE_RUNNING}: macos: Claude is still running. Quit Claude with Cmd+Q, then click Add to Claude again.")
        } else {
            format!(
                "{CLAUDE_RUNNING}: windows: Claude is still running. Quit Claude from the system tray (right-click the Claude icon, then Quit), then click Add to Claude again."
            )
        });
    }
    for path in paths {
        add_to_file(path, command)?;
    }
    // Every file was just written with the entry, so a read that fails now doesn't undo that.
    Ok(combined_status(paths, command).unwrap_or(ClaudeStatus::Added))
}

#[tauri::command]
pub(crate) fn cowmark_claude_status() -> Result<ClaudeStatus, String> {
    match bundled_cowmark() {
        Some(cowmark) => combined_status(&config_paths(), &command_string(&cowmark)),
        None => Ok(ClaudeStatus::Unavailable),
    }
}

#[tauri::command]
pub(crate) fn cowmark_claude_add() -> Result<ClaudeStatus, String> {
    let cowmark = bundled_cowmark().ok_or_else(|| "This copy of CowMark has no cowmark command to add.".to_string())?;
    add_to_claude(&config_paths(), &command_string(&cowmark), claude_desktop_running())
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: &str = "/Applications/CowMark.app/Contents/MacOS/cowmark";

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn creates_a_config_with_only_the_cowmark_server() {
        let text = merged_config(None, COMMAND).unwrap();
        assert_eq!(parse(&text), json!({ "mcpServers": { "cowmark": { "command": COMMAND, "args": ["mcp"] } } }));
        assert_eq!(merged_config(Some("  \n"), COMMAND).unwrap(), text);
    }

    #[test]
    fn keeps_other_servers_settings_and_order() {
        let existing = r#"{
  "globalShortcut": "Ctrl+Space",
  "mcpServers": {
    "files": { "command": "npx", "args": ["files"] },
    "cowmark": { "command": "/old/cowmark", "args": ["mcp"], "env": { "DEBUG": "1" } }
  }
}"#;
        let text = merged_config(Some(existing), COMMAND).unwrap();
        let config = parse(&text);
        assert_eq!(config["globalShortcut"], "Ctrl+Space");
        assert_eq!(config["mcpServers"]["files"], json!({ "command": "npx", "args": ["files"] }));
        assert_eq!(
            config["mcpServers"]["cowmark"],
            json!({ "command": COMMAND, "args": ["mcp"], "env": { "DEBUG": "1" } })
        );
        assert!(text.find("globalShortcut").unwrap() < text.find("mcpServers").unwrap());
        assert!(text.find("\"files\"").unwrap() < text.find("\"cowmark\"").unwrap());
    }

    #[test]
    fn leaves_files_it_cannot_understand_alone() {
        assert!(merged_config(Some("{ not json"), COMMAND).unwrap_err().contains("isn't valid JSON"));
        assert!(merged_config(Some("[]"), COMMAND).unwrap_err().contains("isn't a JSON object"));
        assert!(merged_config(Some(r#"{ "mcpServers": [] }"#), COMMAND).unwrap_err().contains("\"mcpServers\""));
    }

    #[test]
    fn reports_missing_outdated_and_added_entries() {
        assert_eq!(entry_status(&json!({}), COMMAND), ClaudeStatus::Missing);
        assert_eq!(entry_status(&json!({ "mcpServers": {} }), COMMAND), ClaudeStatus::Missing);
        assert_eq!(
            entry_status(&json!({ "mcpServers": { "cowmark": { "command": "/old/cowmark", "args": ["mcp"] } } }), COMMAND),
            ClaudeStatus::Outdated
        );
        assert_eq!(
            entry_status(&json!({ "mcpServers": { "cowmark": { "command": COMMAND } } }), COMMAND),
            ClaudeStatus::Outdated
        );
        assert_eq!(
            entry_status(&json!({ "mcpServers": { "cowmark": { "command": COMMAND, "args": ["mcp"] } } }), COMMAND),
            ClaudeStatus::Added
        );
    }

    #[test]
    fn writes_the_file_and_backs_up_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Claude").join(CONFIG_FILE);

        add_to_file(&path, COMMAND).unwrap();
        assert_eq!(file_status(&path, COMMAND).unwrap(), ClaudeStatus::Added);
        assert!(!path.with_extension("json.bak").exists());

        let old = r#"{ "mcpServers": { "files": { "command": "npx" } } }"#;
        fs::write(&path, old).unwrap();
        add_to_file(&path, COMMAND).unwrap();
        assert_eq!(fs::read_to_string(path.with_extension("json.bak")).unwrap(), old);
        assert_eq!(parse(&fs::read_to_string(&path).unwrap())["mcpServers"]["files"]["command"], "npx");
        assert!(!path.with_extension("json.cowmark-tmp").exists());

        fs::write(&path, "{ broken").unwrap();
        assert!(add_to_file(&path, COMMAND).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ broken");
    }

    #[test]
    fn needs_every_config_claude_may_read_to_be_added() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a.json");
        let second = dir.path().join("b.json");
        assert_eq!(combined_status(&[], COMMAND).unwrap(), ClaudeStatus::Unavailable);
        assert_eq!(combined_status(&[first.clone(), second.clone()], COMMAND).unwrap(), ClaudeStatus::Missing);
        add_to_file(&first, COMMAND).unwrap();
        assert_eq!(combined_status(&[first.clone(), second.clone()], COMMAND).unwrap(), ClaudeStatus::Outdated);
        add_to_file(&second, COMMAND).unwrap();
        assert_eq!(combined_status(&[first, second], COMMAND).unwrap(), ClaudeStatus::Added);
    }

    #[test]
    fn finds_store_installs_of_claude() {
        let dir = tempfile::tempdir().unwrap();
        let packages = dir.path();
        fs::create_dir_all(packages.join("Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude")).unwrap();
        fs::create_dir_all(packages.join("Claude_unused/LocalCache")).unwrap();
        fs::create_dir_all(packages.join("Other_123/LocalCache/Roaming/Claude")).unwrap();
        assert_eq!(
            store_config_paths(packages),
            vec![packages.join("Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude").join(CONFIG_FILE)]
        );
    }

    #[test]
    fn lists_a_file_reached_through_two_paths_once() {
        let dir = tempfile::tempdir().unwrap();
        let usual = dir.path().join("Roaming/Claude");
        let store = dir.path().join("Store/Claude");
        let other = dir.path().join("Other/Claude");
        for folder in [&usual, &store, &other] {
            fs::create_dir_all(folder).unwrap();
        }
        fs::write(usual.join(CONFIG_FILE), "{}").unwrap();
        // A hard link: one file, two paths, neither a symlink (as with the Store package).
        fs::hard_link(usual.join(CONFIG_FILE), store.join(CONFIG_FILE)).unwrap();
        let paths = vec![usual.join(CONFIG_FILE), store.join(CONFIG_FILE), other.join(CONFIG_FILE), usual.join(CONFIG_FILE)];
        assert_eq!(distinct_files(paths), vec![usual.join(CONFIG_FILE), other.join(CONFIG_FILE)]);
    }

    #[cfg(unix)]
    #[test]
    fn lists_a_missing_file_once_when_its_folder_is_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let usual = dir.path().join("Claude");
        fs::create_dir_all(&usual).unwrap();
        let alias = dir.path().join("Alias");
        std::os::unix::fs::symlink(&usual, &alias).unwrap();
        let paths = vec![usual.join(CONFIG_FILE), alias.join(CONFIG_FILE)];
        assert_eq!(distinct_files(paths), vec![usual.join(CONFIG_FILE)]);
    }

    fn io_error(kind: io::ErrorKind) -> io::Error {
        io::Error::new(kind, "test")
    }

    #[test]
    fn retries_a_failed_read_instead_of_calling_the_entry_missing() {
        let added = merged_config(None, COMMAND).unwrap();
        let path = Path::new("claude_desktop_config.json");

        // Locked for a moment (a sharing violation on Windows), then fine.
        let mut reads = vec![Err(io_error(io::ErrorKind::PermissionDenied)), Ok(added.clone())].into_iter();
        let mut waits = 0;
        let status = file_status_with(path, COMMAND, |_| reads.next().unwrap(), || waits += 1);
        assert_eq!((status, waits), (Ok(ClaudeStatus::Added), 1));

        // Half-written by Claude, then fine.
        let mut reads = vec![Ok("{ \"mcpSer".to_string()), Ok(added.clone())].into_iter();
        assert_eq!(file_status_with(path, COMMAND, |_| reads.next().unwrap(), || {}), Ok(ClaudeStatus::Added));

        // Still failing after every attempt: an error, not Missing.
        let mut attempts = 0;
        let status = file_status_with(path, COMMAND, |_| { attempts += 1; Err(io_error(io::ErrorKind::PermissionDenied)) }, || {});
        assert!(status.unwrap_err().contains("couldn't be read"));
        assert_eq!(attempts, READ_ATTEMPTS);
        let status = file_status_with(path, COMMAND, |_| Ok("{ broken".to_string()), || {});
        assert!(status.unwrap_err().contains("isn't valid JSON"));

        // No file, or an empty one, is Missing at once.
        let mut attempts = 0;
        let status = file_status_with(path, COMMAND, |_| { attempts += 1; Err(io_error(io::ErrorKind::NotFound)) }, || {});
        assert_eq!((status, attempts), (Ok(ClaudeStatus::Missing), 1));
        assert_eq!(file_status_with(path, COMMAND, |_| Ok(" \n".to_string()), || {}), Ok(ClaudeStatus::Missing));
    }

    #[test]
    fn retries_reading_before_writing() {
        let path = Path::new("claude_desktop_config.json");
        let mut reads = vec![Err(io_error(io::ErrorKind::PermissionDenied)), Ok("{}".to_string())].into_iter();
        assert_eq!(read_existing(path, |_| reads.next().unwrap(), || {}), Ok(Some("{}".to_string())));
        assert_eq!(read_existing(path, |_| Err(io_error(io::ErrorKind::NotFound)), || {}), Ok(None));
        let error = read_existing(path, |_| Err(io_error(io::ErrorKind::PermissionDenied)), || {}).unwrap_err();
        assert!(error.starts_with("Couldn't read"));
    }

    #[test]
    fn an_unreadable_file_makes_the_combined_status_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("a.json");
        add_to_file(&good, COMMAND).unwrap();
        let broken = dir.path().join("b.json");
        fs::write(&broken, "{ broken").unwrap();
        assert!(combined_status(&[good, broken], COMMAND).is_err());
    }

    #[test]
    fn tells_claude_desktop_from_claude_code() {
        for desktop in [
            r"C:\Program Files\WindowsApps\Claude_4.16336.0.0_x64__pzs8sxrjxfjjc\app\Claude.exe",
            r"C:\Users\pat\AppData\Local\AnthropicClaude\app-0.10.14\claude.exe",
            "/Applications/Claude.app/Contents/MacOS/Claude",
            "/Users/pat/Applications/Claude.app/Contents/MacOS/Claude",
        ] {
            assert!(is_claude_desktop(desktop), "{desktop}");
        }
        for other in [
            r"C:\Users\pat\AppData\Roaming\Claude\claude-code\2.1.317\a33aa5518497\claude.exe",
            r"C:\Users\pat\.local\bin\claude.exe",
            r"C:\Program Files\WindowsApps\Claude_4.16336.0.0_x64__pzs8sxrjxfjjc\app\resources\helper.exe",
            "/Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper",
            "/Users/pat/.local/bin/claude",
        ] {
            assert!(!is_claude_desktop(other), "{other}");
        }
    }

    #[test]
    fn writes_nothing_while_claude_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        let error = add_to_claude(&[path.clone()], COMMAND, true).unwrap_err();
        assert!(error.starts_with("claude_running: "), "{error}");
        assert!(!path.exists());

        assert_eq!(add_to_claude(&[path.clone()], COMMAND, false), Ok(ClaudeStatus::Added));
        assert_eq!(file_status(&path, COMMAND), Ok(ClaudeStatus::Added));
        assert!(add_to_claude(&[], COMMAND, false).is_err());
    }
}
