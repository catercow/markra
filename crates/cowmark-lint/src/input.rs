//! What goes in a component's slot (`input` in `components.json`) and link props (`format`):
//! tables with known columns, gallery images, search and review links. Values are read
//! leniently ([`crate::formats`]); anything not written in its canonical form is an error that
//! carries a fix, and [`crate::format()`] applies the fixes.

use crate::formats::{self, ValueError};
use crate::mdc::MdcBlock;
use crate::schema::{ComponentInput, ComponentSchema, InputColumn, PropSchema};
use crate::text::{Range, lines};
use crate::{Report, Rule, Severity};

/// Checks the slot of `block` against the component's `input`. `nested` are the ranges of
/// components inside other components, whose lines are not this slot's.
pub fn check_slot(
    text: &str,
    block: &MdcBlock,
    component: &ComponentSchema,
    nested: &[Range],
    report: &mut Report,
) {
    let (Some(input), Some(body)) = (&component.input, block.body()) else {
        return;
    };
    let lines = slot_lines(text, body, nested);
    match input {
        ComponentInput::Table { columns } => TableCheck {
            text,
            block,
            columns,
            report,
        }
        .run(&lines),
        ComponentInput::Images => check_gallery(text, block, &lines, report),
    }
}

/// A line of the slot: its range, where its content starts after indentation, and whether it
/// belongs to a nested component (and so ends any table or paragraph).
#[derive(Debug, Clone, Copy)]
struct SlotLine {
    from: usize,
    content: usize,
    to: usize,
    foreign: bool,
}

impl SlotLine {
    fn blank(&self, text: &str) -> bool {
        text[self.from..self.to].trim().is_empty()
    }
}

fn slot_lines(text: &str, body: Range, nested: &[Range]) -> Vec<SlotLine> {
    if body.from >= body.to {
        return Vec::new();
    }
    let slot = &text[body.from..body.to];
    let mut out = Vec::new();
    for line in lines(slot) {
        let from = body.from + line.from;
        let to = body.from + line.to;
        if line.from == slot.len() {
            break;
        }
        let indent = line.text.len() - line.text.trim_start_matches([' ', '\t']).len();
        out.push(SlotLine {
            from,
            content: from + indent,
            to,
            foreign: nested
                .iter()
                .any(|range| range.from <= from && from < range.to),
        });
    }
    out
}

fn line_break(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

// --- tables ------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Cell {
    /// The trimmed cell text's range.
    range: Range,
}

impl Cell {
    fn text<'a>(&self, text: &'a str) -> &'a str {
        &text[self.range.from..self.range.to]
    }
}

/// Byte offsets of the `|` that separate cells (not `\|`).
fn pipes(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'|' => out.push(i),
            _ => {}
        }
        i += 1;
    }
    out
}

/// The cells of a table line from `from` to `to` (content only, no indentation).
fn split_row(text: &str, from: usize, to: usize) -> Vec<Cell> {
    let line = text[from..to].trim_end();
    let mut bounds = pipes(line);
    let mut start = 0;
    if bounds.first() == Some(&0) {
        bounds.remove(0);
        start = 1;
    }
    let mut end = line.len();
    if bounds.last().is_some_and(|&last| last + 1 == line.len()) && start < line.len() {
        end = bounds.pop().unwrap();
    }
    let mut cells = Vec::new();
    let mut cell_from = start;
    for bound in bounds.into_iter().chain(std::iter::once(end)) {
        let raw = &line[cell_from.min(bound)..bound];
        let lead = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();
        let begin = from + cell_from.min(bound) + lead;
        cells.push(Cell {
            range: Range::new(begin, begin + trimmed.len()),
        });
        cell_from = bound + 1;
    }
    cells
}

fn is_delimiter(text: &str, cells: &[Cell]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.text(text).replace(' ', "");
            let dashes = cell.trim_start_matches(':').trim_end_matches(':');
            !dashes.is_empty() && dashes.bytes().all(|b| b == b'-')
        })
}

#[derive(Debug)]
struct TableRow {
    line: Range,
    cells: Vec<Cell>,
    delimiter: bool,
}

#[derive(Debug)]
struct Table {
    /// From the first line's content to the end of the last line.
    range: Range,
    indent: String,
    rows: Vec<TableRow>,
}

