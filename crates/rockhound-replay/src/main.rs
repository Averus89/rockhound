//! Replay reference videos through the Rockhound pipeline.
//!
//! Workflow:
//! 1. `frames`  cut the readout region out of a video at a fixed rate.
//! 2. `learn`   build digit templates from labelled frames.
//! 3. `eval`    score the pipeline against the labels.

mod labels;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, Subcommand};
use image::imageops;
use rockhound_core::{preprocess, read_crop, segment, SignatureDb, TemplateSet, Voter};

use crate::labels::Labels;

/// File inside a frames directory recording the extraction rate.
const FPS_FILE: &str = "fps.txt";
/// Cap on stored samples per digit; more adds time without adding accuracy.
const MAX_SAMPLES_PER_DIGIT: usize = 12;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Extract readout crops from a video with ffmpeg.
    Frames {
        video: PathBuf,
        /// Readout region in video pixels: x,y,width,height.
        #[arg(long, value_parser = parse_region)]
        region: [u32; 4],
        #[arg(long, default_value_t = 5.0)]
        fps: f64,
        #[arg(long)]
        out: PathBuf,
    },
    /// Build digit templates from labelled frames.
    Learn {
        frames: PathBuf,
        #[arg(long)]
        labels: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Score the pipeline against labelled frames.
    Eval {
        frames: PathBuf,
        #[arg(long)]
        labels: PathBuf,
        #[arg(long)]
        templates: PathBuf,
        #[arg(long, default_value = "data/signatures.json")]
        signatures: PathBuf,
        /// Print every frame, not just mistakes.
        #[arg(long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::Frames {
            video,
            region,
            fps,
            out,
        } => extract_frames(&video, region, fps, &out),
        Cmd::Learn {
            frames,
            labels,
            out,
        } => learn(&frames, &labels, &out),
        Cmd::Eval {
            frames,
            labels,
            templates,
            signatures,
            verbose,
        } => eval(&frames, &labels, &templates, &signatures, verbose),
    }
}

fn parse_region(s: &str) -> Result<[u32; 4], String> {
    let v: Vec<u32> = s
        .split(',')
        .map(|p| p.trim().parse().map_err(|e| format!("{p:?}: {e}")))
        .collect::<Result<_, _>>()?;
    v.try_into()
        .map_err(|_| "expected x,y,width,height".to_string())
}

fn extract_frames(video: &Path, [x, y, w, h]: [u32; 4], fps: f64, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(video)
        .args(["-vf", &format!("fps={fps},crop={w}:{h}:{x}:{y}")])
        .arg(out.join("f_%06d.png"))
        .status()
        .context("failed to run ffmpeg; is it installed and on PATH?")?;
    ensure!(status.success(), "ffmpeg exited with {status}");
    std::fs::write(out.join(FPS_FILE), fps.to_string())?;
    println!(
        "wrote {} frames to {}",
        list_frames(out)?.len(),
        out.display()
    );
    Ok(())
}

/// Frames in order with their timestamp in seconds.
fn list_frames(dir: &Path) -> Result<Vec<(f64, PathBuf)>> {
    let fps: f64 = std::fs::read_to_string(dir.join(FPS_FILE))
        .with_context(|| format!("{} missing; create frames with `frames`", FPS_FILE))?
        .trim()
        .parse()?;
    let mut frames: Vec<(f64, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let index: u32 = p.file_stem()?.to_str()?.strip_prefix("f_")?.parse().ok()?;
            // ffmpeg numbers from 1.
            Some((f64::from(index - 1) / fps, p))
        })
        .collect();
    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(frames)
}

fn load_labels(path: &Path) -> Result<Labels> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Labels::parse(&text)
}

