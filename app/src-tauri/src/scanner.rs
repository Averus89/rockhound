//! Background scan loop: grab a crop, read it, vote, and report changes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use rockhound_core::{SignatureDb, SignatureMatch, Voter};
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
        let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| format!("cannot open replay folder {}: {e}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "png"))
            .collect();
        if frames.is_empty() {
            return Err(format!("no PNG frames in {}", dir.display()));
        }
        frames.sort();
        Ok(Self { frames, next: 0 })
    }
}

impl FrameSource for ReplaySource {
    fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
        let path = &self.frames[self.next];
        self.next = (self.next + 1) % self.frames.len();
        image::open(path)
            .map(|img| Some(img.to_rgba8()))
            .map_err(|e| format!("cannot read {}: {e}", path.display()))
    }
}

/// Sent to the UI whenever the shown value changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub value: Option<u32>,
    pub raw: String,
    pub confidence: f32,
    pub corrected: bool,
    pub matches: Vec<SignatureMatch>,
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
/// the voted value; `on_stop` gets the error that ended the loop, if any.
pub fn spawn(
    mut source: Box<dyn FrameSource>,
    mut reader: Reader,
    db: Arc<RwLock<SignatureDb>>,
    interval: Duration,
    on_reading: impl Fn(Reading) + Send + 'static,
    on_stop: impl FnOnce(Option<String>) + Send + 'static,
) -> ScannerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let mut voter = Voter::new(3, 2);
        let mut shown: Option<u32> = None;
        let mut error = None;
        while !stop_flag.load(Ordering::Relaxed) {
            match source.next_crop() {
                Ok(crop) => {
                    let db = db.read().unwrap_or_else(|e| e.into_inner());
                    let resolved = crop.and_then(|c| reader.read(&c, &db));
                    let value = voter.push(resolved.as_ref().and_then(|r| r.value));
                    if value != shown {
                        shown = value;
                        let matches = value.map(|v| db.matches(v)).unwrap_or_default();
                        on_reading(to_reading(value, matches, resolved));
                    }
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

    /// Serves the same real crop a few times, then fails.
    struct Frames(usize);

    impl FrameSource for Frames {
        fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
            if self.0 == 0 {
                return Err("out of frames".into());
            }
            self.0 -= 1;
            Ok(Some(image::open(CROP).unwrap().to_rgba8()))
        }
    }

    #[test]
    fn reports_the_voted_value_once_and_the_stop_reason() {
        let db = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let mut templates = TemplateSet::default();
        learn_crop(
            &image::open(CROP).unwrap().to_rgba8(),
            17200,
            &mut templates,
        );
        let (readings, got_readings) = mpsc::channel();
        let (stopped, got_stop) = mpsc::channel();
        let handle = spawn(
            Box::new(Frames(3)),
            Reader::new(templates, None, None),
            Arc::new(RwLock::new(db)),
            Duration::from_millis(1),
            move |r| readings.send(r).unwrap(),
            move |e| stopped.send(e).unwrap(),
        );
        let stop_reason = got_stop.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.stop();
        assert_eq!(stop_reason.as_deref(), Some("out of frames"));
        let shown: Vec<_> = got_readings.try_iter().collect();
        assert_eq!(shown.len(), 1, "only the change is reported");
        assert_eq!(shown[0].value, Some(17200));
        assert_eq!(shown[0].raw, "17,200");
        assert!(!shown[0].matches.is_empty());
    }
}
