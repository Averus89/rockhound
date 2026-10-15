//! Windows' built-in OCR as the template [`Teacher`].
//!
//! Too slow and too unsure of itself for every frame, but it needs no
//! training, which is exactly what a fresh install lacks.

use image::{imageops, RgbaImage};
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::reader::Teacher;

/// Windows OCR misses text much under ~20 px tall; HUD digits are ~11 px.
const UPSCALE: u32 = 4;

#[derive(Default)]
pub struct WindowsOcr {
    /// Created on first use, on the scan thread that uses it.
    engine: Option<OcrEngine>,
}

impl WindowsOcr {
    fn engine(&mut self) -> windows::core::Result<&OcrEngine> {
        if self.engine.is_none() {
            // The scan thread is new to WinRT. Already-initialized results are
            // fine, so the return value is ignored.
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            self.engine = Some(OcrEngine::TryCreateFromUserProfileLanguages()?);
        }
        Ok(self.engine.as_ref().expect("engine set above"))
    }

    fn recognize(&mut self, crop: &RgbaImage) -> windows::core::Result<String> {
        // Dark text on white, larger: the input OCR engines are tuned for.
        let mut gray = rockhound_core::preprocess::normalize(crop);
        imageops::invert(&mut gray);
        let (w, h) = (gray.width() * UPSCALE, gray.height() * UPSCALE);
        let big = imageops::resize(&gray, w, h, imageops::FilterType::Triangle);
        let bgra: Vec<u8> = big
            .pixels()
            .flat_map(|p| [p.0[0], p.0[0], p.0[0], 255])
            .collect();

        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &writer.DetachBuffer()?,
            BitmapPixelFormat::Bgra8,
            w as i32,
            h as i32,
        )?;
        let result = self.engine()?.RecognizeAsync(&bitmap)?.join()?;
        Ok(result.Text()?.to_string())
    }
}

impl Teacher for WindowsOcr {
    fn read_text(&mut self, crop: &RgbaImage) -> Option<String> {
        self.recognize(crop)
            .inspect_err(|e| eprintln!("Windows OCR failed: {e}"))
            .ok()
    }
}
