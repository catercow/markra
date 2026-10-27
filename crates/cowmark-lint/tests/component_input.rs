//! Component input: tables, galleries and link props, read leniently, written canonically.

use cowmark_lint::{Diagnostic, Linter, Rule, Schemas, Severity, format, utf16_to_utf8};

fn schemas() -> Schemas {
    Schemas::from_json(Some(include_str!("fixtures/input-components.json")), None).unwrap()
}

fn lint(text: &str) -> Vec<Diagnostic> {
    Linter::new(schemas()).lint("content/blog/a.md", text, &|_: &str| true)
}

fn fmt(text: &str) -> String {
    format("content/blog/a.md", text, &schemas()).0
}

fn errors(text: &str) -> Vec<(Rule, String)> {
    lint(text)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.rule, d.message))
        .collect()
}

#[test]
fn a_pasted_pricing_table_is_written_canonically() {
    let text = "Intro\n\n::pricing-grid\nname | cost | NOTES\n--|--|--\nTacos | 7.25 | **Best** seller\n | | \nBurrito bowl|$12 USD|\n::\n";
    let found = lint(text);
    assert!(
        found
            .iter()
            .all(|d| d.severity == Severity::Error && d.fix.is_some()),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|d| d.message == "`Price` in row 1 will be saved as `$7.25`."),
        "{found:#?}"
    );
    let formatted = fmt(text);
    assert_eq!(
        formatted,
        "Intro\n\n::pricing-grid\n| Item | Price | Description |\n| --- | --- | --- |\n| Tacos | $7.25 | **Best** seller |\n| Burrito bowl | $12.00 | |\n::\n"
    );
    assert!(lint(&formatted).is_empty(), "{:#?}", lint(&formatted));
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn columns_are_put_in_schema_order_with_canonical_names() {
    let text = "::pricing-grid\n| Badge | Dish | Price per person |\n| --- | --- | --- |\n| New | Pho | 9 |\n::\n";
    assert_eq!(
        fmt(text),
        "::pricing-grid\n| Item | Price | Tag |\n| --- | --- | --- |\n| Pho | $9.00 | New |\n::\n"
    );
}

#[test]
fn a_table_without_a_delimiter_row_is_understood() {
    let text = "::facts-grid\nLabel | Detail\nFounded | 2014\n::\n";
    assert_eq!(
        fmt(text),
        "::facts-grid\n| Fact | Value |\n| --- | --- |\n| Founded | 2014 |\n::\n"
    );
    // A missing or short `| --- |` row is reported: without it Markdown has no table.
    for broken in [
        "::facts-grid\n| Fact | Value |\n| Founded | 2014 |\n::\n",
        "::facts-grid\n| Fact | Value |\n| --- |\n| Founded | 2014 |\n::\n",
    ] {
        let found = lint(broken);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].message.starts_with("Add a `| --- |` row"));
        assert_eq!(
            fmt(broken),
            "::facts-grid\n| Fact | Value |\n| --- | --- |\n| Founded | 2014 |\n::\n"
        );
    }
}

#[test]
fn layout_alone_is_formatted_but_never_reported() {
    for text in [
        "::facts-grid\nFact|Value\n-|-\nFounded|2014\n::\n",
        "::facts-grid\n| Fact    | Value |\n| :------ | ----: |\n| Founded | 2014  |\n|  |  |\n::\n",
        "::facts-grid\n|Fact|Value|  |\n|---|---|---|\n|  Founded |2014|  |\n::\n",
        "::gallery\n![A](/blog/a/1.jpg)\n![B](/blog/a/2.jpg)\n::\n",
    ] {
        assert!(lint(text).is_empty(), "{text}\n{:#?}", lint(text));
        let formatted = fmt(text);
        assert_ne!(formatted, text);
        assert_eq!(fmt(&formatted), formatted);
    }
    assert_eq!(
        fmt("::facts-grid\nFact|Value\n-|-\nFounded|2014\n::\n"),
        "::facts-grid\n| Fact | Value |\n| --- | --- |\n| Founded | 2014 |\n::\n"
    );
}

