//! Per-frame reading: digit templates first, a slower OCR "teacher" when
//! they can't read the frame, and learning new templates from what the
//! teacher reads.
//!
//! This lets a fresh install work without shipping templates for every HUD
//! scale: early reads come from OCR while templates fill in, then the fast
//! template path takes over.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::{imageops, GrayImage, Luma, RgbaImage};
use rockhound_core::preprocess::otsu;
use rockhound_core::{
    crop_line, hud_format, learn_crop, read_crop, Resolved, SignatureDb, TemplateSet,
};

use crate::debug;

/// OCR is slow (tens of ms) so it runs at most this often.
const TEACHER_INTERVAL: Duration = Duration::from_millis(1000);

/// A slower reader whose text is trusted to teach templates.
pub trait Teacher: Send {
    /// Text read from [`teacher_input`]: dark digits on white.
    fn read_text(&mut self, line: &GrayImage) -> Option<String>;
}

/// Scale of the digit line handed to the teacher, over the template
/// line height (24 px): OCR engines want text well over 20 px tall.
const TEACHER_SCALE: u32 = 2;
/// White margin around it, in pixels; OCR misses text touching the edge.
const TEACHER_MARGIN: u32 = 24;

/// The readout's digit line cleaned up for OCR: only the digits (no pin or
/// box), thresholded so the colour fringes are gone, dark on white, larger
/// and with a margin.
pub fn teacher_input(crop: &RgbaImage) -> Option<GrayImage> {
    let line = crop_line(crop)?;
    let threshold = otsu(&line);
    let (w, h) = (line.width() * TEACHER_SCALE, line.height() * TEACHER_SCALE);
    let big = imageops::resize(&line, w, h, imageops::FilterType::Triangle);
    let mut out =
        GrayImage::from_pixel(w + 2 * TEACHER_MARGIN, h + 2 * TEACHER_MARGIN, Luma([255]));
    for (x, y, p) in big.enumerate_pixels() {
        let ink = p.0[0] > threshold;
        out.put_pixel(
            x + TEACHER_MARGIN,
            y + TEACHER_MARGIN,
            Luma([if ink { 0 } else { 255 }]),
        );
    }
    Some(out)
}

pub struct Reader {
    templates: TemplateSet,
    /// Where learned templates are saved; `None` keeps them in memory.
    template_dir: Option<PathBuf>,
    teacher: Option<Box<dyn Teacher>>,
    last_taught: Option<Instant>,
    /// Last table-valid teacher read, kept across failed reads: OCR often
    /// returns nothing, and two good reads rarely come back to back.
    last_teacher_value: Option<u32>,
}

impl Reader {
    pub fn new(
        templates: TemplateSet,
        template_dir: Option<PathBuf>,
        teacher: Option<Box<dyn Teacher>>,
    ) -> Self {
        Self {
            templates,
            template_dir,
            teacher,
            last_taught: None,
            last_teacher_value: None,
        }
    }

    pub fn read(&mut self, crop: &RgbaImage, db: &SignatureDb) -> Option<Resolved> {
        let resolved = read_crop(crop, &self.templates, db).filter(|r| r.value.is_some());
        let learning = !self.templates.missing_digits().is_empty();
        if resolved.is_some() && !learning {
            return resolved;
        }
        let taught = self.ask_teacher(crop, db);
        // While templates are incomplete a missing digit is read as its
        // closest lookalike, so the teacher's answer wins when there is one.
        taught
            .map(|value| Resolved {
                raw: hud_format(value),
                value: Some(value),
                // OCR gives no per-glyph scores.
                confidence: 0.0,
                corrected: false,
                matches: db.matches(value),
            })
            .or(resolved)
    }

    /// Ask the teacher (throttled) and learn from its answer when it is
    /// plausible: a value the table explains, read the same way twice in a
    /// row so one OCR slip isn't baked into the templates.
    fn ask_teacher(&mut self, crop: &RgbaImage, db: &SignatureDb) -> Option<u32> {
        let teacher = self.teacher.as_mut()?;
        if self
            .last_taught
            .is_some_and(|t| t.elapsed() < TEACHER_INTERVAL)
        {
            return None;
        }
        self.last_taught = Some(Instant::now());
        let input = teacher_input(crop)?;
        let text = teacher.read_text(&input);
        let value = text
            .as_deref()
            .and_then(parse_value)
            .filter(|v| db.is_valid(*v));
        if debug::enabled() {
            debug::image(
                "ocr-input",
                &image::DynamicImage::ImageLuma8(input).to_rgba8(),
            );
            debug::note(format_args!(
                "OCR {text:?} -> {value:?}; templates read {:?}, missing {:?}",
                read_crop(crop, &self.templates, db).map(|r| r.raw),
                self.templates.missing_digits()
            ));
        }
        let v = value?;
        let confirmed = self.last_teacher_value == Some(v);
        self.last_teacher_value = Some(v);
        if confirmed && learn_crop(crop, v, &mut self.templates) > 0 {
            debug::note(format_args!("learned from {v}"));
            self.save_templates();
        }
        value
    }

