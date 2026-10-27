//! Tauri commands for CowMark's in-editor content lint. The rules are in the
//! `cowmark-lint` crate, shared with `cowmark check` and CI, so all three report
//! the same problems. This keeps one prepared `Linter` per content folder and
//! rebuilds it when the schema changes: the schema the site serves, once
//! `cowmark_schema` has fetched it (or its cached copy), else the folder's
//! `schema/` files, rebuilt when they change on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use cowmark_core::content_check::read_schemas;
use cowmark_core::csv_table::{self, ComponentTable};
use cowmark_core::schema_source::{self, Loaded, Memo, Origin, Request};
use cowmark_lint::fs::FsLookup;
use cowmark_lint::{Diagnostic, Formatted, Linter, Messages, Schemas};
use serde::Serialize;
use serde_json::Value;

const SCHEMA_FILES: [&str; 2] = ["schema/components.json", "schema/collections.json"];

/// What a schema file looked like when the linter was built: absent, or its mtime and size.
type Fingerprint = [Option<(SystemTime, u64)>; 2];

/// Which schema a linter was built from.
#[derive(Clone, PartialEq, Eq)]
enum SchemaKey {
    /// The schema `cowmark_schema` loaded, by hash.
    Loaded(String),
    /// The folder's own files (or the cached copy) before that, by fingerprint.
    Folder(Fingerprint),
}

struct Prepared {
    key: SchemaKey,
    linter: Linter,
}

#[derive(Default, Clone)]
pub(crate) struct ContentLintState {
    linters: Arc<Mutex<HashMap<PathBuf, Prepared>>>,
    /// The last schema `cowmark_schema` loaded for each folder.
    loaded: Arc<Mutex<HashMap<PathBuf, Arc<Loaded>>>>,
    memo: Arc<Memo>,
}

fn fingerprint(root: &Path) -> Fingerprint {
    SCHEMA_FILES.map(|file| {
        let meta = std::fs::metadata(root.join(file)).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    })
}

impl ContentLintState {
    /// Lints `text` as the repository-relative `path` in the content folder `root`.
    /// `messages` replaces the message templates when given (the app's language).
    pub(crate) fn lint(
        &self,
        root: &Path,
        path: &str,
        text: &str,
        messages: Option<Messages>,
    ) -> Vec<Diagnostic> {
        self.with_linter(root, messages, |linter| linter.lint(path, text, &FsLookup::new(root)))
    }

    /// `text` with component input in canonical form, and the problems left in it.
    pub(crate) fn format(
        &self,
        root: &Path,
        path: &str,
        text: &str,
        messages: Option<Messages>,
    ) -> Formatted {
        self.with_linter(root, messages, |linter| linter.format(path, text, &FsLookup::new(root)))
    }

    /// Loads the schema for `root` (see `cowmark_core::schema_source`): the site's, fetched at
    /// most every ten minutes unless `force`, else its cached copy, else the folder's files.
    /// `site` is used when the folder has no `preview.json`. The linter follows on its next run.
    pub(crate) fn refresh(&self, root: &Path, site: Option<&str>, force: bool) -> Arc<Loaded> {
        if force {
            self.memo.clear();
        }
        let request = Request {
            url: schema_source::default_url(root, None, site),
            offline: false,
            dir: None,
        };
        self.store(root, schema_source::load_with(root, &request, Some(&self.memo)))
    }

    fn store(&self, root: &Path, loaded: Loaded) -> Arc<Loaded> {
        let loaded = Arc::new(loaded);
        self.loaded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(root.to_path_buf(), loaded.clone());
        loaded
    }

    /// The schema loaded for `root`, unless it was the folder's files (or nothing), which are
    /// read afresh so edits to them show up.
    fn served(&self, root: &Path) -> Option<Arc<Loaded>> {
        let loaded = self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        loaded
            .get(root)
            .filter(|loaded| !matches!(loaded.origin, Origin::Repo | Origin::None))
            .cloned()
    }

    fn schema_key(&self, root: &Path) -> (SchemaKey, Option<Arc<Loaded>>) {
        match self.served(root) {
            Some(loaded) => (SchemaKey::Loaded(loaded.hash.clone()), Some(loaded)),
            None => (SchemaKey::Folder(fingerprint(root)), None),
        }
    }