#[test]
fn values_that_cannot_be_read_are_errors_without_a_fix() {
    let text = "::pricing-grid\n| Item | Price |\n| --- | --- |\n| Tacos | TBD |\n| Wings | 7-9 |\n| Pie | ~$13 |\n| Cake | 7,25 |\n| Tea | €3 |\n| Soup | 1.255 |\n|  | $4 |\n::\n";
    let found: Vec<Diagnostic> = lint(text)
        .into_iter()
        .filter(|d| d.rule == Rule::CellFormat || d.message.starts_with("Fill in"))
        .collect();
    let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "`Price` in row 1 must be a price like $7.25 (got “TBD”).",
            "`Price` in row 2 must be one price like $7.25, not a range (got “7-9”).",
            "`Price` in row 3 must be an exact price like $7.25 (got “~$13”).",
            "`Price` in row 4 uses a comma for cents. Use a dot, like $7.25 (got “7,25”).",
            "`Price` in row 5 must be in US dollars, like $7.25 (got “€3”).",
            "`Price` in row 6 can have at most two decimals, like $7.25 (got “1.255”).",
            "Fill in `Item` in row 7.",
            "`Price` in row 7 will be saved as `$4.00`.",
        ],
        "{found:#?}"
    );
    let tbd = &found[0];
    assert_eq!(&text[tbd.from..tbd.to], "TBD");
    assert!(found[..7].iter().all(|d| d.fix.is_none()));
    // Formatting fixes what it can and keeps the rest as written.
    let formatted = fmt(text);
    assert!(
        formatted.contains("| TBD |")
            && formatted.contains("| €3 |")
            && formatted.contains("$4.00"),
        "{formatted}"
    );
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn tables_that_would_lose_text_are_left_alone() {
    let text = "::pricing-grid\nItem | Price | Calories\n--- | --- | ---\nTacos | 7 | 300\nPie | 4 | 200 | extra\n::\n";
    let found = errors(text);
    assert!(found.contains(&(
        Rule::ComponentTable,
        "`pricing-grid` has no “Calories” column. Its columns are: Item, Price, Description, Tag.".into()
    )));
    assert!(found.contains(&(
        Rule::ComponentTable,
        "Row 2 has more cells than the header has columns.".into()
    )));
    assert!(
        !lint(text)
            .iter()
            .any(|d| d.rule == Rule::ComponentTable && d.fix.is_some())
    );
    // Prices are still fixed in place; the table keeps its columns.
    assert_eq!(
        fmt(text),
        "::pricing-grid\nItem | Price | Calories\n--- | --- | ---\nTacos | $7.00 | 300\nPie | $4.00 | 200 | extra\n::\n"
    );

    let duplicate =
        "::pricing-grid\n| Item | Name | Price |\n| --- | --- | --- |\n| A | B | 1 |\n::\n";
    assert!(
        errors(duplicate)
            .iter()
            .any(|(_, m)| m == "“Name” is a second `Item` column. Keep one of them.")
    );
    assert_eq!(fmt(duplicate), duplicate.replace("| 1 |", "| $1.00 |"));
}

#[test]
fn the_slot_needs_exactly_one_table() {
    assert_eq!(
        errors("::pricing-grid\n::\n"),
        [(
            Rule::ComponentTable,
            "Add a table inside `pricing-grid`, starting with the header row `| Item | Price | Description | Tag |`.".into()
        )]
    );
    let two = "::facts-grid\n| Fact | Value |\n| ---- | ----- |\n| A    | B     |\n\n| Fact | Value |\n| --- | --- |\n| C | D |\n::\n";
    assert_eq!(
        errors(two),
        [(Rule::ComponentTable, "`facts-grid` takes one table. Move these rows into the first table, or start another `facts-grid`.".into())]
    );
    let missing = "::facts-grid\n| Fact |\n| ---- |\n| A    |\n::\n";
    assert_eq!(
        errors(missing),
        [(
            Rule::ComponentTable,
            "The table needs a `Value` column.".into()
        )]
    );
    let empty = "::facts-grid\n| Fact | Value |\n| ---- | ----- |\n|  |  |\n::\n";
    assert_eq!(
        errors(empty)[0].1,
        "The table has no rows. Add a row under the header for each item."
    );
    assert_eq!(
        fmt(empty),
        "::facts-grid\n| Fact | Value |\n| --- | --- |\n::\n"
    );
}