    fn save_templates(&self) {
        if let Some(dir) = &self.template_dir {
            if let Err(e) = self.templates.save_dir(dir) {
                // Learning still helps this session; it just won't persist.
                eprintln!("{e}");
            }
        }
    }
}

/// Parse OCR text such as "17,200" or "17.2OO" into a number, mapping the
/// letters OCR confuses with digits.
fn parse_value(text: &str) -> Option<u32> {
    let digits: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | '.' | ' ' | '\''))
        .map(|c| match c {
            'O' | 'o' | 'D' => '0',
            'l' | 'I' | '|' => '1',
            'S' | 's' => '5',
            'B' => '8',
            c => c,
        })
        .collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/rockhound-core/tests/fixtures"
    );

    fn fixture(name: &str) -> RgbaImage {
        image::open(format!("{FIXTURES}/{name}"))
            .unwrap()
            .to_rgba8()
    }

    fn db() -> SignatureDb {
        SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap()
    }

    struct FixedTeacher(&'static str);

    impl Teacher for FixedTeacher {
        fn read_text(&mut self, _line: &GrayImage) -> Option<String> {
            Some(self.0.to_string())
        }
    }

    #[test]
    fn teacher_reads_until_templates_learn() {
        let mut reader = Reader::new(
            TemplateSet::default(),
            None,
            Some(Box::new(FixedTeacher("17,2OO"))),
        );
        let crop = fixture("hud_17200.png");
        let first = reader.read(&crop, &db()).unwrap();
        assert_eq!(first.value, Some(17200));
        assert!(
            reader.templates.is_empty(),
            "one OCR read isn't trusted yet"
        );

        reader.last_taught = None; // skip the throttle
        reader.read(&crop, &db());
        assert!(!reader.templates.is_empty(), "two agreeing reads teach");

        // Templates alone now read the other frame.
        let mut templates_only = Reader::new(reader.templates.clone(), None, None);
        let read = templates_only.read(&fixture("hud_17200_pin.png"), &db());
        assert_eq!(read.and_then(|r| r.value), Some(17200));
    }

    #[test]
    fn teacher_values_the_table_cant_explain_are_ignored() {
        let mut reader = Reader::new(
            TemplateSet::default(),
            None,
            Some(Box::new(FixedTeacher("17,201"))),
        );
        assert!(reader.read(&fixture("hud_17200.png"), &db()).is_none());
    }

    #[test]
    fn teacher_reads_need_not_be_back_to_back() {
        struct Flaky(usize);
        impl Teacher for Flaky {
            fn read_text(&mut self, _line: &GrayImage) -> Option<String> {
                self.0 += 1;
                // Every other OCR read comes back empty.
                Some(
                    if self.0.is_multiple_of(2) {
                        ""
                    } else {
                        "17,200"
                    }
                    .to_string(),
                )
            }
        }
        let mut reader = Reader::new(TemplateSet::default(), None, Some(Box::new(Flaky(0))));
        let crop = fixture("hud_17200.png");
        for _ in 0..3 {
            reader.last_taught = None;
            reader.read(&crop, &db());
        }
        assert!(!reader.templates.is_empty());
    }

    #[test]
    fn teacher_input_is_dark_digits_on_white_with_a_margin() {
        let input = teacher_input(&fixture("hud_17200_pin.png")).unwrap();
        assert_eq!(input.height(), 24 * TEACHER_SCALE + 2 * TEACHER_MARGIN);
        assert!(input.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
        let dark = input.pixels().filter(|p| p.0[0] == 0).count();
        assert!(dark > 0 && dark * 3 < input.pixels().len(), "{dark}");
        // Margin rows are blank.
        assert!((0..input.width()).all(|x| input.get_pixel(x, 0).0[0] == 255));
    }

    #[test]
    fn parses_ocr_text() {
        assert_eq!(parse_value("17,200"), Some(17200));
        assert_eq!(parse_value("l7.2OO"), Some(17200));
        assert_eq!(parse_value("17,2x0"), None);
        assert_eq!(parse_value(""), None);
    }
}
