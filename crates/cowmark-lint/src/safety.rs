//! What the site refuses to render: raw HTML, links with scripting schemes and attributes that
//! run code. Raw HTML is found by a CommonMark parser over the prose only (components, props
//! blocks, front matter and fenced code are blanked out first, offsets kept), so autolinks,
//! code, escaped `\<` and a `<` that isn't a tag are left alone exactly as the site's parser
//! leaves them.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::mdc::{AttributeKind, parse_attributes, scan_attributes};
use crate::text::Range;
use crate::value::Value;
use crate::yaml::{Node, NodeKind};
use crate::{Report, Rule, Severity};

pub fn check_prose(text: &str, prose: &[Range], report: &mut Report) {
    let mut masked: Vec<u8> = text
        .bytes()
        .map(|b| if b == b'\n' { b'\n' } else { b' ' })
        .collect();
    for range in prose {
        masked[range.from..range.to].copy_from_slice(&text.as_bytes()[range.from..range.to]);
    }
    // Every byte outside the prose became ASCII and the prose ranges start and end on
    // character boundaries, so this is still UTF-8.
    let masked = String::from_utf8(masked).expect("masked text is UTF-8");

    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut code: Vec<std::ops::Range<usize>> = Vec::new();
    let mut html: Vec<std::ops::Range<usize>> = Vec::new();
    let mut in_html_block = false;
    for (event, range) in Parser::new_ext(&masked, options).into_offset_iter() {
        match event {
            Event::Start(Tag::HtmlBlock) => {
                in_html_block = true;
                html.push(range);
            }
            Event::End(TagEnd::HtmlBlock) => in_html_block = false,
            Event::Html(_) | Event::InlineHtml(_) if !in_html_block => html.push(range),
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                if !is_safe_url(&dest_url) {
                    unsafe_link(Range::new(range.start, range.end), report);
                }
            }
            Event::Start(Tag::CodeBlock(_)) | Event::Code(_) => code.push(range),
            _ => {}
        }
    }
    // MDC reads `{…}` after an inline element before Markdown sees it, so a `<` in an
    // attribute value is not HTML.
    let attributes = inline_attributes(&masked, prose, &code, report);
    for range in html {
        if !attributes.iter().any(|group| group.contains(&range.start)) {
            raw_html(&masked, range, report);
        }
    }
}

fn raw_html(text: &str, range: std::ops::Range<usize>, report: &mut Report) {
    let chunk = &text[range.clone()];
    let start = range.start + (chunk.len() - chunk.trim_start().len());
    let end = range.start + chunk.trim_end().len();
    let messages = report.messages;
    report.add(
        Range::new(start, end.max(start)),
        Severity::Error,
        Rule::RawHtml,
        &messages.raw_html,
        &[],
    );
}

fn unsafe_link(range: Range, report: &mut Report) {
    let messages = report.messages;
    report.add(
        range,
        Severity::Error,
        Rule::UnsafeLink,
        &messages.unsafe_link,
        &[],
    );
}

/// `{…}` written directly after an inline element (`[text]{…}`, `**b**{…}`, `` `c`{…} ``,
/// `[a](b){…}`, `:name{…}`, `:name[label]{…}`), where MDC reads it as attributes.
fn inline_attributes(
    text: &str,
    prose: &[Range],
    code: &[std::ops::Range<usize>],
    report: &mut Report,
) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let b = text.as_bytes();
    for range in prose {
        let mut i = range.from;
        while i < range.to {
            if b[i] != b'{' || code.iter().any(|s| s.contains(&i)) || !follows_inline(text, i) {
                i += 1;
                continue;
            }
            match scan_attributes(text, i).filter(|&end| end <= range.to) {
                Some(end) => {
                    check_attributes(&text[i + 1..end - 1], i + 1, report);
                    groups.push(i..end);
                    i = end;
                }
                None => i += 1,
            }
        }
    }
    groups
}

fn follows_inline(text: &str, at: usize) -> bool {
    let b = text.as_bytes();
    let Some(&prev) = at.checked_sub(1).and_then(|p| b.get(p)) else {
        return false;
    };
    if matches!(prev, b']' | b')' | b'`' | b'*' | b'_' | b'~') {
        return true;
    }
    // `:name{`
    let name_start = text[..at]
        .bytes()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_')
        .count();
    name_start > 0
        && at > name_start
        && b[at - name_start - 1] == b':'
        && b[at - name_start].is_ascii_alphabetic()
}

/// Attributes between braces, `base` being the offset of the first byte after `{`.
pub fn check_attributes(source: &str, base: usize, report: &mut Report) {
    for attribute in parse_attributes(source) {
        let range = Range::new(base + attribute.from, base + attribute.to);
        match attribute.kind {
            AttributeKind::Id | AttributeKind::Class => {}
            _ if is_dangerous_attribute(&attribute.key) => {
                unsafe_attribute(&attribute.key, range, report)
            }
            AttributeKind::Text if !is_safe_value(&attribute.key, &attribute.value) => {
                unsafe_link(range, report)
            }
            AttributeKind::Bound => {
                let value = serde_json::from_str::<serde_json::Value>(&attribute.value)
                    .map(Value::from_json)
                    .ok()
                    .or_else(|| {
                        crate::yaml::parse(&attribute.value)
                            .ok()
                            .and_then(|parsed| parsed.root.map(|root| root.to_value()))
                    })
                    .unwrap_or_else(|| Value::Str(attribute.value.clone()));
                check_value(&attribute.key, &value, range, report);
            }
            _ => {}
        }
    }
}

