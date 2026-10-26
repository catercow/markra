//! Compares the component checks with the editor's TypeScript linter on a corpus it produced:
//! a folder with `index.json` (document names), `docs/<i>.md` and `ts.json` (`lintMdcDocument`
//! output per document, UTF-16 offsets), plus `components.json`. Skipped unless
//! `COWMARK_TS_CORPUS` names that folder.

use std::collections::BTreeMap;

use cowmark_lint::{Linter, Rule, Schemas, Severity};
use serde_json::Value;

const COMPONENT_RULES: [Rule; 9] = [
    Rule::UnknownComponent,
    Rule::MissingProp,
    Rule::MissingItemField,
    Rule::UnknownProp,
    Rule::UnknownItemField,
    Rule::PropType,
    Rule::UnclosedComponent,
    Rule::UnclosedProps,
    Rule::PropsYaml,
];

#[test]
fn matches_the_typescript_linter() {
    let Ok(dir) = std::env::var("COWMARK_TS_CORPUS") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let names: Vec<String> = serde_json::from_str(&read("index.json")).unwrap();
    let expected: BTreeMap<String, Vec<Value>> = serde_json::from_str(&read("ts.json")).unwrap();
    let linter = Linter::new(Schemas::from_json(Some(&read("components.json")), None).unwrap());

    let mut differences = 0;
    let mut yaml_positions = 0;
    let mut total = 0;
    for (i, name) in names.iter().enumerate() {
        let text = read(&format!("docs/{i}.md"));
        let path = name.split('#').next().unwrap();
        let ours: Vec<(usize, usize, &str, String)> = linter
            .lint(path, &text, &|_: &str| true)
            .into_iter()
            .filter(|d| COMPONENT_RULES.contains(&d.rule))
            .map(|d| {
                (
                    d.from_utf16,
                    d.to_utf16,
                    if d.severity == Severity::Error {
                        "error"
                    } else {
                        "warning"
                    },
                    d.message,
                )
            })
            .collect();
        let theirs: Vec<(usize, usize, &str, String)> = expected[&i.to_string()]
            .iter()
            .map(|d| {
                (
                    d["from"].as_u64().unwrap() as usize,
                    d["to"].as_u64().unwrap() as usize,
                    d["severity"].as_str().unwrap(),
                    d["message"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        total += theirs.len();
        // YAML error positions are each parser's own: the npm parser blames the line after an
        // unterminated quote, saphyr the line with the quote. Same message, same block.
        let yaml_message = linter.messages.invalid_yaml.as_str();
        let strip = |list: &[(usize, usize, &str, String)]| -> Vec<(usize, usize, String)> {
            list.iter()
                .map(|(from, to, _, message)| {
                    if message == yaml_message {
                        (0, 0, message.clone())
                    } else {
                        (*from, *to, message.clone())
                    }
                })
                .collect()
        };
        if ours != theirs && strip(&ours) == strip(&theirs) {
            yaml_positions += 1;
            continue;
        }
        if ours != theirs {
            differences += 1;
            eprintln!("--- {name} (docs/{i}.md)\n  ts:   {theirs:?}\n  rust: {ours:?}");
        }
    }
    eprintln!(
        "{} documents, {total} TypeScript diagnostics, {differences} documents differ, {yaml_positions} differ only in where unreadable YAML is pointed at",
        names.len()
    );
    assert_eq!(differences, 0);
}
