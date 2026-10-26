//! YAML with source ranges, resolved like the `yaml` npm package's default (YAML 1.2 core
//! schema), which both the editor and the site's front matter parser use: `2026-10-06` stays
//! text, only `true`/`false` are booleans, and duplicate keys are an error.

use std::collections::HashMap;

use saphyr_parser::{Event, Parser, ScalarStyle};

use crate::value::Value;

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    /// Byte range in the parsed text. Quoted scalars include their quotes.
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Scalar { value: Value },
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

#[derive(Debug, Clone)]
pub struct YamlError {
    /// Byte offset in the parsed text.
    pub pos: usize,
    /// The parser's own wording, for debugging; users see a plain-language message.
    #[allow(dead_code)]
    pub message: String,
}

pub struct Parsed {
    /// None for an empty document (blank or only comments).
    pub root: Option<Node>,
}

impl Node {
    pub fn to_value(&self) -> Value {
        match &self.kind {
            NodeKind::Scalar { value, .. } => value.clone(),
            NodeKind::Seq(items) => Value::Arr(items.iter().map(Node::to_value).collect()),
            NodeKind::Map(pairs) => {
                let mut entries: Vec<(String, Value)> = Vec::new();
                for (key, value) in pairs {
                    let key = key.to_value().js_string();
                    let value = value.to_value();
                    if let Some(entry) = entries.iter_mut().find(|(name, _)| *name == key) {
                        entry.1 = value;
                    } else {
                        entries.push((key, value));
                    }
                }
                Value::Obj(entries)
            }
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self.kind, NodeKind::Map(_))
    }

    pub fn is_seq(&self) -> bool {
        matches!(self.kind, NodeKind::Seq(_))
    }

    pub fn pairs(&self) -> &[(Node, Node)] {
        match &self.kind {
            NodeKind::Map(pairs) => pairs,
            _ => &[],
        }
    }

