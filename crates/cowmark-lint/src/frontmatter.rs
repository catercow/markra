//! Front matter: found the way the site finds it (remark-mdc's `parseFrontMatter`: the file
//! starts with `---`, up to the next line starting with `---`), read as YAML 1.2, and checked
//! against the page's collection in `collections.json`, which mirrors the zod schemas in the
//! site's `content.config.ts`.

use crate::refs::{Context, is_image_ref};
use crate::schema::{FieldType, ObjectField};
use crate::text::{LineIndex, Range};
use crate::value::Value;
use crate::yaml::{self, Node, NodeKind};
use crate::{Report, Rule, Severity};

/// Fields the schema types as text but `content.config.ts` restricts to `YYYY-MM-DD`.
const DATE_FIELDS: [&str; 1] = ["published_at"];

/// Fields every Nuxt Content page collection accepts besides its own.
const PAGE_FIELDS: [&str; 2] = ["navigation", "seo"];

/// Key names whose text values the site treats as images (rewrite.ts IMAGE_KEY).
fn is_image_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "image",
        "src",
        "photo",
        "logo",
        "icon",
        "thumbnail",
        "avatar",
    ]
    .iter()
    .any(|part| key.contains(part))
}

/// Checks the front matter and returns where the Markdown body starts.
pub fn check(text: &str, context: &Context, report: &mut Report) -> usize {
    let messages = report.messages;
    let first_line_end = text.find('\n').unwrap_or(text.len());
    if text[..first_line_end].trim_end() != "---" {
        if let Some(collection) = context.collection {
            missing_fields(
                collection.fields.iter().map(|(field, _)| field),
                &[],
                Range::new(0, 0),
                report,
            );
        }
        return 0;
    }
    let opening = Range::new(0, 3);
    let content_from = first_line_end + 1;
    let Some(close) = text
        .get(first_line_end..)
        .and_then(|rest| rest.find("\n---"))
        .map(|i| first_line_end + i)
    else {
        report.add(
            opening,
            Severity::Error,
            Rule::FrontmatterUnclosed,
            &messages.frontmatter_unclosed,
            &[],
        );
        return 0;
    };
    let content_to = if close > 0 && text.as_bytes()[close - 1] == b'\r' {
        close - 1
    } else {
        close
    };
    let body_start = text[close + 1..]
        .find('\n')
        .map_or(text.len(), |i| close + 1 + i + 1);
    let source = if content_from <= content_to {
        &text[content_from..content_to]
    } else {
        ""
    };

    let root = match yaml::parse(source) {
        Err(error) => {
            let index = LineIndex::new(text);
            let line = index.line_at((content_from + error.pos).min(text.len()));
            let line_text = &text[line.from..line.to];
            let indent = line_text.len() - line_text.trim_start().len();
            report.add(
                Range::new(line.from + indent, line.to),
                Severity::Error,
                Rule::FrontmatterYaml,
                &messages.frontmatter_yaml,
                &[],
            );
            return body_start;
        }
        Ok(parsed) => parsed.root,
    };
    let root = match root {
        Some(root) if root.is_map() => root,
        None => Node {
            kind: NodeKind::Map(Vec::new()),
            from: 0,
            to: 0,
        },
        Some(_) => {
            report.add(
                opening,
                Severity::Error,
                Rule::FrontmatterYaml,
                &messages.frontmatter_not_mapping,
                &[],
            );
            return body_start;
        }
    };

    let checker = Checker {
        base: content_from,
        text,
    };
    checker.images(&root, None, context, report);

    let Some(collection) = context.collection else {
        return body_start;
    };
    let pairs = root.pairs();
    let key_of = |pair: &(Node, Node)| pair.0.key_text();
    missing_fields(
        collection.fields.iter().map(|(field, _)| field),
        &pairs.iter().filter_map(key_of).collect::<Vec<_>>(),
        opening,
        report,
    );

    for (field, type_) in &collection.fields {
        let Some((key, value)) = pairs
            .iter()
            .find(|pair| key_of(pair).as_deref() == Some(&field.name))
        else {
            continue;
        };
        let range = checker.pair_range(key, value);
        let required = field.required && field.default.is_none();
        match value.to_value() {
            Value::Null => {
                if required {
                    report.add(
                        range,
                        Severity::Error,
                        Rule::MissingField,
                        &messages.missing_field_here,
                        &[("field", &field.name)],
                    );
                } else {
                    report.add(
                        range,
                        Severity::Error,
                        Rule::EmptyField,
                        &messages.empty_field,
                        &[("field", &field.name)],
                    );
                }
                continue;
            }
            Value::Str(text) if required && text.trim().is_empty() => {
                report.add(
                    range,
                    Severity::Error,
                    Rule::MissingField,
                    &messages.missing_field_here,
                    &[("field", &field.name)],
                );
                continue;
            }
            Value::Str(text) if DATE_FIELDS.contains(&field.name.as_str()) => {
                if !is_date(&text) {
                    report.add(
                        range,
                        Severity::Error,
                        Rule::DateFormat,
                        &messages.date_format,
                        &[("field", &field.name)],
                    );
                }
                continue;
            }
            _ => {}
        }
        checker.check_type(value, type_, &field.name, range, report);
    }

    for (key, value) in pairs {
        let Some(name) = key.key_text() else { continue };
        if !collection
            .fields
            .iter()
            .any(|(field, _)| field.name == name)
            && !PAGE_FIELDS.contains(&name.as_str())
        {
            report.add(
                checker.pair_range(key, value),
                Severity::Warning,
                Rule::UnknownField,
                &messages.unknown_field,
                &[("field", &name)],
            );
        }
    }
    body_start
}