fn find_tables(text: &str, lines: &[SlotLine]) -> Vec<Table> {
    let mut tables: Vec<Table> = Vec::new();
    let mut open = false;
    for line in lines {
        let content = &text[line.content..line.to];
        let is_row = !line.foreign && !pipes(content).is_empty();
        if !is_row {
            open = false;
            continue;
        }
        let cells = split_row(text, line.content, line.to);
        let row = TableRow {
            line: Range::new(line.content, line.to),
            delimiter: is_delimiter(text, &cells),
            cells,
        };
        match tables.last_mut() {
            Some(table) if open => {
                table.range.to = line.to;
                table.rows.push(row);
            }
            _ => tables.push(Table {
                range: Range::new(line.content, line.to),
                indent: text[line.from..line.content].to_string(),
                rows: vec![row],
            }),
        }
        open = true;
    }
    tables
}

struct TableCheck<'a, 'r, 'm> {
    text: &'a str,
    block: &'a MdcBlock,
    columns: &'a [InputColumn],
    report: &'r mut Report<'m>,
}

/// A header cell's column in the schema, or why it has none.
enum Header {
    Column(usize),
    /// An empty header over empty cells: dropped.
    Dropped,
}

impl TableCheck<'_, '_, '_> {
    fn header_example(&self) -> String {
        let names: Vec<&str> = self
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        format!("| {} |", names.join(" | "))
    }

    fn column_list(&self) -> String {
        self.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn run(mut self, lines: &[SlotLine]) {
        let messages = self.report.messages;
        let component = self.block.name.clone();
        let tables = find_tables(self.text, lines);
        let Some(table) = tables.first() else {
            let header = self.header_example();
            self.report.add(
                self.block.name_range,
                Severity::Error,
                Rule::ComponentTable,
                &messages.table_missing,
                &[("component", &component), ("header", &header)],
            );
            return;
        };
        for extra in &tables[1..] {
            let first = extra.rows[0].line;
            self.report.add(
                first,
                Severity::Error,
                Rule::ComponentTable,
                &messages.table_extra,
                &[("component", &component)],
            );
        }
        self.table(table);
    }

    fn table(&mut self, table: &Table) {
        let messages = self.report.messages;
        let text = self.text;
        let component = self.block.name.clone();
        let header_row = &table.rows[0];
        if header_row.delimiter {
            let header = self.header_example();
            self.report.add(
                header_row.line,
                Severity::Error,
                Rule::ComponentTable,
                &messages.table_header_row,
                &[("header", &header)],
            );
            return;
        }
        let body: Vec<(usize, &TableRow)> = table.rows[1..]
            .iter()
            .filter(|row| !row.delimiter)
            .enumerate()
            .map(|(index, row)| (index + 1, row))
            .collect();
        let column_empty = |index: usize| {
            body.iter().all(|(_, row)| {
                row.cells
                    .get(index)
                    .is_none_or(|cell| cell.text(text).is_empty())
            })
        };

        // Which schema column each header cell is; anything else means the table can't be
        // rewritten without losing text.
        let mut structural = false;
        let mut headers: Vec<Header> = Vec::new();
        for (index, cell) in header_row.cells.iter().enumerate() {
            let name = cell.text(text);
            if name.is_empty() {
                if column_empty(index) {
                    headers.push(Header::Dropped);
                } else {
                    structural = true;
                    headers.push(Header::Dropped);
                    let columns = self.column_list();
                    self.report.add(
                        header_row.line,
                        Severity::Error,
                        Rule::ComponentTable,
                        &messages.table_unnamed_column,
                        &[("component", &component), ("columns", &columns)],
                    );
                }
                continue;
            }
            match self.columns.iter().position(|column| column.matches(name)) {
                None => {
                    structural = true;
                    headers.push(Header::Dropped);
                    let columns = self.column_list();
                    self.report.add(
                        cell.range,
                        Severity::Error,
                        Rule::ComponentTable,
                        &messages.table_unknown_column,
                        &[
                            ("component", &component),
                            ("column", name),
                            ("columns", &columns),
                        ],
                    );
                }
                Some(column)
                    if headers
                        .iter()
                        .any(|header| matches!(header, Header::Column(c) if *c == column)) =>
                {
                    structural = true;
                    headers.push(Header::Dropped);
                    self.report.add(
                        cell.range,
                        Severity::Error,
                        Rule::ComponentTable,
                        &messages.table_duplicate_column,
                        &[("column", name), ("canonical", &self.columns[column].name)],
                    );
                }
                Some(column) => headers.push(Header::Column(column)),
            }
        }
        for column in self.columns {
            let present = headers
                .iter()
                .any(|header| matches!(header, Header::Column(c) if self.columns[*c].name == column.name));
            if column.required && !present {
                self.report.add(
                    header_row.line,
                    Severity::Error,
                    Rule::ComponentTable,
                    &messages.table_missing_column,
                    &[("column", &column.name)],
                );
            }
        }

        // The rows that hold something; empty ones are dropped.
        let mut kept: Vec<(usize, &TableRow)> = Vec::new();
        for &(number, row) in &body {
            let extra = row
                .cells
                .iter()
                .skip(header_row.cells.len())
                .any(|cell| !cell.text(text).is_empty());
            if extra {
                structural = true;
                self.report.add(
                    row.line,
                    Severity::Error,
                    Rule::ComponentTable,
                    &messages.table_extra_cells,
                    &[("row", &number.to_string())],
                );
            }
            if row.cells.iter().any(|cell| !cell.text(text).is_empty()) {
                kept.push((number, row));
            }
        }
        if kept.is_empty() {
            self.report.add(
                header_row.line,
                Severity::Error,
                Rule::ComponentTable,
                &messages.table_no_rows,
                &[],
            );
        }

        // Each cell's value as written and in canonical form.
        let order: Vec<(usize, usize)> = {
            let mut order: Vec<(usize, usize)> = headers
                .iter()
                .enumerate()
                .filter_map(|(index, header)| match header {
                    Header::Column(column) => Some((*column, index)),
                    Header::Dropped => None,
                })
                .collect();
            order.sort();
            order
        };
        let mut written: Vec<Vec<String>> = Vec::new();
        let mut canonical: Vec<Vec<String>> = Vec::new();
        for &(number, row) in &kept {
            let mut row_written = Vec::new();
            let mut row_canonical = Vec::new();
            for &(column, index) in &order {
                let column = &self.columns[column];
                let cell = row.cells.get(index);
                let value = cell.map_or("", |cell| cell.text(text));
                row_written.push(value.to_string());
                row_canonical.push(self.cell(column, number, cell, value, row.line));
            }
            written.push(row_written);
            canonical.push(row_canonical);
        }
        if structural {
            return;
        }

        let headers: Vec<String> = order
            .iter()
            .map(|&(column, _)| self.columns[column].name.clone())
            .collect();
        let rendered = render_table(
            &headers,
            &written,
            &table.indent,
            line_break(&text[table.range.from..table.range.to]),
        );
        if rendered != text[table.range.from..table.range.to] {
            let fix = render_table(
                &headers,
                &canonical,
                &table.indent,
                line_break(&text[table.range.from..table.range.to]),
            );
            self.report.add_fix(
                header_row.line,
                Rule::ComponentTable,
                &messages.table_layout,
                &[],
                (table.range, fix),
            );
        }
    }

    /// Checks one cell and returns its canonical value (the text as written when it has none).
    fn cell(
        &mut self,
        column: &InputColumn,
        row: usize,
        cell: Option<&Cell>,
        value: &str,
        line: Range,
    ) -> String {
        let messages = self.report.messages;
        let row_text = row.to_string();
        let range = cell.map_or(line, |cell| {
            if cell.range.from == cell.range.to {
                line
            } else {
                cell.range
            }
        });
        if value.is_empty() {
            if column.required {
                self.report.add(
                    range,
                    Severity::Error,
                    Rule::ComponentTable,
                    &messages.table_empty_cell,
                    &[("row", &row_text), ("column", &column.name)],
                );
            }
            return String::new();
        }
        let parsed = match column.format() {
            "price" => formats::price(value),
            "code" => formats::code(value),
            "link" => formats::link(value),
            _ => return value.to_string(),
        };
        match parsed {
            Ok(fixed) if fixed == value => fixed,
            Ok(fixed) => {
                if let Some(cell) = cell {
                    self.report.add_fix(
                        cell.range,
                        Rule::CellFormat,
                        &messages.cell_fix,
                        &[
                            ("column", &column.name),
                            ("row", &row_text),
                            ("fixed", &fixed),
                        ],
                        (cell.range, fixed.clone()),
                    );
                }
                fixed
            }
            Err(error) => {
                let template = match error {
                    ValueError::PriceRange => &messages.price_range,
                    ValueError::PriceApproximate => &messages.price_approximate,
                    ValueError::PriceDecimals => &messages.price_decimals,
                    ValueError::PriceComma => &messages.price_comma,
                    ValueError::PriceCurrency => &messages.price_currency,
                    ValueError::Code => &messages.code_invalid,
                    ValueError::Link => &messages.link_invalid,
                    _ => &messages.price_invalid,
                };
                self.report.add(
                    range,
                    Severity::Error,
                    Rule::CellFormat,
                    template,
                    &[
                        ("column", &column.name),
                        ("row", &row_text),
                        ("value", value),
                    ],
                );
                value.to_string()
            }
        }
    }
}

