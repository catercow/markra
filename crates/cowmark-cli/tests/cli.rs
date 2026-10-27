//! The CLI against a control server run in-process, and the built binary.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use common::{FakeApp, content_repo};
use cowmark_cli::app::{App, DATA_DIR_ENV};
use cowmark_cli::cli::{Cli, run};
use cowmark_cli::ops::Context;
use cowmark_core::git_tool::find_git;
use serde_json::{Value, json};

fn cowmark(context: &Context, args: &[&str]) -> (i32, String, String) {
    let cli = Cli::try_parse_from(std::iter::once("cowmark").chain(args.iter().copied())).unwrap();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(cli, context, &mut out, &mut err);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

fn context_for(app: &FakeApp) -> Context {
    Context {
        root_flag: None,
        cwd: None,
        app: Some(App::at(&app.data_dir)),
        ..Context::default()
    }
}

#[test]
fn commands_drive_the_running_app() {
    if find_git().is_none() {
        return;
    }
    let mut app = FakeApp::start("cli");
    let context = context_for(&app);
    let folder = app.folder.clone();

    // The folder comes from the app's ping.
    assert_eq!(context.root().unwrap().path, folder);
    assert_eq!(context.root().unwrap().source, cowmark_cli::root::Source::App);

    fs::write(
        folder.join("content/blog/hello.md"),
        "---\ntitle: Hello\ndescription: Changed\npublished_at: '2026-01-01'\n---\n",
    )
    .unwrap();
    fs::write(folder.join("content/blog/new.md"), "---\ntitle: New\n---\n").unwrap();

    let (code, out, _) = cowmark(&context, &["status"]);
    assert_eq!(code, 0);
    assert!(out.contains("on studio: 2 changed files"), "{out}");
    assert!(out.contains("content/blog/new.md"), "{out}");
    let (_, out, _) = cowmark(&context, &["status", "--json"]);
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(status["cowmark"], true);

    let (code, out, err) = cowmark(&context, &["open", "content/blog/hello.md"]);
    assert_eq!(
        (code, out.trim(), err.as_str()),
        (0, "Opened content/blog/hello.md in CowMark.", "")
    );
    assert_eq!(
        app.ui.take(),
        vec![json!({ "open": folder.join("content/blog/hello.md") })]
    );

    let (code, out, _) = cowmark(
        &context,
        &[
            "propose",
            "content/blog/new.md",
            "content/blog/hello.md",
            "-m",
            "New post",
        ],
    );
    assert_eq!(code, 0);
    assert!(out.contains("Nothing is published"), "{out}");
    assert_eq!(
        app.ui.take(),
        vec![
            json!({ "propose": ["content/blog/new.md", "content/blog/hello.md"], "message": "New post" }),
            json!({ "focus": true })
        ]
    );

    // Unchanged and outside files are refused by the app; nothing reaches the UI.
    let (code, _, err) = cowmark(
        &context,
        &["propose", "content/blog/new.md", "schema/collections.json", "-m", "x"],
    );
    assert_eq!(code, 2);
    assert!(err.contains("no changes to save in: schema/collections.json"), "{err}");
    let (code, _, err) = cowmark(&context, &["open", "/etc/hosts"]);
    assert_eq!(code, 2);
    assert!(err.contains("outside the content folder"), "{err}");
    assert!(app.ui.take().is_empty());

    let (code, out, _) = cowmark(&context, &["preview-url", "content/blog/new.md"]);
    assert_eq!((code, out.trim()), (0, "http://localhost:4100/blog/new?token=t0k"));
    let (_, out, _) = cowmark(&context, &["preview-url"]);
    assert_eq!(out.trim(), "http://localhost:4100/?token=t0k");

    assert_eq!(cowmark(&context, &["focus"]).0, 0);
    assert_eq!(cowmark(&context, &["get-latest"]).0, 0);
    assert_eq!(
        app.ui.take(),
        vec![json!({ "focus": true }), json!({ "getLatest": true })]
    );

    // The check fails on the new post's missing fields.
    let (code, out, _) = cowmark(&context, &["check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("content/blog/new.md:1:1: error [missing-field]"), "{out}");

    // Once the app quits: status falls back to git, the folder to the saved one,
    // and app commands say it isn't running (a test data dir never launches it).
    app.stop();
    assert_eq!(context.root().unwrap().source, cowmark_cli::root::Source::Saved);
    let (code, out, _) = cowmark(&context, &["status", "--json"]);
    assert_eq!(code, 0);
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (status["cowmark"].clone(), status["changes"].as_array().unwrap().len()),
        (json!(false), 2)
    );
    let (code, _, err) = cowmark(&context, &["propose", "content/blog/new.md", "-m", "x"]);
    assert_eq!(code, 2);
    assert!(err.contains("CowMark isn't running"), "{err}");
}

#[test]
fn the_binary_checks_without_the_app_and_exits_by_severity() {
    let root = content_repo("bin");
    let bin = env!("CARGO_BIN_EXE_cowmark");
    let data = common::temp_dir("bin-d");
    let check = |args: &[&str], cwd: &Path| {
        let output = Command::new(bin)
            .args(args)
            .current_dir(cwd)
            .env(DATA_DIR_ENV, &data)
            .output()
            .unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    };

    let (code, out, _) = check(&["check", "--root", root.to_str().unwrap()], &data);
    assert_eq!((code, out.trim()), (0, "1 file checked: 0 errors, 0 warnings"));

    // A warning passes unless --strict; found from the working directory.
    fs::create_dir_all(root.join("public/blog/hello")).unwrap();
    fs::write(root.join("public/blog/hello/a.png"), "png").unwrap();
    fs::write(
        root.join("content/blog/hello.md"),
        "---\ntitle: Hello\ndescription: First post\npublished_at: '2026-01-01'\n---\n\n![](/blog/hello/a.png)\n",
    )
    .unwrap();
    assert_eq!(check(&["check"], &root.join("content/blog")).0, 0);
    assert_eq!(check(&["check", "--strict"], &root).0, 1);
    assert_eq!(check(&["check", "hello.md"], &root.join("content/blog")).0, 0);

    // An error fails; --json reports it.
    fs::write(
        root.join("content/blog/bad.md"),
        "::callout\nNo props and never closed\n",
    )
    .unwrap();
    let (code, out, _) = check(&["check", "--json"], &root);
    assert_eq!(code, 1);
    let report: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["files"], 2);
    assert!(report["errors"].as_u64().unwrap() >= 1);
    assert_eq!(report["diagnostics"][0]["file"], "content/blog/bad.md");

    // No folder anywhere: a usage error, not a check failure.
    let (code, _, err) = check(&["check"], &data);
    assert_eq!(code, 2);
    assert!(err.contains("Couldn't find the content folder"), "{err}");
    let (code, _, err) = check(&["check", "--root", root.to_str().unwrap(), "../outside.md"], &data);
    assert_eq!(code, 2);
    assert!(err.contains("outside the content folder"), "{err}");

    // mcp-config prints this binary's absolute path.
    let (code, out, err) = check(&["mcp-config"], &data);
    assert_eq!(code, 0);
    let config: Value = serde_json::from_str(&out).unwrap();
    let command = config["mcpServers"]["cowmark"]["command"].as_str().unwrap();
    assert!(
        Path::new(command).is_absolute() && command.ends_with(if cfg!(windows) { "cowmark.exe" } else { "cowmark" })
    );
    assert_eq!(config["mcpServers"]["cowmark"]["args"], json!(["mcp"]));
    assert!(err.contains("claude_desktop_config.json"));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(data);
}

const TABLE_COMPONENTS: &str = r#"{"components":[{"name":"pricing-grid","props":[],"input":{"kind":"table","columns":[
  {"name":"Item","aliases":["Name"],"required":true,"markdown":true},
  {"name":"Price","aliases":["Cost"],"required":true,"format":"price"}]}}]}"#;

