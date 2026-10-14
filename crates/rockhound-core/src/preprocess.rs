//! Crop -> clean grayscale image with bright text on a dark background.

use image::{imageops, GrayImage, Luma, RgbaImage};

/// Upscale factor applied before segmentation. The HUD digits are ~13 px tall
/// at 1440p; at native size neighbouring glyphs touch after thresholding.
pub const UPSCALE: u32 = 3;

/// Percentiles used for the contrast stretch, so a few hot pixels (stars,
/// rock highlights) don't flatten the range.
const STRETCH_LOW: f32 = 0.01;
const STRETCH_HIGH: f32 = 0.99;

/// Convert an RGBA crop into a contrast-stretched, upscaled grayscale image
/// where text is bright.
pub fn normalize(crop: &RgbaImage) -> GrayImage {
    let gray = to_gray_without_pin(crop);
    let (w, h) = gray.dimensions();
    let mut big = imageops::resize(
        &gray,
        w * UPSCALE,
        h * UPSCALE,
        imageops::FilterType::Triangle,
    );
    stretch(&mut big);
    let threshold = otsu(&big);
    if bright_fraction(&big, threshold) > 0.5 {
        // Text is always the minority of the crop; if most pixels are
        // "bright", we're looking at dark-ish text over a bright rock.
        imageops::invert(&mut big);
    }
    big
}

/// Grayscale conversion that blanks the orange map-pin icon. It sits just
/// left of the number and was read as a digit by the old scanner.
fn to_gray_without_pin(crop: &RgbaImage) -> GrayImage {
    GrayImage::from_fn(crop.width(), crop.height(), |x, y| {
        let [r, g, b, _] = crop.get_pixel(x, y).0;
        let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
        if r > 150 && r - b > 80 && r > g {
            return Luma([0]);
        }
        // ITU-R BT.601 luma, integer form.
        Luma([((299 * r + 587 * g + 114 * b) / 1000) as u8])
    })
}

fn stretch(img: &mut GrayImage) {
    let hist = histogram(img);
    let total = (img.width() * img.height()) as f32;
    let lo = percentile(&hist, total * STRETCH_LOW);
    let hi = percentile(&hist, total * STRETCH_HIGH);
    if hi <= lo {
        return;
    }
    let range = f32::from(hi - lo);
    for p in img.pixels_mut() {
        let v = (f32::from(p.0[0].clamp(lo, hi) - lo) / range * 255.0).round();
        p.0[0] = v as u8;
    }
}

fn histogram(img: &GrayImage) -> [u32; 256] {
    let mut hist = [0u32; 256];
    for p in img.pixels() {
        hist[usize::from(p.0[0])] += 1;
    }
    hist
}

fn percentile(hist: &[u32; 256], rank: f32) -> u8 {
    let mut seen = 0.0;
    for (value, &count) in hist.iter().enumerate() {
        seen += count as f32;
        if seen >= rank {
            return value as u8;
        }
    }
    255
}

/// Otsu's threshold: the gray level that best separates two pixel classes.
pub fn otsu(img: &GrayImage) -> u8 {
    let hist = histogram(img);
    let total = f64::from(img.width() * img.height());
    let sum_all: f64 = hist
        .iter()
        .enumerate()
        .map(|(v, &c)| v as f64 * f64::from(c))
        .sum();
    let (mut weight_bg, mut sum_bg) = (0.0, 0.0);
    let (mut best, mut best_var) = (0u8, 0.0);
    for (v, &count) in hist.iter().enumerate() {
        weight_bg += f64::from(count);
        if weight_bg == 0.0 {
            continue;
        }
        let weight_fg = total - weight_bg;
        if weight_fg == 0.0 {
            break;
        }
        sum_bg += v as f64 * f64::from(count);
        let mean_bg = sum_bg / weight_bg;
        let mean_fg = (sum_all - sum_bg) / weight_fg;
        let between = weight_bg * weight_fg * (mean_bg - mean_fg).powi(2);
        if between > best_var {
            best_var = between;
            best = v as u8;
        }
    }
    best
}

fn bright_fraction(img: &GrayImage, threshold: u8) -> f32 {
    let bright = img.pixels().filter(|p| p.0[0] > threshold).count();
    bright as f32 / (img.width() * img.height()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn otsu_splits_two_levels() {
        let img = GrayImage::from_fn(10, 1, |x, _| Luma([if x < 5 { 20 } else { 200 }]));
        let t = otsu(&img);
        assert!((20..200).contains(&t));
    }

    #[test]
    fn dark_text_on_bright_rock_is_inverted() {
        // 80% bright background with a dark stripe of "text".
        let crop = RgbaImage::from_fn(10, 10, |x, _| {
            if x < 2 {
                Rgba([30, 30, 30, 255])
            } else {
                Rgba([220, 220, 220, 255])
            }
        });
        let out = normalize(&crop);
        assert!(out.get_pixel(0, 0).0[0] > out.get_pixel(out.width() - 1, 0).0[0]);
    }

    #[test]
    fn orange_pin_is_blanked() {
        let crop = RgbaImage::from_pixel(2, 2, Rgba([240, 140, 30, 255]));
        let gray = to_gray_without_pin(&crop);
        assert!(gray.pixels().all(|p| p.0[0] == 0));
    }
}
