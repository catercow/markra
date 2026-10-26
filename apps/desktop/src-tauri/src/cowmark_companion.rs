//! CowMark as a hub: the browser preview server and the control connection for
//! the `cowmark` command, both from `cowmark_core`. They start when the managed
//! workspace opens and stop with the app, so nothing runs once CowMark quits.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cowmark_core::control::{ControlOptions, ControlServer, ControlUi};
use cowmark_core::preview_server::{PreviewServer, PreviewServerOptions, DEFAULT_PORT};
use tauri::{Emitter, Manager};

const CONTROL_EVENT: &str = "cowmark://control";

#[derive(Default)]
pub(crate) struct CowMarkCompanionState {
    preview: Arc<Mutex<Option<PreviewServer>>>,
    control: Mutex<Option<ControlServer>>,
    /// The editor window that started the companion; control requests go to it.
    window: Arc<Mutex<Option<String>>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanionStatus {
    /// `http://localhost:4100`, while the browser preview runs.
    preview_origin: Option<String>,
    preview_token: Option<String>,
    preview_error: Option<String>,
    control_error: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "action")]
enum ControlEvent {
    Open { path: String },
    ProposePublish { paths: Vec<String>, message: String },
    GetLatest,
}

struct TauriControlUi {
    app: tauri::AppHandle,
    window: Arc<Mutex<Option<String>>>,
    preview: Arc<Mutex<Option<PreviewServer>>>,
}

impl TauriControlUi {
    fn window(&self) -> Result<tauri::WebviewWindow, String> {
        let label = self.window.lock().map_err(|error| error.to_string())?.clone();
        label
            .and_then(|label| self.app.get_webview_window(&label))
            .or_else(|| self.app.get_webview_window("main"))
            .ok_or_else(|| "CowMark has no open window".to_string())
    }

    /// Only the companion's window hears it, so a second editor window doesn't act too.
    fn emit(&self, event: ControlEvent) -> Result<(), String> {
        let label = self.window()?.label().to_string();
        self.app
            .emit_to(tauri::EventTarget::labeled(label), CONTROL_EVENT, event)
            .map_err(|error| error.to_string())
    }
}

impl ControlUi for TauriControlUi {
    fn focus(&self) -> Result<(), String> {
        let window = self.window()?;
        let _ = window.unminimize();
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())
    }

    fn open(&self, path: &Path) -> Result<(), String> {
        self.emit(ControlEvent::Open { path: path.to_string_lossy().into_owned() })
    }

    fn propose_publish(&self, paths: &[String], message: &str) -> Result<(), String> {
        self.emit(ControlEvent::ProposePublish { paths: paths.to_vec(), message: message.to_string() })
    }

    fn get_latest(&self) -> Result<(), String> {
        self.emit(ControlEvent::GetLatest)
    }

    fn preview_url(&self) -> Option<String> {
        self.preview.lock().ok()?.as_ref().map(|server| server.page_url("/"))
    }
}

/// Starts (or reports) the browser preview for `root` and the control socket.
/// Safe to call again: a running server is kept, a failed one retried, so the
/// panel can call it when "Open in browser" is clicked after the port frees up.
#[tauri::command]
pub(crate) fn cowmark_companion_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, CowMarkCompanionState>,
    root: String,
    site: String,
) -> Result<CompanionStatus, String> {
    *state.window.lock().map_err(|error| error.to_string())? = Some(window.label().to_string());
    let root = PathBuf::from(root);

    let mut preview_error = None;
    {
        let mut preview = state.preview.lock().map_err(|error| error.to_string())?;
        if preview.as_ref().is_some_and(|server| server.root() != root) {
            *preview = None;
        }
        if preview.is_none() {
            match PreviewServer::start(PreviewServerOptions { root, site, port: DEFAULT_PORT }) {
                Ok(server) => *preview = Some(server),
                Err(error) => preview_error = Some(error),
            }
        }
    }

    let mut control_error = None;
    {
        let mut control = state.control.lock().map_err(|error| error.to_string())?;
        if control.is_none() {
            let options = ControlOptions {
                data_dir: crate::user_data::app_data_dir(&app)?,
                app_version: app.package_info().version.to_string(),
                ui: Arc::new(TauriControlUi {
                    app: app.clone(),
                    window: state.window.clone(),
                    preview: state.preview.clone(),
                }),
            };
            match ControlServer::start(options) {
                Ok(server) => *control = Some(server),
                Err(error) => control_error = Some(error),
            }
        }
    }

    let preview = state.preview.lock().map_err(|error| error.to_string())?;
    Ok(CompanionStatus {
        preview_origin: preview.as_ref().map(PreviewServer::origin),
        preview_token: preview.as_ref().map(|server| server.token().to_string()),
        preview_error,
        control_error,
    })
}

/// Closes the socket and the port before the process exits; drops don't run on exit.
pub(crate) fn shutdown(app: &tauri::AppHandle) {
    let state = app.state::<CowMarkCompanionState>();
    if let Ok(mut control) = state.control.lock() {
        control.take();
    }
    if let Ok(mut preview) = state.preview.lock() {
        preview.take();
    };
}
