# Devlog

## 2026-10-15: v0.5.1, more live crops; 25,680 is not in the table

Alan's v0.5.0 debug session misread 25,620 and 25,680. Reading the 74
saved crops with the shipped templates showed two causes:

- Templates kept only the first 12 samples per digit, in file order, so
  the 8 had samples from the 17,080 crops alone, and new crops added
  nothing. The cap is now 40, which keeps every bundled sample.
- 24 more labelled crops from that session are shipped (now 46: 11,565,
  15,420, 6,740 and 25,680 are new values). One 25,620 crop whose digit
  line picked up a rock above it was removed.
- Leave-one-out over the crops: 40/46 (was 18/22); the ratchet is raised to
  40. Trained on the old 22 only, the new session's crops read 21/25 (was
  19/25). A read takes ~15 ms with ~260 samples.
- 25,680 is not a multiple of any signature in the table (it is
  2 x 4,270 + 4 x 4,285, a mixed cluster), so even a correct read is
  rejected. Still open.

## 2026-10-15: v0.5.0, built-in digit templates from live crops

Alan's v0.4.5 debug session: the readout was located on most frames, but
Windows OCR returned nothing on every crop (one "T7,080", one "zł500"), so
nothing was learned. OCR at this size is a dead end for teaching.

- 22 of the session's readout crops, labelled by eye, are shipped in
  `data/glyphs/1440p/<value>-<n>.png` (all ten digits). A build script in
  rockhound-core embeds them; `bundled_templates()` learns from them, and
  the app reads with those plus whatever the player has learned. Only the
  learned part is saved to the templates folder.
- The matcher only accepts reads in the HUD's number format (1-3 digits,
  then ",ddd" groups, no leading zero), so stray commas and extra digits
  can't win.
- Learning splits a labelled line with a DP over all cuts instead of
  placing them one by one; the narrow "1" in "14,000" used to make every
  14,000 crop fail to teach.
- Lines are matched at 48 px instead of 24 (leave-one-out over the live
  crops: 14/22 -> 18/22), with correlation via prefix sums so a read takes
  ~5 ms instead of ~60. Templates saved at another height are rescaled on
  load.
- New tests: bundled templates cover every digit; leave-one-out accuracy on
  the live crops stays at or above 18/22; HUD grammar.

## 2026-10-15: v0.4.5, OCR teacher input and learning

From Alan's first debug log (v0.4.4): the readout was located on most
frames, but Windows OCR returned an empty string almost every time and read
"17.200" only now and then, never twice in a row, so nothing was learned.

- The teacher now gets only the digit line (from the layout step, no pin or
  box), thresholded so the colour fringes are gone, black on white, 48 px
  tall with a 24 px white margin. Teacher::read_text takes that image.
- Two agreeing teacher reads no longer need to be back to back: the last
  table-valid read is kept across empty ones.
- Debug mode also saves the OCR input (`ocr-input.png`) and numbered readout
  crops (`readouts/000.png`..`199.png`) named in the log, for fixtures.

## 2026-10-15: v0.4.4, readout not found with colour fringing

- Alan's live session (21,500 on screen) never located the readout. The
  game's chromatic aberration paints the white digits in alternating
  blue, green and orange columns, so per-pixel saturation called them
  "coloured art" and dropped them with the pin. Text vs pin is now decided
  by the saturation of a cluster's mean colour: the fringes average back to
  white, the pin stays orange. New fixture `hud_band_21500.png` and test.
- While templates are incomplete, the OCR teacher's answer is shown in
  preference to the template read (a missing 5 was read as 0).
- Settings: "Save debug images". While scanning, every 2 s it saves the
  captured band and the located readout, plus a log of locator and OCR
  results, to `%APPDATA%\io.github.averus89.rockhound\debug`.

## 2026-10-15: v0.4.3, "state not managed" on startup

- Tauri builds the configured windows before the setup hook runs, so on
  Windows the main window's first `get_status` could arrive before
  `AppState` was registered. Both windows now have `create: false` and are
  built in setup right after the state is managed.

## 2026-10-15: v0.4.2, game window not found

- Start reported "Star Citizen isn't running" with the game open. The
  window was matched by process name through `Window::process_name`, which
  opens the process with memory-read rights; Easy Anti-Cheat denies those,
  so every lookup failed. The executable path is now read with limited
  query rights only, and a visible window titled exactly "Star Citizen"
  also matches when even that is refused.

## 2026-10-14: v0.4.1, look and feel

- Theme: "ember" accent taken from the in-game pin orange, a dark blue-grey
  gradient background, translucent panels. Inter and JetBrains Mono are
  bundled (fontsource), so the look doesn't depend on installed fonts.
- Main window: pin logo, Scanning/Idle status pill, icon buttons, a large
  glowing value with a tier-coloured edge, category icon, and a history
  panel with tier dots.
- Overlay: glass card with a tier-coloured edge; each new value fades in.
- Animations are skipped when Windows asks for reduced motion.

## 2026-10-14: v0.4.0, overlay

- Second window `overlay`: transparent, undecorated, always on top, no
  taskbar entry, click-through. It loads the same frontend, which renders a
  compact card (value, best match, tier) when its window label is
  `overlay`, and nothing while there is no value.
- Settings: "Show overlay over the game" (off by default). Saving settings
  shows or hides it; it is restored at startup.
- Placed top-centre of the primary monitor, 8% down. Closing the main
  window exits the app so the hidden overlay can't keep it running.
- On Linux the overlay must be shown before it can be made click-through
  (tao panics otherwise), so it is shown first, then placed.

Verified under Xvfb with a replay of the real "17,200" crop: main window
and overlay both showed 17,200 as Ice x4.

## 2026-10-14: v0.3.1, CI quality gates

- Coverage gate: `cargo llvm-cov` runs the test suite and fails under 85%
  line coverage (91.7% now). Tauri command glue (`src-tauri/src/lib.rs`) and
  the entry points are excluded since they only wire tested code together.
- Scan loop test: real crop in, one voted reading out, stop reason reported.
- Frontend tests with Vitest + Testing Library (jsdom): value formatting and
  the readout card states. `npm test` runs in CI after the build.
- Security: new `audit` job runs `cargo audit` (fails on vulnerabilities,
  unmaintained notices only warn) and `npm audit --audit-level=high`.
  Dependabot opens weekly update PRs for cargo, npm and Actions.
- CodeQL is not set up: it needs GitHub Advanced Security on a private repo.

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
