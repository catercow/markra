//! Images and links in Markdown prose: `![alt](src)`, `[text](href)`, reference links and
//! `<https://…>` autolinks. Code spans and HTML comments are skipped.

use std::collections::HashMap;

use crate::refs::Context;
use crate::text::{Range, lines};
use crate::{Report, Rule, Severity};

pub fn check_prose(text: &str, prose: &[Range], context: &Context, report: &mut Report) {
    let definitions = definitions(text, prose);
    for (dest, range) in definitions.values() {
        context.check_link(dest, *range, report);
    }
    let scanner = Scanner {
        text,
        definitions: &definitions,
        context,
    };
    for range in prose {
        scanner.scan(range.from, range.to, report);
    }
}

/// `[label]: destination` lines, by normalized label.
fn definitions(text: &str, prose: &[Range]) -> HashMap<String, (String, Range)> {
    let mut found = HashMap::new();
    for range in prose {
        for line in lines(&text[range.from..range.to]) {
            let indent = line.text.len() - line.text.trim_start_matches(' ').len();
            let rest = &line.text[indent..];
            if indent > 3 || !rest.starts_with('[') {
                continue;
            }
            let Some(close) = rest.find("]:") else {
                continue;
            };
            let label = &rest[1..close];
            if label.trim().is_empty() || label.contains(['[', ']']) {
                continue;
            }
            let after = &rest[close + 2..];
            let dest_start = after.len() - after.trim_start().len();
            let dest_text = after[dest_start..].split_whitespace().next().unwrap_or("");
            if dest_text.is_empty() {
                continue;
            }
            let from = range.from + line.from + indent + close + 2 + dest_start;
            let dest = dest_text.trim_start_matches('<').trim_end_matches('>');
            found
                .entry(normalize(label))
                .or_insert((dest.to_string(), Range::new(from, from + dest_text.len())));
        }
    }
    found
}

fn normalize(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

struct Scanner<'a> {
    text: &'a str,
    definitions: &'a HashMap<String, (String, Range)>,
    context: &'a Context<'a>,
}

struct Link {
    /// Inside the brackets.
    label: Range,
    /// The destination and where it is written (the whole link for a reference link).
    dest: Option<(String, Range)>,
    end: usize,
}

impl Scanner<'_> {
    fn scan(&self, from: usize, to: usize, report: &mut Report) {
        let b = self.text.as_bytes();
        let mut i = from;
        while i < to {
            match b[i] {
                b'\\' => i += 2,
                b'`' => {
                    let run = b[i..to].iter().take_while(|&&c| c == b'`').count();
                    i = self.code_span_end(i, run, to).unwrap_or(i + run);
                }
                b'<' if self.text[i..to].starts_with("<!--") => {
                    i = self.text[i..to].find("-->").map_or(to, |end| i + end + 3);
                }
                b'<' => {
                    let rest = &self.text[i + 1..to];
                    let end = rest.find(|c: char| c == '>' || c == '<' || c.is_whitespace());
                    match end {
                        Some(end) if rest.as_bytes()[end] == b'>' && rest[..end].contains(':') => {
                            self.context.check_link(
                                &rest[..end],
                                Range::new(i + 1, i + 1 + end),
                                report,
                            );
                            i += end + 2;
                        }
                        _ => i += 1,
                    }
                }
                b'!' if b.get(i + 1) == Some(&b'[') => match self.link(i + 1, to) {
                    Some(link) => {
                        let alt = &self.text[link.label.from..link.label.to];
                        if alt.trim().is_empty() {
                            let messages = report.messages;
                            report.add(
                                Range::new(i, link.end),
                                Severity::Warning,
                                Rule::ImageAlt,
                                &messages.image_alt,
                                &[],
                            );
                        }
                        if let Some((src, range)) = &link.dest {
                            self.context.check_image(src, *range, report);
                        }
                        i = link.end;
                    }
                    None => i += 1,
                },
                b'[' => match self.link(i, to) {
                    Some(link) => {
                        if let Some((href, range)) = &link.dest {
                            self.context.check_link(href, *range, report);
                        }
                        self.scan(link.label.from, link.label.to, report);
                        i = link.end;
                    }
                    None => i += 1,
                },
                _ => i += 1,
            }
        }
    }

    fn code_span_end(&self, start: usize, run: usize, to: usize) -> Option<usize> {
        let b = self.text.as_bytes();
        let mut i = start + run;
        while i < to {
            if b[i] == b'`' {
                let close = b[i..to].iter().take_while(|&&c| c == b'`').count();
                if close == run {
                    return Some(i + close);
                }
                i += close;
            } else {
                i += 1;
            }
        }
        None
    }

    /// A link starting at the `[` at `open`: inline, full reference or shortcut reference.
    fn link(&self, open: usize, to: usize) -> Option<Link> {
        let b = self.text.as_bytes();
        let mut depth = 0;
        let mut i = open;
        let close = loop {
            if i >= to {
                return None;
            }
            match b[i] {
                b'\\' => i += 1,
                b'`' => {
                    let run = b[i..to].iter().take_while(|&&c| c == b'`').count();
                    i = self
                        .code_span_end(i, run, to)
                        .map_or(i + run - 1, |end| end - 1);
                }
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
        };
        let label = Range::new(open + 1, close);
        let after = close + 1;

        if b.get(after) == Some(&b'(')
            && let Some((dest, end)) = self.inline_destination(after + 1, to)
        {
            return Some(Link { label, dest, end });
        }
        let (key, end) = if b.get(after) == Some(&b'[') {
            let ref_close = self.text[after + 1..to].find(']').map(|i| after + 1 + i)?;
            let key = &self.text[after + 1..ref_close];
            (
                if key.trim().is_empty() {
                    &self.text[label.from..label.to]
                } else {
                    key
                },
                ref_close + 1,
            )
        } else {
            (&self.text[label.from..label.to], after)
        };
        let (dest, _) = self.definitions.get(&normalize(key))?;
        Some(Link {
            label,
            dest: Some((dest.clone(), Range::new(open, end))),
            end,
        })
    }

    /// `(dest "title")` after the label; `start` is just past the `(`.
    fn inline_destination(
        &self,
        start: usize,
        to: usize,
    ) -> Option<(Option<(String, Range)>, usize)> {
        let b = self.text.as_bytes();
        let mut i = start;
        while i < to && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let dest = if b.get(i) == Some(&b'<') {
            let end = self.text[i + 1..to].find(['>', '\n'])? + i + 1;
            if b[end] != b'>' {
                return None;
            }
            let range = Range::new(i + 1, end);
            i = end + 1;
            range
        } else {
            let dest_start = i;
            let mut parens = 0;
            while i < to && !b[i].is_ascii_whitespace() {
                match b[i] {
                    b'\\' => i += 1,
                    b'(' => parens += 1,
                    b')' if parens == 0 => break,
                    b')' => parens -= 1,
                    _ => {}
                }
                i += 1;
            }
            Range::new(dest_start, i.min(to))
        };
        while i < to && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < to && matches!(b[i], b'"' | b'\'' | b'(') {
            let closing = if b[i] == b'(' { b')' } else { b[i] };
            i += 1;
            while i < to && b[i] != closing {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            while i < to && b[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        if b.get(i) != Some(&b')') || i >= to {
            return None;
        }
        let value = &self.text[dest.from..dest.to];
        let dest = (!value.is_empty()).then(|| (value.to_string(), dest));
        Some((dest, i + 1))
    }
}
