//! The content lint gate on Save to studio: ticked pages with lint errors (raw HTML, a missing
//! required field, an unclosed component…) are refused before anything is committed, so no
//! caller (the app, the `cowmark` command, an agent) can publish them. Warnings don't block.

use std::path::Path;

use cowmark_lint::fs::FsLookup;
use cowmark_lint::{Linter, Schemas, Severity};
use serde::Serialize;

use crate::schema_source::{self, Request, default_url};

/// Follows a `lint:` message: a JSON array of [`Problem`]s, one per file.
pub const LINT_PROBLEMS_PREFIX: &str = "\nlint-problems: ";

/// The folder's schemas without going online: the copy cached from the site in `preview.json`
/// (else the newest cached copy), else the folder's `schema/` files. A schema text that can't
/// be read is left out, as if absent, so a broken `components.json` still leaves front matter,
/// images and links checked. The app refreshes the cache (see [`crate::schema_source`]).
pub fn read_schemas(root: &Path) -> Schemas {
    let request = Request { url: default_url(root, None, None), offline: true, dir: None };
    schema_source::load(root, &request).schemas()
}

/// A file's first lint error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// Folder-relative, as ticked.
    pub file: String,
    pub line: usize,
    pub message: String,
}

/// Content pages are the Markdown files under `content/`; nothing else is linted.
fn is_page(path: &str) -> bool {
    path.starts_with("content/") && path.to_ascii_lowercase().ends_with(".md")
}

/// The first error in each ticked page that has one. Deleted files are skipped.
pub fn first_errors(root: &Path, paths: &[String]) -> Vec<Problem> {
    let pages: Vec<&String> = paths.iter().filter(|path| is_page(path)).collect();
    if pages.is_empty() {
        return Vec::new();
    }
    let linter = Linter::new(read_schemas(root));
    let files = FsLookup::new(root);
    pages
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(root.join(path)).ok()?;
            let first = linter
                .lint(path, &text, &files)
                .into_iter()
                .find(|diagnostic| diagnostic.severity == Severity::Error)?;
            Some(Problem { file: path.clone(), line: first.line, message: first.message })
        })
        .collect()
}

/// `lint: <plain message, one line per file>` followed by the problems as JSON.
pub fn lint_error(problems: &[Problem]) -> String {
    let lines: Vec<String> = problems
        .iter()
        .map(|problem| format!("{}, line {}: {}", problem.file, problem.line, problem.message))
        .collect();
    let list = serde_json::to_string(problems).unwrap_or_else(|_| "[]".into());
    format!(
        "lint: Fix these problems, then save again.\n{}{LINT_PROBLEMS_PREFIX}{list}",
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lints_only_pages_and_reports_the_first_error_per_file() {
        let dir = std::env::temp_dir().join(format!("cowmark-core-lint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("content/blog")).unwrap();
        std::fs::write(dir.join("content/blog/a.md"), "Hi\n\n<div>x</div>\n\n::callout\n").unwrap();
        std::fs::write(dir.join("content/blog/ok.md"), "![](/missing-alt-is-a-warning.png)\n").unwrap();
        std::fs::write(dir.join("README.md"), "<div>not a page</div>\n").unwrap();
        let ticked: Vec<String> = ["content/blog/a.md", "content/blog/ok.md", "README.md", "content/blog/gone.md"]
            .map(String::from)
            .to_vec();

        let problems = first_errors(&dir, &ticked);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].file, "content/blog/a.md");
        assert_eq!(problems[0].line, 3);
        assert!(problems[0].message.starts_with("HTML isn't allowed"), "{problems:?}");

        let error = lint_error(&problems);
        assert!(error.starts_with("lint: Fix these problems, then save again.\ncontent/blog/a.md, line 3: HTML"), "{error}");
        let (_, json) = error.split_once(LINT_PROBLEMS_PREFIX).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(parsed[0]["file"], "content/blog/a.md");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
