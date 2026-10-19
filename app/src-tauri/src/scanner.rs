//! Background scan loop: grab a crop, read it, vote, and report changes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use rockhound_core::{Location, SignatureDb, SignatureMatch, Voter};
use serde::Serialize;

use crate::reader::Reader;

/// Where readout crops come from.
pub trait FrameSource: Send {
    /// The next crop of the readout, or `None` when the readout isn't on
    /// screen (which clears the shown value once the vote agrees).
    fn next_crop(&mut self) -> Result<Option<RgbaImage>, String>;
}

/// Plays back a folder of crops made by `rockhound-replay frames`, looping.
/// Lets the UI be developed and demoed without the game running.
pub struct ReplaySource {
    frames: Vec<PathBuf>,
    next: usize,
}

impl ReplaySource {
    pub fn open(dir: &Path) -> Result<Self, String> {
        Ok(Self {
            frames: list_pngs(dir)?,
            next: 0,
        })
    }
}

impl FrameSource for ReplaySource {
    fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
        next_png(&self.frames, &mut self.next).map(Some)
    }
}

/// The PNG files in `dir`, in name order; an error when there are none.
pub fn list_pngs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot open frame folder {}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    if frames.is_empty() {
        return Err(format!("no PNG frames in {}", dir.display()));
    }
    frames.sort();
    Ok(frames)
}

/// Load `frames[*next]` and advance, looping back to the first frame.
pub fn next_png(frames: &[PathBuf], next: &mut usize) -> Result<RgbaImage, String> {
    let path = &frames[*next];
    *next = (*next + 1) % frames.len();
    image::open(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// How long a shown value stays up while frames stop confirming it.
const SHOW_HOLD: Duration = Duration::from_secs(3);

/// Sent to the UI whenever the shown value changes, and again (same `at`)
/// when a location change reorders its matches.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub value: Option<u32>,
    pub raw: String,
    pub confidence: f32,
    pub corrected: bool,
    pub matches: Vec<SignatureMatch>,
    /// Two-mineral clusters that add up to the value, filled only when no
    /// single source explains it.
    pub mixes: Vec<[SignatureMatch; 2]>,
    /// Unix time in milliseconds.
    pub at: u64,
}

pub struct ScannerHandle {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl ScannerHandle {
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        // A panicked scan thread has nothing left to clean up.
        let _ = self.thread.join();
    }

    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }
}

/// Start the scan loop on its own thread. `on_reading` gets every change of
/// the voted value or of its ranking; `on_stop` gets the error that ended
/// the loop, if any.
pub fn spawn(
    mut source: Box<dyn FrameSource>,
    mut reader: Reader,
    db: Arc<RwLock<SignatureDb>>,
    location: Arc<RwLock<Location>>,
    interval: Duration,
    on_reading: impl Fn(Reading) + Send + 'static,
    on_stop: impl FnOnce(Option<String>) + Send + 'static,
) -> ScannerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let hold = (SHOW_HOLD.as_millis() / interval.as_millis().max(1)) as usize;
        let mut voter = Voter::new(3, 2).hold(hold);
        let mut shown: Option<Reading> = None;
        let mut ranked_for = Location::Unknown;
        let mut error = None;
        while !stop_flag.load(Ordering::Relaxed) {
            match source.next_crop() {
                Ok(crop) => {
                    let db = db.read().unwrap_or_else(|e| e.into_inner());
                    let here = *location.read().unwrap_or_else(|e| e.into_inner());
                    let resolved = crop.and_then(|c| reader.read(&c, &db));
                    let value = voter.push(resolved.as_ref().and_then(|r| r.value));
                    if value != shown.as_ref().and_then(|r| r.value) {
                        let matches = value.map(|v| db.matches_at(v, here)).unwrap_or_default();
                        let mixes = match value {
                            Some(v) if matches.is_empty() => db.mixes(v),
                            _ => Vec::new(),
                        };
                        let reading = to_reading(value, matches, mixes, resolved);
                        on_reading(reading.clone());
                        shown = Some(reading);
                    } else if here != ranked_for {
                        // Same value and time, so the UI updates it in place
                        // instead of logging a new reading.
                        if let Some(reading) = shown.as_mut() {
                            if let Some(v) = reading.value {
                                reading.matches = db.matches_at(v, here);
                                on_reading(reading.clone());
                            }
                        }
                    }
                    ranked_for = here;
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
            std::thread::sleep(interval);
        }
        on_stop(error);
    });
    ScannerHandle { stop, thread }
}

