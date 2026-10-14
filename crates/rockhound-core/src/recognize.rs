//! Glyph recognition by template matching along the digit line.
//!
//! The readout uses one HUD font at one size, but neighbouring glyphs touch
//! at ~11 px, so the line can't be split into glyphs up front. Instead the
//! line is scaled to a fixed height and a dynamic program picks the sequence
//! of templates that best explains it, left to right. Each template is a
//! glyph's full advance cell (glyph plus its spacing), learned from frames
//! whose value is known.

use std::path::Path;

use image::{imageops, GrayImage};

use crate::layout::TextLine;
use crate::preprocess::otsu;

/// Lines are scaled to this height before learning or matching.
pub const LINE_HEIGHT: u32 = 24;
/// Samples kept per character; more adds time without adding accuracy.
pub const MAX_SAMPLES_PER_CHAR: usize = 12;
/// Separator cell width relative to a digit cell, used to split a labelled
/// line into cells before any templates exist.
const SEPARATOR_WIDTH_RATIO: f32 = 0.4;
/// Cost per inked column the matcher leaves unexplained, so it can't skip
/// a hard digit to keep its average score high.
const SKIP_INK_COST: f32 = 1.0;
/// Correlation a match must beat to be worth more than leaving its columns
/// unexplained, so weak partial matches don't fill the gaps between glyphs.
const MATCH_BIAS: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("cannot read template directory {path}: {source}")]
    Dir {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot load or save template image {path}: {source}")]
    Image {
        path: String,
        source: image::ImageError,
    },
    #[error("template file name must start with a digit or 'comma' and '_': {0}")]
    Name(String),
    #[error("no templates found in {0}")]
    Empty(String),
}

/// One recognized glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRead {
    /// Best-matching character: a digit or ','.
    pub best: char,
    /// Correlation of the best match, in [-1, 1].
    pub score: f32,
    /// Second-best character and its score, if any template disagreed.
    pub runner_up: Option<(char, f32)>,
}

impl GlyphRead {
    /// Gap between best and runner-up; small means the glyph is ambiguous.
    pub fn margin(&self) -> f32 {
        self.runner_up.map_or(1.0, |(_, s)| self.score - s)
    }
}

#[derive(Debug, Clone)]
struct Sample {
    ch: char,
    image: GrayImage,
    /// Zero-mean, unit-length pixels of `image`.
    vector: Vec<f32>,
}

impl Sample {
    fn new(ch: char, image: GrayImage) -> Self {
        let vector = unit_vector(image.pixels().map(|p| f32::from(p.0[0])));
        Self { ch, image, vector }
    }

    fn width(&self) -> u32 {
        self.image.width()
    }
}

