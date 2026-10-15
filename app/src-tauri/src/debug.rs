//! Optional diagnostics for players reporting "it doesn't read": what the
//! scanner saw and what it made of it, saved to a folder they can send.
//!
//! Global rather than threaded through every type because it is a side
//! channel: nothing reads it back, and it is off unless the player asks.

use std::fmt::Display;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::RgbaImage;

/// Snapshots are throttled; a few seconds apart is plenty to diagnose.
pub const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(2);
/// The log is restarted past this size so a forgotten switch can't fill
/// the disk.
const MAX_LOG_BYTES: u64 = 1 << 20;

static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Turn saving on (into `dir`) or off.
pub fn set_dir(dir: Option<PathBuf>) {
    if let Some(d) = &dir {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("cannot create debug folder {}: {e}", d.display());
        }
    }
    *DIR.write().unwrap_or_else(|e| e.into_inner()) = dir;
}

fn dir() -> Option<PathBuf> {
    DIR.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn enabled() -> bool {
    dir().is_some()
}

/// Append a timestamped line to `log.txt`.
pub fn note(message: impl Display) {
    let Some(dir) = dir() else { return };
    let path = dir.join("log.txt");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, dir.join("log.old.txt"));
    }
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| writeln!(f, "{ms} {message}"));
    if let Err(e) = written {
        eprintln!("cannot write {}: {e}", path.display());
    }
}

/// Save `img` as `<name>.png`, replacing the previous one.
pub fn image(name: &str, img: &RgbaImage) {
    let Some(dir) = dir() else { return };
    if let Err(e) = img.save(dir.join(format!("{name}.png"))) {
        eprintln!("cannot save debug image {name}: {e}");
    }
}

/// Numbered readout crops kept, oldest replaced first: enough to collect
/// test fixtures from a session without filling the disk.
const MAX_SAMPLES: usize = 200;
static NEXT_SAMPLE: AtomicUsize = AtomicUsize::new(0);

/// Save `img` as the next numbered sample in `readouts/`; returns its
/// number so log lines can refer to it.
pub fn sample(img: &RgbaImage) -> Option<usize> {
    let dir = dir()?.join("readouts");
    let n = NEXT_SAMPLE.fetch_add(1, Ordering::Relaxed) % MAX_SAMPLES;
    let saved = std::fs::create_dir_all(&dir)
        .map_err(image::ImageError::IoError)
        .and_then(|()| img.save(dir.join(format!("{n:03}.png"))));
    if let Err(e) = saved {
        eprintln!("cannot save readout sample: {e}");
    }
    Some(n)
}

/// Rate limiter for snapshots taken on every frame.
#[derive(Default)]
pub struct Throttle(Option<Instant>);

impl Throttle {
    /// True at most once per [`SNAPSHOT_INTERVAL`], and only when saving
    /// is on.
    pub fn due(&mut self) -> bool {
        if !enabled() || self.0.is_some_and(|t| t.elapsed() < SNAPSHOT_INTERVAL) {
            return false;
        }
        self.0 = Some(Instant::now());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_notes_and_images_only_when_enabled() {
        let dir = std::env::temp_dir().join(format!("rockhound-debug-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let img = RgbaImage::new(2, 2);

        set_dir(None);
        note("ignored");
        assert!(!Throttle::default().due());

        set_dir(Some(dir.clone()));
        let mut throttle = Throttle::default();
        assert!(throttle.due());
        assert!(!throttle.due(), "throttled");
        note("hello");
        image("band", &img);
        assert!(sample(&img).is_some());
        set_dir(None);
        assert!(sample(&img).is_none());

        let log = std::fs::read_to_string(dir.join("log.txt")).unwrap();
        assert!(log.trim_end().ends_with(" hello"), "{log}");
        assert!(dir.join("band.png").exists());
        assert_eq!(std::fs::read_dir(dir.join("readouts")).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