    /// The schemas the linter uses for `root`.
    pub(crate) fn schemas(&self, root: &Path) -> Schemas {
        match self.served(root) {
            Some(loaded) => loaded.schemas(),
            None => read_schemas(root),
        }
    }

    fn with_linter<T>(&self, root: &Path, messages: Option<Messages>, run: impl FnOnce(&Linter) -> T) -> T {
        let (key, served) = self.schema_key(root);
        let build = || served.as_ref().map_or_else(|| read_schemas(root), |loaded| loaded.schemas());
        let mut linters = self
            .linters
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prepared = linters
            .entry(root.to_path_buf())
            .or_insert_with(|| Prepared {
                key: key.clone(),
                linter: Linter::new(build()),
            });
        if prepared.key != key {
            prepared.key = key;
            prepared.linter.schemas = build();
        }
        if let Some(messages) = messages {
            prepared.linter.messages = messages;
        }
        run(&prepared.linter)
    }
}

/// `path` is relative to `root`, e.g. `content/blog/a.md`. The text is the open
/// document's, saved or not.
#[tauri::command]
pub(crate) async fn cowmark_lint(
    state: tauri::State<'_, ContentLintState>,
    root: String,
    path: String,
    text: String,
    messages: Option<Messages>,
) -> Result<Vec<Diagnostic>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.lint(Path::new(&root), &path, &text, messages)
    })
    .await
    .map_err(|error| error.to_string())
}

/// A formatted document, as the editor applies it on save.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormatResult {
    text: String,
    changed: bool,
    diagnostics: Vec<Diagnostic>,
}

/// Formats a managed page before it is saved: tables, prices, codes and links in canonical
/// form. Never removes text; what can't be read is left as written and stays in `diagnostics`.
#[tauri::command]
pub(crate) async fn cowmark_format(
    state: tauri::State<'_, ContentLintState>,
    root: String,
    path: String,
    text: String,
    messages: Option<Messages>,
) -> Result<FormatResult, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let formatted = state.format(Path::new(&root), &path, &text, messages);
        FormatResult {
            text: formatted.text,
            changed: formatted.changed,
            diagnostics: formatted.diagnostics,
        }
    })
    .await
    .map_err(|error| error.to_string())
}

/// Loads the content schema for the managed folder `root` and returns it for the editor's
/// component list and page forms: `{ components, collections, origin, warnings, hash, url,
/// stale, description }` (see `Loaded::to_json`). `site` is the app's preview site, used when
/// the folder has no `preview.json`; `force` skips the ten-minute reuse of the last fetch.
#[tauri::command]
pub(crate) async fn cowmark_schema(
    state: tauri::State<'_, ContentLintState>,
    root: String,
    site: Option<String>,
    force: Option<bool>,
) -> Result<Value, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.refresh(Path::new(&root), site.as_deref(), force.unwrap_or(false)).to_json()
    })
    .await
    .map_err(|error| error.to_string())
}

