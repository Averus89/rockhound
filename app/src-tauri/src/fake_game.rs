//! A stand-in for the Star Citizen window, so the app can be run and
//! tested end to end without the game: frames come from a recording (a
//! video, or a folder of full-window screenshots) and go through the same
//! search band and pin locator as live capture.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

use image::{imageops, RgbaImage};

use crate::capture::{capture_rect, readout_crop};
use crate::debug;
use crate::scanner::{list_pngs, next_png, FrameSource};

/// Plays back a recorded game, looping at the end.
pub struct FakeGame {
    frames: Frames,
    /// The player's fixed readout box, if they set one.
    region: Option<[u32; 4]>,
    snapshot: debug::Throttle,
}

enum Frames {
    Screenshots { paths: Vec<PathBuf>, next: usize },
    Video(Video),
}

impl FakeGame {
    /// Open a folder of PNG screenshots or a video file. A video is sampled
    /// once per scan `interval`, so it plays back in real time.
    pub fn open(path: &Path, region: Option<[u32; 4]>, interval: Duration) -> Result<Self, String> {
        let frames = if path.is_dir() {
            Frames::Screenshots {
                paths: list_pngs(path)?,
                next: 0,
            }
        } else {
            Frames::Video(Video::open(path, region, interval)?)
        };
        Ok(Self {
            frames,
            region,
            snapshot: debug::Throttle::default(),
        })
    }
}

impl FrameSource for FakeGame {
    fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
        let band = match &mut self.frames {
            Frames::Screenshots { paths, next } => {
                let frame = next_png(paths, next)?;
                let Some([x, y, w, h]) = capture_rect(frame.width(), frame.height(), self.region)
                else {
                    return Ok(None);
                };
                imageops::crop_imm(&frame, x, y, w, h).to_image()
            }
            Frames::Video(video) => video.next_band()?,
        };
        Ok(readout_crop(
            &band,
            self.region.is_some(),
            &mut self.snapshot,
        ))
    }
}

/// Frames of a video decoded by ffmpeg, cut to the search band by ffmpeg
/// itself, as live capture copies only the band off the GPU.
struct Video {
    path: PathBuf,
    rect: [u32; 4],
    fps: f64,
    decoder: Option<(Child, ChildStdout)>,
}

impl Video {
    fn open(path: &Path, region: Option<[u32; 4]>, interval: Duration) -> Result<Self, String> {
        let (width, height) = video_size(path)?;
        let rect = capture_rect(width, height, region)
            .ok_or_else(|| format!("the readout box is outside the {width}x{height} video"))?;
        Ok(Self {
            path: path.to_path_buf(),
            rect,
            fps: 1.0 / interval.as_secs_f64().max(0.001),
            decoder: None,
        })
    }

    fn next_band(&mut self) -> Result<RgbaImage, String> {
        let [x, y, w, h] = self.rect;
        let mut pixels = vec![0; (w * h * 4) as usize];
        // At the end of the video, start it again once; a decoder that
        // can't produce even one frame means the file is unusable.
        for _ in 0..2 {
            if self.decoder.is_none() {
                self.decoder = Some(self.spawn_decoder(x, y, w, h)?);
            }
            let (_, stdout) = self.decoder.as_mut().expect("decoder was just started");
            if stdout.read_exact(&mut pixels).is_ok() {
                return RgbaImage::from_raw(w, h, pixels)
                    .ok_or_else(|| "ffmpeg frame has the wrong size".into());
            }
            self.stop_decoder();
        }
        Err(format!(
            "no frames could be decoded from {}",
            self.path.display()
        ))
    }

    fn spawn_decoder(
        &self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Result<(Child, ChildStdout), String> {
        let mut child = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&self.path)
            .args(["-vf", &format!("fps={},crop={w}:{h}:{x}:{y}", self.fps)])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run ffmpeg (is it on PATH?): {e}"))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok((child, stdout))
    }

