//! Turn glyph reads into a signature value the table can explain.

use std::collections::VecDeque;

use crate::recognize::GlyphRead;
use crate::signatures::{SignatureDb, SignatureMatch};

/// A glyph whose best and runner-up scores are closer than this is ambiguous
/// (the classic case is 6 vs 8 on a blurry frame).
pub const AMBIGUOUS_MARGIN: f32 = 0.08;
/// Every glyph must score at least this for a value only a mixed cluster
/// explains: almost any multiple of 5 is some mix, so the table can't catch
/// a misread there and the read itself has to be clean.
pub const MIXED_MIN_CONFIDENCE: f32 = 0.8;
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
/// "corrected" into some unrelated valid value. A reading only a mixed
/// cluster explains is kept as read, when every glyph is clean.
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
        None => {
            let confidence = digits.iter().map(|r| r.score).fold(f32::INFINITY, f32::min);
            let text: String = digits.iter().map(|r| r.best).collect();
            let value = text
                .parse::<u32>()
                .ok()
                .filter(|&v| confidence >= MIXED_MIN_CONFIDENCE && !db.mixes(v).is_empty());
            Resolved {
                raw,
                value,
                confidence,
                corrected: false,
                matches: Vec::new(),
            }
        }
    }
}

/// Majority vote over the last few frames, so one bad frame never reaches
/// the UI and a real change shows up within a few reads.
#[derive(Debug, Clone)]
pub struct Voter {
    window: VecDeque<Option<u32>>,
    size: usize,
    needed: usize,
    hold: usize,
    shown: Option<u32>,
    misses: usize,
}

impl Voter {
    /// Show a value once `needed` of the last `size` frames agree on it.
    pub fn new(size: usize, needed: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(size),
            size,
            needed,
            hold: 0,
            shown: None,
            misses: 0,
        }
    }

    /// Keep showing a value through up to `frames` frames that don't
    /// confirm it. Live, the locator often lands on a rock or another label
    /// for a frame or two, and without this the value blinks off and on.
    pub fn hold(mut self, frames: usize) -> Self {
        self.hold = frames;
        self
    }

    pub fn push(&mut self, value: Option<u32>) -> Option<u32> {
        if self.window.len() == self.size {
            self.window.pop_front();
        }
        self.window.push_back(value);
        if let Some(voted) = self.vote() {
            self.shown = Some(voted);
            self.misses = 0;
        } else if self.shown.is_some() {
            self.misses += 1;
            if self.misses > self.hold {
                self.shown = None;
            }
        }
        self.shown
    }

    fn vote(&self) -> Option<u32> {
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
    fn impossible_reads_are_rejected_however_clear() {
        // No rock or cluster adds up to a value that isn't a multiple of 5,
        // so a confident misread like 17,201 must still show nothing.
        for text in ["17,201", "25,681", "1,234"] {
            assert_eq!(resolve(&clear(text), &sample_db()).value, None, "{text}");
        }
    }

    #[test]
    fn clean_read_of_a_mixed_cluster_resolves() {
        // 2 Laranite + 3 Copper; no single source explains it.
        let r = resolve(&clear("20,370"), &sample_db());
        assert_eq!(r.value, Some(20370));
        assert!(r.matches.is_empty());
    }

    #[test]
    fn unsure_read_of_a_mixed_cluster_is_rejected() {
        let mut reads = clear("20,370");
        reads[3] = read('3', 0.7, Some(('8', 0.4)));
        assert_eq!(resolve(&reads, &sample_db()).value, None);
    }

    #[test]
    fn voter_needs_agreement() {
        let mut v = Voter::new(3, 2);
        assert_eq!(v.push(Some(16960)), None);
        assert_eq!(v.push(Some(16980)), None);
        assert_eq!(v.push(Some(16960)), Some(16960));
        assert_eq!(v.push(None), None);
    }

    #[test]
    fn voter_holds_a_value_through_bad_frames() {
        let mut v = Voter::new(3, 2).hold(3);
        v.push(Some(25680));
        assert_eq!(v.push(Some(25680)), Some(25680));
        // The vote itself survives one bad frame, then the hold takes over.
        for _ in 0..4 {
            assert_eq!(v.push(None), Some(25680));
        }
        assert_eq!(v.push(None), None);
    }

    #[test]
    fn voter_switches_to_a_new_value_at_once() {
        let mut v = Voter::new(3, 2).hold(10);
        v.push(Some(25680));
        v.push(Some(25680));
        assert_eq!(v.push(Some(16960)), Some(25680));
        assert_eq!(v.push(Some(16960)), Some(16960));
    }
}
