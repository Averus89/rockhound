//! Rockhound core: turns a crop of the Star Citizen signature readout into a
//! validated signature value.
//!
//! Pipeline: [`preprocess`] (crop pixels to a clean grayscale image) ->
//! [`segment`] (split it into glyph boxes) -> [`recognize`] (match each glyph
//! against digit templates) -> [`resolve`] (pick the reading that exists in the
//! [`signatures`] table, and vote over recent frames).

pub mod preprocess;
pub mod recognize;
pub mod resolve;
pub mod segment;
pub mod signatures;

pub use recognize::{GlyphRead, TemplateSet};
pub use resolve::{resolve, Resolved, Voter};
pub use signatures::{SignatureDb, SignatureMatch};

/// Run the full single-frame pipeline on an RGBA crop of the readout.
///
/// Returns `None` when nothing that looks like a number is in the crop.
pub fn read_crop(
    crop: &image::RgbaImage,
    templates: &TemplateSet,
    db: &SignatureDb,
) -> Option<Resolved> {
    let gray = preprocess::normalize(crop);
    let glyphs = segment::segment(&gray);
    if glyphs.is_empty() {
        return None;
    }
    let reads = templates.read_glyphs(&gray, &glyphs);
    Some(resolve(&reads, db))
}