#[test]
fn fmt_writes_canonical_tables_and_check_fix_does_too() {
    let root = content_repo("fmt");
    fs::write(root.join("schema/components.json"), TABLE_COMPONENTS).unwrap();
    let page = root.join("content/blog/menu.md");
    let messy = "---\ntitle: Menu\ndescription: Prices\npublished_at: '2026-01-01'\n---\n\n::pricing-grid\nname,cost\n| Tacos | 7.5 USD |\n::\n";
    fs::write(&page, messy.replace("name,cost", "name | cost")).unwrap();
    let context = Context {
        root_flag: Some(root.clone()),
        cwd: None,
        app: None,
        ..Context::default()
    };

    let (code, out, _) = cowmark(&context, &["check"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("[cell-format] `Price` in row 1 will be saved as `$7.50`."),
        "{out}"
    );

    let (code, out, _) = cowmark(&context, &["fmt", "--check"]);
    assert_eq!((code, out.trim()), (1, "Would format content/blog/menu.md"));
    assert!(fs::read_to_string(&page).unwrap().contains("7.5 USD"));

    let (code, out, _) = cowmark(&context, &["fmt"]);
    assert_eq!(code, 0, "{out}");
    let formatted = fs::read_to_string(&page).unwrap();
    assert!(
        formatted.ends_with("::pricing-grid\n| Item | Price |\n| --- | --- |\n| Tacos | $7.50 |\n::\n"),
        "{formatted}"
    );
    assert_eq!(cowmark(&context, &["fmt", "--check"]).0, 0);
    let (code, out, _) = cowmark(&context, &["check"]);
    assert_eq!(code, 0, "{out}");

    fs::write(&page, messy.replace("name,cost", "| Name | Cost |\n|--|--|")).unwrap();
    let (code, out, err) = cowmark(&context, &["check", "--fix", "content/blog/menu.md"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        err.trim(),
        "cowmark: schema: the folder's schema/ files (legacy)\nFixed content/blog/menu.md"
    );
    assert_eq!(fs::read_to_string(&page).unwrap(), formatted);

    // What can't be read stays as written and fails the check.
    fs::write(&page, formatted.replace("$7.50", "TBD")).unwrap();
    assert_eq!(cowmark(&context, &["fmt", "--check"]).0, 0);
    let (code, out, _) = cowmark(&context, &["check", "--fix"]);
    assert_eq!(code, 1);
    assert!(out.contains("must be a price like $7.25 (got “TBD”)"), "{out}");
}

#[test]
fn table_turns_csv_into_markdown() {
    let root = content_repo("table");
    fs::write(root.join("schema/components.json"), TABLE_COMPONENTS).unwrap();
    let csv = root.join("prices.csv");
    fs::write(&csv, "Cost,Name\n7,\"Tacos, three\"\nsoon,Pie\n").unwrap();
    let context = Context {
        root_flag: Some(root.clone()),
        cwd: None,
        app: None,
        ..Context::default()
    };
    let path = csv.to_str().unwrap();
    let (code, out, _) = cowmark(&context, &["table", path]);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        "| Cost | Name |\n| --- | --- |\n| 7 | Tacos, three |\n| soon | Pie |\n"
    );
    let (code, out, err) = cowmark(&context, &["table", path, "--component", "pricing-grid"]);
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "::pricing-grid\n| Item | Price |\n| --- | --- |\n| Tacos, three | $7.00 |\n| Pie | soon |\n::\n"
    );
    assert_eq!(
        err.trim(),
        "Line 4: `Price` in row 2 must be a price like $7.25 (got “soon”)."
    );
    let (code, _, err) = cowmark(&context, &["table", path, "--component", "nope"]);
    assert_eq!(code, 2);
    assert!(err.contains("no component named `nope`"), "{err}");
}

