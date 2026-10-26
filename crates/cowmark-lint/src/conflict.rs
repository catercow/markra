//! Leftover git conflict blocks: `<<<<<<< label`, optionally `||||||| base`, `=======`,
//! `>>>>>>> label`. A `=======` line outside such a block (a setext heading underline) is
//! ordinary Markdown.

use crate::text::{Range, lines};
use crate::{Report, Rule, Severity};

fn is_marker(line: &str, marker: &str) -> bool {
    line.strip_prefix(marker)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

pub fn check(text: &str, report: &mut Report) {
    let messages = report.messages;
    let mut start: Option<usize> = None;
    let mut last_end = 0;
    for line in lines(text) {
        if is_marker(line.text, "<<<<<<<") {
            if let Some(from) = start {
                report.add(
                    Range::new(from, last_end),
                    Severity::Error,
                    Rule::UnresolvedConflict,
                    &messages.unresolved_conflict,
                    &[],
                );
            }
            start = Some(line.from);
        } else if is_marker(line.text, ">>>>>>>")
            && let Some(from) = start.take()
        {
            report.add(
                Range::new(from, line.to),
                Severity::Error,
                Rule::UnresolvedConflict,
                &messages.unresolved_conflict,
                &[],
            );
        }
        last_end = line.to;
    }
    if let Some(from) = start {
        report.add(
            Range::new(from, text.len()),
            Severity::Error,
            Rule::UnresolvedConflict,
            &messages.unresolved_conflict,
            &[],
        );
    }
}
