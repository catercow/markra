//! Message templates. `{name}`-style placeholders are filled in; text between backticks is a
//! name the host may show as code. The component messages are the editor's, word for word.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Messages {
    // Components (mdc-lint.ts).
    pub unknown_component: String,
    pub missing_prop: String,
    pub missing_item_field: String,
    pub unknown_prop: String,
    pub unknown_item_field: String,
    pub expect_text: String,
    pub expect_number: String,
    pub expect_boolean: String,
    pub expect_list: String,
    pub expect_option: String,
    pub expect_item: String,
    pub unclosed: String,
    pub unclosed_props: String,
    pub invalid_yaml: String,
    // Front matter.
    pub frontmatter_yaml: String,
    pub frontmatter_not_mapping: String,
    pub frontmatter_unclosed: String,
    pub missing_field: String,
    pub missing_field_here: String,
    pub empty_field: String,
    pub unknown_field: String,
    pub field_item: String,
    pub missing_item_field_here: String,
    pub unknown_field_item: String,
    pub field_object: String,
    pub date_format: String,
    // Images and links.
    pub missing_image: String,
    pub image_relative: String,
    pub image_alt: String,
    pub broken_link: String,
    // Merges.
    pub unresolved_conflict: String,
    // What the site refuses to render.
    pub raw_html: String,
    pub unsafe_link: String,
    pub unsafe_attribute: String,
    // Component input: tables, cell values, link props, galleries.
    pub table_missing: String,
    pub table_extra: String,
    pub table_header_row: String,
    pub table_unknown_column: String,
    pub table_unnamed_column: String,
    pub table_missing_column: String,
    pub table_duplicate_column: String,
    pub table_extra_cells: String,
    pub table_empty_cell: String,
    pub table_no_rows: String,
    pub table_columns: String,
    pub table_delimiter: String,
    pub cell_fix: String,
    pub price_invalid: String,
    pub price_range: String,
    pub price_approximate: String,
    pub price_decimals: String,
    pub price_comma: String,
    pub price_currency: String,
    pub code_invalid: String,
    pub link_invalid: String,
    pub prop_fix: String,
    pub search_link_invalid: String,
    pub search_link_param: String,
    pub search_link_value: String,
    pub search_link_suggest: String,
    pub review_link_invalid: String,
    pub gallery_empty: String,
    pub gallery_text: String,
}

