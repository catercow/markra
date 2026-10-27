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