/// A prop value (from a bound attribute) under `key`, nested objects included.
fn check_value(key: &str, value: &Value, range: Range, report: &mut Report) {
    match value {
        Value::Str(text) if !is_safe_value(key, text) => unsafe_link(range, report),
        Value::Arr(items) => items
            .iter()
            .for_each(|item| check_value(key, item, range, report)),
        Value::Obj(entries) => {
            for (name, item) in entries {
                if is_dangerous_attribute(name) {
                    unsafe_attribute(name, range, report);
                } else {
                    check_value(name, item, range, report);
                }
            }
        }
        _ => {}
    }
}

/// A component's YAML props block; `base` is the offset of the parsed text.
pub fn check_yaml(node: &Node, key: Option<&str>, base: usize, report: &mut Report) {
    match &node.kind {
        NodeKind::Scalar {
            value: Value::Str(text),
        } => {
            if let Some(key) = key
                && !is_safe_value(key, text)
            {
                unsafe_link(Range::new(base + node.from, base + node.to), report);
            }
        }
        NodeKind::Scalar { .. } => {}
        NodeKind::Seq(items) => items
            .iter()
            .for_each(|item| check_yaml(item, key, base, report)),
        NodeKind::Map(pairs) => {
            for (name_node, value) in pairs {
                let name = name_node.key_text();
                match name.as_deref() {
                    Some(name) if is_dangerous_attribute(name) => unsafe_attribute(
                        name,
                        Range::new(base + name_node.from, base + name_node.to),
                        report,
                    ),
                    name => check_yaml(value, name, base, report),
                }
            }
        }
    }
}

fn unsafe_attribute(name: &str, range: Range, report: &mut Report) {
    let messages = report.messages;
    report.add(
        range,
        Severity::Error,
        Rule::UnsafeAttribute,
        &messages.unsafe_attribute,
        &[("attribute", name)],
    );
}

/// Event handlers (`onclick`, Vue's `onClick`, `@click`, `v-on:click`), Vue directives that
/// bind arbitrary props or listeners, and attributes that carry a document or a form target.
pub fn is_dangerous_attribute(key: &str) -> bool {
    let key = key.trim().trim_start_matches(':').to_ascii_lowercase();
    (key.starts_with("on") && key.len() > 2)
        || key.starts_with('@')
        || key.starts_with("v-on")
        || key.starts_with("v-bind")
        || key.starts_with("v-model")
        || key == "v-html"
        || key == "srcdoc"
        || key == "formaction"
}

/// Props and attributes the site treats as URLs: `href`, `src`, `to`, `url`, `link`, `action`,
/// `poster`, `srcset`, and names ending in one of them (`ctaLink`, `image-src`).
fn is_url_key(key: &str) -> bool {
    let key: String = key
        .trim()
        .trim_start_matches(':')
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    matches!(key.as_str(), "to" | "action" | "poster")
        || ["href", "src", "srcset", "url", "link"]
            .iter()
            .any(|suffix| key.ends_with(suffix))
}

fn is_safe_value(key: &str, value: &str) -> bool {
    if !is_url_key(key) {
        return true;
    }
    if key.to_ascii_lowercase().ends_with("srcset") {
        return value
            .split(',')
            .all(|candidate| is_safe_url(candidate.split_whitespace().next().unwrap_or("")));
    }
    is_safe_url(value)
}

/// Relative, `http(s)`, `mailto` or `tel`, read the way a browser reads an `href`: leading and
/// trailing spaces and control characters dropped, tabs and line breaks removed anywhere, and
/// the scheme compared without case.
pub fn is_safe_url(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c <= ' ');
    let Some(end) = cleaned.find([':', '/', '?', '#', '\\']) else {
        return true;
    };
    if cleaned.as_bytes()[end] != b':' {
        return true;
    }
    let scheme = &cleaned[..end];
    let is_scheme = scheme
        .bytes()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'));
    if !is_scheme {
        return true;
    }
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto" | "tel"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_schemes_like_a_browser() {
        for safe in [
            "/blog/a",
            "other-page",
            "#top",
            "?q=1",
            "https://x.com",
            "HTTP://x.com",
            "mailto:a@b.c",
            "tel:+1555",
            "//cdn.example.com/a.png",
            "",
            "java script:alert(1)",
            "java&#x73;cript:alert(1)",
        ] {
            assert!(is_safe_url(safe), "{safe}");
        }
        for unsafe_ in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            " \u{1}javascript:alert(1)",
            "vbscript:x",
            "data:text/html,<script>",
            "file:///etc/passwd",
        ] {
            assert!(!is_safe_url(unsafe_), "{unsafe_:?}");
        }
    }

    #[test]
    fn names_dangerous_attributes() {
        for name in [
            "onclick",
            "onClick",
            "on-click",
            "@click",
            "v-on:click",
            "v-bind",
            "v-model",
            "srcdoc",
            "formaction",
            "FormAction",
        ] {
            assert!(is_dangerous_attribute(name), "{name}");
        }
        for name in ["title", "to", "href", "variant", "o"] {
            assert!(!is_dangerous_attribute(name), "{name}");
        }
    }

    #[test]
    fn url_keys() {
        for key in [
            "href",
            "src",
            "to",
            "url",
            "link",
            "ctaLink",
            "image-src",
            "srcset",
            "poster",
        ] {
            assert!(is_url_key(key), "{key}");
        }
        for key in ["title", "tone", "description", "cta"] {
            assert!(!is_url_key(key), "{key}");
        }
        assert!(!is_safe_value("srcset", "/a.png 1x, javascript:x 2x"));
        assert!(is_safe_value("title", "javascript:x"));
    }
}