/// `| a | b |` lines with the columns padded to the same width, and a `| --- |` row under the
/// header. Lines after the first start with `indent`.
pub fn render_table(
    headers: &[String],
    rows: &[Vec<String>],
    indent: &str,
    line_break: &str,
) -> String {
    let width = |text: &str| text.chars().count();
    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            rows.iter()
                .map(|row| width(&row[column]))
                .chain([width(&headers[column]), 3])
                .max()
                .unwrap_or(3)
        })
        .collect();
    let line = |cells: Vec<String>| {
        let padded: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(cell, &w)| format!("{cell}{}", " ".repeat(w - width(cell))))
            .collect();
        format!("| {} |", padded.join(" | "))
    };
    let mut out = vec![
        line(headers.to_vec()),
        line(widths.iter().map(|&w| "-".repeat(w)).collect()),
    ];
    out.extend(rows.iter().map(|row| line(row.clone())));
    out.join(&format!("{line_break}{indent}"))
}

// --- galleries ---------------------------------------------------------------------

/// `![alt](src)` images in `text[from..to]`, as ranges; and whether anything else is there.
fn images(text: &str, from: usize, to: usize) -> (Vec<Range>, bool) {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut other = false;
    let mut i = from;
    while i < to {
        if bytes[i] == b'!'
            && bytes.get(i + 1) == Some(&b'[')
            && let Some(end) = image_end(text, i + 1, to)
        {
            found.push(Range::new(i, end));
            i = end;
            continue;
        }
        if !bytes[i].is_ascii_whitespace() {
            other = true;
        }
        i += 1;
    }
    (found, other)
}

