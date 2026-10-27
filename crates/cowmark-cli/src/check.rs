//! `cowmark check`: lints content files with cowmark-lint, as the editor does.

use std::path::{Path, PathBuf};

use cowmark_lint::fs::FsLookup;
use cowmark_lint::{Diagnostic, Linter, Schemas, Severity};
use serde_json::{Value, json};

use crate::root;

#[derive(Debug, Default)]
pub struct CheckReport {
    /// Root-relative paths of the files checked.
    pub files: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    /// Excerpt of the flagged text (first line), per diagnostic.
    pub excerpts: Vec<String>,
}

impl CheckReport {
    pub fn errors(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.diagnostics.len() - self.errors()
    }

    /// 1 when there are errors, or warnings with `strict`; else 0.
    pub fn exit_code(&self, strict: bool) -> i32 {
        i32::from(self.errors() > 0 || (strict && self.warnings() > 0))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "files": self.files.len(),
            "errors": self.errors(),
            "warnings": self.warnings(),
            "diagnostics": self.diagnostics,
        })
    }

    /// `file:line:column: severity [rule] message`, one per line, then a summary.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (diagnostic, excerpt) in self.diagnostics.iter().zip(&self.excerpts) {
            let severity = if diagnostic.severity == Severity::Error {
                "error"
            } else {
                "warning"
            };
            out.push_str(&format!(
                "{}:{}:{}: {severity} [{}] {}",
                diagnostic.file, diagnostic.line, diagnostic.column, diagnostic.rule, diagnostic.message
            ));
            if !excerpt.is_empty() {
                out.push_str(&format!("  «{excerpt}»"));
            }
            out.push('\n');
        }
        out.push_str(&self.summary());
        out.push('\n');
        out
    }

    /// GitHub Actions workflow commands that annotate the files: errors, and
    /// warnings too when `warnings`.
    pub fn to_github(&self, warnings: bool) -> String {
        let escape_data = |text: &str| text.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A");
        let escape_property = |text: &str| escape_data(text).replace(':', "%3A").replace(',', "%2C");
        let mut out = String::new();
        for diagnostic in &self.diagnostics {
            let level = match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warning if warnings => "warning",
                Severity::Warning => continue,
            };
            out.push_str(&format!(
                "::{level} file={},line={},col={},endLine={},endColumn={},title={}::{}\n",
                escape_property(&diagnostic.file),
                diagnostic.line,
                diagnostic.column,
                diagnostic.end_line,
                diagnostic.end_column,
                escape_property(&format!("cowmark {}", diagnostic.rule)),
                escape_data(&diagnostic.message)
            ));
        }
        out
    }

    pub fn summary(&self) -> String {
        let plural = |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
        format!(
            "{} checked: {}, {}",
            plural(self.files.len(), "file"),
            plural(self.errors(), "error"),
            plural(self.warnings(), "warning")
        )
    }
}

/// The folder's own `schema/` files, offline; for tests.
#[cfg(test)]
pub(crate) fn check_folder(root: &Path, paths: &[String]) -> Result<CheckReport, String> {
    let schemas = cowmark_core::schema_source::load(root, &Default::default()).parse()?;
    check(root, &schemas, paths)
}

/// Checks the Markdown files at `paths` (files or folders; relative to the
/// root, or absolute inside it), or everything under `content/` when empty.
pub fn check(root: &Path, schemas: &Schemas, paths: &[String]) -> Result<CheckReport, String> {
    let linter = Linter::new(schemas.clone());
    let lookup = FsLookup::cached(root);

    let mut report = CheckReport::default();
    for file in resolve_files(root, paths)? {
        let relative = relative_path(root, &file);
        let text = std::fs::read_to_string(&file).map_err(|error| format!("{relative}: {error}"))?;
        for diagnostic in linter.lint(&relative, &text, &lookup) {
            let excerpt = text
                .get(diagnostic.from..diagnostic.to)
                .and_then(|text| text.lines().next())
                .unwrap_or("");
            report.excerpts.push(excerpt.chars().take(80).collect());
            report.diagnostics.push(diagnostic);
        }
        report.files.push(relative);
    }
    Ok(report)
}