impl Default for Messages {
    fn default() -> Self {
        let s = |text: &str| text.to_string();
        Messages {
            expect_boolean: s("`{prop}` should be true or false."),
            expect_item: s("Each item in `{prop}` needs its own fields, like `{field}: …`."),
            expect_list: s("`{prop}` should be a list."),
            expect_number: s("`{prop}` should be a number."),
            expect_option: s("`{prop}` should be one of: {options}."),
            expect_text: s(
                "`{prop}` should be text. If it is a number or true/false, put quotes around it.",
            ),
            invalid_yaml: s(
                "The props block can't be read. Check the indentation and quotes around here.",
            ),
            missing_item_field: s("This component needs a `{prop}` for each item."),
            missing_prop: s("This component needs a `{prop}`."),
            unclosed: s(
                "This component isn't closed. Add a line with just `{closer}` where it ends.",
            ),
            unclosed_props: s(
                "The props block isn't closed. Add a line with just `---` after the last prop.",
            ),
            unknown_component: s(
                "There is no “{name}” component. Check the spelling, or pick one from the Components list.",
            ),
            unknown_item_field: s("Items here don't use `{prop}`, so the site ignores it."),
            unknown_prop: s("This component doesn't use `{prop}`, so the site ignores it."),

            frontmatter_yaml: s(
                "The page details at the top can't be read. Check the indentation and quotes around here.",
            ),
            frontmatter_not_mapping: s(
                "The page details at the top should be a list of `name: value` lines.",
            ),
            frontmatter_unclosed: s(
                "The page details at the top aren't closed. Add a line with just `---` after the last one.",
            ),
            missing_field: s("This page needs a `{field}`."),
            missing_field_here: s("This page needs a `{field}`. Fill it in."),
            empty_field: s("`{field}` is empty. Fill it in, or remove the line."),
            unknown_field: s("Pages here don't use `{field}`, so the site ignores it."),
            field_item: s("Each item in `{field}` needs its own fields, like `{item}: …`."),
            missing_item_field_here: s("Each item in `{field}` is missing `{item}`."),
            unknown_field_item: s("Items in `{field}` don't use `{item}`, so the site ignores it."),
            field_object: s("`{field}` should be a group of `name: value` lines."),
            date_format: s("`{field}` should be a date written as YYYY-MM-DD, like 2026-10-06."),

            missing_image: s(
                "The image `{src}` can't be found. Check the file name, or add the image again.",
            ),
            image_relative: s(
                "The site can't show `{src}`: images go in the `public/` folder and start with `/`. Move the file to `public{path}` and write `{path}`, or add the image again.",
            ),
            image_alt: s(
                "This image has no description. Add one between the square brackets: it is shown as the caption and read aloud to people who can't see the image.",
            ),
            broken_link: s(
                "There is no page at `{path}`. Check the link: the page may have been renamed or removed.",
            ),

            unresolved_conflict: s(
                "This part still has two versions from a merge. Choose one (or keep both) before saving.",
            ),

            raw_html: s(
                "HTML isn't allowed in pages. Use Markdown or a component from the Components list instead.",
            ),
            unsafe_link: s(
                "This link can't be used on the site. Links must start with https://, mailto:, tel: or /.",
            ),
            unsafe_attribute: s("`{attribute}` can't be used on the site. Remove it."),

            table_missing: s(
                "Add a table inside `{component}`, starting with the header row `{header}`.",
            ),
            table_extra: s(
                "`{component}` takes one table. Move these rows into the first table, or start another `{component}`.",
            ),
            table_header_row: s("Start the table with a header row, like `{header}`."),
            table_unknown_column: s(
                "`{component}` has no “{column}” column. Its columns are: {columns}.",
            ),
            table_unnamed_column: s(
                "A column has no name. The columns of `{component}` are: {columns}.",
            ),
            table_missing_column: s("The table needs a `{column}` column."),
            table_duplicate_column: s(
                "“{column}” is a second `{canonical}` column. Keep one of them.",
            ),
            table_extra_cells: s("Row {row} has more cells than the header has columns."),
            table_empty_cell: s("Fill in `{column}` in row {row}."),
            table_no_rows: s("The table has no rows. Add a row under the header for each item."),
            table_columns: s(
                "Column names and order in this table are tidied automatically when you save.",
            ),
            table_delimiter: s(
                "Add a `| --- |` row under the header, one `---` per column, or this isn't read as a table. It's added automatically when you save.",
            ),
            cell_fix: s("`{column}` in row {row} will be saved as `{fixed}`."),
            price_invalid: s("`{column}` in row {row} must be a price like $7.25 (got “{value}”)."),
            price_range: s(
                "`{column}` in row {row} must be one price like $7.25, not a range (got “{value}”).",
            ),
            price_approximate: s(
                "`{column}` in row {row} must be an exact price like $7.25 (got “{value}”).",
            ),
            price_decimals: s(
                "`{column}` in row {row} can have at most two decimals, like $7.25 (got “{value}”).",
            ),
            price_comma: s(
                "`{column}` in row {row} uses a comma for cents. Use a dot, like $7.25 (got “{value}”).",
            ),
            price_currency: s(
                "`{column}` in row {row} must be in US dollars, like $7.25 (got “{value}”).",
            ),
            code_invalid: s(
                "`{column}` in row {row} can only use letters, numbers, - and _ (got “{value}”).",
            ),
            link_invalid: s(
                "`{column}` in row {row} must be a web address starting with https:// or a site path starting with / (got “{value}”).",
            ),
            prop_fix: s("`{prop}` will be saved as `{fixed}`."),
            search_link_invalid: s(
                "`{prop}` must be a CaterCow search address: filter the search page, then copy the address bar (got “{value}”).",
            ),
            search_link_param: s(
                "The search address filters by `{param}`, which this component can't use. It can use: {params}.",
            ),
            search_link_value: s("`{param}` can't be “{value}”. Use one of: {values}."),
            search_link_suggest: s("`{param}` can't be “{value}”. Did you mean `{suggestion}`?"),
            review_link_invalid: s(
                "`{prop}` must be a caterer or package page address, like /catering/123-name or /packages/123-name (got “{value}”).",
            ),
            gallery_empty: s(
                "Add images inside `gallery`, like `![Description](/blog/my-post/photo.jpg)`. Images on one line form a row.",
            ),
            gallery_text: s(
                "Only images go here. Put the caption after the last row of images, separated by a blank line.",
            ),
        }
    }
}

pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let key_len = after
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
            .count();
        if key_len > 0
            && after.as_bytes().get(key_len) == Some(&b'}')
            && let Some((_, value)) = values.iter().find(|(key, _)| *key == &after[..key_len])
        {
            out.push_str(value);
            rest = &after[key_len + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn fills_known_placeholders_only() {
        assert_eq!(
            super::fill("`{prop}` {missing} {", &[("prop", "to")]),
            "`to` {missing} {"
        );
    }
}
