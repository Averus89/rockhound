# Devlog

## 2026-10-14: v0.3.0, live capture and a reader that handles the real HUD

The first real 1440p crop ("17,200") broke the old segmenter: HUD digits are
~11 px tall, bold, and touch each other, so there are no blank columns to
split on, and the 3x upscale smeared what gaps there were.

- Recognition rewritten. `layout` finds the digit line (column clusters,
  dropping saturated ones, i.e. the pin, and rejoining pieces split by a
  comma). `recognize` scales the line to 24 px and a dynamic program picks
  the template sequence that best explains it. Templates are learned from a
  line plus its known text ("17,200"), trimmed to their ink. Pin-colour
  blanking and the segmenter are gone.
- `locate` finds the readout on screen from the orange pin (blob of pin
  colour, then a text line to its right). ~4 ms on a 2048x720 band, so it
  runs every frame and follows the pin as the ship turns. Only the pin's
  own height is searched, to its right: a taller search picked up an
  asteroid in Alan's 5120x1440 screenshot.
- Live capture (Windows): Windows Graphics Capture of the `StarCitizen.exe`
  window, copying only the middle band (30-70% wide, 15-65% high) off the
  GPU. A fixed box from Settings overrides the locator.
- Windows OCR teaches templates: when templates can't read a frame (or
  digits are still missing), OCR reads it at most once a second. A value the
  signature table explains is shown, and once OCR reads it twice in a row it
  is learned and saved to the templates folder. A fresh install therefore
  starts on OCR and moves to templates by itself.
- Settings: "Find the readout automatically" switch (default on).
- Real HUD crops are test fixtures (`crates/rockhound-core/tests/fixtures`):
  learn from one frame, locate and read the other with the pin in view.
- CI runs clippy and the app tests on Windows too, since capture and OCR
  only compile there.

Not verified on a real game session yet (no Windows machine here).

## 2026-10-14: v0.2.0, Tauri shell

Added `app/`: Tauri v2 backend plus a React + Mantine UI (dark, orange accent).

- Main window: start/stop, big current value with best match, tier badge,
  other possible matches, a "corrected" badge when a 6/8-style swap was used,
  the last 8 reads, and the active signature data version.
- Settings modal: readout box (x, y, width, height) and read interval
  (default 250 ms), saved as JSON in the app config folder.
- Scan loop on its own thread behind a `FrameSource` trait. Only the replay
  source exists for now (`ROCKHOUND_REPLAY_DIR`); live WGC capture is next.
  The UI only gets an event when the voted value changes.
- Signature sync: bundled table compiled in, newer cached table preferred,
  background fetch from the repo's raw `data/signatures.json` at startup
  (`data_version` dates compared as strings). Offline or a bad file keeps the
  local table. While the repo is private the fetch returns 404 and is ignored.
- CI builds the frontend and installs WebKitGTK so the app crate is checked.
- CI `windows-build` job builds the release exe and NSIS installer and uploads
  them as the `rockhound-windows` artifact.

Verified: app built and run under Xvfb against the synthetic replay clip;
clicking Start showed 12,720 as Copper x3.

## 2026-10-14: v0.1.0, project start

Fresh Rust rewrite of the SC signature scanner idea. Plan and decisions are
from the project discussion with Alan.

Decisions:
- Template matching instead of general OCR (EasyOCR was ~180 ms per read and
  a 275 MB download; Windows OCR gave no confidence score).
- Per-glyph scores drive correction: only glyphs whose best and runner-up
  templates are within 0.08 may be swapped, and only to a value the signature
  table explains. This targets the 6 vs 8 misreads without "correcting" clear
  reads into unrelated values.
- The orange map pin is blanked by colour during grayscale conversion.
- Majority vote (2 of last 3 frames) before a value is shown.
- No manual image scan (old PING feature dropped).
- Signature table is plain JSON with a schema number, ready for syncing.

Added:
- `rockhound-core` with unit tests on synthetic digits.
- `rockhound-replay` (`frames`, `learn`, `eval`).
- CI: fmt, clippy, tests.

Smoke test on a synthetic noisy video (DejaVu Sans, ~13 px digits, orange
box as the pin): 110/110 labelled frames correct, no false reads on blank
frames, ~2.8 ms per read in release.