/// Serves `body` at any path until the test ends.
fn serve(body: String) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/_cowmark/schema.json", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    format!("http://127.0.0.1:{port}/_cowmark/schema.json")
}

#[test]
fn check_takes_the_schema_from_the_site_then_the_cache_then_the_folder() {
    if find_git().is_none() {
        return;
    }
    let root = content_repo("schema-source");
    // The page uses a component only the site's schema has.
    fs::write(
        root.join("content/blog/hello.md"),
        "---\ntitle: Hello\ndescription: First post\npublished_at: '2026-01-01'\n---\n\n::site-only\n::\n",
    )
    .unwrap();
    let site_components = r#"{"components":[{"name":"site-only","props":[]}]}"#;
    let envelope = format!(
        r#"{{"format":1,"build":{{"revision":"abc","contentRevision":null,"generatedAt":"2026-10-27T00:00:00Z"}},"components":{site_components},"collections":{}}}"#,
        common::COLLECTIONS
    );
    let url = serve(envelope);
    let context = Context { root_flag: Some(root.clone()), ..Context::default() };

    // The folder's schema/ doesn't know the component.
    let (code, out, err) = cowmark(&context, &["check", "--offline"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("[unknown-component]"), "{out}");
    assert!(err.contains("cowmark: schema: the folder's schema/ files (legacy)"), "{err}");

    // The site's does; the source is reported.
    let (code, out, err) = cowmark(&context, &["check", "--schema-url", &url]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(err.trim(), format!("cowmark: schema: {url}"));

    // Offline afterwards: the cached copy (in .git, so nothing tracked changes).
    let (code, _, err) = cowmark(&context, &["check", "--schema-url", &url, "--offline"]);
    assert_eq!(code, 0);
    assert!(err.contains(&format!("cached copy of {url} from just now")), "{err}");
    assert!(root.join(".git/cowmark").is_dir());

    // preview.json's site is the default endpoint.
    let site = url.trim_end_matches("/_cowmark/schema.json");
    fs::write(root.join("preview.json"), format!(r#"{{"site":"{site}"}}"#)).unwrap();
    let (code, _, err) = cowmark(&context, &["check"]);
    assert_eq!((code, err.trim()), (0, format!("cowmark: schema: {url}").as_str()));
    fs::remove_file(root.join("preview.json")).unwrap();

    // An unreachable endpoint: warn and use the cache (here) or the folder's files.
    fs::remove_dir_all(root.join(".git/cowmark")).unwrap();
    let dead = dead_url();
    let (code, out, err) = cowmark(&context, &["check", "--github", "--schema-url", &dead]);
    assert_eq!(code, 1, "{out}");
    assert!(err.contains(&format!("cowmark: warning: Couldn't get the schema from {dead}")), "{err}");
    assert!(err.contains("schema/ files (legacy)"), "{err}");
    assert!(out.contains("::warning title=cowmark schema::Couldn't get the schema"), "{out}");

    // No schema anywhere: exit 2.
    fs::remove_dir_all(root.join("schema")).unwrap();
    let (code, _, err) = cowmark(&context, &["check", "--schema-url", &dead]);
    assert_eq!(code, 2);
    assert!(err.contains("No content schema: couldn't get"), "{err}");
    let (code, _, err) = cowmark(&context, &["check", "--offline"]);
    assert_eq!(code, 2);
    assert!(err.contains("no --schema-url, no preview.json"), "{err}");

    // An explicit schema folder.
    let dir = root.join("elsewhere");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("components.json"), site_components).unwrap();
    fs::write(dir.join("collections.json"), common::COLLECTIONS).unwrap();
    let (code, out, err) = cowmark(&context, &["check", "--schema", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(err.contains("elsewhere"), "{err}");
}
