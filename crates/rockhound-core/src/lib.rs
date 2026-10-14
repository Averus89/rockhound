//! Rockhound core: turns a crop of the Star Citizen signature readout into a
//! validated signature value.
//!
//! Pipeline: [`preprocess`] (crop pixels to a clean grayscale image) ->
//! [`layout`] (find the digit line) -> [`recognize`] (match templates along
//! the line) -> [`resolve`] (pick the reading that exists in the
//! [`signatures`] table, and vote over recent frames).

pub mod layout;
pub mod locate;
pub mod preprocess;
pub mod recognize;
pub mod resolve;
pub mod signatures;

pub use recognize::{GlyphRead, TemplateSet};
pub use resolve::{resolve, Resolved, Voter};
pub use signatures::{SignatureDb, SignatureMatch};

/// The digit line of a crop, scaled for [`TemplateSet`], or `None` when the
/// crop holds nothing that looks like text.
pub fn crop_line(crop: &image::RgbaImage) -> Option<image::GrayImage> {
    let gray = preprocess::normalize(crop);
    let line = layout::find_line(crop, &gray)?;
    Some(recognize::line_image(&gray, &line))
}

/// Run the full single-frame pipeline on an RGBA crop of the readout.
///
/// Returns `None` when nothing that looks like a number is in the crop.
pub fn read_crop(
    crop: &image::RgbaImage,
    templates: &TemplateSet,
    db: &SignatureDb,
) -> Option<Resolved> {
    let reads = templates.read(&crop_line(crop)?);
    if reads.is_empty() {
        return None;
    }
    Some(resolve(&reads, db))
}

/// Learn templates from a crop whose value is known. Returns how many new
/// samples were stored (0 when the crop doesn't split cleanly).
pub fn learn_crop(crop: &image::RgbaImage, value: u32, templates: &mut TemplateSet) -> usize {
    crop_line(crop).map_or(0, |line| templates.learn(&line, &hud_format(value)))
}

/// Format a value the way the HUD shows it: `17200` -> `"17,200"`.
pub fn hud_format(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_format_groups_thousands() {
        assert_eq!(hud_format(7), "7");
        assert_eq!(hud_format(999), "999");
        assert_eq!(hud_format(17200), "17,200");
        assert_eq!(hud_format(1234567), "1,234,567");
    }
}
