//! Live capture of the Star Citizen window.
//!
//! Windows Graphics Capture delivers frames of the game window on its own
//! thread; only the part of the frame the readout can be in is copied off
//! the GPU. Each scan tick locates the readout in the newest copy, since the
//! pin moves with the ship.

use image::{imageops, RgbaImage};
use rockhound_core::locate;

use crate::debug;

/// Process whose window is captured.
#[cfg_attr(not(windows), allow(dead_code))]
const GAME_PROCESS: &str = "StarCitizen.exe";
/// Title of the game window, used when the process can't be inspected.
#[cfg_attr(not(windows), allow(dead_code))]
const GAME_TITLE: &str = "Star Citizen";

/// Whether a window with `title`, owned by the executable at `exe_path`
/// (when it could be read), is the game.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_game_window(title: &str, exe_path: Option<&str>) -> bool {
    let exe = exe_path.and_then(|p| p.rsplit(['\\', '/']).next());
    exe.is_some_and(|e| e.eq_ignore_ascii_case(GAME_PROCESS)) || title.trim() == GAME_TITLE
}

/// Part of the game window searched for the readout, as fractions of its
/// size (x, y, width, height). The readout appears near the crosshair.
const SEARCH_BAND: [f32; 4] = [0.3, 0.15, 0.4, 0.5];

/// Pixel rectangle (x, y, width, height) of the search band, or of the
/// player's own region when they set one, clamped to the window.
#[cfg_attr(not(windows), allow(dead_code))]
fn capture_rect(width: u32, height: u32, region: Option<[u32; 4]>) -> Option<[u32; 4]> {
    let [x, y, w, h] = region.unwrap_or_else(|| {
        let [fx, fy, fw, fh] = SEARCH_BAND;
        let scale = |f: f32, size: u32| (f * size as f32) as u32;
        [
            scale(fx, width),
            scale(fy, height),
            scale(fw, width),
            scale(fh, height),
        ]
    });
    let (w, h) = (
        w.min(width.saturating_sub(x)),
        h.min(height.saturating_sub(y)),
    );
    (w > 0 && h > 0).then_some([x, y, w, h])
}

/// The readout crop inside a captured band, or `None` when it isn't on
/// screen. A player-set region is used as is.
#[cfg_attr(not(windows), allow(dead_code))]
fn readout_crop(
    band: &RgbaImage,
    manual_region: bool,
    snapshot: &mut debug::Throttle,
) -> Option<RgbaImage> {
    let save = snapshot.due();
    if save {
        debug::image("band", band);
    }
    if manual_region {
        return Some(band.clone());
    }
    let found = locate::find_readout(band);
    let crop = found.map(|r| imageops::crop_imm(band, r.x, r.y, r.width, r.height).to_image());
    if save {
        debug::note(format_args!(
            "band {}x{}, readout {found:?}",
            band.width(),
            band.height()
        ));
        if let Some(c) = &crop {
            debug::image("readout", c);
        }
    }
    crop
}

#[cfg(windows)]
pub use wgc::LiveSource;

