//! CSV (or tab-separated text copied from a spreadsheet) to a Markdown table, for the
//! `cowmark table` command, the MCP `csv_to_table` tool and pasting into CowMark. With a
//! component, the table is wrapped in it and formatted, so headers and values come out in
//! their canonical form and anything that can't be read is reported.

use cowmark_lint::{render_table, ComponentInput, Diagnostic, Schemas, Severity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    Comma,
    Tab,
}

impl Delimiter {
    fn byte(self) -> u8 {
        match self {
            Delimiter::Comma => b',',
            Delimiter::Tab => b'\t',
        }
    }
}

fn records(text: &str, delimiter: Delimiter) -> Result<Vec<Vec<String>>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter.byte())
        .from_reader(text.as_bytes());
    let mut out = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| format!("The CSV can't be read: {error}"))?;
        let cells: Vec<String> = record.iter().map(str::to_string).collect();
        if cells.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        out.push(cells);
    }
    Ok(out)
}

fn consistent(records: &[Vec<String>]) -> bool {
    let width = |record: &Vec<String>| {
        record.len()
            - record
                .iter()
                .rev()
                .take_while(|cell| cell.trim().is_empty())
                .count()
    };
    let columns = records.first().map_or(0, Vec::len);
    columns >= 2
        && records
            .iter()
            .all(|record| record.len() == columns || width(record) <= columns)
}

/// Whether pasted text is a table: tab-separated lines (a spreadsheet copy), or comma-separated
/// ones that don't read as prose. `csv_type` is true when the clipboard said `text/csv`.
pub fn detect(text: &str, csv_type: bool) -> Option<Delimiter> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let first = lines[0];
    if csv_type {
        return Some(if first.contains('\t') {
            Delimiter::Tab
        } else {
            Delimiter::Comma
        });
    }
    if lines.len() < 2 {
        return None;
    }
    let markdown = |line: &&str| {
        let line = line.trim_start();
        ["#", "- ", "* ", "> ", "|", "::", "```", "![", "---"]
            .iter()
            .any(|start| line.starts_with(start))
    };
    if lines.iter().any(markdown) {
        return None;
    }
    if first.contains('\t') {
        let found = records(text, Delimiter::Tab).ok()?;
        return (found.len() >= 2 && consistent(&found)).then_some(Delimiter::Tab);
    }
    let found = records(text, Delimiter::Comma).ok()?;
    let columns = found.first().map_or(0, Vec::len);
    let prose = found
        .iter()
        .flatten()
        .any(|cell| cell.chars().count() > 160)
        || found[0]
            .iter()
            .any(|cell| cell.trim_end().ends_with(['.', '!', '?']));
    (found.len() >= 2 && consistent(&found) && (columns >= 3 || found.len() >= 3) && !prose)
        .then_some(Delimiter::Comma)
}

/// A cell as Markdown table text: one line, pipes escaped.
fn cell(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

/// The CSV as a Markdown table: the first row is the header; empty rows are dropped. A tab in
/// the first line makes it tab-separated.
pub fn to_markdown(text: &str, delimiter: Option<Delimiter>) -> Result<String, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let delimiter = delimiter.unwrap_or(if first.contains('\t') {
        Delimiter::Tab
    } else {
        Delimiter::Comma
    });
    let found = records(text, delimiter)?;
    let Some((header, rows)) = found.split_first() else {
        return Err("The CSV is empty.".into());
    };
    let columns = found.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let fill = |record: &Vec<String>| -> Vec<String> {
        (0..columns)
            .map(|index| record.get(index).map_or(String::new(), |text| cell(text)))
            .collect()
    };
    Ok(render_table(
        &fill(header),
        &rows.iter().map(fill).collect::<Vec<_>>(),
        "",
        "\n",
    ))
}

/// A table for a component, and the problems left after formatting it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentTable {
    /// `::name` … `::` around the table, or the table alone when not wrapped.
    pub markdown: String,
    /// Plain-language problems, one per line of the table they're on.
    pub problems: Vec<String>,
}

