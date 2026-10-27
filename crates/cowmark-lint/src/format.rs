//! Writes component input in its canonical form: applies the fixes diagnostics carry, until
//! none are left. Only values the linter understood are rewritten, and a table is rewritten
//! only when every cell has a place in it, so formatting never drops what someone wrote.

use crate::messages::Messages;
use crate::schema::Schemas;
use crate::{Diagnostic, FileLookup, Fix};

/// A document after [`format()`].
#[derive(Debug, Clone, PartialEq)]
pub struct Formatted {
    pub text: String,
    /// Whether `text` differs from the input.
    pub changed: bool,
    /// The problems left in `text`.
    pub diagnostics: Vec<Diagnostic>,
}

/// The document with every fix applied, and the problems left (checked as if every
/// referenced file exists; use [`crate::Linter::format`] to check images and links too).
/// Formatting is idempotent: formatting the result changes nothing.
pub fn format(path: &str, text: &str, schemas: &Schemas) -> (String, Vec<Diagnostic>) {
    let formatted = format_with(schemas, &Messages::default(), path, text, &|_: &str| true);
    (formatted.text, formatted.diagnostics)
}

pub(crate) fn format_with(
    schemas: &Schemas,
    messages: &Messages,
    path: &str,
    text: &str,
    files: &dyn FileLookup,
) -> Formatted {
    let everything = |_: &str| true;
    let mut current = text.to_string();
    // A fix can uncover another (a fixed price widens its column); a few passes settle it.
    for _ in 0..4 {
        let found = crate::run(schemas, messages, path, &current, &everything);
        let fixes: Vec<&Fix> = found
            .iter()
            .filter_map(|diagnostic| diagnostic.fix.as_ref())
            .collect();
        if fixes.is_empty() {
            break;
        }
        let next = apply(&current, &fixes);
        if next == current {
            break;
        }
        current = next;
    }
    let diagnostics = crate::run(schemas, messages, path, &current, files);
    Formatted {
        changed: current != text,
        text: current,
        diagnostics,
    }
}

/// Applies `fixes`; one inside another is left out (the outer one already rewrites it).
pub(crate) fn apply(text: &str, fixes: &[&Fix]) -> String {
    let mut sorted: Vec<&Fix> = fixes.to_vec();
    sorted.sort_by(|a, b| a.from.cmp(&b.from).then(b.to.cmp(&a.to)));
    let mut kept: Vec<&Fix> = Vec::new();
    for fix in sorted {
        if kept.last().is_some_and(|last| {
            fix.from < last.to || (fix.from == last.from && last.from != last.to)
        }) {
            continue;
        }
        if fix.from > fix.to
            || fix.to > text.len()
            || !text.is_char_boundary(fix.from)
            || !text.is_char_boundary(fix.to)
        {
            continue;
        }
        kept.push(fix);
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for fix in kept {
        out.push_str(&text[at..fix.from]);
        out.push_str(&fix.insert);
        at = fix.to;
    }
    out.push_str(&text[at..]);
    out
}