/// Pasted or dropped CSV as a Markdown table, or None when `text` doesn't look like a table
/// (and `csv_type`, the clipboard's `text/csv`, isn't set). With `component`, the table gets
/// that table component's columns and canonical values; `wrap` puts it inside the component.
#[tauri::command]
pub(crate) async fn cowmark_csv_table(
    state: tauri::State<'_, ContentLintState>,
    root: String,
    text: String,
    csv_type: bool,
    component: Option<String>,
    wrap: bool,
) -> Result<Option<ComponentTable>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&root);
        csv_to_table(&state.schemas(root), text.as_str(), csv_type, component.as_deref(), wrap)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn csv_to_table(schemas: &Schemas, text: &str, csv_type: bool, component: Option<&str>, wrap: bool) -> Result<Option<ComponentTable>, String> {
    let Some(delimiter) = csv_table::detect(text, csv_type) else {
        return Ok(None);
    };
    match component {
        Some(component) => csv_table::for_component(text, Some(delimiter), component, schemas, wrap).map(Some),
        None => Ok(Some(ComponentTable {
            markdown: csv_table::to_markdown(text, Some(delimiter))?,
            problems: Vec::new(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn rules(found: &[Diagnostic]) -> Vec<&'static str> {
        found
            .iter()
            .map(|diagnostic| diagnostic.rule.id())
            .collect()
    }

    fn write_components(root: &Path, json: &str, modified: SystemTime) {
        let path = root.join("schema/components.json");
        fs::write(&path, json).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn rebuilds_when_the_components_schema_changes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("schema")).unwrap();
        let start = SystemTime::now() - Duration::from_secs(60);
        write_components(dir.path(), r#"[{"name":"card-a","props":[]}]"#, start);
        let state = ContentLintState::default();
        let text = "::banner\nHi\n::\n";

        assert_eq!(
            rules(&state.lint(dir.path(), "content/a.md", text, None)),
            ["unknown-component"]
        );

        // Same size, later mtime: still noticed.
        write_components(
            dir.path(),
            r#"[{"name":"banner","props":[]}]"#,
            start + Duration::from_secs(5),
        );
        assert!(state
            .lint(dir.path(), "content/a.md", text, None)
            .is_empty());
    }

    /// Serves `body()` as the schema endpoint until the test ends; returns the site.
    fn serve(body: Arc<Mutex<String>>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let site = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                let body = body.lock().unwrap().clone();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        site
    }

    fn envelope(component: &str) -> String {
        format!(r#"{{"format":1,"components":{{"components":[{{"name":"{component}","props":[]}}]}},"collections":{{"collections":[]}}}}"#)
    }

    #[test]
    fn follows_the_schema_the_site_serves() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join("schema")).unwrap();
        fs::write(dir.path().join("schema/components.json"), r#"[{"name":"card-a","props":[]}]"#).unwrap();
        let body = Arc::new(Mutex::new(envelope("banner")));
        let site = serve(body.clone());
        let state = ContentLintState::default();
        let text = "::banner\nHi\n::\n";

        // Before the first load: the folder's files.
        assert_eq!(rules(&state.lint(dir.path(), "content/a.md", text, None)), ["unknown-component"]);

        let loaded = state.refresh(dir.path(), Some(&site), false);
        assert_eq!(loaded.origin, Origin::Site { url: format!("{site}/_cowmark/schema.json") });
        assert_eq!(loaded.to_json()["components"].as_str().unwrap(), r#"{"components":[{"name":"banner","props":[]}]}"#);
        assert!(state.lint(dir.path(), "content/a.md", text, None).is_empty());
        assert_eq!(state.schemas(dir.path()).components[0].name, "banner");

        // Within ten minutes the last fetch is reused; forcing fetches, and the linter is rebuilt.
        *body.lock().unwrap() = envelope("other");
        let same = state.refresh(dir.path(), Some(&site), false);
        assert_eq!(same.hash, loaded.hash);
        let changed = state.refresh(dir.path(), Some(&site), true);
        assert_ne!(changed.hash, loaded.hash);
        assert_eq!(rules(&state.lint(dir.path(), "content/a.md", text, None)), ["unknown-component"]);

        // A preview.json in the folder names the site instead.
        fs::write(dir.path().join("preview.json"), r#"{"site":"http://127.0.0.1:9"}"#).unwrap();
        let cached = state.refresh(dir.path(), Some(&site), true);
        assert!(matches!(cached.origin, Origin::Repo), "{:?}", cached.origin);
        assert!(!cached.warnings.is_empty());
        // The folder's files are linted again.
        assert_eq!(state.schemas(dir.path()).components[0].name, "card-a");
    }

    #[test]
    fn keeps_the_linter_while_the_schema_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("schema")).unwrap();
        let start = SystemTime::now() - Duration::from_secs(60);
        write_components(dir.path(), r#"[{"name":"card-a","props":[]}]"#, start);
        let state = ContentLintState::default();
        state.lint(dir.path(), "content/a.md", "", None);

        // Rewritten with the old mtime and size: the cached linter is kept.
        write_components(dir.path(), r#"[{"name":"banner","props":[]}]"#, start);
        assert_eq!(
            rules(&state.lint(dir.path(), "content/a.md", "::banner\n::\n", None)),
            ["unknown-component"]
        );
    }

    #[test]
    fn a_broken_components_schema_still_checks_structure_and_front_matter() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("schema")).unwrap();
        fs::write(dir.path().join("schema/components.json"), "{ nope").unwrap();
        let state = ContentLintState::default();

        let found = state.lint(dir.path(), "content/a.md", "::banner\nHi\n", None);
        assert_eq!(rules(&found), ["unclosed-component"]);
    }

    #[test]
    fn uses_the_messages_it_is_given_and_checks_files_under_the_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("content")).unwrap();
        fs::create_dir_all(dir.path().join("public/blog")).unwrap();
        fs::write(dir.path().join("public/blog/there.jpg"), "").unwrap();
        let state = ContentLintState::default();
        let text = "![a](/blog/there.jpg)\n\n![b](/blog/gone.jpg)\n";

        let found = state.lint(dir.path(), "content/a.md", text, None);
        assert_eq!(rules(&found), ["missing-image"]);
        assert!(
            found[0].message.contains("`/blog/gone.jpg`"),
            "{}",
            found[0].message
        );

        let messages: Messages =
            serde_json::from_str(r#"{"missingImage":"Kein Bild: `{src}`"}"#).unwrap();
        let found = state.lint(dir.path(), "content/a.md", text, Some(messages));
        assert_eq!(found[0].message, "Kein Bild: `/blog/gone.jpg`");
        // Templates not given keep their defaults.
        assert!(state.lint(dir.path(), "content/a.md", "::x\n", None)[0]
            .message
            .contains("isn't closed"));
    }

    #[test]
    fn formats_with_the_folder_schema_and_turns_csv_into_tables() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("schema")).unwrap();
        fs::write(
            dir.path().join("schema/components.json"),
            r#"[{"name":"facts-grid","props":[],"input":{"kind":"table","columns":[
              {"name":"Fact","aliases":["Label"],"required":true},{"name":"Value","required":true}]}}]"#,
        )
        .unwrap();
        let state = ContentLintState::default();
        let formatted = state.format(dir.path(), "content/a.md", "::facts-grid
label|value
Café ✓|1
::
", None);
        assert!(formatted.changed);
        assert_eq!(formatted.text, "::facts-grid\n| Fact | Value |\n| --- | --- |\n| Café ✓ | 1 |\n::\n");
        assert!(formatted.diagnostics.is_empty());

        assert!(csv_to_table(&state.schemas(dir.path()), "Just a sentence, really.", false, None, false).unwrap().is_none());
        let table = csv_to_table(&state.schemas(dir.path()), "Label\tValue\nFounded\t2014\n", false, Some("facts-grid"), false)
            .unwrap()
            .unwrap();
        assert_eq!(table.markdown, "| Fact | Value |\n| --- | --- |\n| Founded | 2014 |");
        let wrapped = csv_to_table(&state.schemas(dir.path()), "a,b\n1,2\n", true, Some("facts-grid"), true).unwrap().unwrap();
        assert!(wrapped.markdown.starts_with("::facts-grid\n") && !wrapped.problems.is_empty());
        assert!(csv_to_table(&state.schemas(dir.path()), "a,b\n1,2\n", true, Some("nope"), true).is_err());
    }

    /// The app sends `editor.mdcLint.*` and `editor.contentLint.*` as the templates; in English
    /// they must be the linter's own, so the editor and `cowmark check` read the same.
    #[test]
    fn english_app_messages_match_the_linter() {
        let en = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../packages/shared/src/i18n/locales/en.ts"),
        )
        .unwrap();
        let mut templates = serde_json::Map::new();
        for line in en.lines().map(str::trim) {
            let Some(rest) = line
                .strip_prefix("\"editor.mdcLint.")
                .or_else(|| line.strip_prefix("\"editor.contentLint."))
            else {
                continue;
            };
            let (key, value) = rest.split_once("\": ").unwrap();
            let value: String = serde_json::from_str(value.trim_end_matches(',')).unwrap();
            templates.insert(key.to_string(), value.into());
        }
        assert_eq!(templates.len(), 34 + 29);
        let defaults = format!("{:?}", Messages::default());
        for key in templates.keys() {
            // A key the linter doesn't know would be ignored silently.
            let one: Messages =
                serde_json::from_value(serde_json::json!({ key: "changed" })).unwrap();
            assert_ne!(
                format!("{one:?}"),
                defaults,
                "the linter has no `{key}` message"
            );
        }
        let app: Messages = serde_json::from_value(templates.into()).unwrap();
        assert_eq!(format!("{app:?}"), format!("{:?}", Messages::default()));
    }
}
