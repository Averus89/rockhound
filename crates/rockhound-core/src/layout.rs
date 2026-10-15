//! Find the line of digits inside a readout crop.
//!
//! The digits are only ~11 px tall at 1440p and neighbouring glyphs often
//! touch, so this module only finds the whole line; [`crate::recognize`]
//! splits it into glyphs by matching templates.

use image::{GrayImage, RgbaImage};

use crate::preprocess::otsu;

/// Axis-aligned rectangle in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Ink clusters this saturated are the orange map pin (or other coloured HUD
/// art), not the white digits.
const MAX_TEXT_SATURATION: f32 = 0.45;
/// Largest gap inside one number, relative to the glyph height.
const MAX_GAP_TO_HEIGHT: f32 = 0.6;
/// Lines shorter than this many pixels are noise.
const MIN_LINE_HEIGHT: u32 = 5;

/// Locate the digit line. `rgba` is the raw crop (for colour), `gray` its
/// normalized version from [`crate::preprocess::normalize`].
pub fn find_line(rgba: &RgbaImage, gray: &GrayImage) -> Option<Rect> {
    let threshold = otsu(gray);
    let ink = |x: u32, y: u32| gray.get_pixel(x, y).0[0] > threshold;
    let (w, h) = gray.dimensions();

    // Column clusters split at gaps of 2+ empty columns, minus the pin.
    let col_has_ink: Vec<bool> = (0..w).map(|x| (0..h).any(|y| ink(x, y))).collect();
    let clusters: Vec<(u32, u32)> = runs(&col_has_ink, 2)
        .into_iter()
        .filter(|&(x0, x1)| saturation(rgba, &ink, x0, x1, 0, h) <= MAX_TEXT_SATURATION)
        .collect();

    // Rejoin pieces of one number: a comma or a narrow "1" can leave a gap of
    // a few pixels, which scales with the font, so the limit does too.
    let ink_height = |(x0, x1): (u32, u32)| {
        let rows: Vec<u32> = (0..h).filter(|&y| (x0..x1).any(|x| ink(x, y))).collect();
        rows.last().map_or(0, |last| last - rows[0] + 1)
    };
    let mut groups: Vec<(u32, u32)> = Vec::new();
    for c in clusters {
        match groups.last_mut() {
            Some(g)
                if (c.0 - g.1) as f32
                    <= MAX_GAP_TO_HEIGHT * ink_height(*g).max(ink_height(c)) as f32 =>
            {
                g.1 = c.1;
            }
            _ => groups.push(c),
        }
    }
    let text = groups.into_iter().max_by_key(|&(x0, x1)| x1 - x0)?;

    // Rows of the chosen cluster: the longest run of inked rows, allowing a
    // 1-row break (anti-aliasing can leave a faint row inside a glyph).
    let row_has_ink: Vec<bool> = (0..h)
        .map(|y| (text.0..text.1).filter(|&x| ink(x, y)).count() >= 2)
        .collect();
    let (y0, y1) = runs(&row_has_ink, 2)
        .into_iter()
        .max_by_key(|&(a, b)| b - a)?;
    if y1 - y0 < MIN_LINE_HEIGHT {
        return None;
    }

    // Re-trim columns to the chosen rows so specks above or below don't widen
    // the line.
    let cols: Vec<u32> = (text.0..text.1)
        .filter(|&x| (y0..y1).any(|y| ink(x, y)))
        .collect();
    let (&x0, &x1) = (cols.first()?, cols.last()?);
    Some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0,
    })
}

/// Half-open runs of `true`, merging runs separated by fewer than `min_gap`
/// `false` entries.
fn runs(flags: &[bool], min_gap: usize) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut start = None;
    for (i, &f) in flags.iter().chain(std::iter::once(&false)).enumerate() {
        match (f, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                match out.last_mut() {
                    Some(last) if s - (last.1 as usize) < min_gap => last.1 = i as u32,
                    _ => out.push((s as u32, i as u32)),
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// HSV saturation of the mean colour of the inked pixels in a rectangle.
///
/// The mean, not per-pixel saturation: the game's chromatic aberration
/// fringes white digits with blue and yellow, which average back to white,
/// while the pin stays orange.
fn saturation(
    rgba: &RgbaImage,
    ink: &impl Fn(u32, u32) -> bool,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
) -> f32 {
    let mut sum = [0u64; 3];
    for y in y0..y1 {
        for x in x0..x1 {
            if ink(x, y) {
                let p = rgba.get_pixel(x, y).0;
                for (s, c) in sum.iter_mut().zip(p) {
                    *s += u64::from(c);
                }
            }
        }
    }
    let max = sum.iter().copied().max().unwrap_or(0);
    let min = sum.iter().copied().min().unwrap_or(0);
    if max == 0 {
        0.0
    } else {
        (max - min) as f32 / max as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::normalize;
    use image::Rgba;

    fn crop(rects: &[(u32, u32, u32, u32, [u8; 3])]) -> RgbaImage {
        RgbaImage::from_fn(60, 30, |x, y| {
            for &(rx, ry, rw, rh, c) in rects {
                if x >= rx && x < rx + rw && y >= ry && y < ry + rh {
                    return Rgba([c[0], c[1], c[2], 255]);
                }
            }
            Rgba([20, 25, 30, 255])
        })
    }

    #[test]
    fn skips_orange_pin_and_finds_digits() {
        const WHITE: [u8; 3] = [240, 240, 240];
        let img = crop(&[
            (2, 2, 8, 24, [255, 160, 40]), // pin: taller, orange
            (16, 8, 5, 12, WHITE),
            (22, 8, 5, 12, WHITE),
            (28, 8, 5, 12, WHITE),
        ]);
        let line = find_line(&img, &normalize(&img)).unwrap();
        assert_eq!((line.x, line.y, line.width, line.height), (16, 8, 17, 12));
    }

    #[test]
    fn runs_merge_small_gaps() {
        let f = [true, true, false, true, false, false, true];
        assert_eq!(runs(&f, 2), vec![(0, 4), (6, 7)]);
    }
}
