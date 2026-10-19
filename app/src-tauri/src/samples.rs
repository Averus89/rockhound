//! Opt-in glyph samples: readout crops the scanner read, labelled with the
//! value it read, kept so the player can check them and send them in as a
//! "glyph pack". Packs from other screens teach the bundled templates HUD
//! sizes we have no crops for.
//!
//! Only the small readout crop is kept, never a screenshot. Global like
//! [`crate::debug`]: it is a side channel the scan loop writes to and the
//! settings switch turns on and off mid-scan.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// Consecutive frames show the same number; one sample this often is enough.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
/// Samples kept per value, so one long-scanned rock doesn't fill the pack.
const MAX_PER_VALUE: usize = 4;
/// Collection stops here until the player sends or clears the samples.
pub const MAX_SAMPLES: usize = 150;

struct Collector {
    dir: PathBuf,
    last: Option<Instant>,
}

static COLLECTOR: Mutex<Option<Collector>> = Mutex::new(None);

fn collector() -> std::sync::MutexGuard<'static, Option<Collector>> {
    COLLECTOR.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turn collecting on (into `dir`) or off.
pub fn set_dir(dir: Option<PathBuf>) {
    let mut c = collector();
    if c.as_ref().map(|c| &c.dir) != dir.as_ref() {
        *c = dir.map(|dir| Collector { dir, last: None });
    }
}

/// Keep `crop` as a sample of `value` when collecting is on and the
/// throttle and caps allow it.
pub fn offer(crop: &RgbaImage, value: u32) {
    let mut guard = collector();
    let Some(c) = guard.as_mut() else { return };
    if c.last.is_some_and(|t| t.elapsed() < SAMPLE_INTERVAL) {
        return;
    }
    let names = file_names(&c.dir);
    let same_value = names
        .iter()
        .filter(|n| parse_value(n) == Some(value))
        .count();
    if names.len() >= MAX_SAMPLES || same_value >= MAX_PER_VALUE {
        return;
    }
    c.last = Some(Instant::now());
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let saved = std::fs::create_dir_all(&c.dir)
        .map_err(image::ImageError::IoError)
        .and_then(|()| crop.save(c.dir.join(format!("{value}-{ms}.png"))));
    if let Err(e) = saved {
        eprintln!("cannot save glyph sample: {e}");
    }
}

/// A kept sample, for the review screen.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub file: String,
    pub value: u32,
    pub png: Vec<u8>,
}

/// A sample with the value the player confirmed or corrected.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labelled {
    pub file: String,
    pub value: u32,
}

/// Sample file names in `dir`, oldest first.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| parse_value(n).is_some())
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.split_once('-').map(|(_, rest)| rest.to_string()));
    names
}

/// The value of a sample named `<value>-<n>.png`; `None` for any other
/// name, which also keeps names from the UI inside the samples folder.
fn parse_value(name: &str) -> Option<u32> {
    let (value, n) = name.strip_suffix(".png")?.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(value) && digits(n)).then(|| value.parse().ok())?
}

pub fn list(dir: &Path) -> Vec<Sample> {
    file_names(dir)
        .into_iter()
        .filter_map(|file| {
            let png = std::fs::read(dir.join(&file)).ok()?;
            Some(Sample {
                value: parse_value(&file)?,
                file,
                png,
            })
        })
        .collect()
}

pub fn remove(dir: &Path, file: &str) -> Result<(), String> {
    parse_value(file).ok_or_else(|| format!("not a glyph sample: {file}"))?;
    std::fs::remove_file(dir.join(file)).map_err(|e| format!("cannot remove {file}: {e}"))
}

/// Zip `samples` from `dir` as `<value>-<n>.png` into `out_dir`, then
/// delete them so the next pack holds only new ones. Returns the zip path.
pub fn export(dir: &Path, samples: &[Labelled], out_dir: &Path) -> Result<PathBuf, String> {
    if samples.is_empty() {
        return Err("no samples to send".into());
    }
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = out_dir.join(format!("rockhound-glyphs-{ms}.zip"));
    let written = write_zip(dir, samples, &path);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    for s in samples {
        let _ = std::fs::remove_file(dir.join(&s.file));
    }
    Ok(path)
}

fn write_zip(dir: &Path, samples: &[Labelled], path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    // PNGs are already compressed.
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (n, s) in samples.iter().enumerate() {
        parse_value(&s.file).ok_or_else(|| format!("not a glyph sample: {}", s.file))?;
        let png = std::fs::read(dir.join(&s.file)).map_err(|e| e.to_string())?;
        zip.start_file(format!("{}-{}.png", s.value, n + 1), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&png).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The collector is global; tests that switch it must take turns.
    static SWITCH: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rockhound-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn names_must_be_value_dash_number_png() {
        assert_eq!(parse_value("17200-1729.png"), Some(17200));
        assert_eq!(parse_value("17200.png"), None);
        assert_eq!(parse_value("../17200-1.png"), None);
        assert_eq!(parse_value("17200-1.png.txt"), None);
        assert_eq!(parse_value("-1.png"), None);
    }

    #[test]
    fn collects_throttled_reviews_and_exports_a_pack() {
        let _turn = SWITCH.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("samples");
        let out = temp_dir("samples-out");
        std::fs::create_dir_all(&out).unwrap();
        let crop = RgbaImage::new(4, 2);

        set_dir(None);
        offer(&crop, 17200);
        assert!(list(&dir).is_empty(), "off by default");

        set_dir(Some(dir.clone()));
        offer(&crop, 17200);
        offer(&crop, 17200);
        let samples = list(&dir);
        assert_eq!(samples.len(), 1, "throttled");
        assert_eq!(samples[0].value, 17200);
        // Make the next offer due without waiting.
        collector().as_mut().unwrap().last = None;
        offer(&crop, 3000);
        set_dir(None);
        let samples = list(&dir);
        assert_eq!(samples.len(), 2);

        assert!(remove(&dir, "../settings.json").is_err());
        remove(&dir, &samples[1].file).unwrap();

        // The player corrected the value before sending.
        let fixed = [Labelled {
            file: samples[0].file.clone(),
            value: 17300,
        }];
        let zip_path = export(&dir, &fixed, &out).unwrap();
        assert!(list(&dir).is_empty(), "sent samples are cleared");
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(zip.len(), 1);
        assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "17300-1.png");
        assert!(export(&dir, &[], &out).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn caps_samples_per_value() {
        let _turn = SWITCH.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("samples-cap");
        set_dir(Some(dir.clone()));
        for _ in 0..MAX_PER_VALUE + 2 {
            collector().as_mut().unwrap().last = None;
            offer(&RgbaImage::new(2, 2), 4000);
            // Distinct file names; they carry the time in milliseconds.
            std::thread::sleep(Duration::from_millis(2));
        }
        set_dir(None);
        assert_eq!(list(&dir).len(), MAX_PER_VALUE);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
