//! "Add to Claude": puts the bundled `cowmark` command into Claude Desktop's MCP
//! config, so people can use CowMark from Claude without a terminal or editing JSON
//! by hand. Only the `cowmark` server entry is written; everything else in the file
//! is kept, and the previous file is copied to `claude_desktop_config.json.bak`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

const SERVER_NAME: &str = "cowmark";
const CONFIG_FILE: &str = "claude_desktop_config.json";

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
/// any such copies are updated too.
fn config_paths() -> Vec<PathBuf> {
    let Some(config_dir) = dirs::config_dir() else { return Vec::new() };
    let mut paths = vec![config_dir.join("Claude").join(CONFIG_FILE)];
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            paths.extend(store_config_paths(&local.join("Packages")));
        }
    }
    paths
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

fn file_status(path: &Path, command: &str) -> ClaudeStatus {
    let Ok(text) = fs::read_to_string(path) else { return ClaudeStatus::Missing };
    match serde_json::from_str::<Value>(&text) {
        Ok(config) => entry_status(&config, command),
        Err(_) => ClaudeStatus::Missing,
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
    let existing = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Couldn't read {}: {error}", path.display())),
    };
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
fn combined_status(paths: &[PathBuf], command: &str) -> ClaudeStatus {
    let statuses: Vec<ClaudeStatus> = paths.iter().map(|path| file_status(path, command)).collect();
    if statuses.is_empty() {
        ClaudeStatus::Unavailable
    } else if statuses.iter().all(|status| *status == ClaudeStatus::Added) {
        ClaudeStatus::Added
    } else if statuses.iter().any(|status| *status != ClaudeStatus::Missing) {
        ClaudeStatus::Outdated
    } else {
        ClaudeStatus::Missing
    }
}

fn command_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[tauri::command]
pub(crate) fn cowmark_claude_status() -> ClaudeStatus {
    match bundled_cowmark() {
        Some(cowmark) => combined_status(&config_paths(), &command_string(&cowmark)),
        None => ClaudeStatus::Unavailable,
    }
}

#[tauri::command]
pub(crate) fn cowmark_claude_add() -> Result<ClaudeStatus, String> {
    let cowmark = bundled_cowmark().ok_or_else(|| "This copy of CowMark has no cowmark command to add.".to_string())?;
    let command = command_string(&cowmark);
    let paths = config_paths();
    if paths.is_empty() {
        return Err("Couldn't find where Claude keeps its settings.".to_string());
    }
    for path in &paths {
        add_to_file(path, &command)?;
    }
    Ok(combined_status(&paths, &command))
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
        assert_eq!(file_status(&path, COMMAND), ClaudeStatus::Added);
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
        assert_eq!(combined_status(&[], COMMAND), ClaudeStatus::Unavailable);
        assert_eq!(combined_status(&[first.clone(), second.clone()], COMMAND), ClaudeStatus::Missing);
        add_to_file(&first, COMMAND).unwrap();
        assert_eq!(combined_status(&[first.clone(), second.clone()], COMMAND), ClaudeStatus::Outdated);
        add_to_file(&second, COMMAND).unwrap();
        assert_eq!(combined_status(&[first, second], COMMAND), ClaudeStatus::Added);
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
}
