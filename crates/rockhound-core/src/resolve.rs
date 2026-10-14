//! Turn glyph reads into a signature value the table can explain.

use std::collections::VecDeque;

use crate::recognize::GlyphRead;
use crate::signatures::{SignatureDb, SignatureMatch};

/// A glyph whose best and runner-up scores are closer than this is ambiguous
/// (the classic case is 6 vs 8 on a blurry frame).
pub const AMBIGUOUS_MARGIN: f32 = 0.08;
/// At most this many ambiguous glyphs are tried both ways (2^n candidates).
const MAX_AMBIGUOUS: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// What the glyphs say, separators included.
    pub raw: String,
    /// The value the table can explain, if any.
    pub value: Option<u32>,
    /// Lowest glyph score in the chosen reading.
    pub confidence: f32,
    /// True when an ambiguous glyph was swapped to its runner-up.
    pub corrected: bool,
    pub matches: Vec<SignatureMatch>,
}

/// Pick the highest-scoring reading that exists in the signature table.
///
/// Only ambiguous glyphs may be swapped, so a clear misread is never
/// "corrected" into some unrelated valid value.
pub fn resolve(reads: &[GlyphRead], db: &SignatureDb) -> Resolved {
    let raw: String = reads.iter().map(|r| r.best).collect();
    let digits: Vec<&GlyphRead> = reads.iter().filter(|r| r.best != ',').collect();

    let mut ambiguous: Vec<usize> = (0..digits.len())
        .filter(|&i| digits[i].runner_up.is_some() && digits[i].margin() < AMBIGUOUS_MARGIN)
        .collect();
    ambiguous.sort_by(|&a, &b| digits[a].margin().total_cmp(&digits[b].margin()));
    ambiguous.truncate(MAX_AMBIGUOUS);

    let mut best: Option<(f32, u32, f32, bool)> = None;
    for mask in 0u32..(1 << ambiguous.len()) {
        let mut text = String::with_capacity(digits.len());
        let (mut total, mut lowest) = (0.0, f32::INFINITY);
        for (i, read) in digits.iter().enumerate() {
            let swap = ambiguous
                .iter()
                .position(|&a| a == i)
                .is_some_and(|bit| mask & (1 << bit) != 0);
            let (ch, score) = match (swap, read.runner_up) {
                (true, Some(alt)) => alt,
                _ => (read.best, read.score),
            };
            text.push(ch);
            total += score;
            lowest = lowest.min(score);
        }
        let Ok(value) = text.parse::<u32>() else {
            continue;
        };
        if !db.is_valid(value) {
            continue;
        }
        if best.is_none_or(|(t, ..)| total > t) {
            best = Some((total, value, lowest, mask != 0));
        }
    }

    match best {
        Some((_, value, confidence, corrected)) => Resolved {
            raw,
            value: Some(value),
            confidence,
            corrected,
            matches: db.matches(value),
        },
        None => Resolved {
            raw,
            value: None,
            confidence: digits.iter().map(|r| r.score).fold(f32::INFINITY, f32::min),
            corrected: false,
            matches: Vec::new(),
        },
    }
}

/// Majority vote over the last few frames, so one bad frame never reaches
/// the UI and a real change shows up within a few reads.
#[derive(Debug, Clone)]
pub struct Voter {
    window: VecDeque<Option<u32>>,
    size: usize,
    needed: usize,
}

impl Voter {
    /// Show a value once `needed` of the last `size` frames agree on it.
    pub fn new(size: usize, needed: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(size),
            size,
            needed,
        }
    }

    pub fn push(&mut self, value: Option<u32>) -> Option<u32> {
        if self.window.len() == self.size {
            self.window.pop_front();
        }
        self.window.push_back(value);
        let latest = self.window.iter().rev().flatten().next()?;
        let agree = self.window.iter().filter(|v| **v == Some(*latest)).count();
        (agree >= self.needed).then_some(*latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatures::tests::sample_db;

    fn read(best: char, score: f32, runner_up: Option<(char, f32)>) -> GlyphRead {
        GlyphRead {
            best,
            score,
            runner_up,
        }
    }

    fn clear(text: &str) -> Vec<GlyphRead> {
        text.chars().map(|c| read(c, 0.95, None)).collect()
    }

    #[test]
    fn clean_read_resolves() {
        let r = resolve(&clear("16,960"), &sample_db());
        assert_eq!(r.value, Some(16960));
        assert!(!r.corrected);
        assert_eq!(r.matches[0].name, "Copper");
    }

    #[test]
    fn ambiguous_eight_becomes_six() {
        let mut reads = clear("16,980");
        reads[4] = read('8', 0.81, Some(('6', 0.78)));
        let r = resolve(&reads, &sample_db());
        assert_eq!(r.value, Some(16960));
        assert!(r.corrected);
    }

    #[test]
    fn confident_misread_is_not_corrected() {
        let mut reads = clear("16,980");
        reads[4] = read('8', 0.95, Some(('6', 0.50)));
        let r = resolve(&reads, &sample_db());
        assert_eq!(r.value, None);
        assert_eq!(r.raw, "16,980");
    }

    #[test]
    fn voter_needs_agreement() {
        let mut v = Voter::new(3, 2);
        assert_eq!(v.push(Some(16960)), None);
        assert_eq!(v.push(Some(16980)), None);
        assert_eq!(v.push(Some(16960)), Some(16960));
        assert_eq!(v.push(None), None);
    }
}
