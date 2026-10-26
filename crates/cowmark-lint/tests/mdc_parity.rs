//! Every case from the editor's `mdc-lint.test.ts` (`lintMdcDocument`), with the same
//! fixture catalog (`components.fixtures.ts`, exported to tests/fixtures/components.json).

use cowmark_lint::{Linter, Messages, Schemas, Severity};

const CATALOG: &str = include_str!("fixtures/components.json");

fn schemas(with_components: bool) -> Schemas {
    Schemas::from_json(with_components.then_some(CATALOG), None).unwrap()
}

fn run(doc: &str, with_components: bool) -> Vec<(String, &'static str, String)> {
    Linter::new(schemas(with_components))
        .lint("content/test.md", doc, &|_: &str| true)
        .into_iter()
        .map(|d| {
            let severity = if d.severity == Severity::Error {
                "error"
            } else {
                "warning"
            };
            (d.message, severity, doc[d.from..d.to].to_string())
        })
        .collect()
}

fn lint(doc: &str) -> Vec<(String, &'static str, String)> {
    run(doc, true)
}

fn d(message: &str, severity: &'static str, text: &str) -> (String, &'static str, String) {
    (message.to_string(), severity, text.to_string())
}

fn example(name: &str) -> String {
    let catalog: serde_json::Value = serde_json::from_str(CATALOG).unwrap();
    catalog["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == name)
        .and_then(|component| component["example"]["markdown"].as_str())
        .unwrap_or_else(|| panic!("no example for {name}"))
        .to_string()
}

#[test]
fn accepts_every_schema_example() {
    let catalog: serde_json::Value = serde_json::from_str(CATALOG).unwrap();
    for component in catalog["components"].as_array().unwrap() {
        if let Some(markdown) = component["example"]["markdown"].as_str() {
            assert_eq!(lint(markdown), vec![], "{}", component["name"]);
        }
    }
}

#[test]
fn flags_a_component_the_schema_doesnt_have() {
    assert_eq!(
        lint("::promo-code\n::"),
        vec![d(
            "There is no “promo-code” component. Check the spelling, or pick one from the Components list.",
            "error",
            "promo-code"
        )]
    );
}

#[test]
fn flags_a_missing_required_prop_on_the_components_name() {
    let doc =
        example("cta-banner").replace("to=\"https://www.catercow.com/brands/9109-chickfila\" ", "");
    assert_eq!(
        lint(&doc),
        vec![d("This component needs a `to`.", "error", "cta-banner")]
    );
}

#[test]
fn flags_a_required_prop_left_empty() {
    let doc =
        example("instagram-post").replace("url: https://www.instagram.com/p/DMd8x-TRioJ/", "url:");
    assert_eq!(
        lint(&doc),
        vec![d("This component needs a `url`.", "error", "url:")]
    );
}

#[test]
fn flags_each_list_item_missing_a_required_field() {
    let doc = example("promo-codes").replace("    code: 100OFFWAWADC\n", "");
    assert_eq!(
        lint(&doc),
        vec![d(
            "This component needs a `code` for each item.",
            "error",
            "label: Washington, DC Area"
        )]
    );
}

#[test]
fn flags_props_the_component_doesnt_use() {
    assert_eq!(
        lint("::callout{titel=\"Need every meal labeled?\"}\nBody\n::"),
        vec![d(
            "This component doesn't use `titel`, so the site ignores it.",
            "warning",
            "titel=\"Need every meal labeled?\""
        )]
    );
    let doc = example("address-cta").replace("cta: Search", "button: Search");
    assert_eq!(
        lint(&doc),
        vec![d(
            "This component doesn't use `button`, so the site ignores it.",
            "warning",
            "button: Search"
        )]
    );
}

#[test]
fn flags_item_fields_the_component_doesnt_use() {
    let doc = example("promo-codes").replace(
        "    code: 100OFFWAWANYC\n",
        "    code: 100OFFWAWANYC\n    expires: soon\n",
    );
    assert_eq!(
        lint(&doc),
        vec![d(
            "Items here don't use `expires`, so the site ignores it.",
            "warning",
            "expires: soon"
        )]
    );
}

#[test]
fn flags_values_that_dont_match_the_props_type() {
    assert_eq!(
        lint(&example("faqs").replace("structured-data: true", "structured-data: \"yes\"")),
        vec![d(
            "`structured-data` should be true or false.",
            "warning",
            "structured-data: \"yes\""
        )]
    );
    assert_eq!(
        lint(&example("bullet-list").replace("variant=\"warning\"", "variant=\"danger\"")),
        vec![d(
            "`variant` should be one of: default, warning.",
            "warning",
            "variant=\"danger\""
        )]
    );
    assert_eq!(
        lint(&example("pricing-grid").replace("price: 11.50", "price: \"11.50\"")),
        vec![d(
            "`price` should be a number.",
            "warning",
            "price: \"11.50\""
        )]
    );
    assert_eq!(
        lint(&example("promo-codes").replace("code: 100OFFWAWADC", "code: 100")),
        vec![d(
            "`code` should be text. If it is a number or true/false, put quotes around it.",
            "warning",
            "code: 100"
        )]
    );
    assert_eq!(
        lint("::bullet-list\n---\nitems: Just one line\n---\n::"),
        vec![d(
            "`items` should be a list.",
            "warning",
            "items: Just one line"
        )]
    );
    assert_eq!(
        lint("::faqs\n---\nitems:\n  - How far ahead?\n---\n::"),
        vec![d(
            "Each item in `items` needs its own fields, like `question: …`.",
            "warning",
            "How far ahead?"
        )]
    );
}

#[test]
fn reads_attribute_values_the_way_the_site_does() {
    assert_eq!(lint(&example("grid")), vec![]);
    assert_eq!(lint("::card{title=\"Tip\" number}\nBody\n::"), vec![]);
    assert_eq!(
        lint("::card{title=\"Tip\" number=\"yes\"}\nBody\n::"),
        vec![d(
            "`number` should be true or false.",
            "warning",
            "number=\"yes\""
        )]
    );
}

#[test]
fn flags_unclosed_components_and_props_blocks() {
    let callout = example("callout");
    assert_eq!(
        lint(callout.strip_suffix("\n::").unwrap()),
        vec![d(
            "This component isn't closed. Add a line with just `::` where it ends.",
            "error",
            "callout"
        )]
    );
    let grid = example("grid");
    let found = lint(grid.strip_suffix("\n:::").unwrap());
    assert_eq!(
        found[0],
        d(
            "This component isn't closed. Add a line with just `:::` where it ends.",
            "error",
            "grid"
        )
    );
    assert_eq!(
        lint("::address-cta\n---\nheadline: Hi\n::"),
        vec![d(
            "The props block isn't closed. Add a line with just `---` after the last prop.",
            "error",
            "---"
        )]
    );
}

#[test]
fn points_at_yaml_that_cant_be_read() {
    assert_eq!(
        lint("::address-cta\n---\nheadline: Hi\n  cta: Search\n---\n::"),
        vec![d(
            "The props block can't be read. Check the indentation and quotes around here.",
            "error",
            "headline: Hi"
        )]
    );
}

#[test]
fn checks_nested_and_indented_components() {
    let doc = "::page-section\n  :::card{title=\"A\" colour=\"red\"}\n  Body\n  :::\n::";
    assert_eq!(
        lint(doc),
        vec![d(
            "This component doesn't use `colour`, so the site ignores it.",
            "warning",
            "colour=\"red\""
        )]
    );
    let yaml =
        "::page-section\n  :::promo-codes\n  ---\n  items:\n    - label: A\n  ---\n  :::\n::";
    assert_eq!(
        lint(yaml),
        vec![d(
            "This component needs a `code` for each item.",
            "error",
            "label: A"
        )]
    );
}

#[test]
fn checks_only_the_structure_without_a_schema() {
    assert_eq!(
        run("::anything{foo=1}\n::\n\n::other\n", false),
        vec![d(
            "This component isn't closed. Add a line with just `::` where it ends.",
            "error",
            "other"
        )]
    );
}

#[test]
fn uses_the_hosts_messages() {
    let messages: Messages =
        serde_json::from_str(r#"{"unknownComponent": "Kein {name}"}"#).unwrap();
    let linter = Linter::new(schemas(true)).with_messages(messages);
    assert_eq!(
        linter.lint("content/test.md", "::nope\n::", &|_: &str| true)[0].message,
        "Kein nope"
    );
}