/// The end of `[alt](src …)` starting at the `[`.
fn image_end(text: &str, open: usize, to: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0;
    let mut i = open;
    while i < to {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if i >= to || bytes.get(i + 1) != Some(&b'(') {
        return None;
    }
    let mut depth = 0;
    let mut j = i + 1;
    while j < to {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn check_gallery(text: &str, block: &MdcBlock, lines: &[SlotLine], report: &mut Report) {
    let messages = report.messages;
    // Paragraphs: runs of non-blank lines of this slot.
    let mut paragraphs: Vec<&[SlotLine]> = Vec::new();
    let mut start: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        let ends = line.foreign || line.blank(text);
        match (ends, start) {
            (true, Some(from)) => {
                paragraphs.push(&lines[from..index]);
                start = None;
            }
            (false, None) => start = Some(index),
            _ => {}
        }
    }
    if let Some(from) = start {
        paragraphs.push(&lines[from..]);
    }

    struct Paragraph {
        range: Range,
        images: Vec<Range>,
        other: bool,
    }
    let read: Vec<Paragraph> = paragraphs
        .iter()
        .map(|paragraph| {
            let first = paragraph[0];
            let last = paragraph[paragraph.len() - 1];
            let mut all = Vec::new();
            let mut other = false;
            for line in paragraph.iter() {
                let (found, rest) = images(text, line.content, line.to);
                all.extend(found);
                other |= rest;
            }
            Paragraph {
                range: Range::new(first.content, last.to),
                images: all,
                other,
            }
        })
        .collect();

    let last_row = read
        .iter()
        .rposition(|paragraph| !paragraph.images.is_empty());
    let Some(last_row) = last_row else {
        report.add(
            block.name_range,
            Severity::Error,
            Rule::GalleryImages,
            &messages.gallery_empty,
            &[],
        );
        return;
    };
    for (index, paragraph) in read.iter().enumerate() {
        if paragraph.other && (index < last_row || !paragraph.images.is_empty()) {
            report.add(
                paragraph.range,
                Severity::Error,
                Rule::GalleryImages,
                &messages.gallery_text,
                &[],
            );
            continue;
        }
        if paragraph.images.is_empty() {
            continue;
        }
        let row: Vec<&str> = paragraph
            .images
            .iter()
            .map(|image| &text[image.from..image.to])
            .collect();
        let canonical = row.join(" ");
        if text[paragraph.range.from..paragraph.range.to] != canonical {
            report.add_fix(
                paragraph.range,
                Rule::GalleryImages,
                &messages.gallery_layout,
                &[],
                (paragraph.range, canonical),
            );
        }
    }
}

// --- link props --------------------------------------------------------------------

/// Where a prop's value is written: an attribute in the braces (its whole `key="…"`), or a
/// YAML scalar (with its quotes, if any).
pub enum Written<'a> {
    Attribute { range: Range, key: &'a str },
    Yaml { range: Range },
}

/// Checks a `search-link` or `review-link` prop's value.
pub fn check_prop(
    text: &str,
    prop: &PropSchema,
    name: &str,
    value: &str,
    written: Written,
    report: &mut Report,
) {
    let messages = report.messages;
    let format = prop.format.as_deref().unwrap_or("");
    if value.trim().is_empty() || !matches!(format, "search-link" | "review-link") {
        return;
    }
    let parsed = match format {
        "search-link" => formats::search_link(value, prop.search_params.as_ref()),
        _ => formats::review_link(value),
    };
    let range = match written {
        Written::Attribute { range, .. } | Written::Yaml { range } => range,
    };
    match parsed {
        Ok(fixed) if fixed == value => {}
        Ok(fixed) => {
            let insert = match written {
                Written::Attribute { key, .. } => Some(format!("{key}=\"{fixed}\"")),
                Written::Yaml { range } => {
                    let source = &text[range.from..range.to];
                    match source.chars().next() {
                        Some(quote @ ('"' | '\'')) => Some(format!("{quote}{fixed}{quote}")),
                        Some('|' | '>') => None,
                        _ => Some(fixed.clone()),
                    }
                }
            };
            match insert {
                Some(insert) => report.add_fix(
                    range,
                    Rule::LinkFormat,
                    &messages.prop_fix,
                    &[("prop", name), ("fixed", &fixed)],
                    (range, insert),
                ),
                None => report.add(
                    range,
                    Severity::Error,
                    Rule::LinkFormat,
                    &messages.prop_fix,
                    &[("prop", name), ("fixed", &fixed)],
                ),
            }
        }
        Err(ValueError::SearchParam { param }) => {
            let params = prop
                .search_params
                .as_ref()
                .map(|params| {
                    params
                        .keys()
                        .map(|key| format!("`{key}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            report.add(
                range,
                Severity::Error,
                Rule::LinkFormat,
                &messages.search_link_param,
                &[("param", &param), ("params", &params)],
            );
        }
        Err(ValueError::SearchValue {
            param,
            value,
            suggestion,
            values,
        }) => match suggestion {
            Some(suggestion) => report.add(
                range,
                Severity::Error,
                Rule::LinkFormat,
                &messages.search_link_suggest,
                &[
                    ("param", &param),
                    ("value", &value),
                    ("suggestion", &suggestion),
                ],
            ),
            None => report.add(
                range,
                Severity::Error,
                Rule::LinkFormat,
                &messages.search_link_value,
                &[
                    ("param", &param),
                    ("value", &value),
                    ("values", &values.join(", ")),
                ],
            ),
        },
        Err(ValueError::ReviewLink) => report.add(
            range,
            Severity::Error,
            Rule::LinkFormat,
            &messages.review_link_invalid,
            &[("prop", name), ("value", value)],
        ),
        Err(_) => report.add(
            range,
            Severity::Error,
            Rule::LinkFormat,
            &messages.search_link_invalid,
            &[("prop", name), ("value", value)],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_rows_into_cells() {
        let text = "| a | b \\| c |  d  |";
        let cells = split_row(text, 0, text.len());
        let texts: Vec<&str> = cells.iter().map(|cell| cell.text(text)).collect();
        assert_eq!(texts, ["a", "b \\| c", "d"]);
        let text = "a | b";
        assert_eq!(
            split_row(text, 0, text.len())
                .iter()
                .map(|c| c.text(text))
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        let text = "| a |  |";
        assert_eq!(
            split_row(text, 0, text.len())
                .iter()
                .map(|c| c.text(text))
                .collect::<Vec<_>>(),
            ["a", ""]
        );
        let text = "|";
        assert_eq!(split_row(text, 0, text.len()).len(), 1);
        let text = "| :--- | ---: |";
        assert!(is_delimiter(text, &split_row(text, 0, text.len())));
    }

    #[test]
    fn renders_aligned_tables() {
        let rendered = render_table(
            &["Item".into(), "Price".into()],
            &[
                vec!["Tacos ñ".into(), "$7.25".into()],
                vec!["A".into(), "".into()],
            ],
            "  ",
            "\n",
        );
        assert_eq!(
            rendered,
            "| Item    | Price |\n  | ------- | ----- |\n  | Tacos ñ | $7.25 |\n  | A       |       |"
        );
    }
}
