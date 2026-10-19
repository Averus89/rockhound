//! Rockhound desktop app: Tauri commands and events around the scan loop.
//!
//! Events sent to the UI:
//! - `reading`: the shown signature value changed ([`scanner::Reading`]).
//! - `scanner-stopped`: the scan loop ended; payload is the error, if any.
//! - `signatures-updated`: a newer signature table was synced.

mod capture;
mod debug;
mod fake_game;
#[cfg(windows)]
mod ocr;
mod overlay;
mod reader;
mod samples;
mod scanner;
mod settings;
mod sync;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rockhound_core::recognize::TemplateError;
use rockhound_core::{Location, SignatureDb, TemplateSet};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder, WindowEvent};

use crate::fake_game::FakeGame;
use crate::reader::{Reader, Teacher};
use crate::scanner::{Reading, ReplaySource, ScannerHandle};
use crate::settings::Settings;

/// Development aid: scan a folder of crops instead of the game.
const REPLAY_DIR_ENV: &str = "ROCKHOUND_REPLAY_DIR";
/// Testing aid: play a recorded game (a video or a folder of full-window
/// screenshots) instead of capturing Star Citizen. Used by the end-to-end
/// tests, which run without the game.
const FAKE_GAME_ENV: &str = "ROCKHOUND_FAKE_GAME";

struct Paths {
    settings: PathBuf,
    signature_cache: PathBuf,
    templates: PathBuf,
    /// Diagnostics, when the player turns them on.
    debug: PathBuf,
    /// Glyph samples, when the player turns them on.
    samples: PathBuf,
}

struct AppState {
    paths: Paths,
    db: Arc<RwLock<SignatureDb>>,
    settings: Mutex<Settings>,
    shown: Arc<Mutex<Shown>>,
    scanner: Mutex<Option<ScannerHandle>>,
}

/// The reading on screen, ranked for where the player is. Kept here rather
/// than in the scan loop so a location change re-ranks it even after the
/// scan stopped, and so a reading and a location change can't cross.
#[derive(Default)]
struct Shown {
    reading: Option<Reading>,
    location: Location,
}

impl Shown {
    /// Rank `reading` (or the shown one, when `None`) for the current
    /// location; the reading to send to the UI, if there is one.
    fn update(&mut self, reading: Option<Reading>, db: &SignatureDb) -> Option<Reading> {
        if let Some(r) = reading {
            self.reading = Some(r);
        }
        let shown = self.reading.as_mut()?;
        if let Some(value) = shown.value {
            // Mixes are all ship rocks, found everywhere, so only matches move.
            shown.matches = db.matches_at(value, self.location);
        }
        Some(shown.clone())
    }
}

/// Send `reading` (or the shown one re-ranked) to the UI.
fn show(app: &AppHandle, shown: &Mutex<Shown>, db: &RwLock<SignatureDb>, reading: Option<Reading>) {
    let db = db.read().unwrap_or_else(|e| e.into_inner());
    // Held through the emit, so a reading ranked for the old location can't
    // be sent after the re-ranked one.
    let mut shown = lock(shown);
    if let Some(r) = shown.update(reading, &db) {
        let _ = app.emit("reading", r);
    }
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
    // Applied now, not at the next scan start, so switching them off
    // mid-scan stops the writes.
    apply_switches(&settings, &state.paths);
    // The file is written, so memory must match it even if the overlay
    // can't be shown; otherwise a restart would apply different settings.
    let shown = overlay::set_visible(&app, settings.overlay);
    let moved =
        std::mem::replace(&mut lock(&state.shown).location, settings.location) != settings.location;
    if moved {
        show(&app, &state.shown, &state.db, None);
    }
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
    apply_switches(&settings, &state.paths);
    if settings.debug {
        debug::note(format_args!("scan started, v{}", env!("CARGO_PKG_VERSION")));
    }
    // A hand-edited settings file could hold 0, which would spin a core.
    let interval = Duration::from_millis(settings.interval_ms.clamp(50, 2000));
    let source = open_source(&settings, interval)?;
    let (reading_app, stop_app) = (app.clone(), app);
    let (shown, db) = (Arc::clone(&state.shown), Arc::clone(&state.db));
    *slot = Some(scanner::spawn(
        source,
        reader,
        Arc::clone(&state.db),
        interval,
        move |reading| show(&reading_app, &shown, &db, Some(reading)),
        move |error| {
            let _ = stop_app.emit("scanner-stopped", error);
        },
    ));
    Ok(())
}

/// Point the side channels (debug images, glyph samples) at their folders
/// when the player has them on.
fn apply_switches(settings: &Settings, paths: &Paths) {
    debug::set_dir(settings.debug.then(|| paths.debug.clone()));
    samples::set_dir(settings.glyph_samples.then(|| paths.samples.clone()));
}

