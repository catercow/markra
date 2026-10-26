//! Front matter, image, link and merge conflict checks, against the content repository's real
//! schemas (copied to tests/fixtures) and an in-memory set of files.

use std::collections::HashSet;

use cowmark_lint::{Diagnostic, FileLookup, Linter, Rule, Schemas, Severity};

struct Files(HashSet<String>);

impl Files {
    fn new(paths: &[&str]) -> Files {
        let mut all = HashSet::new();
        for path in paths {
            let mut prefix = String::new();
            for part in path.split('/') {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                all.insert(prefix.clone());
            }
        }
        Files(all)
    }
}

impl FileLookup for Files {
    fn exists(&self, path: &str) -> bool {
        self.0.contains(path)
    }
}

fn linter() -> Linter {
    Linter::new(
        Schemas::from_json(
            Some(include_str!("fixtures/site-components.json")),
            Some(include_str!("fixtures/collections.json")),
        )
        .unwrap(),
    )
}

fn files() -> Files {
    Files::new(&[
        "content/blog/my-post.md",
        "content/blog/my-post/photo.jpg",
        "content/blog/other-post.md",
        "content/cuisines/thai.md",
        "public/blog/old-post/banner.png",
    ])
}

fn lint(path: &str, doc: &str) -> Vec<(Rule, Severity, String, String)> {
    linter()
        .lint(path, doc, &files())
        .into_iter()
        .map(|d| (d.rule, d.severity, d.message, doc[d.from..d.to].to_string()))
        .collect()
}

fn rules(path: &str, doc: &str) -> Vec<(Rule, String)> {
    lint(path, doc)
        .into_iter()
        .map(|(rule, _, _, text)| (rule, text))
        .collect()
}

const POST: &str = "---\ntitle: Breakfast catering\ndescription: Ideas for the morning.\nfeature_image: my-post/photo.jpg\npublished_at: '2026-10-06'\ncategory: ordering-catering\n---\n\nBody.\n";

fn post(front: &str, body: &str) -> String {
    format!("---\n{front}---\n\n{body}\n")
}

#[test]
fn accepts_a_correct_post() {
    assert_eq!(lint("content/blog/my-post.md", POST), vec![]);
}

#[test]
fn flags_missing_and_empty_required_fields() {
    let doc = post("description: d\npublished_at: '2026-10-06'\n", "");
    assert_eq!(
        lint("content/blog/my-post.md", &doc),
        vec![(
            Rule::MissingField,
            Severity::Error,
            "This page needs a `title`.".into(),
            "---".into()
        )]
    );
    let doc = post("title: ''\ndescription:\npublished_at: '2026-10-06'\n", "");
    assert_eq!(
        rules("content/blog/my-post.md", &doc),
        vec![
            (Rule::MissingField, "title: ''".into()),
            (Rule::MissingField, "description:".into())
        ]
    );
    let doc = "Just a body.\n";
    assert_eq!(
        rules("content/blog/my-post.md", doc)
            .into_iter()
            .map(|(rule, _)| rule)
            .collect::<Vec<_>>(),
        [Rule::MissingField; 3]
    );
}

#[test]
fn flags_unknown_fields_types_dates_and_options() {
    let doc = post(
        "title: T\ndescription: D\npublished_at: 2026-13-01\ncategory: news\nauthor: Sam\n",
        "",
    );
    assert_eq!(
        lint("content/blog/my-post.md", &doc),
        vec![
            (
                Rule::DateFormat,
                Severity::Error,
                "`published_at` should be a date written as YYYY-MM-DD, like 2026-10-06.".into(),
                "published_at: 2026-13-01".into()
            ),
            (
                Rule::FieldType,
                Severity::Error,
                "`category` should be one of: ordering-catering, company-culture, caterer-profile, catering-promotions.".into(),
                "category: news".into()
            ),
            (
                Rule::UnknownField,
                Severity::Warning,
                "Pages here don't use `author`, so the site ignores it.".into(),
                "author: Sam".into()
            ),
        ]
    );
}