    fn stop_decoder(&mut self) {
        if let Some((mut child, _)) = self.decoder.take() {
            // Already exited at the end of the video; killing is a no-op then.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.stop_decoder();
    }
}

/// Width and height of the first video stream, from ffprobe.
fn video_size(path: &Path) -> Result<(u32, u32), String> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width,height", "-of", "csv=p=0:s=x"])
        .arg(path)
        .output()
        .map_err(|e| format!("cannot run ffprobe (is ffmpeg on PATH?): {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    parse_size(text.trim()).ok_or_else(|| format!("{} is not a readable video", path.display()))
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use rockhound_core::{SignatureDb, TemplateSet};

    use super::*;
    use crate::reader::Reader;

    const BAND: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/rockhound-core/tests/fixtures/hud_band_17200.png"
    );
    /// Alan's screen; the HUD band sits in the middle of the search band.
    const SCREEN: (u32, u32) = (5120, 1440);

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rockhound-fake-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A full game window with the readout where the game draws it.
    fn screenshot() -> RgbaImage {
        let band = image::open(BAND).unwrap().to_rgba8();
        let mut frame = RgbaImage::from_pixel(SCREEN.0, SCREEN.1, image::Rgba([0, 0, 0, 255]));
        let [x, y, w, h] = capture_rect(SCREEN.0, SCREEN.1, None).unwrap();
        let (bx, by) = (x + (w - band.width()) / 2, y + (h - band.height()) / 2);
        imageops::overlay(&mut frame, &band, i64::from(bx), i64::from(by));
        frame
    }

    fn read(source: &mut FakeGame) -> Option<u32> {
        let db = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let mut reader = Reader::new(
            rockhound_core::bundled_templates(),
            TemplateSet::default(),
            None,
            None,
        );
        let crop = source.next_crop().unwrap()?;
        reader.read(&crop, &db)?.value
    }

    #[test]
    fn screenshots_are_read_like_the_live_game_and_loop() {
        let dir = temp_dir("shots");
        screenshot().save(dir.join("a.png")).unwrap();
        RgbaImage::from_pixel(SCREEN.0, SCREEN.1, image::Rgba([0, 0, 0, 255]))
            .save(dir.join("b.png"))
            .unwrap();
        let mut game = FakeGame::open(&dir, None, Duration::from_millis(250)).unwrap();
        assert_eq!(read(&mut game), Some(17200));
        assert!(
            game.next_crop().unwrap().is_none(),
            "empty sky has no readout"
        );
        assert_eq!(read(&mut game), Some(17200), "loops to the first frame");
        // A fixed box outside the window sees nothing.
        let mut off_screen =
            FakeGame::open(&dir, Some([9000, 0, 10, 10]), Duration::from_millis(250)).unwrap();
        assert!(off_screen.next_crop().unwrap().is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn bad_recordings_are_reported() {
        let dir = temp_dir("bad");
        assert!(FakeGame::open(&dir, None, Duration::from_millis(250)).is_err());
        let not_video = dir.join("notes.txt");
        std::fs::write(&not_video, "not a video").unwrap();
        assert!(FakeGame::open(&not_video, None, Duration::from_millis(250)).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn parses_ffprobe_size() {
        assert_eq!(parse_size("5120x1440"), Some((5120, 1440)));
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("5120xabc"), None);
    }

    #[test]
    fn video_is_decoded_cropped_and_looped() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("ffmpeg not on PATH; skipping the video test");
            return;
        }
        let dir = temp_dir("video");
        screenshot().save(dir.join("frame.png")).unwrap();
        let video = dir.join("game.mkv");
        // Lossless, so the 11 px digits survive; 1 s at 4 fps is 4 frames.
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-loop",
                "1",
                "-t",
                "1",
            ])
            .arg("-i")
            .arg(dir.join("frame.png"))
            .args(["-r", "4", "-c:v", "ffv1"])
            .arg(&video)
            .status()
            .unwrap();
        assert!(status.success());
        let mut game = FakeGame::open(&video, None, Duration::from_millis(250)).unwrap();
        for _ in 0..6 {
            assert_eq!(
                read(&mut game),
                Some(17200),
                "plays past the end by looping"
            );
        }
        // The readout box can be off screen for a whole video.
        assert!(
            FakeGame::open(&video, Some([9000, 0, 10, 10]), Duration::from_millis(250)).is_err()
        );
        drop(game);
        std::fs::remove_dir_all(dir).ok();
    }
}
