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

use crate::layout::Rect;
use crate::preprocess::otsu;

/// Lines are scaled to this height before learning or matching.
pub const LINE_HEIGHT: u32 = 48;
/// Samples kept per character. Learning keeps the first ones, so the cap
/// must leave room for every bundled crop or late crops (often the ones
/// with rare digits like 8) are dropped; it still bounds read time.
pub const MAX_SAMPLES_PER_CHAR: usize = 40;
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
        // Templates saved by an older version may use another line height.
        let image = if image.height() == LINE_HEIGHT {
            image
        } else {
            let width = (image.width() * LINE_HEIGHT)
                .div_ceil(image.height().max(1))
                .max(1);
            imageops::resize(&image, width, LINE_HEIGHT, imageops::FilterType::Triangle)
        };
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
pub fn line_image(gray: &GrayImage, line: &Rect) -> GrayImage {
    let crop = imageops::crop_imm(gray, line.x, line.y, line.width, line.height).to_image();
    let width = (line.width * LINE_HEIGHT).div_ceil(line.height).max(1);
    imageops::resize(&crop, width, LINE_HEIGHT, imageops::FilterType::Triangle)
}

/// How the matcher reached a DP cell.
#[derive(Debug, Clone, Copy)]
enum Step {
    None,
    Skip,
    Glyph { prev: usize, sample: usize },
}

/// The HUD's number format as a tiny automaton: one to three digits (no
/// leading zero), then groups of a comma and three digits. Matching only
/// reads it can produce stops stray commas and extra digits from winning.
struct Hud;

impl Hud {
    const START: usize = 0;
    /// 1..=3: leading digits so far; 4..=7: digits after the last comma.
    const STATES: usize = 8;
    const ACCEPT: [usize; 4] = [1, 2, 3, 7];

    fn next(q: usize, ch: char) -> Option<usize> {
        match (q, ch) {
            (Self::START, '1'..='9') => Some(1),
            (1 | 2, '0'..='9') => Some(q + 1),
            (1..=3 | 7, ',') => Some(4),
            (4..=6, '0'..='9') => Some(q + 1),
            _ => None,
        }
    }
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

    /// Append all of `other`'s samples, ignoring the per-character cap.
    pub fn extend(&mut self, other: TemplateSet) {
        self.samples.extend(other.samples);
    }

    /// The samples from index `start` on, e.g. the ones learned after a
    /// bundled base set.
    pub fn tail(&self, start: usize) -> TemplateSet {
        TemplateSet {
            samples: self.samples.get(start..).unwrap_or_default().to_vec(),
        }
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
        let prepared = Line::new(line);

        // best[e][q]: best score explaining columns 0..e and ending in
        // grammar state q; from[e][q]: how we got there.
        let mut best = vec![[f32::NEG_INFINITY; Hud::STATES]; w + 1];
        let mut from: Vec<[Step; Hud::STATES]> = vec![[Step::None; Hud::STATES]; w + 1];
        best[0][Hud::START] = 0.0;
        for e in 1..=w {
            for q in 0..Hud::STATES {
                let skipped = best[e - 1][q] - ink[e - 1] * SKIP_INK_COST;
                if skipped > best[e][q] {
                    best[e][q] = skipped;
                    from[e][q] = Step::Skip;
                }
            }
            for (i, s) in self.samples.iter().enumerate() {
                let sw = s.width() as usize;
                if sw > e {
                    continue;
                }
                let gain = sw as f32 * (prepared.correlate(e - sw, s) - MATCH_BIAS);
                for q in 0..Hud::STATES {
                    let Some(next) = Hud::next(q, s.ch) else {
                        continue;
                    };
                    let score = best[e - sw][q] + gain;
                    if score > best[e][next] {
                        best[e][next] = score;
                        from[e][next] = Step::Glyph { prev: q, sample: i };
                    }
                }
            }
        }

        let Some(mut q) = Hud::ACCEPT
            .iter()
            .copied()
            .filter(|&q| best[w][q] > f32::NEG_INFINITY)
            .max_by(|&a, &b| best[w][a].total_cmp(&best[w][b]))
        else {
            return Vec::new();
        };
        let mut glyphs = Vec::new();
        let mut e = w;
        while e > 0 {
            match from[e][q] {
                Step::Glyph { prev, sample } => {
                    let start = e - self.samples[sample].width() as usize;
                    glyphs.push(self.describe(&prepared, start, self.samples[sample].ch));
                    (e, q) = (start, prev);
                }
                Step::Skip => e -= 1,
                Step::None => break,
            }
        }
        glyphs.reverse();
        glyphs
    }

