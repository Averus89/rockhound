# Devlog

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