    /// The key as the object key it becomes, for scalar keys.
    pub fn key_text(&self) -> Option<String> {
        match &self.kind {
            NodeKind::Scalar { value, .. } => match value {
                Value::Str(_) | Value::Num(_) => Some(value.js_string()),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Byte offset of each character index, for saphyr's character-based markers.
struct CharBytes(Option<Vec<usize>>);

impl CharBytes {
    fn new(text: &str) -> Self {
        if text.is_ascii() {
            return CharBytes(None);
        }
        let mut offsets: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
        offsets.push(text.len());
        CharBytes(Some(offsets))
    }

    fn byte(&self, index: usize) -> usize {
        match &self.0 {
            None => index,
            Some(offsets) => offsets[index.min(offsets.len() - 1)],
        }
    }
}

enum Frame {
    Seq {
        from: usize,
        anchor: usize,
        items: Vec<Node>,
    },
    Map {
        from: usize,
        anchor: usize,
        pairs: Vec<(Node, Node)>,
        key: Option<Node>,
    },
}

pub fn parse(text: &str) -> Result<Parsed, YamlError> {
    let bytes = CharBytes::new(text);
    let mut parser = Parser::new_from_str(text);
    let mut stack: Vec<Frame> = Vec::new();
    let mut anchors: HashMap<usize, Node> = HashMap::new();
    let mut root: Option<Node> = None;
    let mut documents = 0;
    let mut last_scalar: Option<(usize, usize)> = None;

    loop {
        let (event, span) = match parser.next_event() {
            None => break,
            Some(Ok(item)) => item,
            Some(Err(error)) => {
                let mut pos = bytes.byte(error.marker().index());
                // The npm parser blames the node that broke (a plain scalar that ran into a
                // `key:` on its next line), not the character where reading stopped.
                if let Some((from, to)) = last_scalar
                    && from <= pos
                    && pos <= to
                {
                    pos = from;
                }
                return Err(YamlError {
                    pos,
                    message: error.info().to_string(),
                });
            }
        };
        let from = bytes.byte(span.start.index());
        let to = bytes.byte(span.end.index());
        let end_event = (from, to);
        let node = match event {
            Event::StreamStart | Event::StreamEnd | Event::Nothing | Event::DocumentEnd => continue,
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(YamlError {
                        pos: from,
                        message: "Source contains multiple documents".into(),
                    });
                }
                continue;
            }
            Event::Alias(id) => match anchors.get(&id) {
                Some(node) => Node {
                    kind: node.kind.clone(),
                    from,
                    to,
                },
                None => {
                    return Err(YamlError {
                        pos: from,
                        message: "Unknown alias".into(),
                    });
                }
            },
            Event::Scalar(value, style, anchor, tag) => {
                last_scalar = Some((from, to));
                let plain = style == ScalarStyle::Plain;
                let forced_text = tag.as_ref().is_some_and(|tag| tag.suffix == "str");
                let value = if plain && !forced_text {
                    resolve_plain(&value)
                } else {
                    Value::Str(value.into_owned())
                };
                let (from, to) = if plain && from == to && matches!(value, Value::Null) {
                    empty_value_at(text, from, &stack)
                } else {
                    (from, to)
                };
                let node = Node {
                    kind: NodeKind::Scalar { value },
                    from,
                    to,
                };
                if anchor > 0 {
                    anchors.insert(anchor, node.clone());
                }
                node
            }
            Event::SequenceStart(anchor, _) => {
                stack.push(Frame::Seq {
                    from,
                    anchor,
                    items: Vec::new(),
                });
                continue;
            }
            Event::MappingStart(anchor, _) => {
                stack.push(Frame::Map {
                    from,
                    anchor,
                    pairs: Vec::new(),
                    key: None,
                });
                continue;
            }
            Event::SequenceEnd => match stack.pop() {
                Some(Frame::Seq {
                    from,
                    anchor,
                    items,
                }) => {
                    let to = collection_end(from, end_event, items.last().map(|last| last.to));
                    let node = Node {
                        kind: NodeKind::Seq(items),
                        from,
                        to,
                    };
                    if anchor > 0 {
                        anchors.insert(anchor, node.clone());
                    }
                    node
                }
                _ => continue,
            },
            Event::MappingEnd => match stack.pop() {
                Some(Frame::Map {
                    from,
                    anchor,
                    pairs,
                    ..
                }) => {
                    let to = collection_end(from, end_event, pairs.last().map(|(_, last)| last.to));
                    let node = Node {
                        kind: NodeKind::Map(pairs),
                        from,
                        to,
                    };
                    if anchor > 0 {
                        anchors.insert(anchor, node.clone());
                    }
                    node
                }
                _ => continue,
            },
        };

        match stack.last_mut() {
            None => root = Some(node),
            Some(Frame::Seq { items, .. }) => items.push(node),
            Some(Frame::Map { pairs, key, .. }) => match key.take() {
                None => {
                    if let Some(name) = node.key_text()
                        && pairs
                            .iter()
                            .any(|(existing, _)| existing.key_text().as_deref() == Some(&name))
                    {
                        return Err(YamlError {
                            pos: node.from,
                            message: "Map keys must be unique".into(),
                        });
                    }
                    *key = Some(node);
                }
                Some(k) => pairs.push((k, node)),
            },
        }
    }

    Ok(Parsed { root })
}

/// Flow collections end at their bracket; block collections at their last entry.
fn collection_end(from: usize, end_event: (usize, usize), last: Option<usize>) -> usize {
    let (start, end) = end_event;
    if end > start {
        end
    } else {
        last.unwrap_or(from).max(from)
    }
}

/// An implicit null (`url:`) sits just after the key's colon, as in the npm parser, so the
/// pair's range covers `url:`.
fn empty_value_at(text: &str, at: usize, stack: &[Frame]) -> (usize, usize) {
    if let Some(Frame::Map { key: Some(key), .. }) = stack.last() {
        let rest = &text[key.to..];
        let skipped = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if rest[skipped..].starts_with(':') {
            let pos = key.to + skipped + 1;
            return (pos, pos);
        }
    }
    (at, at)
}

/// YAML 1.2 core schema resolution of a plain scalar.
pub fn resolve_plain(text: &str) -> Value {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => return Value::Num(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => return Value::Num(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => return Value::Num(f64::NAN),
        _ => {}
    }
    if let Some(octal) = text.strip_prefix("0o")
        && !octal.is_empty()
        && octal.bytes().all(|b| (b'0'..=b'7').contains(&b))
    {
        return u64::from_str_radix(octal, 8)
            .map_or(Value::Str(text.into()), |n| Value::Num(n as f64));
    }
    if let Some(hex) = text.strip_prefix("0x")
        && !hex.is_empty()
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return u64::from_str_radix(hex, 16)
            .map_or(Value::Str(text.into()), |n| Value::Num(n as f64));
    }
    if is_core_number(text)
        && let Ok(number) = text.parse::<f64>()
    {
        return Value::Num(number);
    }
    Value::Str(text.into())
}

/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`
fn is_core_number(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if int_digits == 0 && i == frac_start {
            return false;
        }
    } else if int_digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_core_schema() {
        assert_eq!(resolve_plain("11.50"), Value::Num(11.5));
        assert_eq!(resolve_plain("2026-10-06"), Value::Str("2026-10-06".into()));
        assert_eq!(resolve_plain("yes"), Value::Str("yes".into()));
        assert_eq!(resolve_plain("100OFF"), Value::Str("100OFF".into()));
        assert_eq!(resolve_plain("1e3"), Value::Num(1000.0));
        assert_eq!(resolve_plain(""), Value::Null);
    }

    #[test]
    fn ranges_cover_quotes_and_empty_values() {
        let text = "a: \"yes\"\nurl:\nlist:\n  - label: Café\n    code: 1\n";
        let root = parse(text).unwrap().root.unwrap();
        let pairs = root.pairs();
        assert_eq!(&text[pairs[0].1.from..pairs[0].1.to], "\"yes\"");
        assert_eq!(&text[pairs[1].0.from..pairs[1].1.to], "url:");
        let NodeKind::Seq(items) = &pairs[2].1.kind else {
            panic!()
        };
        assert!(text[items[0].from..].starts_with("label: Café"));
        let code = &items[0].pairs()[1];
        assert_eq!(&text[code.0.from..code.1.to], "code: 1");
    }

    #[test]
    fn errors_point_at_the_broken_node() {
        let error = parse("headline: Hi\n  cta: Search").err().unwrap();
        assert_eq!(error.pos, 10);
        assert!(parse("a: 1\na: 2").is_err());
    }
}