fn unit_vector(values: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut v: Vec<f32> = values.collect();
    let mean = v.iter().sum::<f32>() / v.len().max(1) as f32;
    v.iter_mut().for_each(|x| *x -= mean);
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// Crop the text line out of a normalized image and scale it to
/// [`LINE_HEIGHT`].
pub fn line_image(gray: &GrayImage, line: &TextLine) -> GrayImage {
    let crop = imageops::crop_imm(gray, line.x, line.y, line.width, line.height).to_image();
    let width = (line.width * LINE_HEIGHT).div_ceil(line.height).max(1);
    imageops::resize(&crop, width, LINE_HEIGHT, imageops::FilterType::Triangle)
}

/// Stored glyph samples.
#[derive(Debug, Clone, Default)]
pub struct TemplateSet {
    samples: Vec<Sample>,
}

impl TemplateSet {
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Digits that have no sample yet.
    pub fn missing_digits(&self) -> Vec<char> {
        ('0'..='9')
            .filter(|d| !self.samples.iter().any(|s| s.ch == *d))
            .collect()
    }

    fn count(&self, ch: char) -> usize {
        self.samples.iter().filter(|s| s.ch == ch).count()
    }

    /// Learn from a scaled line image whose text is known, e.g. "17,200".
    /// Returns how many new samples were stored.
    pub fn learn(&mut self, line: &GrayImage, text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        let Some(cells) = split_cells(line, &chars) else {
            return 0;
        };
        let ink = column_ink(line);
        let mut added = 0;
        for (&ch, (x0, x1)) in chars.iter().zip(cells) {
            if self.count(ch) >= MAX_SAMPLES_PER_CHAR {
                continue;
            }
            // Store the glyph without the blank columns around it: cell cuts
            // land a pixel or two either way, and padding at varying offsets
            // would make samples of one digit disagree on alignment.
            let inked = |x: &u32| ink[*x as usize] > 0.0;
            let (Some(x0), Some(x1)) = ((x0..x1).find(inked), (x0..x1).rfind(inked)) else {
                continue;
            };
            let x1 = x1 + 1;
            let image = imageops::crop_imm(line, x0, 0, x1 - x0, line.height()).to_image();
            self.samples.push(Sample::new(ch, image));
            added += 1;
        }
        added
    }

    /// Read a scaled line image as a sequence of glyphs.
    pub fn read(&self, line: &GrayImage) -> Vec<GlyphRead> {
        if self.samples.is_empty() || line.width() == 0 {
            return Vec::new();
        }
        let w = line.width() as usize;
        let ink = column_ink(line);

        // best[e]: best score explaining columns 0..e; from[e]: how we got there.
        let mut best = vec![f32::NEG_INFINITY; w + 1];
        let mut from: Vec<Option<(usize, usize)>> = vec![None; w + 1]; // (start, sample)
        best[0] = 0.0;
        for e in 1..=w {
            best[e] = best[e - 1] - ink[e - 1] * SKIP_INK_COST;
            for (i, s) in self.samples.iter().enumerate() {
                let sw = s.width() as usize;
                if sw > e || best[e - sw] == f32::NEG_INFINITY {
                    continue;
                }
                let score = best[e - sw] + sw as f32 * (correlate(line, e - sw, s) - MATCH_BIAS);
                if score > best[e] {
                    best[e] = score;
                    from[e] = Some((e - sw, i));
                }
            }
        }

        let mut glyphs = Vec::new();
        let mut e = w;
        while e > 0 {
            match from[e] {
                Some((start, i)) => {
                    glyphs.push(self.describe(line, start, self.samples[i].ch));
                    e = start;
                }
                None => e -= 1,
            }
        }
        glyphs.reverse();
        glyphs
    }

    /// Score every character at `start` to report the best and runner-up.
    fn describe(&self, line: &GrayImage, start: usize, chosen: char) -> GlyphRead {
        let mut per_char: Vec<(char, f32)> = Vec::new();
        for s in &self.samples {
            if start + s.width() as usize > line.width() as usize {
                continue;
            }
            let score = correlate(line, start, s);
            match per_char.iter_mut().find(|(c, _)| *c == s.ch) {
                Some(entry) => entry.1 = entry.1.max(score),
                None => per_char.push((s.ch, score)),
            }
        }
        per_char.sort_by(|a, b| b.1.total_cmp(&a.1));
        let score = per_char
            .iter()
            .find(|(c, _)| *c == chosen)
            .map_or(0.0, |(_, s)| *s);
        let runner_up = per_char.iter().find(|(c, _)| *c != chosen).copied();
        GlyphRead {
            best: chosen,
            score,
            runner_up,
        }
    }

    /// Load templates from PNG files named `<char>_<anything>.png`, where
    /// `<char>` is a digit or `comma`.
    pub fn load_dir(dir: &Path) -> Result<Self, TemplateError> {
        let entries = std::fs::read_dir(dir).map_err(|source| TemplateError::Dir {
            path: dir.display().to_string(),
            source,
        })?;
        let mut set = Self::default();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let ch =
                parse_template_name(&name).ok_or_else(|| TemplateError::Name(name.to_string()))?;
            let image = image::open(&path)
                .map_err(|source| TemplateError::Image {
                    path: path.display().to_string(),
                    source,
                })?
                .to_luma8();
            set.samples.push(Sample::new(ch, image));
        }
        if set.is_empty() {
            return Err(TemplateError::Empty(dir.display().to_string()));
        }
        Ok(set)
    }

    /// Write every sample to `dir` as `<char>_<n>.png`, replacing what's there.
    pub fn save_dir(&self, dir: &Path) -> Result<(), TemplateError> {
        let dir_err = |source| TemplateError::Dir {
            path: dir.display().to_string(),
            source,
        };
        std::fs::create_dir_all(dir).map_err(dir_err)?;
        for entry in std::fs::read_dir(dir).map_err(dir_err)?.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.extension().is_some_and(|e| e == "png") && parse_template_name(&name).is_some()
            {
                std::fs::remove_file(&path).map_err(dir_err)?;
            }
        }
        for (n, s) in self.samples.iter().enumerate() {
            let stem = if s.ch == ',' {
                "comma".to_string()
            } else {
                s.ch.to_string()
            };
            let path = dir.join(format!("{stem}_{n}.png"));
            s.image.save(&path).map_err(|source| TemplateError::Image {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }
}

fn parse_template_name(name: &str) -> Option<char> {
    let (stem, _) = name.split_once('_')?;
    match stem {
        "comma" => Some(','),
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => d.chars().next(),
        _ => None,
    }
}

/// Correlation between the template and the line window starting at `x`.
fn correlate(line: &GrayImage, x: usize, s: &Sample) -> f32 {
    let (sw, h) = (s.width(), line.height().min(s.image.height()));
    let window = unit_vector(
        (0..h)
            .flat_map(|y| (0..sw).map(move |dx| f32::from(line.get_pixel(x as u32 + dx, y).0[0]))),
    );
    window.iter().zip(&s.vector).map(|(a, b)| a * b).sum()
}

/// Fraction of each column that is ink.
fn column_ink(line: &GrayImage) -> Vec<f32> {
    let t = otsu(line);
    (0..line.width())
        .map(|x| {
            let n = (0..line.height())
                .filter(|&y| line.get_pixel(x, y).0[0] > t)
                .count();
            n as f32 / line.height() as f32
        })
        .collect()
}

/// Split a line into one cell per character, before any templates exist:
/// give digits equal widths and separators a narrower share, then move each
/// cut to the emptiest nearby column.
fn split_cells(line: &GrayImage, chars: &[char]) -> Option<Vec<(u32, u32)>> {
    let w = line.width() as f32;
    let seps = chars.iter().filter(|c| **c == ',').count() as f32;
    let digits = chars.len() as f32 - seps;
    if digits < 1.0 {
        return None;
    }
    let digit_w = w / (digits + seps * SEPARATOR_WIDTH_RATIO);
    let ink = column_ink(line);
    let reach = (digit_w / 4.0).round() as i64;

    let mut cuts = vec![0u32];
    let mut pos = 0.0;
    for c in &chars[..chars.len() - 1] {
        pos += if *c == ',' {
            digit_w * SEPARATOR_WIDTH_RATIO
        } else {
            digit_w
        };
        let guess = pos.round() as i64;
        let prev = i64::from(*cuts.last()?);
        let cut = (guess - reach..=guess + reach)
            .filter(|&x| x > prev && x < w as i64)
            .min_by(|&a, &b| {
                ink[a as usize]
                    .total_cmp(&ink[b as usize])
                    .then((a - guess).abs().cmp(&(b - guess).abs()))
            })?;
        cuts.push(cut as u32);
    }
    cuts.push(line.width());
    let cells: Vec<(u32, u32)> = cuts.windows(2).map(|p| (p[0], p[1])).collect();
    cells.iter().all(|(a, b)| b - a >= 2).then_some(cells)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::layout::find_line;
    use crate::preprocess::normalize;
    use image::{Rgba, RgbaImage};

    /// 3x5 bitmap font for synthetic digits.
    const FONT: [[&str; 5]; 10] = [
        ["###", "#.#", "#.#", "#.#", "###"],
        [".#.", "##.", ".#.", ".#.", "###"],
        ["###", "..#", "###", "#..", "###"],
        ["###", "..#", "###", "..#", "###"],
        ["#.#", "#.#", "###", "..#", "..#"],
        ["###", "#..", "###", "..#", "###"],
        ["###", "#..", "###", "#.#", "###"],
        ["###", "..#", "..#", "..#", "..#"],
        ["###", "#.#", "###", "#.#", "###"],
        ["###", "#.#", "###", "..#", "###"],
    ];

    /// Render text white-on-dark with `scale` px per font cell, digits on a
    /// 4-cell pitch (touching at scale 1 like the real HUD) and commas on a
    /// 2-cell pitch.
    pub(crate) fn render(text: &str, scale: u32) -> RgbaImage {
        let width: u32 = text
            .chars()
            .map(|c| if c == ',' { 2 } else { 4 })
            .sum::<u32>()
            + 4;
        let mut img = RgbaImage::from_pixel(width * scale, 9 * scale, Rgba([25, 30, 35, 255]));
        let mut put = |cx: u32, cy: u32| {
            for y in 0..scale {
                for x in 0..scale {
                    img.put_pixel(cx * scale + x, cy * scale + y, Rgba([235, 235, 235, 255]));
                }
            }
        };
        let mut x0 = 2;
        for ch in text.chars() {
            if ch == ',' {
                put(x0, 6);
                put(x0, 7);
                x0 += 2;
                continue;
            }
            for (ry, row) in FONT[ch.to_digit(10).unwrap() as usize].iter().enumerate() {
                for (rx, c) in row.chars().enumerate() {
                    if c == '#' {
                        put(x0 + rx as u32, 2 + ry as u32);
                    }
                }
            }
            x0 += 4;
        }
        img
    }

    pub(crate) fn line_of(text: &str, scale: u32) -> GrayImage {
        let rgba = render(text, scale);
        let gray = normalize(&rgba);
        let line = find_line(&rgba, &gray).expect("line found");
        line_image(&gray, &line)
    }

    pub(crate) fn trained(scale: u32) -> TemplateSet {
        let mut set = TemplateSet::default();
        for text in ["12,345", "67,890", "1,234", "56,789,012"] {
            assert!(set.learn(&line_of(text, scale), text) > 0, "learned {text}");
        }
        assert!(set.missing_digits().is_empty());
        set
    }

    fn read_text(set: &TemplateSet, text: &str, scale: u32) -> String {
        set.read(&line_of(text, scale))
            .iter()
            .map(|g| g.best)
            .collect()
    }

    #[test]
    fn reads_numbers_it_was_not_trained_on() {
        let set = trained(3);
        for text in ["16,960", "3,170", "47,550", "8,480"] {
            assert_eq!(read_text(&set, text, 3), text);
        }
    }

    #[test]
    fn learns_and_reads_at_native_hud_scale() {
        // Scale 2 puts digits at ~10 px, like the real 1440p HUD.
        let set = trained(2);
        assert_eq!(read_text(&set, "16,960", 2), "16,960");
    }

    #[test]
    fn save_and_load_round_trip() {
        let set = trained(3);
        let dir = std::env::temp_dir().join(format!("rockhound-tpl-{}", std::process::id()));
        set.save_dir(&dir).unwrap();
        let loaded = TemplateSet::load_dir(&dir).unwrap();
        assert_eq!(loaded.len(), set.len());
        assert_eq!(read_text(&loaded, "3,170", 3), "3,170");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn template_names() {
        assert_eq!(parse_template_name("6_a.png"), Some('6'));
        assert_eq!(parse_template_name("comma_3.png"), Some(','));
        assert_eq!(parse_template_name("x_a.png"), None);
        assert_eq!(parse_template_name("66.png"), None);
    }
}