#[cfg(windows)]
mod wgc {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use image::RgbaImage;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };
    use windows_capture::window::Window;

    use super::{capture_rect, debug, is_game_window, readout_crop};
    use crate::scanner::FrameSource;

    type Latest = Arc<Mutex<Option<RgbaImage>>>;

    struct Handler {
        latest: Latest,
        region: Option<[u32; 4]>,
        buffer: Vec<u8>,
    }

    impl GraphicsCaptureApiHandler for Handler {
        type Flags = (Latest, Option<[u32; 4]>);
        type Error = String;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (latest, region) = ctx.flags;
            Ok(Self {
                latest,
                region,
                buffer: Vec::new(),
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            _control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            let Some([x, y, w, h]) = capture_rect(frame.width(), frame.height(), self.region)
            else {
                return Ok(());
            };
            let buffer = frame
                .buffer_crop(x, y, x + w, y + h)
                .map_err(|e| e.to_string())?;
            let pixels = buffer.as_nopadding_buffer(&mut self.buffer).to_vec();
            if let Some(band) = RgbaImage::from_raw(w, h, pixels) {
                *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(band);
            }
            Ok(())
        }
    }

    /// Path of the executable that owns `window`. Asks only for limited
    /// query rights: Easy Anti-Cheat denies the memory-read access that
    /// `Window::process_name` needs, which hid the game entirely.
    fn exe_path(window: &Window) -> Option<String> {
        let pid = window.process_id().ok()?;
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        };
        let _ = unsafe { CloseHandle(process) };
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    /// Frames from the running game.
    pub struct LiveSource {
        latest: Latest,
        manual_region: bool,
        snapshot: debug::Throttle,
        control: Option<CaptureControl<Handler, String>>,
    }

    impl LiveSource {
        pub fn start(region: Option<[u32; 4]>, interval: Duration) -> Result<Self, String> {
            let window = Window::enumerate()
                .map_err(|e| format!("cannot list windows: {e}"))?
                .into_iter()
                .find(|w| {
                    w.is_valid()
                        && is_game_window(&w.title().unwrap_or_default(), exe_path(w).as_deref())
                })
                .ok_or("Star Citizen isn't running. Start the game, then press Start.")?;
            let latest = Latest::default();
            let settings = Settings::new(
                window,
                CursorCaptureSettings::WithoutCursor,
                DrawBorderSettings::WithoutBorder,
                SecondaryWindowSettings::Default,
                // Frames faster than the scan interval would only be dropped.
                MinimumUpdateIntervalSettings::Custom(interval),
                DirtyRegionSettings::Default,
                ColorFormat::Rgba8,
                (Arc::clone(&latest), region),
            );
            let control = Handler::start_free_threaded(settings)
                .map_err(|e| format!("cannot capture the game window: {e}"))?;
            Ok(Self {
                latest,
                manual_region: region.is_some(),
                snapshot: debug::Throttle::default(),
                control: Some(control),
            })
        }
    }

    impl FrameSource for LiveSource {
        fn next_crop(&mut self) -> Result<Option<RgbaImage>, String> {
            if self.control.as_ref().is_some_and(|c| c.is_finished()) {
                return Err("Star Citizen was closed.".into());
            }
            let latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
            let Some(band) = latest.as_ref() else {
                if self.snapshot.due() {
                    debug::note("no frame from the game window yet");
                }
                return Ok(None);
            };
            Ok(readout_crop(band, self.manual_region, &mut self.snapshot))
        }
    }

    impl Drop for LiveSource {
        fn drop(&mut self) {
            if let Some(control) = self.control.take() {
                // The capture thread ends with the session either way.
                let _ = control.stop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_band_is_centred_and_clamped() {
        assert_eq!(capture_rect(5120, 1440, None), Some([1536, 216, 2048, 720]));
        assert_eq!(
            capture_rect(100, 100, Some([90, 90, 50, 50])),
            Some([90, 90, 10, 10])
        );
        assert_eq!(capture_rect(100, 100, Some([200, 0, 5, 5])), None);
    }

    #[test]
    fn recognises_the_game_window() {
        let exe = r"C:\Games\StarCitizen\LIVE\Bin64\StarCitizen.exe";
        assert!(is_game_window("whatever", Some(exe)));
        // Process unreadable: fall back to the window title.
        assert!(is_game_window("Star Citizen ", None));
        assert!(!is_game_window("Star Citizen - Wiki - Firefox", None));
        assert!(!is_game_window("Rockhound", Some(r"C:\rockhound.exe")));
    }

    #[test]
    fn blank_band_has_no_readout() {
        let band = RgbaImage::from_pixel(300, 100, image::Rgba([10, 10, 10, 255]));
        let mut snapshot = debug::Throttle::default();
        assert!(readout_crop(&band, false, &mut snapshot).is_none());
        assert!(readout_crop(&band, true, &mut snapshot).is_some());
    }
}
