//! Rockhound desktop app: Tauri commands and events around the scan loop.
//!
//! Events sent to the UI:
//! - `reading`: the shown signature value changed ([`scanner::Reading`]).
//! - `scanner-stopped`: the scan loop ended; payload is the error, if any.
//! - `signatures-updated`: a newer signature table was synced.

mod capture;
#[cfg(windows)]
mod ocr;
mod overlay;
mod reader;
mod scanner;
mod settings;
mod sync;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rockhound_core::recognize::TemplateError;
use rockhound_core::{SignatureDb, TemplateSet};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use crate::reader::{Reader, Teacher};
use crate::scanner::{ReplaySource, ScannerHandle};
use crate::settings::Settings;

/// Development aid: scan a folder of crops instead of the game.
const REPLAY_DIR_ENV: &str = "ROCKHOUND_REPLAY_DIR";

struct Paths {
    settings: PathBuf,
    signature_cache: PathBuf,
    templates: PathBuf,
}

struct AppState {
    paths: Paths,
    db: Arc<RwLock<SignatureDb>>,
    settings: Mutex<Settings>,
    scanner: Mutex<Option<ScannerHandle>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    scanning: bool,
    data_version: String,
    sc_version: String,
    settings: Settings,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // State stays usable after a panic elsewhere; the data is plain values.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> Status {
    let db = state.db.read().unwrap_or_else(|e| e.into_inner());
    let scanning = lock(&state.scanner)
        .as_ref()
        .is_some_and(|s| !s.is_finished());
    Status {
        scanning,
        data_version: db.data_version.clone(),
        sc_version: db.sc_version.clone(),
        settings: lock(&state.settings).clone(),
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    settings.save(&state.paths.settings)?;
    // The file is written, so memory must match it even if the overlay
    // can't be shown; otherwise a restart would apply different settings.
    let shown = overlay::set_visible(&app, settings.overlay);
    *lock(&state.settings) = settings;
    shown
}

#[tauri::command]
fn start_scan(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = lock(&state.scanner);
    if let Some(old) = slot.take() {
        old.stop();
    }
    let reader = open_reader(&state.paths.templates)?;
    let settings = lock(&state.settings).clone();
    // A hand-edited settings file could hold 0, which would spin a core.
    let interval = Duration::from_millis(settings.interval_ms.clamp(50, 2000));
    let source = open_source(&settings, interval)?;
    let (reading_app, stop_app) = (app.clone(), app);
    *slot = Some(scanner::spawn(
        source,
        reader,
        Arc::clone(&state.db),
        interval,
        move |reading| {
            let _ = reading_app.emit("reading", reading);
        },
        move |error| {
            let _ = stop_app.emit("scanner-stopped", error);
        },
    ));
    Ok(())
}

/// Templates learned so far plus the OCR teacher that adds to them.
fn open_reader(template_dir: &Path) -> Result<Reader, String> {
    let templates = match TemplateSet::load_dir(template_dir) {
        Ok(t) => t,
        // First run: nothing learned yet.
        Err(TemplateError::Empty(_)) => TemplateSet::default(),
        Err(TemplateError::Dir { source, .. }) if source.kind() == ErrorKind::NotFound => {
            TemplateSet::default()
        }
        Err(e) => return Err(e.to_string()),
    };
    #[cfg(windows)]
    let teacher: Option<Box<dyn Teacher>> = Some(Box::new(ocr::WindowsOcr::default()));
    #[cfg(not(windows))]
    let teacher: Option<Box<dyn Teacher>> = None;
    if templates.is_empty() && teacher.is_none() {
        return Err("No digit templates yet; build some with rockhound-replay learn.".into());
    }
    Ok(Reader::new(
        templates,
        Some(template_dir.to_path_buf()),
        teacher,
    ))
}

fn open_source(
    settings: &Settings,
    interval: Duration,
) -> Result<Box<dyn scanner::FrameSource>, String> {
    if let Some(dir) = std::env::var_os(REPLAY_DIR_ENV) {
        return Ok(Box::new(ReplaySource::open(&PathBuf::from(dir))?));
    }
    #[cfg(windows)]
    return Ok(Box::new(capture::LiveSource::start(
        // An unfinished box (zero size) means automatic.
        settings.region.filter(|r| r[2] > 0 && r[3] > 0),
        interval,
    )?));
    #[cfg(not(windows))]
    {
        let _ = (settings, interval);
        Err("Live capture needs Windows; set ROCKHOUND_REPLAY_DIR to replay frames.".into())
    }
}

#[tauri::command]
fn stop_scan(state: State<'_, AppState>) {
    if let Some(handle) = lock(&state.scanner).take() {
        handle.stop();
    }
}

/// Check for a newer signature table without blocking startup.
fn spawn_sync(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let current = state.db.read().unwrap_or_else(|e| e.into_inner()).clone();
        match sync::update(&state.paths.signature_cache, &current) {
            Ok(Some(newer)) => {
                let version = newer.data_version.clone();
                *state.db.write().unwrap_or_else(|e| e.into_inner()) = newer;
                let _ = app.emit("signatures-updated", version);
            }
            Ok(None) => {}
            // Offline or the repo is unreachable: keep the local table.
            Err(e) => eprintln!("{e}"),
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let data = app.path().app_data_dir()?;
            let paths = Paths {
                settings: config.join("settings.json"),
                signature_cache: data.join("signatures.json"),
                templates: data.join("templates"),
            };
            let db = sync::load_local(&paths.signature_cache);
            let settings = Settings::load(&paths.settings);
            if settings.overlay {
                // A missing overlay shouldn't stop the main window opening.
                if let Err(e) = overlay::set_visible(app.handle(), true) {
                    eprintln!("{e}");
                }
            }
            app.manage(AppState {
                paths,
                db: Arc::new(RwLock::new(db)),
                settings: Mutex::new(settings),
                scanner: Mutex::new(None),
            });
            spawn_sync(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The hidden overlay would otherwise keep the app running after
            // the main window is closed.
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            save_settings,
            start_scan,
            stop_scan
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rockhound");
}