#[test]
fn promo_codes_and_links_are_normalized() {
    let text = "::promo-codes{label-heading=\"Area\"}\n| City | Promo code | URL |\n|---|---|---|\n| NYC | save 10 | https://www.catercow.com/explore/new-york-city/catering |\n| Austin | ATX_1 | https://example.com/Deal |\n| Boston | bos! | ftp://x |\n::\n";
    let found = errors(text);
    assert!(found.contains(&(
        Rule::CellFormat,
        "`Code` in row 3 can only use letters, numbers, - and _ (got “bos!”).".into()
    )));
    assert!(found.contains(&(
        Rule::CellFormat,
        "`Link` in row 3 must be a web address starting with https:// or a site path starting with / (got “ftp://x”).".into()
    )));
    let formatted = fmt(text);
    assert!(
        formatted.contains("| NYC | SAVE10 | /explore/new-york-city/catering |"),
        "{formatted}"
    );
    assert!(formatted.contains("| Location |") && formatted.contains("https://example.com/Deal"));
}

#[test]
fn galleries_put_each_row_on_one_line() {
    let text = "::gallery\n![A](/blog/a/1.jpg)\n![B](/blog/a/2.jpg)\n\n![C](/blog/a/3.jpg)   ![D](/blog/a/4.jpg)\n\nPhotos by **Sam**.\n::\n";
    // Line breaks between images are layout: tidied, not reported.
    assert!(lint(text).is_empty(), "{:#?}", lint(text));
    let formatted = fmt(text);
    assert_eq!(
        formatted,
        "::gallery\n![A](/blog/a/1.jpg) ![B](/blog/a/2.jpg)\n\n![C](/blog/a/3.jpg) ![D](/blog/a/4.jpg)\n\nPhotos by **Sam**.\n::\n"
    );
    assert!(lint(&formatted).is_empty());

    assert_eq!(
        errors("::gallery\nJust words\n::\n")[0].0,
        Rule::GalleryImages
    );
    let text_first = "::gallery\nCaption first\n\n![A](/blog/a/1.jpg)\n::\n";
    assert_eq!(
        errors(text_first),
        [(Rule::GalleryImages, "Only images go here. Put the caption after the last row of images, separated by a blank line.".into())]
    );
    let mixed = "::gallery\n![A](/blog/a/1.jpg) and some words\n::\n";
    assert_eq!(errors(mixed).len(), 1);
    assert_eq!(fmt(mixed), mixed);
}

