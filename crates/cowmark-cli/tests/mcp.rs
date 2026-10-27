//! `cowmark mcp` over stdio, as Claude Desktop runs it, against a control
//! server run in-process.

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::FakeApp;
use cowmark_cli::app::DATA_DIR_ENV;
use cowmark_core::git_tool::find_git;
use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start(data_dir: &std::path::Path, cwd: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cowmark"))
            .arg("mcp")
            .env(DATA_DIR_ENV, data_dir)
            .env_remove("COWMARK_ROOT")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "the server closed stdout"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// A tools/call; returns (is_error, text).
    fn call(&mut self, name: &str, arguments: Value) -> (bool, String) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        let result = &reply["result"];
        assert!(result.is_object(), "{name}: {reply}");
        let text = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (result["isError"] == true, text)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn an_agent_writes_checks_and_proposes_over_stdio() {
    if find_git().is_none() {
        return;
    }
    let app = FakeApp::start("mcp");
    let folder = app.folder.clone();
    let mut session = Session::start(&app.data_dir, &app.data_dir);

    let init = session.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "cowmark");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Save to studio")
    );
    assert!(init["result"]["capabilities"]["tools"].is_object());
    session.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let tools = session.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    for expected in [
        "read_file",
        "write_file",
        "list_files",
        "rename_file",
        "delete_file",
        "list_components",
        "get_component_schema",
        "get_collection_schema",
        "new_page",
        "add_image",
        "check",
        "status",
        "get_latest",
        "open_in_cowmark",
        "preview_link",
        "propose_publish",
    ] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }
    let annotations = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap()["annotations"].clone();
    assert_eq!(annotations("read_file")["readOnlyHint"], true);
    assert_eq!(annotations("delete_file")["destructiveHint"], true);
    assert_eq!(annotations("propose_publish")["destructiveHint"], false);
    assert_eq!(annotations("propose_publish")["readOnlyHint"], false);
    let propose = tools.iter().find(|tool| tool["name"] == "propose_publish").unwrap();
    assert!(
        propose["description"]
            .as_str()
            .unwrap()
            .contains("NOTHING IS PUBLISHED")
    );
    assert_eq!(propose["inputSchema"]["required"], json!(["paths", "message"]));

    // Schemas.
    let (error, text) = session.call("get_component_schema", json!({ "name": "callout" }));
    assert!(!error && text.contains("\"tone\""), "{text}");
    let (_, text) = session.call("get_collection_schema", json!({ "name": "blog" }));
    assert!(text.contains("published_at"), "{text}");

    // Create, write and check a page.
    let (error, text) = session.call(
        "new_page",
        json!({ "collection": "blog", "title": "Holiday Party Catering" }),
    );
    assert!(!error, "{text}");
    let made: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(made["path"], "content/blog/holiday-party-catering.md");
    assert_eq!(made["fieldsToFill"], json!(["description"]));

    let (error, text) = session.call("check", json!({ "paths": ["content/blog/holiday-party-catering.md"] }));
    assert!(
        !error && text.starts_with("FAILED") && text.contains("needs a `description`"),
        "{text}"
    );

    let page = "---\ntitle: Holiday Party Catering\ndescription: Plan the office party.\npublished_at: '2026-10-26'\n---\n\n::callout\n---\ntone: info\n---\nBook early.\n::\n";
    let (error, text) = session.call(
        "write_file",
        json!({ "path": "content/blog/holiday-party-catering.md", "content": page }),
    );
    assert!(!error && text.contains("no problems found"), "{text}");
    assert_eq!(
        fs::read_to_string(folder.join("content/blog/holiday-party-catering.md")).unwrap(),
        page
    );
    let (_, text) = session.call("check", json!({}));
    assert!(text.starts_with("PASSED"), "{text}");

    // Images go in the page's folder under public/ and are referenced by site path.
    let image = folder
        .join("..")
        .join(format!("cm-mcp-photo-{}.png", std::process::id()));
    fs::write(&image, b"\x89PNG\r\n\x1a\nxxxx").unwrap();
    let (error, text) = session.call(
        "add_image",
        json!({ "page": "content/blog/holiday-party-catering.md", "source": image, "alt": "A buffet" }),
    );
    assert!(!error, "{text}");
    let added: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        added["markdown"],
        format!(
            "![A buffet](/blog/holiday-party-catering/cm-mcp-photo-{}.png)",
            std::process::id()
        )
    );
    let _ = fs::remove_file(image);

    // The guard rails.
    for (name, arguments) in [
        (
            "write_file",
            json!({ "path": "schema/components.json", "content": "{}" }),
        ),
        ("write_file", json!({ "path": "../escape.md", "content": "x" })),
        ("write_file", json!({ "path": ".git/config", "content": "x" })),
        ("delete_file", json!({ "path": "schema/collections.json" })),
        ("read_file", json!({ "path": ".git/HEAD" })),
        ("read_file", json!({ "path": "/etc/hosts" })),
    ] {
        let (error, text) = session.call(name, arguments.clone());
        assert!(error, "{name} {arguments} should fail: {text}");
    }
    assert_eq!(
        fs::read_to_string(folder.join("schema/components.json")).unwrap(),
        common::COMPONENTS
    );

    // Status, preview and the proposal go through the app.
    let (_, text) = session.call("status", json!({}));
    let status: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(status["cowmark"], true);
    let (_, text) = session.call(
        "preview_link",
        json!({ "path": "content/blog/holiday-party-catering.md" }),
    );
    assert!(
        text.starts_with("http://localhost:4100/blog/holiday-party-catering?token=t0k"),
        "{text}"
    );
    app.ui.take();

    let (error, text) = session.call(
        "propose_publish",
        json!({ "paths": ["content/blog/holiday-party-catering.md", added["path"]], "message": "New post: holiday party catering" }),
    );
    assert!(!error && text.contains("Nothing has been published"), "{text}");
    assert_eq!(
        app.ui.take(),
        vec![
            json!({ "propose": ["content/blog/holiday-party-catering.md", added["path"]], "message": "New post: holiday party catering" }),
            json!({ "focus": true })
        ]
    );
    let (error, text) = session.call(
        "propose_publish",
        json!({ "paths": ["content/blog/hello.md"], "message": "x" }),
    );
    assert!(error && text.contains("no changes"), "{text}");

    let (error, _) = session.call("get_latest", json!({}));
    assert!(!error);
    assert_eq!(app.ui.take(), vec![json!({ "getLatest": true })]);

    // Unknown tools are a protocol error.
    let reply = session.request("tools/call", json!({ "name": "publish", "arguments": {} }));
    assert!(reply["error"].is_object(), "{reply}");
}