    /// Score every character at `start` to report the best and runner-up.
    fn describe(&self, line: &Line, start: usize, chosen: char) -> GlyphRead {
        let mut per_char: Vec<(char, f32)> = Vec::new();
        for s in &self.samples {
            if start + s.width() as usize > line.width {
                continue;
            }
            let score = line.correlate(start, s);
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
/// A line image prepared for repeated correlation: pixels as floats plus
/// per-column prefix sums, so each window's mean and spread cost O(1).
struct Line {
    width: usize,
    height: usize,
    pixels: Vec<f32>,
    /// sum[x] / sq[x]: sum of pixel values / squares over columns 0..x.
    sum: Vec<f32>,
    sq: Vec<f32>,
}

impl Line {
    fn new(img: &GrayImage) -> Self {
        let (width, height) = (img.width() as usize, img.height() as usize);
        let pixels: Vec<f32> = img.pixels().map(|p| f32::from(p.0[0])).collect();
        let (mut sum, mut sq) = (vec![0.0; width + 1], vec![0.0; width + 1]);
        for x in 0..width {
            let col = (0..height).map(|y| pixels[y * width + x]);
            sum[x + 1] = sum[x] + col.clone().sum::<f32>();
            sq[x + 1] = sq[x] + col.map(|v| v * v).sum::<f32>();
        }
        Self {
            width,
            height,
            pixels,
            sum,
            sq,
        }
    }

    /// Normalized cross-correlation of sample `s` with the window starting
    /// at column `x`, in [-1, 1]. The sample vector is zero-mean and unit
    /// length, so only the window's spread needs computing.
    fn correlate(&self, x: usize, s: &Sample) -> f32 {
        let sw = s.width() as usize;
        let n = (sw * self.height) as f32;
        let sum = self.sum[x + sw] - self.sum[x];
        let var = (self.sq[x + sw] - self.sq[x]) - sum * sum / n;
        if var <= f32::EPSILON {
            return 0.0;
        }
        let mut dot = 0.0;
        for y in 0..self.height {
            let row = &self.pixels[y * self.width + x..y * self.width + x + sw];
            let tmpl = &s.vector[y * sw..(y + 1) * sw];
            dot += row.iter().zip(tmpl).map(|(a, b)| a * b).sum::<f32>();
        }
        dot / var.sqrt()
    }
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

/// Cell widths allowed, as fractions of the expected width: a "1" is much
/// narrower than other digits in the HUD font.
const CELL_WIDTH_RANGE: (f32, f32) = (0.4, 1.7);
/// Weight of a cell's deviation from its expected width against the ink
/// a cut passes through.
const WIDTH_COST: f32 = 0.3;

/// Split a line into one cell per character, before any templates exist.
///
/// Digits are expected to share the width equally and separators to take a
/// narrower share; the cuts are then chosen together (DP) to pass through
/// as little ink as possible while keeping every cell a plausible width.
/// Placing cuts one at a time failed when a narrow "1" shifted the rest.
fn split_cells(line: &GrayImage, chars: &[char]) -> Option<Vec<(u32, u32)>> {
    let w = line.width() as usize;
    let seps = chars.iter().filter(|c| **c == ',').count() as f32;
    let digits = chars.len() as f32 - seps;
    if digits < 1.0 {
        return None;
    }
    let digit_w = w as f32 / (digits + seps * SEPARATOR_WIDTH_RATIO);
    let expected = |c: char| {
        if c == ',' {
            digit_w * SEPARATOR_WIDTH_RATIO
        } else {
            digit_w
        }
    };
    let ink = column_ink(line);

    // cost[k][x]: best cost of cutting the first k cells with cell k ending
    // at column x; prev[k][x] is where that cell started.
    let n = chars.len();
    let mut cost = vec![vec![f32::INFINITY; w + 1]; n + 1];
    let mut prev = vec![vec![0usize; w + 1]; n + 1];
    cost[0][0] = 0.0;
    for (k, &c) in chars.iter().enumerate() {
        let exp = expected(c);
        let min_w = ((exp * CELL_WIDTH_RANGE.0).floor() as usize).max(2);
        let max_w = (exp * CELL_WIDTH_RANGE.1).ceil() as usize;
        for x0 in 0..w {
            if cost[k][x0].is_infinite() {
                continue;
            }
            for cw in min_w..=max_w {
                let x1 = x0 + cw;
                if x1 > w {
                    break;
                }
                // The last cell must end at the line's edge; inner cuts pay
                // for the ink they pass through.
                let cut = if k + 1 == n {
                    if x1 != w {
                        continue;
                    }
                    0.0
                } else {
                    ink[x1.min(w - 1)]
                };
                let dev = (cw as f32 - exp) / exp;
                let total = cost[k][x0] + cut + WIDTH_COST * dev * dev;
                if total < cost[k + 1][x1] {
                    cost[k + 1][x1] = total;
                    prev[k + 1][x1] = x0;
                }
            }
        }
    }
    if cost[n][w].is_infinite() {
        return None;
    }
    let mut cells = Vec::with_capacity(n);
    let mut x1 = w;
    for k in (1..=n).rev() {
        let x0 = prev[k][x1];
        cells.push((x0 as u32, x1 as u32));
        x1 = x0;
    }
    cells.reverse();
    Some(cells)
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
    fn hud_grammar_allows_only_grouped_numbers() {
        let accepts = |text: &str| {
            text.chars()
                .try_fold(Hud::START, Hud::next)
                .is_some_and(|q| Hud::ACCEPT.contains(&q))
        };
        for ok in ["7", "17,200", "3,370", "100,000", "1,000,000"] {
            assert!(accepts(ok), "{ok}");
        }
        for bad in ["", "0", "017", "1,,000", "17,20", "1720,0", "1,0000", ","] {
            assert!(!accepts(bad), "{bad}");
        }
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