#[test]
fn search_links_in_braces_and_yaml() {
    let text = "::packages{link=\"https://www.catercow.com/search?presentation_type=Buffet&package_type=breakfast\"}\n::\n";
    let found = lint(text);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].message,
        "`link` will be saved as `/search?package_type=breakfast&presentation_type=buffet`."
    );
    assert_eq!(
        fmt(text),
        "::packages{link=\"/search?package_type=breakfast&presentation_type=buffet\"}\n::\n"
    );

    let yaml = "::packages\n---\nlink: 'https://catercow.com/search?package_category=lunch_mexican'\n---\n::\n";
    assert_eq!(
        fmt(yaml),
        "::packages\n---\nlink: '/search?package_category=lunch_mexican'\n---\n::\n"
    );
    let plain = "::packages\n---\nlink: https://catercow.com/search\n---\n::\n";
    assert_eq!(fmt(plain), "::packages\n---\nlink: /search\n---\n::\n");

    assert_eq!(
        errors("::packages{link=\"/search?package_type=brekfast\"}\n::\n"),
        [(
            Rule::LinkFormat,
            "`package_type` can't be “brekfast”. Did you mean `breakfast`?".into()
        )]
    );
    assert_eq!(
        errors("::packages{link=\"/search?presentation_type=family\"}\n::\n"),
        [(
            Rule::LinkFormat,
            "`presentation_type` can't be “family”. Use one of: individual, buffet.".into()
        )]
    );
    assert_eq!(
        errors("::packages{link=\"/search?utm_source=mail\"}\n::\n"),
        [(
            Rule::LinkFormat,
            "The search address filters by `utm_source`, which this component can't use. It can use: `package_type`, `presentation_type`, `package_category`.".into()
        )]
    );
    assert_eq!(
        errors("::packages{link=\"https://example.com/search\"}\n::\n")[0].0,
        Rule::LinkFormat
    );
    assert!(lint("::packages{link=\"/search?package_type=dessert\"}\n::\n").is_empty());
}

#[test]
fn review_links_need_an_id() {
    let text =
        "::reviews{link=\"https://www.catercow.com/catering/123-marianos?x=1\" limit=3}\n::\n";
    assert_eq!(
        fmt(text),
        "::reviews{link=\"/catering/123-marianos\" limit=3}\n::\n"
    );
    assert_eq!(
        errors("::reviews{link=\"/brands/marianos\"}\n::\n"),
        [(
            Rule::LinkFormat,
            "`link` must be a caterer or package page address, like /catering/123-name or /packages/123-name (got “/brands/marianos”).".into()
        )]
    );
}

#[test]
fn fix_offsets_count_utf16_units() {
    let text = "Café 🎉 menu\n\n::pricing-grid\n| Item | Price |\n| --- | --- |\n| Crème brûlée 🍮 | 7.5 |\n::\n";
    let price = lint(text)
        .into_iter()
        .find(|d| d.rule == Rule::CellFormat)
        .unwrap();
    let fix = price.fix.unwrap();
    assert_eq!(&text[fix.from..fix.to], "7.5");
    assert_eq!(fix.insert, "$7.50");
    let utf16: Vec<u16> = text.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&utf16[fix.from_utf16..fix.to_utf16]).unwrap(),
        "7.5"
    );
    assert_eq!(utf16_to_utf8(text, fix.from_utf16), fix.from);
    let renamed = text.replace("| Item |", "| Dish |");
    let utf16: Vec<u16> = renamed.encode_utf16().collect();
    let columns = lint(&renamed)
        .into_iter()
        .find(|d| d.rule == Rule::ComponentTable)
        .unwrap()
        .fix
        .unwrap();
    assert_eq!(
        String::from_utf16(&utf16[columns.from_utf16..columns.to_utf16]).unwrap(),
        &renamed[columns.from..columns.to]
    );
    let formatted = fmt(text);
    assert!(
        formatted.contains("| Crème brûlée 🍮 | $7.50 |"),
        "{formatted}"
    );
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn nested_and_indented_tables_keep_their_indentation() {
    let text = ":::section\n  ::facts-grid\n  Fact|Value\n  ---|---\n  A|B\n  ::\n:::\n";
    let schemas = Schemas::from_json(
        Some(&include_str!("fixtures/input-components.json").replace(
            r#"{ "name": "callout", "props": [] }"#,
            r#"{ "name": "callout", "props": [] }, { "name": "section", "props": [] }"#,
        )),
        None,
    )
    .unwrap();
    let (formatted, left) = format("content/blog/a.md", text, &schemas);
    assert_eq!(
        formatted,
        ":::section\n  ::facts-grid\n  | Fact | Value |\n  | --- | --- |\n  | A | B |\n  ::\n:::\n"
    );
    assert!(left.is_empty(), "{left:#?}");
}

