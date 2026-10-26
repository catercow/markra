//! The browser preview: while CowMark is open, http://localhost:4100 serves a page
//! that frames the site and relays the same `catercow:draft*` messages as the
//! app's preview panel. The site only accepts drafts from origins on its
//! allowlist, which names `http://localhost:4100` exactly, so the port is fixed
//! and URLs always say `localhost`.
//!
//! Every request needs the per-launch token in `?token=`. The listener binds
//! 127.0.0.1 only, and the Host header must name this server, which stops
//! DNS-rebinding pages. No CORS headers are sent, so other sites can't read a
//! response even if they guess a URL.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use reqwest::Url;

use crate::preview::{self, Poller, PreviewEvent};
use crate::token;

pub const DEFAULT_PORT: u16 = 4100;
const API_PREFIX: &str = "/__cowmark/";
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const KEEPALIVE: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HEAD_BYTES: usize = 16 * 1024;
const PAGE: &str = include_str!("preview_page.html");

pub struct PreviewServerOptions {
    /// The content folder.
    pub root: PathBuf,
    /// The preview site, e.g. `https://content-preview.catercow.com`.
    pub site: String,
    /// [`DEFAULT_PORT`] in the app; 0 picks a free port (tests).
    pub port: u16,
}

struct Shared {
    address: SocketAddr,
    root: PathBuf,
    origin: String,
    token: String,
    port: u16,
    stop: AtomicBool,
    subscribers: Mutex<Vec<Sender<String>>>,
    poller: Poller,
}

pub struct PreviewServer {
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
}

impl PreviewServer {
    /// Binds 127.0.0.1:`port`. Fails with `port_busy:` when anything already
    /// answers on that port, on IPv4 or IPv6, since the browser could reach it
    /// instead of us.
    pub fn start(options: PreviewServerOptions) -> Result<Self, String> {
        let site = Url::parse(&options.site).map_err(|error| format!("invalid preview site: {error}"))?;
        if !matches!(site.scheme(), "http" | "https") {
            return Err("invalid preview site: not http(s)".into());
        }
        let origin = site.origin().ascii_serialization();
        if options.port != 0 && port_answers(options.port) {
            return Err(port_busy(options.port));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, options.port)).map_err(|error| {
            if error.kind() == ErrorKind::AddrInUse {
                port_busy(options.port)
            } else {
                error.to_string()
            }
        })?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let port = address.port();
        let shared = Arc::new(Shared {
            address,
            root: options.root,
            origin,
            token: token::random_token()?,
            port,
            stop: AtomicBool::new(false),
            subscribers: Mutex::new(Vec::new()),
            poller: Poller::default(),
        });

        let poll_shared = shared.clone();
        thread::spawn(move || poll_loop(&poll_shared));
        let accept_shared = shared.clone();
        let accept = thread::spawn(move || {
            for stream in listener.incoming() {
                if accept_shared.stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let shared = accept_shared.clone();
                thread::spawn(move || handle(&shared, stream));
            }
        });
        Ok(Self { shared, accept: Some(accept) })
    }

    pub fn port(&self) -> u16 {
        self.shared.port
    }

    /// The bound address: always 127.0.0.1.
    pub fn local_addr(&self) -> SocketAddr {
        self.shared.address
    }

    pub fn root(&self) -> &Path {
        &self.shared.root
    }

    pub fn token(&self) -> &str {
        &self.shared.token
    }

    /// `http://localhost:<port>`.
    pub fn origin(&self) -> String {
        format!("http://localhost:{}", self.shared.port)
    }

    /// The browser URL showing the site's `page_path` (e.g. `/blog/my-post`).
    pub fn page_url(&self, page_path: &str) -> String {
        let mut url = Url::parse(&self.origin()).expect("valid origin");
        let path = if page_path.starts_with('/') && !page_path.starts_with(API_PREFIX) { page_path } else { "/" };
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        url.set_path(path);
        url.set_query(None);
        {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in Url::parse(&format!("http://x/?{query}")).expect("valid query").query_pairs() {
                if key != "token" {
                    pairs.append_pair(&key, &value);
                }
            }
            pairs.append_pair("token", &self.shared.token);
        }
        url.into()
    }

    /// Number of browser pages listening for saves.
    pub fn subscriber_count(&self) -> usize {
        self.shared.subscribers.lock().map(|list| list.len()).unwrap_or(0)
    }

    /// Sends `events` to every open browser page now, as the poller does.
    pub fn broadcast(&self, events: &[PreviewEvent]) {
        broadcast(&self.shared, events);
    }
}