#[test]
fn checks_lists_of_items() {
    let doc = "---\ntitle: Arepas\nhero_image: /images/dishes/arepas.jpg\npackage_template_id: '7'\npackage_category: arepas\nmain_description: M\nfaqs:\n  - question: Q?\n  - question: R?\n    answer: A\n    extra: x\ningredients:\n  - category: Arepas\n    items:\n      - Domino\n      - Meat Only: BBQ\n---\n";
    assert_eq!(
        lint("content/dishes/arepas.md", doc),
        vec![
            (
                Rule::FieldType,
                Severity::Error,
                "`package_template_id` should be a number.".into(),
                "package_template_id: '7'".into()
            ),
            (
                Rule::MissingField,
                Severity::Error,
                "Each item in `faqs` is missing `answer`.".into(),
                "question: Q?".into()
            ),
            (
                Rule::UnknownField,
                Severity::Warning,
                "Items in `faqs` don't use `extra`, so the site ignores it.".into(),
                "extra: x".into()
            ),
            (
                Rule::FieldType,
                Severity::Error,
                "`items` should be text. If it is a number or true/false, put quotes around it."
                    .into(),
                "Meat Only: BBQ".into()
            ),
        ]
    );
    assert_eq!(
        rules(
            "content/cuisines/thai.md",
            "---\ntitle: Thai\nsubtitle:\n---\n"
        ),
        vec![(Rule::EmptyField, "subtitle:".into())]
    );
}

#[test]
fn flags_unreadable_and_unclosed_front_matter() {
    assert_eq!(
        rules(
            "content/blog/my-post.md",
            "---\ntitle: A\n  description: B\npublished_at: '2026-10-06'\n---\n"
        ),
        vec![(Rule::FrontmatterYaml, "title: A".into())]
    );
    assert_eq!(
        rules("content/blog/my-post.md", "---\ntitle: A\n"),
        vec![(Rule::FrontmatterUnclosed, "---".into())]
    );
}

#[test]
fn checks_images_next_to_the_page_and_in_public() {
    let doc = post(
        "title: T\ndescription: D\npublished_at: '2026-10-06'\nfeature_image: my-post/missing.jpg\n",
        "![A photo](my-post/photo.jpg)\n\n![Old](/blog/old-post/banner.png)\n\n![Gone](/blog/old-post/gone.png)\n\n![From the app](/images/x.jpg)\n\n![](https://example.com/a.png)\n\n`![Code](nope.jpg)`",
    );
    assert_eq!(
        lint("content/blog/my-post.md", &doc),
        vec![
            (
                Rule::MissingImage,
                Severity::Error,
                "The image `my-post/missing.jpg` can't be found. Check the file name, or add the image again.".into(),
                "my-post/missing.jpg".into()
            ),
            (
                Rule::MissingImage,
                Severity::Error,
                "The image `/blog/old-post/gone.png` can't be found. Check the file name, or add the image again.".into(),
                "/blog/old-post/gone.png".into()
            ),
            (
                Rule::ImageAlt,
                Severity::Warning,
                "This image has no description. Add one between the square brackets: it is shown as the caption and read aloud to people who can't see the image.".into(),
                "![](https://example.com/a.png)".into()
            ),
        ]
    );
}

#[test]
fn matches_image_names_exactly() {
    let doc = post(
        "title: T\ndescription: D\npublished_at: '2026-10-06'\n",
        "![A](my-post/Photo.JPG)",
    );
    assert_eq!(
        rules("content/blog/my-post.md", &doc),
        vec![(Rule::MissingImage, "my-post/Photo.JPG".into())]
    );
    let doc = post(
        "title: T\ndescription: D\npublished_at: '2026-10-06'\n",
        "![A](../../x.jpg)",
    );
    assert_eq!(
        rules("content/blog/my-post.md", &doc),
        vec![(Rule::MissingImage, "../../x.jpg".into())]
    );
}

