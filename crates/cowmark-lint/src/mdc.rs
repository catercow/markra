//! MDC block structure, ported from the editor's grammar (`mdc-syntax.ts`) and block pairing
//! (`mdc-blocks.ts`): `::name[label]{attrs}` openers, a `---` YAML props block directly after
//! the opener, `#slot` lines, `::` closers paired by colon count, and fenced code, inside which
//! nothing is MDC. Everything else is prose, returned for the link and image checks.

use crate::text::{Line, Range, lines};

#[derive(Debug, Clone)]
pub struct MdcBlock {
    pub name: String,
    /// Colon count of the opener; the closer must match it.
    pub colons: usize,
    pub name_range: Range,
    /// `{...}` including the braces.
    pub attributes: Option<Range>,
    /// The YAML props block, `---` to `---`.
    pub props: Option<Range>,
    /// The YAML between the fences; None when the block is empty.
    pub props_content: Option<Range>,
    pub props_closed: bool,
    pub closed: bool,
    /// Where the opener's line starts.
    pub line_from: usize,
    /// Where the slot starts: the line after the opener, or after the props block.
    pub body_from: usize,
    /// The closer's line (without its line break); None while unclosed.
    pub closer: Option<Range>,
}

impl MdcBlock {
    /// The slot's text range, from the line after the opener (or props) to the closer's line.
    pub fn body(&self) -> Option<Range> {
        self.closer.map(|closer| Range::new(self.body_from, closer.from.max(self.body_from)))
    }
}

#[derive(Debug, Default)]
pub struct Structure {
    pub blocks: Vec<MdcBlock>,
    /// Runs of ordinary Markdown lines (no blank lines, code, openers, closers or props).
    pub prose: Vec<Range>,
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// Index just past a balanced `{...}` starting at `start`, or None.
pub fn scan_attributes(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b.get(start) != Some(&b'{') {
        return None;
    }
    let mut quote = 0u8;
    let mut depth = 0i32;
    let mut i = start;
    while i < b.len() {
        let ch = b[i];
        if quote != 0 {
            if ch == b'\\' {
                i += 1;
            } else if ch == quote {
                quote = 0;
            }
        } else if ch == b'"' || ch == b'\'' {
            quote = ch;
        } else if ch == b'{' {
            depth += 1;
        } else if ch == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        } else if ch == b'\n' {
            return None;
        }
        i += 1;
    }
    None
}

fn scan_name(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    if !b.get(start).copied().is_some_and(is_name_start) {
        return None;
    }
    let mut i = start + 1;
    while i < b.len() && is_name_char(b[i]) {
        i += 1;
    }
    Some(i)
}

fn scan_bracket(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b.get(start) != Some(&b'[') {
        return None;
    }
    let mut depth = 0;
    let mut i = start;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_blank_from(text: &str, from: usize) -> bool {
    text[from.min(text.len())..].trim().is_empty()
}

fn colon_run(text: &str, start: usize) -> usize {
    text.as_bytes()[start..]
        .iter()
        .take_while(|&&c| c == b':')
        .count()
}

/// Where a line's content starts after blockquote and list markers and indentation, and the
/// indentation (in columns) right before it.
fn content_start(text: &str) -> (usize, usize) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut indent;
    loop {
        indent = 0;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            indent += if b[i] == b'\t' { 4 } else { 1 };
            i += 1;
        }
        if i < b.len() && b[i] == b'>' {
            i += 1;
            if i < b.len() && b[i] == b' ' {
                i += 1;
            }
            continue;
        }
        let marker_end = if i < b.len() && matches!(b[i], b'-' | b'*' | b'+') {
            Some(i + 1)
        } else {
            let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            (1..=9)
                .contains(&digits)
                .then_some(i + digits)
                .filter(|&end| end < b.len() && matches!(b[end], b'.' | b')'))
                .map(|end| end + 1)
        };
        match marker_end {
            Some(end) if end < b.len() && (b[end] == b' ' || b[end] == b'\t') => {
                i = end + 1;
                continue;
            }
            _ => break,
        }
    }
    (i, indent)
}

struct Opener {
    colons: usize,
    name_end: usize,
    attrs: Option<(usize, usize)>,
}

fn match_opener(text: &str, pos: usize) -> Option<Opener> {
    if text.as_bytes().get(pos) != Some(&b':') {
        return None;
    }
    let colons = colon_run(text, pos);
    if colons < 2 {
        return None;
    }
    let name_end = scan_name(text, pos + colons)?;
    let mut end = name_end;
    if let Some(label_end) = scan_bracket(text, end) {
        end = label_end;
    }
    let attrs_from = end;
    let attrs = scan_attributes(text, end).map(|attrs_end| (attrs_from, attrs_end));
    if let Some((_, attrs_end)) = attrs {
        end = attrs_end;
    }
    is_blank_from(text, end).then_some(Opener {
        colons,
        name_end,
        attrs,
    })
}

fn closer_colons(text: &str, pos: usize) -> Option<usize> {
    if text.as_bytes().get(pos) != Some(&b':') {
        return None;
    }
    let colons = colon_run(text, pos);
    (colons >= 2 && is_blank_from(text, pos + colons)).then_some(colons)
}

