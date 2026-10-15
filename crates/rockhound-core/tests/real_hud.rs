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
