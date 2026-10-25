//! CaterCow content access: Google sign-in through the content preview, a
//! session kept in the app data dir, and one-hour GitHub App tokens fetched with
//! it for git. The token never leaves Rust. Server side: the monorepo's
//! js/nuxt4 content-access routes (/__content_access/*).

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::Url;
use sha2::{Digest, Sha256};
use tauri::Manager;

use crate::content_git::{self, Author, Remote};
use crate::content_preview::find_git;

const FILE_NAME: &str = "content-access.json";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TOKEN_MARGIN_SECS: i64 = 5 * 60;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSession {
    site: String,
    session: String,
    email: String,
    expires_at: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessFile {
    #[serde(default)]
    session: Option<StoredSession>,
    #[serde(default)]
    folder: Option<String>,
}

struct CachedToken {
    session: String,
    token: String,
    expires_at: i64,
    clone_url: String,
}

#[derive(Default)]
pub(crate) struct ContentAccessState(Mutex<Option<CachedToken>>);

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccessState {
    email: Option<String>,
    expires_at: Option<String>,
    folder: Option<String>,
    default_folder: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignedIn {
    email: String,
    expires_at: String,
}

// --- small pure helpers -----------------------------------------------------------

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A PKCE verifier and its S256 challenge.
fn pkce_pair() -> Result<(String, String), String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    let verifier = base64url(&bytes);
    let challenge = challenge_for(&verifier);
    Ok((verifier, challenge))
}

fn challenge_for(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    Error(String),
    /// Some other request (a favicon, say); keep waiting.
    Other,
}

/// Reads the request line of the browser's redirect to the loopback server.
fn parse_callback_request(request: &str) -> Callback {
    let Some(target) = request.lines().next().and_then(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some("GET")).then(|| parts.next()).flatten()
    }) else {
        return Callback::Other;
    };
    let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else { return Callback::Other };
    if url.path() != "/callback" {
        return Callback::Other;
    }
    let value = |name: &str| url.query_pairs().find(|(key, _)| key == name).map(|(_, value)| value.into_owned());
    match (value("code"), value("error")) {
        (_, Some(error)) => Callback::Error(error),
        (Some(code), None) if !code.is_empty() => Callback::Code(code),
        _ => Callback::Error("The sign-in page didn't return a code.".into()),
    }
}

/// Unix seconds for an RFC 3339 UTC time such as `2026-11-24T20:00:00.000Z`.
fn iso_to_unix(value: &str) -> Option<i64> {
    let value = value.trim().strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date_parts.next()??, date_parts.next()??, date_parts.next()??);
    let time = time.split('.').next()?;
    let mut time_parts = time.split(':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (time_parts.next()??, time_parts.next()??, time_parts.next()??);
    // Days from civil (Howard Hinnant).
    let year_adj = if month <= 2 { year - 1 } else { year };
    let era = year_adj.div_euclid(400);
    let yoe = year_adj - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs() as i64).unwrap_or(0)
}

fn endpoint(site: &str, path: &str) -> Result<Url, String> {
    let base = Url::parse(site.trim()).map_err(|_| "The content preview address is invalid.".to_string())?;
    if !matches!(base.scheme(), "https" | "http") {
        return Err("The content preview address must be http(s).".into());
    }
    base.join(path).map_err(|error| error.to_string())
}

// --- session file -------------------------------------------------------------------

fn access_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(crate::user_data::app_data_dir(app)?.join(FILE_NAME))
}

