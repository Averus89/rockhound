//! Reference labels for a video: which value is on screen when.
//!
//! One range per line, `start-end value`, times as seconds or `m:ss(.f)`:
//!
//! ```text
//! # ore cluster near Daymar
//! 0:12-0:19 16960
//! 0:19-0:25 -        (no readout on screen)
//! ```

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    pub start: f64,
    pub end: f64,
    /// `None` means no readout should be detected in this range.
    pub value: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Labels(Vec<Range>);

impl Labels {
    pub fn parse(text: &str) -> Result<Self> {
        let mut ranges = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            ranges.push(parse_line(line).with_context(|| format!("labels line {}", n + 1))?);
        }
        Ok(Self(ranges))
    }

    /// The expected value at `t` seconds. Outer `None`: frame not labelled,
    /// so it is skipped when scoring.
    pub fn at(&self, t: f64) -> Option<Option<u32>> {
        self.0
            .iter()
            .find(|r| t >= r.start && t < r.end)
            .map(|r| r.value)
    }
}

fn parse_line(line: &str) -> Result<Range> {
    let mut parts = line.split_whitespace();
    let (Some(span), Some(value), None) = (parts.next(), parts.next(), parts.next()) else {
        bail!("expected `start-end value`, got {line:?}");
    };
    let (start, end) = span
        .split_once('-')
        .with_context(|| format!("missing '-' in {span:?}"))?;
    let (start, end) = (parse_time(start)?, parse_time(end)?);
    if end <= start {
        bail!("range ends before it starts: {span}");
    }
    let value = match value {
        "-" => None,
        v => Some(
            v.replace(',', "")
                .parse()
                .with_context(|| format!("bad value {v:?}"))?,
        ),
    };
    Ok(Range { start, end, value })
}

fn parse_time(s: &str) -> Result<f64> {
    let secs = match s.split_once(':') {
        Some((m, rest)) => m.parse::<f64>()? * 60.0 + rest.parse::<f64>()?,
        None => s.parse::<f64>()?,
    };
    Ok(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges_and_lookup() {
        let labels = Labels::parse("# comment\n0:12-0:19 16,960\n19.5-25 -\n").unwrap();
        assert_eq!(labels.at(15.0), Some(Some(16960)));
        assert_eq!(labels.at(20.0), Some(None));
        assert_eq!(labels.at(5.0), None);
    }

    #[test]
    fn rejects_backwards_range() {
        assert!(Labels::parse("0:19-0:12 100").is_err());
    }
}
