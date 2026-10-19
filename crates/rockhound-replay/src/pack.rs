//! Glyph packs: labelled readout crops players send in from the app's
//! "Review and send samples" screen, checked here before they join the
//! bundled templates in `data/glyphs/h<digit height>/`.
//!
//! Packs come from anyone, so every file is checked and re-encoded, and
//! a file that fails a check is left out rather than failing the pack.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use rockhound_core::{learn_crop, line_height, SignatureDb, TemplateSet};

/// More than a full app collection (150), fewer than a flood.
const MAX_FILES: usize = 300;
/// A readout crop is a few dozen pixels; anything larger isn't one.
const MAX_WIDTH: u32 = 400;
const MAX_HEIGHT: u32 = 120;

#[derive(Debug)]
pub struct Imported {
    /// The digit-height folder the samples went into.
    pub folder: PathBuf,
    pub added: usize,
    /// File names left out, with why.
    pub rejected: Vec<(String, String)>,
}

/// Check the crops in `pack` and copy the good ones into `glyphs`, named
/// `<value>-<tag>-<n>.png` so packs never overwrite each other.
pub fn import(pack: &Path, glyphs: &Path, tag: &str, db: &SignatureDb) -> Result<Imported> {
    ensure!(
        !tag.is_empty() && tag.bytes().all(|b| b.is_ascii_alphanumeric()),
        "tag must be letters and digits: {tag:?}"
    );
    let mut names: Vec<String> = std::fs::read_dir(pack)
        .with_context(|| format!("reading {}", pack.display()))?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    ensure!(
        names.len() <= MAX_FILES,
        "{} files; at most {MAX_FILES}",
        names.len()
    );

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for name in names {
        match check(&pack.join(&name), &name, db) {
            Ok(sample) => accepted.push(sample),
            Err(why) => rejected.push((name, why.to_string())),
        }
    }
    if accepted.is_empty() {
        bail!("no usable samples; rejected: {rejected:?}");
    }

    let mut heights: Vec<u32> = accepted.iter().map(|s| s.height).collect();
    heights.sort_unstable();
    let folder = glyphs.join(format!("h{}", heights[heights.len() / 2]));
    std::fs::create_dir_all(&folder)?;
    for (n, s) in accepted.iter().enumerate() {
        // Re-encoded from pixels, so nothing else in the file is kept.
        s.image
            .save(folder.join(format!("{}-{tag}-{}.png", s.value, n + 1)))?;
    }
    Ok(Imported {
        folder,
        added: accepted.len(),
        rejected,
    })
}

struct Sample {
    value: u32,
    height: u32,
    image: image::RgbaImage,
}

fn check(path: &Path, name: &str, db: &SignatureDb) -> Result<Sample> {
    let value: u32 = name
        .strip_suffix(".png")
        .and_then(|stem| stem.split_once('-'))
        .filter(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|(v, _)| v.parse().ok())
        .context("name is not <value>-<n>.png")?;
    ensure!(
        db.is_valid(value) || !db.mixes(value).is_empty(),
        "{value} is not a signature value"
    );
    // Size from the header first, so a huge image is never decoded.
    let (w, h) = image::image_dimensions(path).context("not a PNG")?;
    ensure!(
        w <= MAX_WIDTH && h <= MAX_HEIGHT,
        "{w}x{h} is too big for a readout"
    );
    let image = image::open(path)?.to_rgba8();
    let height = line_height(&image).context("no digit line found")?;
    ensure!(
        learn_crop(&image, value, &mut TemplateSet::default()) > 0,
        "doesn't split into the digits of {value}"
    );
    Ok(Sample {
        value,
        height,
        image,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/glyphs/h12");

    fn db() -> SignatureDb {
        SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rockhound-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_good_crops_into_their_digit_height_folder() {
        let pack = temp_dir("pack");
        let glyphs = temp_dir("pack-glyphs");
        let good = format!("{LIVE}/17200-1.png");
        std::fs::copy(&good, pack.join("17200-1.png")).unwrap();
        std::fs::copy(&good, pack.join("17200-2.png")).unwrap();
        // Wrong label: not a value the table can produce.
        std::fs::copy(&good, pack.join("17201-3.png")).unwrap();
        std::fs::copy(&good, pack.join("notes.png")).unwrap();
        std::fs::write(pack.join("4000-4.png"), b"not a png").unwrap();
        image::RgbaImage::new(500, 20)
            .save(pack.join("4000-5.png"))
            .unwrap();

        let got = import(&pack, &glyphs, "i42", &db()).unwrap();
        assert_eq!(got.added, 2);
        let why: Vec<&str> = got.rejected.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            why,
            ["17201-3.png", "4000-4.png", "4000-5.png", "notes.png"]
        );
        assert!(got.folder.starts_with(&glyphs));
        assert!(got.folder.join("17200-i42-1.png").exists());
        assert!(got.folder.join("17200-i42-2.png").exists());

        assert!(import(&pack, &glyphs, "../x", &db()).is_err(), "unsafe tag");
        std::fs::remove_dir_all(&pack).unwrap();
        std::fs::remove_dir_all(&glyphs).unwrap();
    }

    #[test]
    fn a_pack_with_nothing_usable_fails() {
        let pack = temp_dir("pack-empty");
        std::fs::write(pack.join("readme.txt"), b"hi").unwrap();
        assert!(import(&pack, &pack, "i1", &db()).is_err());
        std::fs::remove_dir_all(&pack).unwrap();
    }
}
