//! Find the signature readout on screen, so players don't have to draw a
//! capture region by hand.
//!
//! The readout sits right of an orange map pin. The pin is the most
//! distinctive thing nearby (saturated, solid, taller than the digits), so
//! find pin-shaped blobs first and accept the first one with a line of
//! text to its right.

use image::{imageops, RgbaImage};

use crate::layout::{find_line, Rect};
use crate::preprocess::normalize;

/// Pin height range in pixels; covers 1080p to 4K HUD scales.
const PIN_HEIGHT: (u32, u32) = (8, 80);
/// The readout starts within this many pin heights right of the pin.
const SEARCH_WIDTH_PER_PIN_HEIGHT: u32 = 8;
/// Padding around the found digits, in line heights, so a value that gains
/// a digit still fits in the returned region.
const PAD_RIGHT_LINES: u32 = 4;

/// Locate the readout in `band` (a screenshot or part of one). Returns a
/// region that contains the digits with room to grow, in `band` pixels.
pub fn find_readout(band: &RgbaImage) -> Option<Rect> {
    let center = (band.width() / 2, band.height() / 2);
    let mut pins: Vec<Rect> = blobs(band, is_pin_colour)
        .into_iter()
        .filter(|b| (PIN_HEIGHT.0..=PIN_HEIGHT.1).contains(&b.height) && b.width * 3 >= b.height)
        .collect();
    // Several signatures can be on screen; the one being read is the one the
    // player looks at, near the middle.
    pins.sort_by_key(|p| {
        let (cx, cy) = (p.x + p.width / 2, p.y + p.height / 2);
        cx.abs_diff(center.0).pow(2) + cy.abs_diff(center.1).pow(2)
    });
    pins.into_iter().find_map(|pin| readout_beside(band, &pin))
}

fn readout_beside(band: &RgbaImage, pin: &Rect) -> Option<Rect> {
    // The digits sit inside the pin's vertical span, to its right. Keeping
    // the search that tight leaves out asteroids and other bright HUD text.
    let h = pin.height;
    let x0 = pin.x + pin.width;
    let area = Rect {
        x: x0,
        y: pin.y,
        width: (h * SEARCH_WIDTH_PER_PIN_HEIGHT).min(band.width() - x0),
        height: h,
    };
    if area.width == 0 {
        return None;
    }
    let crop = imageops::crop_imm(band, area.x, area.y, area.width, area.height).to_image();
    let line = find_line(&crop, &normalize(&crop))?;
    // Digits are clearly shorter than the pin; a full-height blob is
    // something else.
    if line.height * 4 < h || line.height >= h {
        return None;
    }
    let pad = line.height / 2;
    let x = area.x + line.x - pad.min(line.x);
    let y = (area.y + line.y).saturating_sub(pad);
    Some(Rect {
        x,
        y,
        width: (line.width + line.height * PAD_RIGHT_LINES).min(band.width() - x),
        height: (line.height + 2 * pad).min(band.height() - y),
    })
}

/// Saturated orange or red, like the pin; white digits and the brownish
/// readout box don't qualify.
fn is_pin_colour(p: &image::Rgba<u8>) -> bool {
    let [r, g, b, _] = p.0;
    r >= 200 && u16::from(g) * 4 <= u16::from(r) * 3 && u16::from(b) * 5 <= u16::from(r) * 2
}

/// Bounding boxes of 8-connected blobs of pixels matching `keep`.
fn blobs(img: &RgbaImage, keep: impl Fn(&image::Rgba<u8>) -> bool) -> Vec<Rect> {
    let (w, h) = img.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start as usize] || !keep(img.get_pixel(start % w, start / w)) {
            continue;
        }
        seen[start as usize] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let n = ny * w + nx;
                    if !seen[n as usize] && keep(img.get_pixel(nx, ny)) {
                        seen[n as usize] = true;
                        stack.push(n);
                    }
                }
            }
        }
        out.push(Rect {
            x: x0,
            y: y0,
            width: x1 - x0 + 1,
            height: y1 - y0 + 1,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const ORANGE: Rgba<u8> = Rgba([255, 150, 40, 255]);
    const WHITE: Rgba<u8> = Rgba([240, 240, 240, 255]);

    fn scene() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(400, 200, Rgba([20, 25, 30, 255]));
        let mut fill = |x0: u32, y0: u32, w: u32, h: u32, c: Rgba<u8>| {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    img.put_pixel(x, y, c);
                }
            }
        };
        fill(180, 90, 12, 20, ORANGE); // pin
        for i in 0..4 {
            fill(198 + i * 7, 95, 5, 11, WHITE); // digits
        }
        fill(20, 20, 2, 12, ORANGE); // thin orange HUD line, not a pin
        fill(10, 150, 12, 20, ORANGE); // a far pin with no text beside it
        img
    }

    #[test]
    fn finds_digits_beside_the_pin() {
        let r = find_readout(&scene()).unwrap();
        assert!(r.x <= 198 && r.x > 180, "{r:?}");
        assert!(r.y <= 95 && r.y + r.height >= 106, "{r:?}");
        assert!(r.x + r.width >= 198 + 3 * 7 + 5, "{r:?}");
    }

    #[test]
    fn nothing_without_a_pin() {
        let img = RgbaImage::from_pixel(100, 50, Rgba([20, 25, 30, 255]));
        assert_eq!(find_readout(&img), None);
    }
}
