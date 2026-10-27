//! Raw HTML, scripting links and event handler attributes: what the site strips must be an
//! error here, and ordinary Markdown and MDC that looks a little like it must not be.

use cowmark_lint::{Linter, Rule, Schemas, Severity};

const FRONT: &str = "---\ntitle: T\ndescription: D\npublished_at: '2026-10-06'\n---\n\n";

fn found(body: &str) -> Vec<(Rule, String)> {
    let doc = format!("{FRONT}{body}\n");
    let linter = Linter::new(
        Schemas::from_json(
            Some(include_str!("fixtures/site-components.json")),
            Some(include_str!("fixtures/collections.json")),
        )
        .unwrap(),
    );
    linter
        .lint("content/blog/my-post.md", &doc, &|_: &str| true)
        .into_iter()
        .filter(|d| {
            matches!(
                d.rule,
                Rule::RawHtml | Rule::UnsafeLink | Rule::UnsafeAttribute
            )
        })
        .inspect(|d| assert_eq!(d.severity, Severity::Error))
        .map(|d| (d.rule, doc[d.from..d.to].to_string()))
        .collect()
}

fn html(text: &str) -> (Rule, String) {
    (Rule::RawHtml, text.into())
}

#[test]
fn flags_html_blocks_once_each() {
    assert_eq!(
        found("Intro.\n\n<div class=\"x\">\n  <p>Hi</p>\n</div>\n\nAfter."),
        [html("<div class=\"x\">\n  <p>Hi</p>\n</div>")]
    );
    assert_eq!(found("<!-- note -->"), [html("<!-- note -->")]);
    assert_eq!(
        found("<script>\nalert(1)\n\n</script>"),
        [html("<script>\nalert(1)\n\n</script>")]
    );
    assert_eq!(found("<?php echo 1 ?>"), [html("<?php echo 1 ?>")]);
    assert_eq!(found("<!DOCTYPE html>"), [html("<!DOCTYPE html>")]);
    assert_eq!(found("<![CDATA[ x ]]>"), [html("<![CDATA[ x ]]>")]);
    assert_eq!(found("<custom-tag>\n"), [html("<custom-tag>")]);
    assert_eq!(
        found("- item\n\n  <div>in a list</div>"),
        [html("<div>in a list</div>")]
    );
}

#[test]
fn flags_inline_html_tag_by_tag() {
    assert_eq!(
        found("Some <b>bold</b> and <br/> text <!-- c --> <img src=x onerror=alert(1)>."),
        [
            html("<b>"),
            html("</b>"),
            html("<br/>"),
            html("<!-- c -->"),
            html("<img src=x onerror=alert(1)>")
        ]
    );
    assert_eq!(
        found("Inside **<span>em</span>**"),
        [html("<span>"), html("</span>")]
    );
}

#[test]
fn flags_html_inside_component_content() {
    assert_eq!(
        found("::callout\nHello <em>you</em>\n::"),
        [html("<em>"), html("</em>")]
    );
}

#[test]
fn leaves_text_that_is_not_html() {
    for body in [
        "Feeds less than <5 people, or a < b and b > c.",
        "Visit <https://www.catercow.com> or <mailto:a@b.co> or <hello@catercow.com>.",
        "Escaped \\<div> stays text.",
        "Code `<div>` stays code.",
        "```html\n<div>fenced</div>\n```",
        "~~~\n<div>fenced</div>\n~~~",
        "Para\n\n    <div>indented code</div>",
        "[a link](/blog/other-post \"Title <https://x.com>\")",
        "::callout{type=\"note\"}\nBody with :badge[New]{color=\"green\"} and [span]{.big}\n::",
        "::cta-banner\n---\nto: /get-started\ncta: \"<b>not html in yaml</b>\"\n---\n::",
        "Price is 2<3 and x <- y.",
    ] {
        assert_eq!(found(body), [], "{body}");
    }
}

#[test]
fn front_matter_is_not_checked_for_html() {
    let doc = "---\ntitle: <b>T</b>\ndescription: D\npublished_at: '2026-10-06'\n---\n\nBody.\n";
    let linter = Linter::new(Schemas::from_json(None, None).unwrap());
    assert!(
        linter
            .lint("content/blog/my-post.md", doc, &|_: &str| true)
            .iter()
            .all(|d| d.rule != Rule::RawHtml)
    );
}

#[test]
fn flags_scripting_links() {
    let link = |text: &str| (Rule::UnsafeLink, text.to_string());
    assert_eq!(
        found("[x](javascript:alert(1))"),
        [link("[x](javascript:alert(1))")]
    );
    assert_eq!(
        found("[x](JAVA&#x09;SCRIPT:alert(1))"),
        [link("[x](JAVA&#x09;SCRIPT:alert(1))")]
    );
    assert_eq!(
        found("![x](data:image/png;base64,AA)"),
        [link("![x](data:image/png;base64,AA)")]
    );
    assert_eq!(
        found("<javascript:alert(1)>"),
        [link("<javascript:alert(1)>")]
    );
    assert_eq!(found("[x][r]\n\n[r]: vbscript:x"), [link("[x][r]")]);
    let safe = "[a](/blog/other-post) [b](https://x.com) [c](mailto:a@b.co) [d](tel:+15555550100) [e](#top) [f](other)";
    assert_eq!(found(safe), []);
}

#[test]
fn flags_unsafe_component_attributes_and_props() {
    let attr = |text: &str| (Rule::UnsafeAttribute, text.to_string());
    let link = |text: &str| (Rule::UnsafeLink, text.to_string());
    assert_eq!(
        found("::cta-banner{to=\"java\tscript:alert(1)\" cta=\"Go\" onclick=\"x()\"}\n::"),
        [
            link("to=\"java\tscript:alert(1)\""),
            attr("onclick=\"x()\"")
        ]
    );
    assert_eq!(
        found(
            "::cta-banner{to=\"/a\" cta=\"Go\" @click=\"x\" v-on:mouseover=\"y\" :v-bind='{}'}\n::"
        ),
        [
            attr("@click=\"x\""),
            attr("v-on:mouseover=\"y\""),
            attr(":v-bind='{}'")
        ]
    );
    assert_eq!(
        found("::cta-banner{:to='\"javascript:x\"' cta=\"Go\"}\n::"),
        [link(":to='\"javascript:x\"'")]
    );
    assert_eq!(
        found(
            "::cta-banner\n---\nto: javascript:alert(1)\ncta: Go\nonClick: x\nitems:\n  - link: 'data:text/html,x'\n---\n::"
        ),
        [
            link("javascript:alert(1)"),
            attr("onClick"),
            link("'data:text/html,x'")
        ]
    );
    assert_eq!(
        found(
            "A [span]{href=\"javascript:x\"} and :badge{srcdoc=\"<p>\"} and **b**{onmouseover=\"y\"}."
        ),
        [
            link("href=\"javascript:x\""),
            attr("srcdoc=\"<p>\""),
            attr("onmouseover=\"y\"")
        ]
    );
}

#[test]
fn leaves_safe_attributes_and_plain_braces() {
    for body in [
        "::cta-banner{to=\"https://www.catercow.com/a\" cta=\"Go\"}\n::",
        "::cta-banner{to=\"/get-started\" cta=\"Go\"}\n::",
        "Plain braces {onclick=\"x\"} after a space are text.",
        "Inside code `{onclick=\"x\"}` braces are text.",
    ] {
        assert_eq!(found(body), [], "{body}");
    }
    // After a code span they are attributes.
    assert_eq!(
        found("Code `x`{onclick}"),
        [(Rule::UnsafeAttribute, "onclick".to_string())]
    );
}