fn learn(frames_dir: &Path, labels: &Path, out: &Path) -> Result<()> {
    let labels = load_labels(labels)?;
    std::fs::create_dir_all(out)?;
    let mut counts: HashMap<char, usize> = HashMap::new();
    let mut skipped = 0;
    for (t, path) in list_frames(frames_dir)? {
        let Some(Some(value)) = labels.at(t) else {
            continue;
        };
        let gray = preprocess::normalize(&image::open(&path)?.to_rgba8());
        let digits: Vec<_> = segment::segment(&gray)
            .into_iter()
            .filter(|b| !b.is_separator)
            .collect();
        let expected: Vec<char> = value.to_string().chars().collect();
        if digits.len() != expected.len() {
            // Segmentation disagrees with the label; don't learn from it.
            skipped += 1;
            continue;
        }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        for (i, (b, digit)) in digits.iter().zip(&expected).enumerate() {
            let n = counts.entry(*digit).or_default();
            if *n >= MAX_SAMPLES_PER_DIGIT {
                continue;
            }
            *n += 1;
            let glyph = imageops::crop_imm(&gray, b.x, b.y, b.width, b.height).to_image();
            glyph.save(out.join(format!("{digit}_{stem}_{i}.png")))?;
        }
    }
    let mut summary: Vec<_> = counts.into_iter().collect();
    summary.sort();
    println!(
        "templates per digit: {summary:?} (skipped {skipped} frames that didn't segment cleanly)"
    );
    let missing: Vec<char> = ('0'..='9')
        .filter(|d| !summary.iter().any(|(c, _)| c == d))
        .collect();
    if !missing.is_empty() {
        println!("warning: no samples for {missing:?}; label more frames that show them");
    }
    Ok(())
}

#[derive(Default)]
struct Score {
    correct: u32,
    wrong: u32,
    missed: u32,
    false_reads: u32,
    empty_ok: u32,
}

impl Score {
    fn add(&mut self, expected: Option<u32>, got: Option<u32>) -> bool {
        match (expected, got) {
            (Some(e), Some(g)) if e == g => self.correct += 1,
            (Some(_), Some(_)) => self.wrong += 1,
            (Some(_), None) => self.missed += 1,
            (None, Some(_)) => self.false_reads += 1,
            (None, None) => self.empty_ok += 1,
        }
        expected == got
    }

    fn print(&self, name: &str) {
        let labelled = self.correct + self.wrong + self.missed;
        let pct = if labelled == 0 {
            0.0
        } else {
            100.0 * f64::from(self.correct) / f64::from(labelled)
        };
        println!(
            "{name:>7}: {pct:5.1}% correct ({}/{labelled}), {} wrong, {} missed, {} false reads, {} blank ok",
            self.correct, self.wrong, self.missed, self.false_reads, self.empty_ok
        );
    }
}

fn eval(
    frames_dir: &Path,
    labels: &Path,
    templates: &Path,
    signatures: &Path,
    verbose: bool,
) -> Result<()> {
    let labels = load_labels(labels)?;
    let templates = TemplateSet::load_dir(templates)?;
    let db = SignatureDb::from_json(&std::fs::read_to_string(signatures)?)?;
    let mut voter = Voter::new(3, 2);
    let (mut per_frame, mut voted) = (Score::default(), Score::default());
    let (mut total_time, mut reads) = (Duration::ZERO, 0u32);

    let frames = list_frames(frames_dir)?;
    if frames.is_empty() {
        bail!("no frames in {}", frames_dir.display());
    }
    for (t, path) in frames {
        let crop = image::open(&path)?.to_rgba8();
        let started = Instant::now();
        let resolved = read_crop(&crop, &templates, &db);
        total_time += started.elapsed();
        reads += 1;

        let got = resolved.as_ref().and_then(|r| r.value);
        let shown = voter.push(got);
        let Some(expected) = labels.at(t) else {
            continue;
        };
        let ok = per_frame.add(expected, got);
        voted.add(expected, shown);
        if verbose || !ok {
            let raw = resolved.as_ref().map_or("", |r| r.raw.as_str());
            let conf = resolved.as_ref().map_or(0.0, |r| r.confidence);
            println!(
                "{t:8.2}s {:<14} expected {:>8} got {:>8} raw {raw:<10} conf {conf:.2}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                fmt(expected),
                fmt(got),
            );
        }
    }
    per_frame.print("frame");
    voted.print("voted");
    println!(
        "avg read time: {:.2} ms",
        total_time.as_secs_f64() * 1000.0 / f64::from(reads)
    );
    Ok(())
}

fn fmt(v: Option<u32>) -> String {
    v.map_or_else(|| "-".to_string(), |v| v.to_string())
}
