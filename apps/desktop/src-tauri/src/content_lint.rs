//! Tauri command for CowMark's in-editor content lint. The rules are in the
//! `cowmark-lint` crate, shared with `cowmark check` and CI, so all three report
//! the same problems. This keeps one prepared `Linter` per content folder and
//! rebuilds it when the folder's schema files change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use cowmark_lint::fs::FsLookup;
use cowmark_core::content_check::read_schemas;
use cowmark_core::csv_table::{self, ComponentTable};
use cowmark_lint::{Diagnostic, Formatted, Linter, Messages};
use serde::Serialize;

const SCHEMA_FILES: [&str; 2] = ["schema/components.json", "schema/collections.json"];

/// What a schema file looked like when the linter was built: absent, or its mtime and size.
type Fingerprint = [Option<(SystemTime, u64)>; 2];

struct Prepared {
    fingerprint: Fingerprint,
    linter: Linter,
}

#[derive(Default, Clone)]
pub(crate) struct ContentLintState {
    linters: Arc<Mutex<HashMap<PathBuf, Prepared>>>,
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

    fn with_linter<T>(&self, root: &Path, messages: Option<Messages>, run: impl FnOnce(&Linter) -> T) -> T {
        let current = fingerprint(root);
        let mut linters = self
            .linters
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prepared = linters
            .entry(root.to_path_buf())
            .or_insert_with(|| Prepared {
                fingerprint: current,
                linter: Linter::new(read_schemas(root)),
            });
        if prepared.fingerprint != current {
            prepared.fingerprint = current;
            prepared.linter.schemas = read_schemas(root);
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

/// Pasted or dropped CSV as a Markdown table, or None when `text` doesn't look like a table
/// (and `csv_type`, the clipboard's `text/csv`, isn't set). With `component`, the table gets
/// that table component's columns and canonical values; `wrap` puts it inside the component.
#[tauri::command]
pub(crate) async fn cowmark_csv_table(
    root: String,
    text: String,
    csv_type: bool,
    component: Option<String>,
    wrap: bool,
) -> Result<Option<ComponentTable>, String> {
    tauri::async_runtime::spawn_blocking(move || csv_to_table(Path::new(&root), &text, csv_type, component.as_deref(), wrap))
        .await
        .map_err(|error| error.to_string())?
}

fn csv_to_table(root: &Path, text: &str, csv_type: bool, component: Option<&str>, wrap: bool) -> Result<Option<ComponentTable>, String> {
    let Some(delimiter) = csv_table::detect(text, csv_type) else {
        return Ok(None);
    };
    match component {
        Some(component) => csv_table::for_component(text, Some(delimiter), component, &read_schemas(root), wrap).map(Some),
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
        assert_eq!(formatted.text, "::facts-grid\n| Fact   | Value |\n| ------ | ----- |\n| Café ✓ | 1     |\n::\n");
        assert!(formatted.diagnostics.is_empty());

        assert!(csv_to_table(dir.path(), "Just a sentence, really.", false, None, false).unwrap().is_none());
        let table = csv_to_table(dir.path(), "Label\tValue\nFounded\t2014\n", false, Some("facts-grid"), false)
            .unwrap()
            .unwrap();
        assert_eq!(table.markdown, "| Fact    | Value |\n| ------- | ----- |\n| Founded | 2014  |");
        let wrapped = csv_to_table(dir.path(), "a,b\n1,2\n", true, Some("facts-grid"), true).unwrap().unwrap();
        assert!(wrapped.markdown.starts_with("::facts-grid\n") && !wrapped.problems.is_empty());
        assert!(csv_to_table(dir.path(), "a,b\n1,2\n", true, Some("nope"), true).is_err());
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
