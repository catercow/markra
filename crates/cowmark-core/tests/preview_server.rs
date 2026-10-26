//! The browser preview server over real sockets: token, Host and bind checks,
//! the pending/media endpoints and the SSE relay of saved files.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cowmark_core::git_tool::find_git;
use cowmark_core::preview_server::{PreviewServer, PreviewServerOptions};

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cowmark-preview-server-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("content/blog")).unwrap();
    fs::create_dir_all(root.join("public/blog")).unwrap();
    root
}

fn start(root: &Path) -> PreviewServer {
    PreviewServer::start(PreviewServerOptions {
        root: root.to_path_buf(),
        site: "https://preview.example.com/some/page".into(),
        port: 0,
    })
    .unwrap()
}

struct Response {
    status: u16,
    head: String,
    body: Vec<u8>,
}

fn get(port: u16, target: &str, host: &str) -> Response {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(stream, "GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    Response { status, head, body: raw[split + 4..].to_vec() }
}

fn git(root: &Path, args: &[&str]) {
    let binary = find_git().expect("git");
    let status = Command::new(binary).args(args).current_dir(root).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn binds_loopback_only() {
    let root = temp_root("bind");
    let server = start(&root);
    assert_eq!(server.local_addr().ip(), Ipv4Addr::LOCALHOST);
    // A non-loopback address of this machine, if it has one, must not reach the server.
    let lan = UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.0.2.1:9").map(|_| socket))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.ip());
    if let Ok(ip) = lan {
        if !ip.is_loopback() && !ip.is_unspecified() {
            let address = SocketAddr::new(ip, server.port());
            assert!(TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_err());
        }
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_busy_port_is_reported_not_replaced() {
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let error = PreviewServer::start(PreviewServerOptions {
        root: temp_root("busy"),
        site: "https://preview.example.com".into(),
        port,
    })
    .err()
    .expect("should fail");
    assert!(error.starts_with("port_busy:"), "{error}");
}

#[test]
fn requires_the_token_and_this_host() {
    let root = temp_root("token");
    let server = start(&root);
    let port = server.port();
    let host = format!("localhost:{port}");
    let token = server.token().to_string();

    assert_eq!(get(port, "/blog/a", &host).status, 403);
    assert_eq!(get(port, "/blog/a?token=nope", &host).status, 403);
    assert_eq!(get(port, &format!("/__cowmark/pending?token={}", &token[1..]), &host).status, 403);
    assert_eq!(get(port, "/__cowmark/events", &host).status, 403);
    assert_eq!(get(port, &format!("/blog/a?token={token}"), "evil.example:4100").status, 421);
    assert_eq!(get(port, &format!("/blog/a?token={token}"), &format!("127.0.0.1:{port}")).status, 200);

    let page = get(port, &format!("/blog/a?x=1&token={token}"), &host);
    assert_eq!(page.status, 200);
    assert!(page.head.contains("frame-src https://preview.example.com;"), "{}", page.head);
    assert!(page.head.contains("Referrer-Policy: no-referrer"));
    assert!(!page.head.to_ascii_lowercase().contains("access-control-allow-origin"));
    let body = String::from_utf8(page.body).unwrap();
    assert!(body.contains(r#""frame":"https://preview.example.com/blog/a?x=1""#), "{body}");
    assert!(body.contains(r#""origin":"https://preview.example.com""#));
    assert!(!body.contains(&token), "the token must not be written into the page");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn serves_pending_changes_and_media() {
    let root = temp_root("pending");
    if find_git().is_none() {
        return;
    }
    git(&root, &["init", "--quiet", "-b", "studio"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    fs::write(root.join("content/blog/a.md"), "a").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "base"]);
    fs::write(root.join("content/blog/a.md"), "a2").unwrap();
    fs::write(root.join("public/blog/a.png"), [137u8, 80, 78, 71]).unwrap();

    let server = start(&root);
    let (port, token) = (server.port(), server.token().to_string());
    let host = format!("localhost:{port}");
    let pending = get(port, &format!("/__cowmark/pending?commit=&token={token}"), &host);
    assert_eq!(pending.status, 200);
    let json: serde_json::Value = serde_json::from_slice(&pending.body).unwrap();
    assert_eq!(json["base"], "studio");
    let events = json["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["kind"] == "draft" && event["fsPath"] == "blog/a.md" && event["content"] == "a2"));
    assert!(events.iter().any(|event| event["kind"] == "media" && event["path"] == "/blog/a.png"));

    let media = get(port, &format!("/__cowmark/media?path=%2Fblog%2Fa.png&token={token}"), &host);
    assert_eq!(media.status, 200);
    assert!(media.head.contains("Content-Type: image/png"));
    assert_eq!(media.body, vec![137u8, 80, 78, 71]);
    assert_eq!(get(port, &format!("/__cowmark/media?path=%2F..%2Fsecret.png&token={token}"), &host).status, 404);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn relays_saved_files_over_server_sent_events() {
    let root = temp_root("sse");
    fs::write(root.join("content/blog/a.md"), "one").unwrap();
    let server = start(&root);
    let port = server.port();

    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(stream, "GET /__cowmark/events?token={} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n", server.token()).unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.starts_with("HTTP/1.1 200"), "{line}");
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        assert!(!line.to_ascii_lowercase().starts_with("access-control"), "{line}");
    }

    // The first poll after a page connects records the folder; later saves are sent.
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.subscriber_count() == 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(1500));
    fs::write(root.join("content/blog/a.md"), "two, longer").unwrap();

    let data = loop {
        assert!(Instant::now() < deadline, "no event arrived");
        line.clear();
        reader.read_line(&mut line).unwrap();
        if let Some(data) = line.strip_prefix("data: ") {
            break data.trim().to_string();
        }
    };
    let message: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert_eq!(
        message["events"],
        serde_json::json!([{ "kind": "draft", "fsPath": "blog/a.md", "content": "two, longer" }])
    );
    drop(server);
    let _ = fs::remove_dir_all(&root);
}
