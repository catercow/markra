//! Lines and offsets. Diagnostics are found in UTF-8 byte offsets and also reported in UTF-16
//! code units, which is what JavaScript strings (and CodeMirror positions) count.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: usize,
    pub to: usize,
}

impl Range {
    pub fn new(from: usize, to: usize) -> Range {
        Range { from, to }
    }
}

/// A line of the document: `from..to` excludes the line break (and a `\r` before it).
#[derive(Debug, Clone, Copy)]
pub struct Line<'a> {
    pub from: usize,
    pub to: usize,
    pub text: &'a str,
}

pub fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let content_end = if end > start && text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        out.push(Line {
            from: start,
            to: content_end,
            text: &text[start..content_end],
        });
        if end == text.len() {
            break;
        }
        start = end + 1;
    }
    out
}

/// Converts byte offsets to lines, columns and UTF-16 offsets.
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> LineIndex<'a> {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, starts }
    }

    /// 0-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    /// The line containing `offset`, without its line break.
    pub fn line_at(&self, offset: usize) -> Range {
        let line = self.line_of(offset.min(self.text.len()));
        let from = self.starts[line];
        let mut to = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |next| next - 1);
        if to > from && self.text.as_bytes()[to - 1] == b'\r' {
            to -= 1;
        }
        Range::new(from, to)
    }

    /// 1-based line and 1-based column counted in characters.
    pub fn line_column(&self, offset: usize) -> (usize, usize) {
        let offset = floor_char_boundary(self.text, offset);
        let line = self.line_of(offset);
        let column = self.text[self.starts[line]..offset].chars().count() + 1;
        (line + 1, column)
    }

    pub fn utf16(&self, offset: usize) -> usize {
        utf8_to_utf16(self.text, offset)
    }
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The UTF-16 offset (JavaScript string index) of a UTF-8 byte offset in `text`.
pub fn utf8_to_utf16(text: &str, offset: usize) -> usize {
    text[..floor_char_boundary(text, offset)]
        .chars()
        .map(char::len_utf16)
        .sum()
}

/// The UTF-8 byte offset of a UTF-16 offset (JavaScript string index) in `text`. An offset
/// inside a surrogate pair maps to the start of that character.
pub fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += c.len_utf16();
        if units > offset {
            return byte;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_offsets_with_non_ascii_text() {
        let text = "Mariano’s 🎉 menu";
        let menu = text.find("menu").unwrap();
        assert_eq!(menu, 17);
        // ’ is one UTF-16 unit, 🎉 is two.
        assert_eq!(utf8_to_utf16(text, menu), 13);
        assert_eq!(utf16_to_utf8(text, 13), menu);
        assert_eq!(utf16_to_utf8(text, 11), text.find('🎉').unwrap());
        let index = LineIndex::new("a\nMariano’s 🎉 menu");
        assert_eq!(index.line_column(2 + menu), (2, 13));
    }

    #[test]
    fn splits_lines_without_carriage_returns() {
        let all = lines("a\r\nb\n");
        assert_eq!(
            all.iter().map(|line| line.text).collect::<Vec<_>>(),
            ["a", "b", ""]
        );
    }
}