/// The CSV as the body of the table component `component`, formatted: header names and
/// order, prices, codes and links in their canonical form. With `wrap`, the table comes inside
/// `::component` … `::`.
pub fn for_component(
    text: &str,
    delimiter: Option<Delimiter>,
    component: &str,
    schemas: &Schemas,
    wrap: bool,
) -> Result<ComponentTable, String> {
    let schema = schemas
        .component(component)
        .ok_or_else(|| format!("There is no component named `{component}`."))?;
    if !matches!(schema.input, Some(ComponentInput::Table { .. })) {
        return Err(format!("`{component}` doesn't take a table."));
    }
    let table = to_markdown(text, delimiter)?;
    let opener = format!("::{component}\n");
    let wrapped = format!("{opener}{table}\n::\n");
    let (formatted, diagnostics) = cowmark_lint::format("content/table.md", &wrapped, schemas);
    let problems = diagnostics
        .iter()
        .filter(|diagnostic: &&Diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| match diagnostic.line.checked_sub(1) {
            Some(line) if line >= 1 => format!("Line {line}: {}", diagnostic.message),
            _ => diagnostic.message.clone(),
        })
        .collect();
    let markdown = if wrap {
        formatted
    } else {
        formatted
            .strip_prefix(&opener)
            .and_then(|rest| rest.strip_suffix("\n::\n"))
            .map_or(formatted.clone(), str::to_string)
    };
    Ok(ComponentTable { markdown, problems })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schemas() -> Schemas {
        Schemas::from_json(
            Some(
                r#"[{"name":"pricing-grid","input":{"kind":"table","columns":[
                {"name":"Item","aliases":["Name"],"required":true,"markdown":true},
                {"name":"Price","aliases":["Cost"],"required":true,"format":"price"},
                {"name":"Description","aliases":["Notes"]}]}},
                {"name":"callout","props":[]}]"#,
            ),
            None,
        )
        .unwrap()
    }

    #[test]
    fn quotes_commas_newlines_and_pipes() {
        let csv =
            "Name,Cost,Notes\n\"Tacos, three\",7.25,\"Spicy\nwith lime\"\n\nPie | slice,$4,\n";
        assert_eq!(
            to_markdown(csv, None).unwrap(),
            "| Name         | Cost | Notes           |\n| ------------ | ---- | --------------- |\n| Tacos, three | 7.25 | Spicy with lime |\n| Pie \\| slice | $4   |                 |"
        );
    }

    #[test]
    fn ragged_rows_tabs_and_a_byte_order_mark() {
        let tsv = "\u{feff}A\tB\n1\n2\t3\t4\n";
        assert_eq!(
            to_markdown(tsv, None).unwrap(),
            "| A   | B   |     |\n| --- | --- | --- |\n| 1   |     |     |\n| 2   | 3   | 4   |"
        );
        assert!(to_markdown("", None).is_err());
        assert!(to_markdown("a,\"b\nc", None).is_ok());
    }

    #[test]
    fn components_get_canonical_headers_and_values() {
        let table = for_component(
            "Cost,Name\n7.5,Tacos\nTBD,Pie\n",
            None,
            "pricing-grid",
            &schemas(),
            true,
        )
        .unwrap();
        assert_eq!(
            table.markdown,
            "::pricing-grid\n| Item  | Price |\n| ----- | ----- |\n| Tacos | $7.50 |\n| Pie   | TBD   |\n::\n"
        );
        assert_eq!(
            table.problems,
            ["Line 4: `Price` in row 2 must be a price like $7.25 (got “TBD”)."]
        );
        let bare = for_component(
            "Name,Cost\nTacos,7\n",
            None,
            "pricing-grid",
            &schemas(),
            false,
        )
        .unwrap();
        assert_eq!(
            bare.markdown,
            "| Item  | Price |\n| ----- | ----- |\n| Tacos | $7.00 |"
        );
        assert!(bare.problems.is_empty());
        assert!(for_component("a,b", None, "callout", &schemas(), true).is_err());
        assert!(for_component("a,b", None, "nope", &schemas(), true).is_err());
    }

    #[test]
    fn detects_tables_conservatively() {
        assert_eq!(
            detect("Item\tPrice\nTacos\t7\n", false),
            Some(Delimiter::Tab)
        );
        assert_eq!(
            detect("Item,Price,Tag\nTacos,7,New\n", false),
            Some(Delimiter::Comma)
        );
        assert_eq!(
            detect("Fact,Value\nFounded,2014\nCities,40\n", false),
            Some(Delimiter::Comma)
        );
        assert_eq!(detect("Hello, world\nAnother, line\n", false), None);
        assert_eq!(detect("one line, with, commas\n", false), None);
        assert_eq!(
            detect(
                "I went there, ate tacos, and left.\nThen, later, we slept!\nOk, sure, fine.\n",
                false
            ),
            None
        );
        assert_eq!(detect("| a | b |\n| - | - |\n", false), None);
        assert_eq!(detect("- a, b, c\n- d, e, f\n", false), None);
        assert_eq!(detect("a\tb\nc\n\nd\te\tf\tg\n", false), None);
        assert_eq!(detect("Just text", true), Some(Delimiter::Comma));
    }
}
