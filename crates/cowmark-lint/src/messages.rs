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
    pub image_outside_content: String,
    pub image_alt: String,
    pub broken_link: String,
    // Merges.
    pub unresolved_conflict: String,
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
            missing_item_field_here: s("Each item in `{field}` needs a `{item}`."),
            unknown_field_item: s("Items in `{field}` don't use `{item}`, so the site ignores it."),
            field_object: s("`{field}` should be a group of `name: value` lines."),
            date_format: s("`{field}` should be a date written as YYYY-MM-DD, like 2026-10-06."),

            missing_image: s(
                "The image `{src}` can't be found. Check the file name, or add the image again.",
            ),
            image_outside_content: s(
                "The image `{src}` points outside the content folder. Put the image next to the page and pick it again.",
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