fn relative_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Writes component input in its canonical form (`cowmark fmt`): the fixes `check` offers, such
/// as `$7.25` for `7.25 USD` or a tidied table. Returns the root-relative paths that changed (or
/// would, with `write` false). Only pages under `content/` are rewritten.
pub fn format_files(root: &Path, schemas: &Schemas, paths: &[String], write: bool) -> Result<Vec<String>, String> {
    let linter = Linter::new(schemas.clone());
    let mut changed = Vec::new();
    for file in resolve_files(root, paths)? {
        let relative = relative_path(root, &file);
        if !relative.starts_with("content/") {
            continue;
        }
        let text = std::fs::read_to_string(&file).map_err(|error| format!("{relative}: {error}"))?;
        let formatted = linter.format(&relative, &text, &|_: &str| true);
        if !formatted.changed {
            continue;
        }
        if write {
            crate::content::write_atomically(&file, formatted.text.as_bytes())
                .map_err(|error| format!("{relative}: {error}"))?;
        }
        changed.push(relative);
    }
    Ok(changed)
}

/// The Markdown files named by `paths`, or every one under `content/`.
fn resolve_files(root: &Path, paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if paths.is_empty() {
        markdown_files(&root.join("content"), &mut files);
    }
    for path in paths {
        let (absolute, relative) = root::resolve(root, path)?;
        if absolute.is_dir() {
            markdown_files(&absolute, &mut files);
        } else if !absolute.exists() {
            return Err(format!("{relative} doesn't exist"));
        } else if root::is_markdown(&relative) {
            files.push(absolute);
        } else {
            return Err(format!("{relative} isn't a Markdown file"));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Markdown files under `dir`, skipping hidden entries and node_modules.
pub fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            markdown_files(&entry.path(), out);
        } else if kind.is_file() && root::is_markdown(&name) {
            out.push(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("cm-cli-check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("content/blog")).unwrap();
        std::fs::create_dir_all(root.join("schema")).unwrap();
        std::fs::write(
            root.join("schema/collections.json"),
            r#"{"collections":[{"name":"blog","folder":"content/blog","url":"/blog/<file name without .md>","fields":[{"name":"title","type":"string","required":true}]}]}"#,
        )
        .unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn exit_codes_follow_errors_and_strict_warnings() {
        let root = repo("exit");
        std::fs::write(root.join("content/blog/good.md"), "---\ntitle: Good\n---\n\nHello\n").unwrap();
        let clean = check_folder(&root, &[]).unwrap();
        assert_eq!((clean.files.len(), clean.errors(), clean.warnings()), (1, 0, 0));
        assert_eq!((clean.exit_code(false), clean.exit_code(true)), (0, 0));

        // A warning only: image without alt text.
        std::fs::create_dir_all(root.join("public/blog/warn")).unwrap();
        std::fs::write(root.join("public/blog/warn/a.png"), "png").unwrap();
        std::fs::write(
            root.join("content/blog/warn.md"),
            "---\ntitle: Warn\n---\n\n![](/blog/warn/a.png)\n",
        )
        .unwrap();
        let warned = check_folder(&root, &["content/blog/warn.md".into()]).unwrap();
        assert_eq!((warned.errors(), warned.warnings()), (0, 1));
        assert_eq!((warned.exit_code(false), warned.exit_code(true)), (0, 1));
        assert!(
            warned
                .to_text()
                .contains("content/blog/warn.md:5:1: warning [image-alt]"),
            "{}",
            warned.to_text()
        );

        // An error: the required title is missing.
        std::fs::write(root.join("content/blog/bad.md"), "---\nother: x\n---\n").unwrap();
        let all = check_folder(&root, &["content".into()]).unwrap();
        assert_eq!(
            all.files,
            vec!["content/blog/bad.md", "content/blog/good.md", "content/blog/warn.md"]
        );
        assert!(all.errors() >= 1);
        assert_eq!(all.exit_code(false), 1);
        assert_eq!(all.to_json()["files"], 3);
        let annotations = all.to_github(false);
        assert!(
            annotations.starts_with("::error file=content/blog/bad.md,line=1,col=1,"),
            "{annotations}"
        );
        assert!(!annotations.contains("::warning"));
        assert!(
            all.to_github(true)
                .contains("::warning file=content/blog/warn.md,line=5,col=1,")
        );

        assert!(check_folder(&root, &["content/blog/missing.md".into()]).is_err());
        assert!(check_folder(&root, &["../elsewhere".into()]).is_err());
        assert!(check_folder(&root, &["schema/collections.json".into()]).is_err());

        std::fs::write(root.join("schema/components.json"), "{nope").unwrap();
        assert!(check_folder(&root, &[]).unwrap_err().contains("components.json"));
        let _ = std::fs::remove_dir_all(root);
    }
}