#[test]
fn checks_images_in_component_props() {
    let doc = post(
        "title: T\ndescription: D\npublished_at: '2026-10-06'\n",
        "::logo-strip\n---\npreset: custom\nlogos:\n  - src: my-post/photo.jpg\n    alt: A\n  - src: my-post/logo.png\n    alt: B\n---\n::\n\n::video-block{poster=\"/blog/old-post/nope.png\"}\n::",
    );
    assert_eq!(
        rules("content/blog/my-post.md", &doc),
        vec![
            (Rule::MissingImage, "my-post/logo.png".into()),
            (
                Rule::MissingImage,
                "poster=\"/blog/old-post/nope.png\"".into()
            ),
        ]
    );
}

#[test]
fn checks_links_to_content_pages_only() {
    let doc = post(
        "title: T\ndescription: D\npublished_at: '2026-10-06'\n",
        "[a](/blog/other-post) [b](https://www.catercow.com/blog/nope?x=1) [c](/packages/123)\n[d](https://catercow.com/cuisines/thai-catering) [e](/cuisines/greek-catering/)\n[f](other-post) [g](gone) [h](https://example.com/blog/nope) `[i](/blog/code)`\n<https://www.catercow.com/dishes/pizza>\n\n::cta-banner{to=\"https://www.catercow.com/blog/missing\" cta=\"Go\"}\n::",
    );
    assert_eq!(
        lint("content/blog/my-post.md", &doc)
            .into_iter()
            .map(|(rule, severity, message, text)| {
                assert_eq!((rule, severity), (Rule::BrokenLink, Severity::Warning));
                (message, text)
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "There is no page at `/blog/nope`. Check the link: the page may have been renamed or removed.".to_string(),
                "https://www.catercow.com/blog/nope?x=1".to_string()
            ),
            (
                "There is no page at `/cuisines/greek-catering`. Check the link: the page may have been renamed or removed.".into(),
                "/cuisines/greek-catering/".into()
            ),
            (
                "There is no page at `/blog/gone`. Check the link: the page may have been renamed or removed.".into(),
                "gone".into()
            ),
            (
                "There is no page at `/dishes/pizza`. Check the link: the page may have been renamed or removed.".into(),
                "https://www.catercow.com/dishes/pizza".into()
            ),
            (
                "There is no page at `/blog/missing`. Check the link: the page may have been renamed or removed.".into(),
                "to=\"https://www.catercow.com/blog/missing\"".into()
            ),
        ]
    );
}

#[test]
fn flags_each_merge_conflict_once() {
    let doc = "Intro\n<<<<<<< yours\nOur text\n=======\nTheir text\n>>>>>>> theirs\n\nHeading\n=======\n\n<<<<<<< HEAD\nunfinished";
    let found = rules("content/notes.md", doc);
    assert_eq!(
        found,
        vec![
            (
                Rule::UnresolvedConflict,
                "<<<<<<< yours\nOur text\n=======\nTheir text\n>>>>>>> theirs".into()
            ),
            (Rule::UnresolvedConflict, "<<<<<<< HEAD\nunfinished".into()),
        ]
    );
    assert_eq!(
        lint("content/notes.md", doc)[0].2,
        "This part still has two versions from a merge. Choose one (or keep both) before saving."
    );
}

#[test]
fn reports_utf16_offsets_for_javascript() {
    let doc = "Mariano’s 🎉 [menu](/blog/nope)\n";
    let found: Vec<Diagnostic> = linter().lint("content/blog/my-post.md", doc, &files());
    let link = found.iter().find(|d| d.rule == Rule::BrokenLink).unwrap();
    let js: Vec<u16> = doc.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&js[link.from_utf16..link.to_utf16]).unwrap(),
        "/blog/nope"
    );
    assert_eq!(&doc[link.from..link.to], "/blog/nope");
    assert_eq!((link.line, link.column), (1, 20));
    assert_eq!(link.from - link.from_utf16, 4);

    let json = serde_json::to_value(link).unwrap();
    assert_eq!(json["rule"], "broken-link");
    assert_eq!(json["severity"], "warning");
    assert_eq!(json["fromUtf16"], link.from_utf16);
}
