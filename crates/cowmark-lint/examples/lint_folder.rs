//! Lints every Markdown file under `<repo>/content` and prints the problems and a count per rule.
//!
//!     cargo run --example lint_folder -- <content repo> [--json] [--rule <id>]

use std::collections::BTreeMap;
use std::path::Path;

use cowmark_lint::fs::FsLookup;
use cowmark_lint::{Linter, Schemas, Severity};

fn markdown_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first().map(Path::new) else {
        eprintln!("usage: lint_folder <content repo> [--json] [--rule <id>]");
        std::process::exit(2);
    };
    let json = args.iter().any(|arg| arg == "--json");
    let only = args
        .iter()
        .position(|arg| arg == "--rule")
        .and_then(|i| args.get(i + 1));

    let read = |name: &str| std::fs::read_to_string(root.join("schema").join(name)).ok();
    let schemas = Schemas::from_json(
        read("components.json").as_deref(),
        read("collections.json").as_deref(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let linter = Linter::new(schemas);
    let files = FsLookup::cached(root);

    let mut paths = Vec::new();
    markdown_files(&root.join("content"), &mut paths);
    let mut counts: BTreeMap<(&'static str, &'static str), usize> = BTreeMap::new();
    let mut all = Vec::new();
    for path in &paths {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).unwrap();
        for diagnostic in linter.lint(&relative, &text, &files) {
            if only.is_some_and(|rule| rule != diagnostic.rule.id()) {
                continue;
            }
            let severity = if diagnostic.severity == Severity::Error {
                "error"
            } else {
                "warning"
            };
            *counts.entry((diagnostic.rule.id(), severity)).or_default() += 1;
            if !json {
                println!(
                    "{}:{}:{} {severity} [{}] {}  «{}»",
                    diagnostic.file,
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.rule,
                    diagnostic.message,
                    text[diagnostic.from..diagnostic.to]
                        .lines()
                        .next()
                        .unwrap_or("")
                );
            }
            all.push(diagnostic);
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&all).unwrap());
        return;
    }
    eprintln!("\n{} files", paths.len());
    for ((rule, severity), count) in counts {
        eprintln!("{count:>6}  {severity:<7} {rule}");
    }
}
