//! Glyph recognition by normalized cross-correlation against digit templates.
//!
//! The readout uses one HUD font at one size, so each digit looks the same
//! every time except for background and blur. Comparing against stored
//! samples of each digit gives a real per-glyph score, which the resolver
//! uses to decide when a read (e.g. 6 vs 8) can't be trusted.

use std::path::Path;

use image::{imageops, GrayImage};

use crate::segment::GlyphBox;

/// Every glyph and template is resampled to this size before comparison.
pub const SAMPLE_WIDTH: u32 = 16;
pub const SAMPLE_HEIGHT: u32 = 24;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("cannot read template directory {path}: {source}")]
    Dir {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot load template image {path}: {source}")]
    Image {
        path: String,
        source: image::ImageError,
    },
    #[error("template file name must start with a digit and '_': {0}")]
    Name(String),
    #[error("no templates found in {0}")]
    Empty(String),
}

/// One recognized glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRead {
    /// Best-matching character, or ',' for a separator.
    pub best: char,
    /// Correlation of the best match, in [-1, 1]. Separators score 1.
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

/// A zero-mean, unit-length vector of a glyph sample.
#[derive(Debug, Clone)]
struct Sample(Vec<f32>);

impl Sample {
    fn from_region(img: &GrayImage, b: &GlyphBox) -> Self {
        let view = imageops::crop_imm(img, b.x, b.y, b.width, b.height).to_image();
        Self::from_image(&view)
    }

    fn from_image(img: &GrayImage) -> Self {
        let resized = imageops::resize(
            img,
            SAMPLE_WIDTH,
            SAMPLE_HEIGHT,
            imageops::FilterType::Triangle,
        );
        let mut v: Vec<f32> = resized.pixels().map(|p| f32::from(p.0[0])).collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        v.iter_mut().for_each(|x| *x -= mean);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter_mut().for_each(|x| *x /= norm);
        }
        Self(v)
    }

    fn correlate(&self, other: &Self) -> f32 {
        self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum()
    }
}

/// Stored digit samples. Several samples per digit are allowed; the best one
/// counts.
#[derive(Debug, Clone, Default)]
pub struct TemplateSet {
    samples: Vec<(char, Sample)>,
}

impl TemplateSet {
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Add a sample of `digit` cut from a normalized image.
    pub fn add(&mut self, digit: char, img: &GrayImage, b: &GlyphBox) {
        self.samples.push((digit, Sample::from_region(img, b)));
    }

    /// Load templates from PNG files named `<digit>_<anything>.png`.
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
            let digit =
                parse_template_name(&name).ok_or_else(|| TemplateError::Name(name.to_string()))?;
            let img = image::open(&path)
                .map_err(|source| TemplateError::Image {
                    path: path.display().to_string(),
                    source,
                })?
                .to_luma8();
            set.samples.push((digit, Sample::from_image(&img)));
        }
        if set.is_empty() {
            return Err(TemplateError::Empty(dir.display().to_string()));
        }
        Ok(set)
    }

    /// Recognize each glyph box.
    pub fn read_glyphs(&self, img: &GrayImage, boxes: &[GlyphBox]) -> Vec<GlyphRead> {
        boxes.iter().map(|b| self.read_one(img, b)).collect()
    }

    fn read_one(&self, img: &GrayImage, b: &GlyphBox) -> GlyphRead {
        if b.is_separator {
            return GlyphRead {
                best: ',',
                score: 1.0,
                runner_up: None,
            };
        }
        let sample = Sample::from_region(img, b);
        // Best score per digit, then the top two digits.
        let mut per_digit = [f32::NEG_INFINITY; 10];
        for (digit, template) in &self.samples {
            let i = digit.to_digit(10).unwrap_or(0) as usize;
            per_digit[i] = per_digit[i].max(sample.correlate(template));
        }
        let mut ranked: Vec<(char, f32)> = per_digit
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_finite())
            .map(|(i, &s)| (char::from_digit(i as u32, 10).unwrap_or('?'), s))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let (best, score) = ranked.first().copied().unwrap_or(('?', -1.0));
        GlyphRead {
            best,
            score,
            runner_up: ranked.get(1).copied(),
        }
    }
}

fn parse_template_name(name: &str) -> Option<char> {
    let mut chars = name.chars();
    let digit = chars.next().filter(char::is_ascii_digit)?;
    (chars.next() == Some('_')).then_some(digit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::segment;
    use image::Luma;

    /// 3x5 bitmap font for synthetic digits, scaled up when drawn.
    pub(crate) const FONT: [[&str; 5]; 10] = [
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

    /// Render a number string with the bitmap font at `scale` px per cell.
    pub(crate) fn render(text: &str, scale: u32) -> GrayImage {
        let cell = 4 * scale;
        let mut img = GrayImage::new(cell * text.len() as u32 + 2 * scale, 7 * scale);
        for (i, ch) in text.chars().enumerate() {
            let x0 = scale + i as u32 * cell;
            if ch == ',' {
                for y in 5 * scale..6 * scale + scale / 2 {
                    for x in x0..x0 + scale {
                        img.put_pixel(x, y, Luma([255]));
                    }
                }
                continue;
            }
            let rows = FONT[ch.to_digit(10).unwrap() as usize];
            for (ry, row) in rows.iter().enumerate() {
                for (rx, c) in row.chars().enumerate() {
                    if c == '#' {
                        for y in 0..scale {
                            for x in 0..scale {
                                img.put_pixel(
                                    x0 + rx as u32 * scale + x,
                                    scale + ry as u32 * scale + y,
                                    Luma([255]),
                                );
                            }
                        }
                    }
                }
            }
        }
        img
    }

    pub(crate) fn font_templates(scale: u32) -> TemplateSet {
        let img = render("0123456789", scale);
        let boxes = segment(&img);
        assert_eq!(boxes.len(), 10);
        let mut set = TemplateSet::default();
        for (i, b) in boxes.iter().enumerate() {
            set.add(char::from_digit(i as u32, 10).unwrap(), &img, b);
        }
        set
    }

    #[test]
    fn reads_rendered_number() {
        let set = font_templates(4);
        let img = render("16,960", 4);
        let reads = set.read_glyphs(&img, &segment(&img));
        let text: String = reads.iter().map(|r| r.best).collect();
        assert_eq!(text, "16,960");
    }

    #[test]
    fn template_names() {
        assert_eq!(parse_template_name("6_a.png"), Some('6'));
        assert_eq!(parse_template_name("x_a.png"), None);
        assert_eq!(parse_template_name("66.png"), None);
    }
}