fn is_slot(text: &str, pos: usize) -> bool {
    if text.as_bytes().get(pos) != Some(&b'#') {
        return false;
    }
    let Some(name_end) = scan_name(text, pos + 1) else {
        return false;
    };
    let end = scan_attributes(text, name_end).unwrap_or(name_end);
    is_blank_from(text, end)
}

struct Fence {
    marker: u8,
    len: usize,
    /// Opened indented 4+ columns (inside a nested component): any indent closes it.
    deep: bool,
}

fn fence_open(text: &str, pos: usize, indent: usize) -> Option<Fence> {
    let rest = &text[pos..];
    let marker = *rest.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = rest.bytes().take_while(|&c| c == marker).count();
    if len < 3 || (marker == b'`' && rest[len..].contains('`')) {
        return None;
    }
    Some(Fence {
        marker,
        len,
        deep: indent >= 4,
    })
}

fn fence_closes(fence: &Fence, text: &str, pos: usize, indent: usize) -> bool {
    if !fence.deep && indent >= 4 {
        return false;
    }
    let rest = &text[pos..];
    let len = rest.bytes().take_while(|&c| c == fence.marker).count();
    len >= fence.len && rest[len..].trim().is_empty()
}

/// Reads the MDC structure of `text`, starting at byte `body_start` (after any front matter).
pub fn read_structure(text: &str, body_start: usize) -> Structure {
    let all: Vec<Line> = lines(text)
        .into_iter()
        .filter(|line| line.from >= body_start)
        .collect();
    let mut out = Structure::default();
    let mut open: Vec<usize> = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut prose: Option<Range> = None;
    let mut i = 0;

    while i < all.len() {
        let line = all[i];
        let (pos, indent) = content_start(line.text);

        if let Some(current) = &fence {
            if fence_closes(current, line.text, pos, indent) {
                fence = None;
            }
            i += 1;
            continue;
        }

        let structural = fence_open(line.text, pos, indent)
            .map(|opened| fence = Some(opened))
            .is_some()
            || line.text.trim().is_empty()
            || is_slot(line.text, pos);
        if structural {
            out.prose.extend(prose.take());
            i += 1;
            continue;
        }

        if let Some(opener) = match_opener(line.text, pos) {
            out.prose.extend(prose.take());
            let from = line.from + pos;
            let name = &line.text[pos + opener.colons..opener.name_end];
            let mut block = MdcBlock {
                name: name.to_string(),
                colons: opener.colons,
                name_range: Range::new(from + opener.colons, line.from + opener.name_end),
                attributes: opener
                    .attrs
                    .map(|(a, b)| Range::new(line.from + a, line.from + b)),
                props: None,
                props_content: None,
                props_closed: false,
                closed: false,
                line_from: line.from,
                body_from: line.to,
                closer: None,
            };
            i += 1;
            if all.get(i).is_some_and(|next| next.text.trim() == "---") {
                let fence_line = all[i];
                let props_from = fence_line.from + content_start(fence_line.text).0;
                let mut content: Option<Range> = None;
                let mut props_to = None;
                i += 1;
                while i < all.len() {
                    let line = all[i];
                    if line.text.trim() == "---" {
                        props_to = Some(line.from + line.text.trim_end().len());
                        i += 1;
                        break;
                    }
                    if closer_colons(line.text, content_start(line.text).0).is_some() {
                        break;
                    }
                    let content = content.get_or_insert(Range::new(line.from, line.to));
                    content.to = line.to;
                    i += 1;
                }
                block.props_closed = props_to.is_some();
                let props_to = props_to.unwrap_or_else(|| {
                    content.map_or(props_from + 3, |content| content.to.max(props_from + 3))
                });
                block.props = Some(Range::new(props_from, props_to));
                block.props_content = content;
            }
            block.body_from = all.get(i).map_or(text.len(), |next| next.from);
            open.push(out.blocks.len());
            out.blocks.push(block);
            continue;
        }

        if let Some(colons) = closer_colons(line.text, pos) {
            out.prose.extend(prose.take());
            if let Some(at) = open
                .iter()
                .rposition(|&index| out.blocks[index].colons == colons)
            {
                out.blocks[open[at]].closed = true;
                out.blocks[open[at]].closer = Some(Range::new(line.from, line.to));
                open.truncate(at);
            }
            i += 1;
            continue;
        }

        match &mut prose {
            Some(range) => range.to = line.to,
            None => prose = Some(Range::new(line.from, line.to)),
        }
        i += 1;
    }
    out.prose.extend(prose);
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeKind {
    /// `key="value"` or `key=value`.
    Text,
    /// `key` alone: true.
    Bare,
    /// `:key='json'`: the source text, to be parsed.
    Bound,
    Id,
    Class,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub key: String,
    pub value: String,
    pub kind: AttributeKind,
    /// Offsets of the attribute's source in the text between the braces.
    pub from: usize,
    pub to: usize,
}

/// Inline attributes between an opener's `{` and `}`, as `parseMdcAttributes` reads them.
pub fn parse_attributes(src: &str) -> Vec<Attribute> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let at = |index: usize| chars.get(index).map(|&(_, c)| c);
    let byte = |index: usize| chars.get(index).map_or(src.len(), |&(b, _)| b);
    let mut pos = 0;

    while pos < chars.len() {
        while at(pos).is_some_and(char::is_whitespace) {
            pos += 1;
        }
        if pos >= chars.len() {
            break;
        }
        let from = pos;
        let sigil = at(pos).unwrap();

        if sigil == '#' || sigil == '.' {
            pos += 1;
            let name_start = pos;
            while at(pos).is_some_and(|c| !c.is_whitespace()) {
                pos += 1;
            }
            out.push(Attribute {
                key: if sigil == '#' {
                    "id".into()
                } else {
                    "class".into()
                },
                value: src[byte(name_start)..byte(pos)].to_string(),
                kind: if sigil == '#' {
                    AttributeKind::Id
                } else {
                    AttributeKind::Class
                },
                from: byte(from),
                to: byte(pos),
            });
            continue;
        }

        let bound = sigil == ':';
        if bound {
            pos += 1;
        }
        let key_start = pos;
        while at(pos).is_some_and(|c| !c.is_whitespace() && c != '=') {
            pos += 1;
        }
        let key = src[byte(key_start)..byte(pos)].to_string();

        if at(pos) != Some('=') {
            out.push(Attribute {
                key,
                value: "true".into(),
                kind: AttributeKind::Bare,
                from: byte(from),
                to: byte(pos),
            });
            continue;
        }

        pos += 1;
        let mut value = String::new();
        match at(pos) {
            Some(quote @ ('"' | '\'')) => {
                pos += 1;
                while pos < chars.len() && at(pos) != Some(quote) {
                    if at(pos) == Some('\\') && pos + 1 < chars.len() {
                        // Bound values are JSON/YAML source: only an escaped quote belongs to
                        // the attribute, every other escape is left for that parser.
                        if bound && at(pos + 1) != Some(quote) {
                            value.push('\\');
                        }
                        pos += 1;
                    }
                    value.push(at(pos).unwrap());
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
            }
            _ => {
                let value_start = pos;
                while at(pos).is_some_and(|c| !c.is_whitespace()) {
                    pos += 1;
                }
                value = src[byte(value_start)..byte(pos)].to_string();
            }
        }
        out.push(Attribute {
            key,
            value,
            kind: if bound {
                AttributeKind::Bound
            } else {
                AttributeKind::Text
            },
            from: byte(from),
            to: byte(pos),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<(String, bool)> {
        read_structure(text, 0)
            .blocks
            .into_iter()
            .map(|block| (block.name, block.closed))
            .collect()
    }

    #[test]
    fn pairs_openers_and_closers_by_colons() {
        assert_eq!(
            names(":::grid\n::card\nA\n::\n:::\n::other\n"),
            [
                ("grid".into(), true),
                ("card".into(), true),
                ("other".into(), false)
            ]
        );
    }

    #[test]
    fn ignores_code_fences() {
        assert_eq!(
            names("```md\n::callout\n```\n"),
            Vec::<(String, bool)>::new()
        );
        assert_eq!(
            names("::a\n    ```\n    ::\n    ```\n::"),
            [("a".into(), true)]
        );
    }

    #[test]
    fn reads_props_blocks() {
        let text = "::a\n---\nx: 1\n---\nBody\n::";
        let block = &read_structure(text, 0).blocks[0];
        assert!(block.props_closed);
        let content = block.props_content.unwrap();
        assert_eq!(&text[content.from..content.to], "x: 1");
        let unclosed = &read_structure("::a\n---\nx: 1\n::", 0).blocks[0];
        assert!(!unclosed.props_closed && unclosed.closed);
    }

    #[test]
    fn finds_the_slot() {
        let text = "::a\n---\nx: 1\n---\n| A |\n  ::b\n  Hi\n  ::\n::\n::c\n";
        let blocks = read_structure(text, 0).blocks;
        let body = blocks[0].body().unwrap();
        assert_eq!(&text[body.from..body.to], "| A |\n  ::b\n  Hi\n  ::\n");
        let inner = blocks[1].body().unwrap();
        assert_eq!(&text[inner.from..inner.to], "  Hi\n");
        assert_eq!(blocks[0].line_from, 0);
        assert_eq!(blocks[1].line_from, text.find("  ::b").unwrap());
        assert_eq!(blocks[2].body(), None);
        let empty = &read_structure("::a\n::", 0).blocks[0];
        assert_eq!(empty.body(), Some(Range::new(4, 4)));
    }

    #[test]
    fn parses_attributes() {
        let attrs = parse_attributes(r#"title="Café \"x\"" number :items='[1]' #id .wide"#);
        assert_eq!(attrs[0].value, "Café \"x\"");
        assert_eq!(attrs[1].kind, AttributeKind::Bare);
        assert_eq!(attrs[2].value, "[1]");
        assert_eq!(attrs[3].kind, AttributeKind::Id);
        assert_eq!(attrs[4].value, "wide");
    }
}