#[test]
fn old_yaml_props_do_not_break_anything() {
    let text = "::pricing-grid\n---\nitems:\n  - name: Tacos\n    price: 7.25\n---\n::\n\n::gallery\n---\nrows:\n  - - src: /blog/a/1.jpg\n      alt: A\n---\n::\n\n::packages\n---\nsearch-url: https://www.catercow.com/search?x=1\npackage-type: breakfast\n---\n::\n";
    let found = lint(text);
    assert!(found.iter().any(|d| d.rule == Rule::UnknownProp));
    assert!(found.iter().any(|d| d.rule == Rule::ComponentTable));
    assert!(found.iter().any(|d| d.rule == Rule::GalleryImages));
    assert_eq!(fmt(text), text);
    // And the current schema, without inputs, checks nothing new.
    let old =
        Schemas::from_json(Some(include_str!("fixtures/site-components.json")), None).unwrap();
    let (formatted, _) = format("content/blog/a.md", text, &old);
    assert_eq!(formatted, text);
}

#[test]
fn diagnostics_serialize_their_fix_in_camel_case() {
    let found = lint("::pricing-grid\n| Item | Price |\n| ---- | ----- |\n| A    | 1     |\n::\n");
    let json = serde_json::to_value(&found).unwrap();
    let fix = &json[0]["fix"];
    assert_eq!(fix["insert"], "$1.00");
    assert!(fix["fromUtf16"].is_number() && fix["toUtf16"].is_number());
    let clean = serde_json::to_value(lint("::callout\n::\n")).unwrap();
    assert_eq!(clean, serde_json::json!([]));
    let unrelated = serde_json::to_value(lint("::nope\n::\n")).unwrap();
    assert!(unrelated[0].get("fix").is_none());
}

#[test]
fn formatting_is_idempotent_and_keeps_every_cell() {
    let headers = [
        "Item | Price | Notes",
        "dish|COST|details|badge",
        "| Price per person | Name |",
        "Tag | Item | Cost",
    ];
    let prices = [
        "7.25",
        "$7",
        "7.5 USD",
        "1200.5",
        "$1,200.50",
        "TBD",
        "7-9",
        " 12 ",
        ".5",
        "€3",
    ];
    let cells = [
        "Tacos",
        "**Bold** [link](/x)",
        "Crème brûlée 🍮",
        "a \\| b",
        "",
        "x",
    ];
    for (h, header) in headers.iter().enumerate() {
        for (p, price) in prices.iter().enumerate() {
            let columns = header.trim_matches('|').split('|').count();
            let row = |i: usize| -> String {
                (0..columns)
                    .map(|c| {
                        let name = header
                            .trim_matches('|')
                            .split('|')
                            .nth(c)
                            .unwrap()
                            .trim()
                            .to_lowercase();
                        if ["price", "cost", "price per person"].contains(&name.as_str()) {
                            price.to_string()
                        } else {
                            cells[(i + c + h) % cells.len()].to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            let separator = if p % 2 == 0 {
                format!("{}\n", vec!["---"; columns].join("|"))
            } else {
                String::new()
            };
            let text = format!(
                "Intro ✓\n\n::pricing-grid\n{header}\n{separator}{}\n{}\n|{}|\n::\n\nAfter\n",
                row(0),
                row(1),
                " |".repeat(columns)
            );
            let once = fmt(&text);
            let twice = fmt(&once);
            assert_eq!(once, twice, "not idempotent for:\n{text}");
            assert!(
                lint(&once).iter().all(|d| d.fix.is_none()),
                "fixes left after formatting:\n{once}"
            );
            for cell in cells.iter().filter(|cell| !cell.is_empty()) {
                if text.contains(cell) {
                    assert!(
                        once.contains(cell),
                        "lost {cell:?} formatting:\n{text}\n→\n{once}"
                    );
                }
            }
            assert!(
                once.starts_with("Intro ✓\n\n::pricing-grid\n") && once.ends_with("::\n\nAfter\n")
            );
        }
    }
}