/// Where players send glyph packs: an issue form a workflow turns into a PR.
#[cfg(windows)]
const GLYPH_PACK_ISSUE_URL: &str =
    "https://github.com/Averus89/rockhound/issues/new?template=glyph-pack.yml";

#[tauri::command]
fn list_glyph_samples(state: State<'_, AppState>) -> Vec<samples::Sample> {
    samples::list(&state.paths.samples)
}

#[tauri::command]
fn delete_glyph_sample(state: State<'_, AppState>, file: String) -> Result<(), String> {
    samples::remove(&state.paths.samples, &file)
}

/// Zip the reviewed samples into Downloads, then show the zip and open the
/// issue form it gets attached to. Returns the zip's path.
#[tauri::command]
fn export_glyph_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    samples: Vec<samples::Labelled>,
) -> Result<String, String> {
    let out = app.path().download_dir().map_err(|e| e.to_string())?;
    let zip = samples::export(&state.paths.samples, &samples, &out)?;
    #[cfg(windows)]
    {
        // Best effort: the path is shown in the app either way.
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", zip.display()))
            .spawn();
        let _ = std::process::Command::new("explorer")
            .arg(GLYPH_PACK_ISSUE_URL)
            .spawn();
    }
    Ok(zip.display().to_string())
}

/// Templates learned so far plus the OCR teacher that adds to them.
fn open_reader(template_dir: &Path) -> Result<Reader, String> {
    let learned = match TemplateSet::load_dir(template_dir) {
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
    Ok(Reader::bundled(
        learned,
        Some(template_dir.to_path_buf()),
        teacher,
    ))
}

fn open_source(
    settings: &Settings,
    interval: Duration,
) -> Result<Box<dyn scanner::FrameSource>, String> {
    // An unfinished box (zero size) means automatic.
    let region = settings.region.filter(|r| r[2] > 0 && r[3] > 0);
    if let Some(path) = std::env::var_os(FAKE_GAME_ENV) {
        return Ok(Box::new(FakeGame::open(
            &PathBuf::from(path),
            region,
            interval,
        )?));
    }
    if let Some(dir) = std::env::var_os(REPLAY_DIR_ENV) {
        return Ok(Box::new(ReplaySource::open(&PathBuf::from(dir))?));
    }
    #[cfg(windows)]
    return Ok(Box::new(capture::LiveSource::start(region, interval)?));
    #[cfg(not(windows))]
    Err(format!(
        "Live capture needs Windows; set {FAKE_GAME_ENV} to play a recorded game."
    ))
}

/// Whether this copy was installed by the setup exe, which can update it in
/// place. The portable exe can't be replaced by the installer, so it only
/// hears that a new version is out.
#[tauri::command]
fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("uninstall.exe").exists()))
        .unwrap_or(false)
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let data = app.path().app_data_dir()?;
            let paths = Paths {
                settings: config.join("settings.json"),
                signature_cache: data.join("signatures.json"),
                templates: data.join("templates"),
                debug: data.join("debug"),
                samples: data.join("glyph-samples"),
            };
            let db = sync::load_local(&paths.signature_cache);
            let settings = Settings::load(&paths.settings);
            let show_overlay = settings.overlay;
            let shown = Shown {
                reading: None,
                location: settings.location,
            };
            app.manage(AppState {
                paths,
                db: Arc::new(RwLock::new(db)),
                settings: Mutex::new(settings),
                shown: Arc::new(Mutex::new(shown)),
                scanner: Mutex::new(None),
            });
            // Windows are created here, after the state exists, because
            // their first command can arrive before setup returns.
            for config in &app.config().app.windows {
                WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            if show_overlay {
                // A missing overlay shouldn't stop the main window opening.
                if let Err(e) = overlay::set_visible(app.handle(), true) {
                    eprintln!("{e}");
                }
            }
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
            delete_glyph_sample,
            export_glyph_pack,
            get_status,
            is_installed,
            list_glyph_samples,
            save_settings,
            start_scan,
            stop_scan
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rockhound");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(value: u32) -> Reading {
        Reading {
            value: Some(value),
            raw: String::new(),
            confidence: 1.0,
            corrected: false,
            matches: Vec::new(),
            mixes: Vec::new(),
            at: 7,
        }
    }

    #[test]
    fn shown_reading_is_ranked_for_the_location_and_reranked_when_it_moves() {
        let db = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let best = |r: Option<Reading>| r.map(|r| (r.matches[0].name.clone(), r.at));
        let mut shown = Shown {
            reading: None,
            location: Location::Planet,
        };
        assert!(shown.update(None, &db).is_none(), "nothing shown yet");
        assert_eq!(
            best(shown.update(Some(reading(3000)), &db)),
            Some(("Small Ground Deposit".into(), 7))
        );
        shown.location = Location::Space;
        assert_eq!(
            best(shown.update(None, &db)),
            Some(("Capital Wreck Debris".into(), 7))
        );
    }
}
