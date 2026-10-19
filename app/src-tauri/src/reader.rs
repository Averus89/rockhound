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
    crop_line, hud_format, learn_crop, line_height, read_crop, Resolved, SignatureDb, TemplateSet,
};

use crate::debug;

/// OCR is slow (tens of ms) so it runs at most this often.
const TEACHER_INTERVAL: Duration = Duration::from_millis(1000);

/// A slower reader whose text is trusted to teach templates.
pub trait Teacher: Send {
    /// Text read from [`teacher_input`]: dark digits on white.
    fn read_text(&mut self, line: &GrayImage) -> Option<String>;
}

/// Scale of the digit line handed to the teacher, over the template line
/// height (48 px, already well over the ~20 px OCR engines want).
const TEACHER_SCALE: u32 = 1;
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

/// Shipped templates come in digit heights (one per HUD size players sent
/// crops for); the base set is rebuilt around the height on screen.
struct Heights {
    /// Shipped heights, and the one the base set is built for.
    shipped: Vec<u32>,
    current: Option<u32>,
    /// Base templates with the given height's samples first.
    load: fn(Option<u32>) -> TemplateSet,
}

impl Heights {
    /// The shipped height for a crop whose digits are `height` px tall,
    /// when it differs from the current one.
    fn switch_to(&mut self, height: u32) -> Option<u32> {
        let nearest = self
            .shipped
            .iter()
            .copied()
            .min_by_key(|h| (h.abs_diff(height), *h))?;
        (self.current != Some(nearest)).then(|| {
            self.current = Some(nearest);
            nearest
        })
    }
}

pub struct Reader {
    /// Shipped templates followed by the player's learned ones.
    templates: TemplateSet,
    /// How many of `templates` are shipped; only the rest are saved.
    base_len: usize,
    /// Set when there is more than one shipped height to choose from.
    heights: Option<Heights>,
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
        base: TemplateSet,
        learned: TemplateSet,
        template_dir: Option<PathBuf>,
        teacher: Option<Box<dyn Teacher>>,
    ) -> Self {
        let base_len = base.len();
        let mut templates = base;
        templates.extend(learned);
        Self {
            templates,
            base_len,
            heights: None,
            template_dir,
            teacher,
            last_taught: None,
            last_teacher_value: None,
        }
    }

    /// A reader on the shipped templates, picking the shipped digit height
    /// nearest the HUD's once it sees one.
    pub fn bundled(
        learned: TemplateSet,
        template_dir: Option<PathBuf>,
        teacher: Option<Box<dyn Teacher>>,
    ) -> Self {
        let mut reader = Self::new(
            rockhound_core::bundled_templates(),
            learned,
            template_dir,
            teacher,
        );
        let shipped = rockhound_core::bundled_heights();
        if shipped.len() > 1 {
            reader.heights = Some(Heights {
                shipped,
                current: None,
                load: rockhound_core::bundled_templates_for,
            });
        }
        reader
    }

    /// Rebuild the shipped part of the templates when the crop's digit
    /// height is nearer another shipped height, keeping what was learned.
    fn follow_height(&mut self, crop: &RgbaImage) {
        let Some(heights) = self.heights.as_mut() else {
            return;
        };
        let Some(h) = line_height(crop).and_then(|h| heights.switch_to(h)) else {
            return;
        };
        let learned = self.templates.tail(self.base_len);
        self.templates = (heights.load)(Some(h));
        self.base_len = self.templates.len();
        self.templates.extend(learned);
        debug::note(format_args!("using templates for {h} px digits"));
    }

    pub fn read(&mut self, crop: &RgbaImage, db: &SignatureDb) -> Option<Resolved> {
        self.follow_height(crop);
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
            if let Err(e) = self.templates.tail(self.base_len).save_dir(dir) {
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
        let mut templates_only =
            Reader::new(reader.templates.clone(), TemplateSet::default(), None, None);
        let read = templates_only.read(&fixture("hud_17200_pin.png"), &db());
        assert_eq!(read.and_then(|r| r.value), Some(17200));
    }

    #[test]
    fn teacher_values_the_table_cant_explain_are_ignored() {
        let mut reader = Reader::new(
            TemplateSet::default(),
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
        let mut reader = Reader::new(
            TemplateSet::default(),
            TemplateSet::default(),
            None,
            Some(Box::new(Flaky(0))),
        );
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
        assert_eq!(
            input.height(),
            rockhound_core::recognize::LINE_HEIGHT * TEACHER_SCALE + 2 * TEACHER_MARGIN
        );
        assert!(input.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
        let dark = input.pixels().filter(|p| p.0[0] == 0).count();
        assert!(dark > 0 && dark * 3 < input.pixels().len(), "{dark}");
        // Margin rows are blank.
        assert!((0..input.width()).all(|x| input.get_pixel(x, 0).0[0] == 255));
    }

    #[test]
    fn rebuilds_the_shipped_templates_for_the_nearest_height() {
        // Stands in for two shipped heights: one sample per height, so the
        // set's size says which height it was built for.
        fn load(height: Option<u32>) -> TemplateSet {
            let mut set = TemplateSet::default();
            let crop = fixture("hud_17200.png");
            for _ in 0..height.unwrap_or(1) / 10 {
                learn_crop(&crop, 17200, &mut set);
            }
            set
        }
        let one_line = load(Some(10)).len();
        // A sample the player's copy learned; it must survive the switches.
        let mut reader = Reader::new(load(None), load(Some(10)), None, None);
        reader.heights = Some(Heights {
            shipped: vec![12, 30],
            current: None,
            load,
        });

        // The fixture's digits are ~12 px tall.
        let crop = fixture("hud_17200.png");
        assert_eq!(reader.read(&crop, &db()).and_then(|r| r.value), Some(17200));
        assert_eq!(reader.heights.as_ref().unwrap().current, Some(12));
        assert_eq!(reader.base_len, one_line);

        // A bigger HUD switches to the 30 px samples.
        let big = imageops::resize(
            &crop,
            crop.width() * 5 / 2,
            crop.height() * 5 / 2,
            imageops::FilterType::Triangle,
        );
        reader.read(&big, &db());
        assert_eq!(reader.heights.as_ref().unwrap().current, Some(30));
        assert_eq!(reader.base_len, 3 * one_line);
        assert_eq!(reader.templates.len(), 4 * one_line, "learned samples kept");
        assert_eq!(
            reader.heights.as_mut().unwrap().switch_to(31),
            None,
            "already there"
        );
    }

    #[test]
    fn parses_ocr_text() {
        assert_eq!(parse_value("17,200"), Some(17200));
        assert_eq!(parse_value("l7.2OO"), Some(17200));
        assert_eq!(parse_value("17,2x0"), None);
        assert_eq!(parse_value(""), None);
    }
}
