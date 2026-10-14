//! Split a normalized readout image into glyph boxes using column projection.
//!
//! The HUD font is monospaced-ish and the readout is a single line, so a
//! column with no ink is a gap between glyphs. That is far cheaper and more
//! predictable than connected-component labelling for this input.

use image::GrayImage;

use crate::preprocess::otsu;

/// A glyph's bounding box in the normalized image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Short glyph sitting on the baseline: the thousands separator.
    pub is_separator: bool,
}

/// Columns with fewer ink pixels than this are treated as empty, which drops
/// single-pixel speckle from stars and compression noise.
const MIN_COLUMN_INK: u32 = 2;
/// Glyph shorter than this fraction of the line height is a separator (comma).
const SEPARATOR_HEIGHT_RATIO: f32 = 0.5;
/// Blobs shorter than this fraction of the line height (and not on the
/// baseline) are noise.
const NOISE_HEIGHT_RATIO: f32 = 0.25;
/// A span wider than this multiple of the typical digit width holds touching
/// glyphs and gets split evenly.
const MERGED_WIDTH_RATIO: f32 = 1.5;

/// Find glyph boxes, left to right.
pub fn segment(img: &GrayImage) -> Vec<GlyphBox> {
    let threshold = otsu(img);
    let ink = |x: u32, y: u32| img.get_pixel(x, y).0[0] > threshold;

    let spans = column_spans(img, &ink);
    let mut boxes: Vec<GlyphBox> = spans
        .into_iter()
        .filter_map(|(x0, x1)| vertical_bounds(img, &ink, x0, x1))
        .collect();

    let Some(line_height) = boxes.iter().map(|b| b.height).max() else {
        return boxes;
    };
    let baseline = boxes
        .iter()
        .filter(|b| b.height == line_height)
        .map(|b| b.y + b.height)
        .max()
        .unwrap_or(0);

    boxes.retain_mut(|b| {
        let ratio = b.height as f32 / line_height as f32;
        let on_baseline =
            (b.y + b.height).abs_diff(baseline) <= line_height / 4 || b.y + b.height > baseline;
        if ratio < SEPARATOR_HEIGHT_RATIO {
            b.is_separator = on_baseline && ratio >= NOISE_HEIGHT_RATIO / 2.0;
            return b.is_separator;
        }
        true
    });

    split_merged(boxes)
}

fn column_spans(img: &GrayImage, ink: &impl Fn(u32, u32) -> bool) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    let mut start = None;
    for x in 0..img.width() {
        let count = (0..img.height()).filter(|&y| ink(x, y)).count() as u32;
        match (count >= MIN_COLUMN_INK, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                spans.push((s, x));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push((s, img.width()));
    }
    spans
}

fn vertical_bounds(
    img: &GrayImage,
    ink: &impl Fn(u32, u32) -> bool,
    x0: u32,
    x1: u32,
) -> Option<GlyphBox> {
    let rows: Vec<u32> = (0..img.height())
        .filter(|&y| (x0..x1).any(|x| ink(x, y)))
        .collect();
    let (&top, &bottom) = (rows.first()?, rows.last()?);
    Some(GlyphBox {
        x: x0,
        y: top,
        width: x1 - x0,
        height: bottom - top + 1,
        is_separator: false,
    })
}

fn split_merged(boxes: Vec<GlyphBox>) -> Vec<GlyphBox> {
    let mut widths: Vec<u32> = boxes
        .iter()
        .filter(|b| !b.is_separator)
        .map(|b| b.width)
        .collect();
    if widths.len() < 2 {
        return boxes;
    }
    widths.sort_unstable();
    let typical = widths[widths.len() / 2] as f32;

    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        let parts = (b.width as f32 / typical).round() as u32;
        if b.is_separator || (b.width as f32) < typical * MERGED_WIDTH_RATIO || parts < 2 {
            out.push(b);
            continue;
        }
        let step = b.width / parts;
        for i in 0..parts {
            let x = b.x + i * step;
            let width = if i == parts - 1 {
                b.x + b.width - x
            } else {
                step
            };
            out.push(GlyphBox { x, width, ..b });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    /// Draw filled rectangles (x, y, w, h) on a black canvas.
    fn canvas(w: u32, h: u32, rects: &[(u32, u32, u32, u32)]) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let hit = rects
                .iter()
                .any(|&(rx, ry, rw, rh)| x >= rx && x < rx + rw && y >= ry && y < ry + rh);
            Luma([if hit { 255 } else { 0 }])
        })
    }

    #[test]
    fn finds_digits_and_comma() {
        // Two digits, a comma on the baseline, one digit.
        let img = canvas(
            60,
            30,
            &[
                (2, 5, 8, 20),
                (14, 5, 8, 20),
                (25, 21, 3, 6),
                (32, 5, 8, 20),
            ],
        );
        let boxes = segment(&img);
        let kinds: Vec<bool> = boxes.iter().map(|b| b.is_separator).collect();
        assert_eq!(kinds, vec![false, false, true, false]);
    }

    #[test]
    fn drops_floating_speck() {
        let img = canvas(40, 30, &[(2, 5, 8, 20), (20, 2, 3, 3)]);
        assert_eq!(segment(&img).len(), 1);
    }

    #[test]
    fn splits_touching_glyphs() {
        let img = canvas(60, 30, &[(2, 5, 8, 20), (14, 5, 16, 20), (34, 5, 8, 20)]);
        let boxes = segment(&img);
        assert_eq!(boxes.len(), 4);
    }
}