fn read_access(path: &Path) -> AccessFile {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn write_access(path: &Path, file: &AccessFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(file).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn live_session(file: &AccessFile) -> Option<&StoredSession> {
    file.session
        .as_ref()
        .filter(|session| iso_to_unix(&session.expires_at).is_some_and(|expires| expires > now_unix()))
}

fn forget_session(app: &tauri::AppHandle, state: &ContentAccessState) {
    if let Ok(path) = access_path(app) {
        let mut file = read_access(&path);
        file.session = None;
        let _ = write_access(&path, &file);
    }
    if let Ok(mut cache) = state.0.lock() {
        *cache = None;
    }
}

// --- network ------------------------------------------------------------------------

async fn error_message(response: reqwest::Response) -> String {
    let status = response.status();
    let message = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body.get("message").and_then(|message| message.as_str()).map(str::to_string))
        .unwrap_or_else(|| format!("The content preview answered {status}."));
    match status.as_u16() {
        401 => format!("signed_out: {message}"),
        403 => format!("not_allowed: {message}"),
        _ => message,
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().map_err(|error| error.to_string())
}

/// A token for git, from the cache or the content preview. Signs the user out
/// when the preview rejects the session.
async fn token(app: &tauri::AppHandle, state: &ContentAccessState) -> Result<Remote, String> {
    let path = access_path(app)?;
    let file = read_access(&path);
    let session = live_session(&file).cloned().ok_or("signed_out: Sign in to continue.")?;

    if let Ok(cache) = state.0.lock() {
        if let Some(cached) = cache.as_ref() {
            if cached.session == session.session && cached.expires_at - TOKEN_MARGIN_SECS > now_unix() {
                return Ok(Remote { url: cached.clone_url.clone(), env: content_git::auth_env(&cached.token) });
            }
        }
    }

    let response = client()?
        .post(endpoint(&session.site, "/__content_access/token")?)
        .bearer_auth(&session.session)
        // Google's front end answers 411 to a POST without Content-Length.
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|_| "offline: Couldn't reach the content preview. Check your internet connection.".to_string())?;
    if !response.status().is_success() {
        let message = error_message(response).await;
        if message.starts_with("signed_out:") || message.starts_with("not_allowed:") {
            forget_session(app, state);
        }
        return Err(message);
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TokenResponse {
        token: String,
        expires_at: String,
        clone_url: String,
    }
    let body: TokenResponse = response.json().await.map_err(|error| error.to_string())?;
    if !body.clone_url.starts_with("https://github.com/") {
        return Err("The content preview returned an unexpected repository address.".into());
    }
    let expires_at = iso_to_unix(&body.expires_at).unwrap_or_else(|| now_unix() + 3_000);
    let remote = Remote { url: body.clone_url.clone(), env: content_git::auth_env(&body.token) };
    if let Ok(mut cache) = state.0.lock() {
        *cache = Some(CachedToken {
            session: session.session,
            token: body.token,
            expires_at,
            clone_url: body.clone_url,
        });
    }
    Ok(remote)
}

fn signed_in_author(app: &tauri::AppHandle) -> Result<Author, String> {
    let file = read_access(&access_path(app)?);
    live_session(&file).map(|session| Author::from_email(&session.email)).ok_or("signed_out: Sign in to continue.".into())
}

fn git_binary() -> Result<PathBuf, String> {
    find_git().ok_or_else(|| {
        "Git isn't installed. On a Mac, run any `git` command in Terminal to install it; on Windows, install Git for Windows."
            .to_string()
    })
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

// --- loopback sign-in -----------------------------------------------------------------

const SIGNED_IN_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Signed in</title>\
<body style=\"font:16px system-ui;margin:4rem auto;max-width:28rem;text-align:center\">\
<h1 style=\"font-size:1.4rem\">You're signed in</h1><p>Return to CowMark. You can close this tab.</p>";
const FAILED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Sign-in failed</title>\
<body style=\"font:16px system-ui;margin:4rem auto;max-width:28rem;text-align:center\">\
<h1 style=\"font-size:1.4rem\">Sign-in didn't finish</h1><p>Return to CowMark to see why.</p>";

fn respond(mut stream: TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn wait_for_callback(listener: TcpListener, timeout: Duration) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buffer = [0u8; 8192];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                match parse_callback_request(&request) {
                    Callback::Code(code) => {
                        respond(stream, "200 OK", SIGNED_IN_PAGE);
                        return Ok(code);
                    }
                    Callback::Error(error) => {
                        respond(stream, "200 OK", FAILED_PAGE);
                        return Err(format!("Google sign-in failed: {error}"));
                    }
                    Callback::Other => respond(stream, "404 Not Found", ""),
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("Sign-in timed out. Try again.".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

// --- commands -------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn content_access_state(app: tauri::AppHandle) -> Result<AccessState, String> {
    let file = read_access(&access_path(&app)?);
    let session = live_session(&file);
    Ok(AccessState {
        email: session.map(|session| session.email.clone()),
        expires_at: session.map(|session| session.expires_at.clone()),
        folder: file.folder.clone(),
        default_folder: content_git::default_folder(app.path().document_dir().ok()).to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub(crate) async fn content_access_sign_in(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    site: String,
) -> Result<SignedIn, String> {
    let (verifier, challenge) = pkce_pair()?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let mut login = endpoint(&site, "/__content_access/login")?;
    login.query_pairs_mut().append_pair("port", &port.to_string()).append_pair("challenge", &challenge);
    crate::external_urls::open_system_url(login.as_str())?;

    let code = blocking(move || wait_for_callback(listener, SIGN_IN_TIMEOUT)).await?;

    let response = client()?
        .post(endpoint(&site, "/__content_access/session")?)
        .json(&serde_json::json!({ "code": code, "verifier": verifier }))
        .send()
        .await
        .map_err(|_| "offline: Couldn't reach the content preview. Check your internet connection.".to_string())?;
    if !response.status().is_success() {
        return Err(error_message(response).await);
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SessionResponse {
        session: String,
        email: String,
        expires_at: String,
    }
    let body: SessionResponse = response.json().await.map_err(|error| error.to_string())?;

    let path = access_path(&app)?;
    let mut file = read_access(&path);
    file.session = Some(StoredSession {
        site,
        session: body.session,
        email: body.email.clone(),
        expires_at: body.expires_at.clone(),
    });
    write_access(&path, &file)?;
    if let Ok(mut cache) = state.0.lock() {
        *cache = None;
    }
    Ok(SignedIn { email: body.email, expires_at: body.expires_at })
}

#[tauri::command]
pub(crate) fn content_access_sign_out(app: tauri::AppHandle, state: tauri::State<'_, ContentAccessState>) {
    forget_session(&app, &state);
}

#[tauri::command]
pub(crate) fn content_access_set_folder(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    let file_path = access_path(&app)?;
    let mut file = read_access(&file_path);
    file.folder = path;
    write_access(&file_path, &file)
}

#[tauri::command]
pub(crate) async fn content_access_is_checkout(path: String) -> Result<bool, String> {
    blocking(move || Ok(find_git().is_some_and(|git| content_git::is_checkout(&git, Path::new(&path))))).await
}

/// Clones the content into `dest` and remembers it as the managed folder.
#[tauri::command]
pub(crate) async fn content_access_clone(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    dest: String,
) -> Result<(), String> {
    let git = git_binary()?;
    let remote = token(&app, &state).await?;
    let target = PathBuf::from(&dest);
    blocking(move || content_git::clone_into(&git, &remote, &target)).await?;
    content_access_set_folder(app, Some(dest))
}

#[tauri::command]
pub(crate) async fn content_access_status(root: String) -> Result<content_git::ContentGitStatus, String> {
    let git = git_binary()?;
    blocking(move || content_git::status(&git, Path::new(&root))).await
}

#[tauri::command]
pub(crate) async fn content_access_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    root: String,
    message: String,
) -> Result<content_git::SyncResult, String> {
    let git = git_binary()?;
    let author = signed_in_author(&app)?;
    let remote = token(&app, &state).await?;
    blocking(move || content_git::save(&git, Path::new(&root), &remote, &author, &message)).await
}

#[tauri::command]
pub(crate) async fn content_access_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, ContentAccessState>,
    root: String,
) -> Result<content_git::SyncResult, String> {
    let git = git_binary()?;
    let author = signed_in_author(&app)?;
    let remote = token(&app, &state).await?;
    blocking(move || content_git::update(&git, Path::new(&root), &remote, &author)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_the_s256_of_the_verifier() {
        let (verifier, challenge) = pkce_pair().unwrap();
        assert_eq!(verifier.len(), 43);
        assert!(verifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(challenge, challenge_for(&verifier));
        assert_eq!(challenge.len(), 43);
        // RFC 7636 appendix B.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_ne!(pkce_pair().unwrap().0, verifier);
    }

    #[test]
    fn parses_the_loopback_callback() {
        assert_eq!(
            parse_callback_request("GET /callback?code=abc.def HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
            Callback::Code("abc.def".into())
        );
        assert_eq!(
            parse_callback_request("GET /callback?error=access_denied HTTP/1.1\r\n\r\n"),
            Callback::Error("access_denied".into())
        );
        assert!(matches!(parse_callback_request("GET /callback HTTP/1.1\r\n\r\n"), Callback::Error(_)));
        assert_eq!(parse_callback_request("GET /favicon.ico HTTP/1.1\r\n\r\n"), Callback::Other);
        assert_eq!(parse_callback_request("POST /callback?code=x HTTP/1.1\r\n\r\n"), Callback::Other);
        assert_eq!(parse_callback_request(""), Callback::Other);
    }

    #[test]
    fn loopback_server_returns_the_code_and_ignores_other_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiter = std::thread::spawn(move || wait_for_callback(listener, Duration::from_secs(10)));
        let send = |request: &str| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        };
        assert!(send("GET /favicon.ico HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404"));
        assert!(send("GET /callback?code=one-time HTTP/1.1\r\n\r\n").contains("You're signed in"));
        assert_eq!(waiter.join().unwrap(), Ok("one-time".to_string()));
    }

    #[test]
    fn reads_iso_times() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2026-11-24T20:00:00.000Z"), Some(1_795_550_400));
        assert_eq!(iso_to_unix("2000-03-01T00:00:01Z"), Some(951_868_801));
        assert_eq!(iso_to_unix("not a date"), None);
    }

    #[test]
    fn session_file_round_trips_and_expires() {
        let dir = std::env::temp_dir().join(format!("markra-content-access-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let mut file = AccessFile {
            session: Some(StoredSession {
                site: "https://preview.example.com".into(),
                session: "s".into(),
                email: "e@catercow.com".into(),
                expires_at: "2999-01-01T00:00:00.000Z".into(),
            }),
            folder: Some("/x/CaterCow Content".into()),
        };
        write_access(&path, &file).unwrap();
        let read = read_access(&path);
        assert_eq!(live_session(&read).map(|session| session.email.as_str()), Some("e@catercow.com"));
        assert_eq!(read.folder.as_deref(), Some("/x/CaterCow Content"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        file.session.as_mut().unwrap().expires_at = "2001-01-01T00:00:00Z".into();
        assert!(live_session(&file).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn endpoints_join_onto_the_site() {
        assert_eq!(
            endpoint("https://preview.example.com/", "/__content_access/token").unwrap().as_str(),
            "https://preview.example.com/__content_access/token"
        );
        assert!(endpoint("file:///etc", "/x").is_err());
    }
}