fn to_reading(
    value: Option<u32>,
    matches: Vec<SignatureMatch>,
    mixes: Vec<[SignatureMatch; 2]>,
    resolved: Option<rockhound_core::Resolved>,
) -> Reading {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    // The voted value can lag the current frame; only report this frame's
    // details when they describe the shown value.
    let frame = resolved.filter(|r| r.value == value);
    Reading {
        value,
        raw: frame.as_ref().map(|r| r.raw.clone()).unwrap_or_default(),
        confidence: frame.as_ref().map_or(0.0, |r| r.confidence),
        corrected: frame.as_ref().is_some_and(|r| r.corrected),
        matches,
        mixes,
        at,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use rockhound_core::{learn_crop, TemplateSet};

    use super::*;

    const CROP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/rockhound-core/tests/fixtures/hud_17200_pin.png"
    );

    /// Serves the same real crop a few times, then fails. Moves the player
    /// to `move_to` once `move_at` frames are left.
    struct Frames {
        left: usize,
        move_at: usize,
        move_to: (Arc<RwLock<Location>>, Location),
    }

    impl Frames {
        fn new(left: usize) -> Self {
            Self {
                left,
                move_at: usize::MAX,
                move_to: (Arc::default(), Location::Unknown),
            }
        }
    }

    impl FrameSource for Frames {
        fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
            if self.left == 0 {
                return Err("out of frames".into());
            }
            if self.left == self.move_at {
                *self.move_to.0.write().unwrap() = self.move_to.1;
            }
            self.left -= 1;
            Ok(Some(image::open(CROP).unwrap().to_rgba8()))
        }
    }

    /// Run the loop on `frames` of the 17,200 crop; every reading it sent.
    fn scan(frames: Frames, db: SignatureDb, location: Arc<RwLock<Location>>) -> Vec<Reading> {
        let mut templates = TemplateSet::default();
        learn_crop(
            &image::open(CROP).unwrap().to_rgba8(),
            17200,
            &mut templates,
        );
        let (readings, got_readings) = mpsc::channel();
        let (stopped, got_stop) = mpsc::channel();
        let handle = spawn(
            Box::new(frames),
            Reader::new(templates, TemplateSet::default(), None, None),
            Arc::new(RwLock::new(db)),
            location,
            Duration::from_millis(1),
            move |r| readings.send(r).unwrap(),
            move |e| stopped.send(e).unwrap(),
        );
        let stop_reason = got_stop.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.stop();
        assert_eq!(stop_reason.as_deref(), Some("out of frames"));
        got_readings.try_iter().collect()
    }

    #[test]
    fn reports_the_voted_value_once_and_the_stop_reason() {
        let db = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let shown = scan(Frames::new(3), db, Arc::default());
        assert_eq!(shown.len(), 1, "only the change is reported");
        assert_eq!(shown[0].value, Some(17200));
        assert_eq!(shown[0].raw, "17,200");
        assert!(!shown[0].matches.is_empty());
    }

    #[test]
    fn a_location_change_reranks_the_shown_reading_in_place() {
        // 17,200 is both four ground deposits and two wrecks here.
        let db = SignatureDb::from_json(
            r#"{
                "schema": 1, "data_version": "2026.01.01", "sc_version": "4.7",
                "sources": [
                    {"name": "Deposit", "category": "ground_deposit", "base": 4300, "max_count": 10},
                    {"name": "Wreck", "category": "salvage", "base": 8600, "max_count": 10}
                ]
            }"#,
        )
        .unwrap();
        let location = Arc::new(RwLock::new(Location::Planet));
        let frames = Frames {
            left: 6,
            move_at: 2,
            move_to: (Arc::clone(&location), Location::Space),
        };
        let shown = scan(frames, db, location);
        let best: Vec<&str> = shown.iter().map(|r| r.matches[0].name.as_str()).collect();
        assert_eq!(best, ["Deposit", "Wreck"]);
        assert_eq!(shown[0].at, shown[1].at, "same reading, re-ranked");
    }
}
