//! End-to-end checks on crops from a real 1440p game session.

use rockhound_core::{learn_crop, locate, read_crop, SignatureDb, TemplateSet};

fn fixture(name: &str) -> image::RgbaImage {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    image::open(path).unwrap().to_rgba8()
}

fn db() -> SignatureDb {
    SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap()
}

#[test]
fn learns_from_one_frame_and_reads_another_with_the_pin() {
    let mut templates = TemplateSet::default();
    assert!(learn_crop(&fixture("hud_17200.png"), 17200, &mut templates) > 0);
    let read = read_crop(&fixture("hud_17200_pin.png"), &templates, &db()).unwrap();
    assert_eq!(read.value, Some(17200));
}

#[test]
fn locates_the_readout_in_a_wider_screen_band() {
    // Place the real crop in a dark band, as a slice of the screen would be.
    let crop = fixture("hud_17200_pin.png");
    let mut band = image::RgbaImage::from_pixel(600, 200, image::Rgba([12, 16, 20, 255]));
    image::imageops::overlay(&mut band, &crop, 260, 80);
    let r = locate::find_readout(&band).expect("readout found");

    let mut templates = TemplateSet::default();
    learn_crop(&fixture("hud_17200.png"), 17200, &mut templates);
    let region = image::imageops::crop_imm(&band, r.x, r.y, r.width, r.height).to_image();
    assert_eq!(
        read_crop(&region, &templates, &db()).unwrap().value,
        Some(17200)
    );
}

#[test]
fn locates_the_readout_among_asteroids_in_a_real_screenshot() {
    // 600x240 slice of a 5120x1440 screenshot, with rocks above and beside
    // the readout.
    let band = fixture("hud_band_17200.png");
    let r = locate::find_readout(&band).expect("readout found");

    let mut templates = TemplateSet::default();
    learn_crop(&fixture("hud_17200.png"), 17200, &mut templates);
    let region = image::imageops::crop_imm(&band, r.x, r.y, r.width, r.height).to_image();
    assert_eq!(
        read_crop(&region, &templates, &db()).unwrap().value,
        Some(17200)
    );
}

#[test]
fn reads_digits_with_strong_colour_fringes() {
    // A later session where chromatic aberration paints the white digits
    // blue and yellow; they must not be mistaken for the orange pin.
    let band = fixture("hud_band_21500.png");
    let r = locate::find_readout(&band).expect("readout found");
    // The digits span x 298..358, y 126..137 in this slice.
    assert!(r.x <= 298 && r.x + r.width >= 358, "{r:?}");
    assert!(r.y <= 126 && r.y + r.height >= 137, "{r:?}");

    let region = image::imageops::crop_imm(&band, r.x, r.y, r.width, r.height).to_image();
    let mut templates = TemplateSet::default();
    assert!(learn_crop(&region, 21500, &mut templates) > 0);
    assert_eq!(
        read_crop(&region, &templates, &db()).unwrap().value,
        Some(21500)
    );
}

/// Labelled live crops shipped in data/glyphs/h12 (Alan's 1440p HUD, digit
/// lines ~12 px tall), as (value, image).
fn live_crops() -> Vec<(u32, image::RgbaImage)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/glyphs/h12");
    let mut out: Vec<(String, u32, image::RgbaImage)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let value = name.split('-').next().unwrap().parse().unwrap();
            (name, value, image::open(e.path()).unwrap().to_rgba8())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, v, img)| (v, img)).collect()
}

#[test]
fn bundled_templates_cover_every_digit() {
    let missing = rockhound_core::bundled_templates().missing_digits();
    assert!(missing.is_empty(), "{missing:?}");
}

/// Leave-one-out over Alan's live crops: learn from all but one at their
/// own size, then read that one resized by `scale`. How many read right.
fn leave_one_out(scale: f32) -> usize {
    let crops = live_crops();
    let db = db();
    (0..crops.len())
        .filter(|&i| {
            let mut templates = TemplateSet::default();
            for (j, (value, crop)) in crops.iter().enumerate() {
                if j != i {
                    learn_crop(crop, *value, &mut templates);
                }
            }
            let (value, crop) = &crops[i];
            let crop = if scale == 1.0 {
                crop.clone()
            } else {
                let size = |px: u32| (px as f32 * scale).round() as u32;
                image::imageops::resize(
                    crop,
                    size(crop.width()),
                    size(crop.height()),
                    image::imageops::FilterType::Triangle,
                )
            };
            read_crop(&crop, &templates, &db).and_then(|r| r.value) == Some(*value)
        })
        .count()
}

#[test]
fn bundled_templates_read_the_shipped_live_crops() {
    // Guards glyph packs from other HUD sizes: added to the bundled set,
    // they must not crowd out or outscore the samples these crops need.
    let templates = rockhound_core::bundled_templates_for(Some(12));
    let db = db();
    let crops = live_crops();
    let correct = crops
        .iter()
        .filter(|(value, crop)| {
            read_crop(crop, &templates, &db).and_then(|r| r.value) == Some(*value)
        })
        .count();
    assert!(correct >= 44, "{correct}/{} read correctly", crops.len());
}

#[test]
fn reads_unseen_live_crops() {
    // A ratchet: raise the floor as accuracy improves.
    let correct = leave_one_out(1.0);
    assert!(correct >= 40, "{correct}/46 read correctly");
}

#[test]
fn reads_unseen_live_crops_at_other_hud_sizes() {
    // Resized crops stand in for other resolutions until players send real
    // ones (a game renders small text more crisply than a resize does).
    // 1.5x is a 4K HUD, 0.75x a 1080p one.
    let larger = leave_one_out(1.5);
    let smaller = leave_one_out(0.75);
    assert!(larger >= 39, "4K-sized: {larger}/46 read correctly");
    assert!(
        smaller >= 35,
        "1080p-sized: {smaller}/46 read correctly ({larger} 4K-sized)"
    );
}