impl Drop for PreviewServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Wakes the blocking accept so the listener closes and frees the port.
        let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, self.shared.port)), IO_TIMEOUT);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        if let Ok(mut subscribers) = self.shared.subscribers.lock() {
            subscribers.clear();
        }
    }
}

fn port_busy(port: u16) -> String {
    format!("port_busy: Port {port} is already in use by another program, so the browser preview is off.")
}

fn port_answers(port: u16) -> bool {
    let timeout = Duration::from_millis(250);
    [SocketAddr::from((Ipv4Addr::LOCALHOST, port)), SocketAddr::from((Ipv6Addr::LOCALHOST, port))]
        .iter()
        .any(|address| TcpStream::connect_timeout(address, timeout).is_ok())
}

fn poll_loop(shared: &Shared) {
    while !shared.stop.load(Ordering::SeqCst) {
        thread::sleep(POLL_INTERVAL);
        let listening = shared.subscribers.lock().map(|list| !list.is_empty()).unwrap_or(false);
        if !listening {
            // The next page re-baselines, after asking for pending changes itself.
            let _ = shared.poller.stop(&shared.root);
            continue;
        }
        if let Ok(events) = shared.poller.poll(&shared.root) {
            broadcast(shared, &events);
        }
    }
}

fn broadcast(shared: &Shared, events: &[PreviewEvent]) {
    if events.is_empty() {
        return;
    }
    let Ok(message) = serde_json::to_string(&serde_json::json!({ "events": events })) else { return };
    if let Ok(mut subscribers) = shared.subscribers.lock() {
        subscribers.retain(|subscriber| subscriber.send(message.clone()).is_ok());
    }
}

// --- HTTP ------------------------------------------------------------------------

struct Request {
    path: String,
    query: Vec<(String, String)>,
    host: Option<String>,
}

impl Request {
    fn param(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &TcpStream) -> Result<(String, Request), &'static str> {
    let mut reader = BufReader::new(stream.take(MAX_HEAD_BYTES as u64));
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|_| "bad request")?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("bad request")?.to_string();
    let target = parts.next().ok_or("bad request")?;
    if !target.starts_with('/') {
        return Err("bad request");
    }
    let url = Url::parse(&format!("http://localhost{target}")).map_err(|_| "bad request")?;
    let mut host = None;
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).map_err(|_| "bad request")?;
        if read == 0 {
            return Err("bad request");
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("host") {
                host = Some(value.trim().to_ascii_lowercase());
            }
        }
    }
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();
    let query = url.query_pairs().map(|(key, value)| (key.into_owned(), value.into_owned())).collect();
    Ok((method, Request { path, query, host }))
}

const COMMON_HEADERS: &str = "Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nConnection: close\r\n";

fn respond(mut stream: &TcpStream, status: &str, content_type: &str, extra: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{COMMON_HEADERS}{extra}\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body));
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

fn text(stream: &TcpStream, status: &str, body: &str) {
    respond(stream, status, "text/plain; charset=utf-8", "", body.as_bytes());
}