fn missing_fields<'a>(
    fields: impl Iterator<Item = &'a crate::schema::FieldSchema>,
    present: &[String],
    at: Range,
    report: &mut Report,
) {
    let messages = report.messages;
    for field in fields {
        if field.required && field.default.is_none() && !present.contains(&field.name) {
            report.add(
                at,
                Severity::Error,
                Rule::MissingField,
                &messages.missing_field,
                &[("field", &field.name)],
            );
        }
    }
}

/// `YYYY-MM-DD` naming a real day.
fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &text[range];
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

struct Checker<'a> {
    /// Offset of the front matter text in the document.
    base: usize,
    text: &'a str,
}

impl Checker<'_> {
    fn range(&self, node: &Node) -> Range {
        Range::new(self.base + node.from, self.base + node.to)
    }

    fn pair_range(&self, key: &Node, value: &Node) -> Range {
        let to = if value.is_map() || value.is_seq() {
            key.to
        } else {
            value.to
        };
        Range::new(self.base + key.from, self.base + to)
    }

    fn first_line(&self, range: Range) -> Range {
        let line = LineIndex::new(self.text).line_at(range.from);
        Range::new(range.from, range.to.min(line.to))
    }

    fn images(&self, node: &Node, key: Option<&str>, context: &Context, report: &mut Report) {
        match &node.kind {
            NodeKind::Scalar {
                value: Value::Str(text),
                ..
            } => {
                if key.is_some_and(is_image_key) && is_image_ref(text.trim()) {
                    context.check_image(text, self.range(node), report);
                }
            }
            NodeKind::Scalar { .. } => {}
            NodeKind::Seq(items) => items
                .iter()
                .for_each(|item| self.images(item, key, context, report)),
            NodeKind::Map(pairs) => {
                for (name, value) in pairs {
                    self.images(value, name.key_text().as_deref(), context, report);
                }
            }
        }
    }

    /// Reports `node` (at `range`) when it doesn't fit `type_`.
    fn check_type(
        &self,
        node: &Node,
        type_: &FieldType,
        name: &str,
        range: Range,
        report: &mut Report,
    ) {
        let messages = report.messages;
        let value = node.to_value();
        let wrong = |template: &str, options: &str, report: &mut Report| {
            report.add(
                range,
                Severity::Error,
                Rule::FieldType,
                template,
                &[("prop", name), ("options", options)],
            );
        };
        match type_ {
            FieldType::Any => {}
            FieldType::Text => {
                if !matches!(value, Value::Str(_)) {
                    wrong(&messages.expect_text, "", report);
                }
            }
            FieldType::Number => {
                if !matches!(value, Value::Num(_)) {
                    wrong(&messages.expect_number, "", report);
                }
            }
            FieldType::Boolean => {
                if !matches!(value, Value::Bool(_)) {
                    wrong(&messages.expect_boolean, "", report);
                }
            }
            FieldType::Options(options) => {
                let text = value.js_string();
                if !matches!(value, Value::Str(_) | Value::Num(_))
                    || !options.iter().any(|option| option.js_string() == text)
                {
                    let listed = options
                        .iter()
                        .map(Value::js_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    wrong(&messages.expect_option, &listed, report);
                }
            }
            FieldType::List(inner) => {
                let NodeKind::Seq(items) = &node.kind else {
                    wrong(&messages.expect_list, "", report);
                    return;
                };
                for item in items {
                    self.check_item(item, inner, name, report);
                }
            }
            FieldType::Object(_) => self.check_item(node, type_, name, report),
        }
    }

    /// One entry of the list field `name`.
    fn check_item(&self, item: &Node, type_: &FieldType, name: &str, report: &mut Report) {
        let messages = report.messages;
        let item_range = self.range(item);
        match type_ {
            FieldType::Object(fields) => {
                if !item.is_map() {
                    let template = match fields {
                        Some(fields) if !fields.is_empty() => {
                            report.add(
                                item_range,
                                Severity::Error,
                                Rule::FieldType,
                                &messages.field_item,
                                &[("field", name), ("item", &fields[0].name)],
                            );
                            return;
                        }
                        _ => &messages.field_object,
                    };
                    report.add(
                        item_range,
                        Severity::Error,
                        Rule::FieldType,
                        template,
                        &[("field", name)],
                    );
                    return;
                }
                let Some(fields) = fields else { return };
                self.check_fields(item, fields, name, report);
            }
            other => self.check_type(item, other, name, item_range, report),
        }
    }

    fn check_fields(&self, item: &Node, fields: &[ObjectField], name: &str, report: &mut Report) {
        let messages = report.messages;
        let pairs = item.pairs();
        for field in fields {
            let found = pairs
                .iter()
                .find(|(key, _)| key.key_text().as_deref() == Some(&field.name));
            match found {
                None => {
                    if field.required {
                        let range = self.first_line(self.range(item));
                        report.add(
                            range,
                            Severity::Error,
                            Rule::MissingField,
                            &messages.missing_item_field_here,
                            &[("field", name), ("item", &field.name)],
                        );
                    }
                }
                Some((key, value)) => {
                    let range = self.pair_range(key, value);
                    if value.to_value().is_nullish() {
                        report.add(
                            range,
                            Severity::Error,
                            Rule::EmptyField,
                            &messages.empty_field,
                            &[("field", &field.name)],
                        );
                    } else {
                        self.check_type(value, &field.type_, &field.name, range, report);
                    }
                }
            }
        }
        for (key, value) in pairs {
            let Some(key_name) = key.key_text() else {
                continue;
            };
            if !fields.iter().any(|field| field.name == key_name) {
                report.add(
                    self.pair_range(key, value),
                    Severity::Warning,
                    Rule::UnknownField,
                    &messages.unknown_field_item,
                    &[("field", name), ("item", &key_name)],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn checks_real_dates() {
        assert!(super::is_date("2024-02-29"));
        assert!(!super::is_date("2023-02-29"));
        assert!(!super::is_date("2026-13-01"));
        assert!(!super::is_date("2026-1-01"));
    }

    #[test]
    fn image_keys_follow_the_site() {
        assert!(super::is_image_key("feature_image"));
        assert!(super::is_image_key("src"));
        assert!(!super::is_image_key("title"));
    }
}