fn handle(shared: &Shared, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Ok((method, request)) = read_request(&stream) else {
        return text(&stream, "400 Bad Request", "Bad request.");
    };
    let allowed_hosts = [format!("localhost:{}", shared.port), format!("127.0.0.1:{}", shared.port)];
    if !request.host.as_ref().is_some_and(|host| allowed_hosts.contains(host)) {
        return text(&stream, "421 Misdirected Request", "Open this page from CowMark.");
    }
    if method != "GET" {
        return text(&stream, "405 Method Not Allowed", "Only GET is supported.");
    }
    if !request.param("token").is_some_and(|given| token::matches(&shared.token, given)) {
        return text(&stream, "403 Forbidden", "This link has expired. Open the browser preview from CowMark again.");
    }

    match request.path.strip_prefix(API_PREFIX) {
        Some("events") => stream_events(shared, stream),
        Some("pending") => {
            let commit = request.param("commit").unwrap_or_default().to_string();
            let pending = preview::pending_changes(&shared.root, &commit);
            match serde_json::to_vec(&pending) {
                Ok(body) => respond(&stream, "200 OK", "application/json", "", &body),
                Err(error) => text(&stream, "500 Internal Server Error", &error.to_string()),
            }
        }
        Some("media") => {
            let path = request.param("path").unwrap_or_default();
            let extension = Path::new(path).extension().and_then(|value| value.to_str()).unwrap_or_default();
            match (preview::read_media(&shared.root, path), preview::media_type(&extension.to_ascii_lowercase())) {
                (Ok(bytes), Some(mime)) => respond(&stream, "200 OK", mime, "", &bytes),
                (Err(error), _) => text(&stream, "404 Not Found", &error),
                (_, None) => text(&stream, "404 Not Found", "not an image"),
            }
        }
        Some(_) => text(&stream, "404 Not Found", "Not found."),
        None => page(shared, &stream, &request),
    }
}

fn page(shared: &Shared, stream: &TcpStream, request: &Request) {
    let query: Vec<String> = request
        .query
        .iter()
        .filter(|(key, _)| key != "token")
        .map(|(key, value)| {
            let mut pair = Url::parse("http://x/").expect("valid url");
            pair.query_pairs_mut().append_pair(key, value);
            pair.query().unwrap_or_default().to_string()
        })
        .collect();
    let frame = if query.is_empty() {
        format!("{}{}", shared.origin, request.path)
    } else {
        format!("{}{}?{}", shared.origin, request.path, query.join("&"))
    };
    let config = serde_json::json!({ "origin": shared.origin, "frame": frame }).to_string().replace('<', "\\u003c");
    let Ok(nonce) = token::random_token() else {
        return text(stream, "500 Internal Server Error", "Couldn't make a nonce.");
    };
    let body = PAGE.replace("{{CONFIG}}", &config).replace("{{NONCE}}", &nonce);
    let csp = format!(
        "Content-Security-Policy: default-src 'none'; script-src 'nonce-{nonce}'; style-src 'unsafe-inline'; \
         connect-src 'self'; frame-src {}; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n",
        shared.origin
    );
    respond(stream, "200 OK", "text/html; charset=utf-8", &csp, body.as_bytes());
}

fn stream_events(shared: &Shared, mut stream: TcpStream) {
    let (sender, receiver): (Sender<String>, Receiver<String>) = mpsc::channel();
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{COMMON_HEADERS}\r\nretry: 2000\n\n: connected\n\n");
    if stream.write_all(head.as_bytes()).and_then(|_| stream.flush()).is_err() {
        return;
    }
    match shared.subscribers.lock() {
        Ok(mut subscribers) => subscribers.push(sender),
        Err(_) => return,
    }
    while !shared.stop.load(Ordering::SeqCst) {
        let chunk = match receiver.recv_timeout(KEEPALIVE) {
            Ok(message) => format!("data: {message}\n\n"),
            Err(RecvTimeoutError::Timeout) => ": keepalive\n\n".to_string(),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if stream.write_all(chunk.as_bytes()).and_then(|_| stream.flush()).is_err() {
            break;
        }
    }
    // Dropping the receiver makes the next broadcast prune this subscriber.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_urls_keep_the_path_and_replace_the_token() {
        let root = std::env::temp_dir().join(format!("cowmark-preview-server-url-{}", std::process::id()));
        let server = PreviewServer::start(PreviewServerOptions {
            root,
            site: "https://preview.example.com".into(),
            port: 0,
        })
        .unwrap();
        let token = server.token().to_string();
        let port = server.port();
        assert_eq!(server.page_url("/blog/a"), format!("http://localhost:{port}/blog/a?token={token}"));
        assert_eq!(
            server.page_url("/blog/a?x=1&token=old"),
            format!("http://localhost:{port}/blog/a?x=1&token={token}")
        );
        assert_eq!(server.page_url("//evil.example/x"), format!("http://localhost:{port}//evil.example/x?token={token}"));
        assert_eq!(server.page_url("/__cowmark/events"), format!("http://localhost:{port}/?token={token}"));
        assert_eq!(server.page_url("relative"), format!("http://localhost:{port}/?token={token}"));
    }
}
