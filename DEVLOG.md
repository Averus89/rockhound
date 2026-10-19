# Devlog

## 2026-10-19: Website

One-page static site in `site/`, built from the promo kit (colours, type and
motion tokens). Hero with an animated overlay readout, how a read works, the
main window, a "Try a number" lookup using the signature table, download and
donation links, credits and the fan-made disclaimer. No build step and no
app change, so no version bump. Hosting is still to be decided.

## 2026-10-19: Signature table from SC 4.10.2

First `sigextract update` run on Alan's PC against the live game. Every
signature value is unchanged; the table now records `sc_version` 4.10.2
(read from `build_manifest.id`) and `data_version` 2026.10.19, so
installed apps sync it. Data only, so no version bump.

## 2026-10-19: v0.7.0, space or planet

Alan pointed out that some signatures collide across locations: 3000 is a
small ground deposit or a capital (890 Jump) wreck, 4000 a large ground
deposit or two salvage panels. Which one it is depends on where the player is.

- Main window has a "Where you are" switch: Anywhere, In space, On a planet.
  It is saved with the settings (`location`) and applies to the shown reading
  at once, without restarting the scan.
- Ground deposits belong on planets and salvage in space
  (`Category::found_in`); ship-mining rocks are both asteroids and surface
  rocks, so they never move. `SignatureDb::matches_at` moves sources that
  don't belong at the location to the end of the list. Nothing is hidden, so
  a wrong setting still shows the right answer under "Could also be".
- Anywhere keeps the old order and, when the matches split by location, says
  to choose where you are.
- No schema change to `data/signatures.json`; location comes from the
  existing categories.
- Detection is manual for now. Automatic detection needs a signal from the
  game (Game.log or the HUD), still to be found.

## 2026-10-19: Signature extractor for game files

Alan asked for a way to take signature values straight from the game files
after each patch, as SC Signature Scanner did, so he can run it and push the
result. Mallachi's extractor scripts were never committed upstream, only
their notes (`SignatureValue.md`), so this is a new implementation of the
same method. No app change, so no version bump.

- New crate `crates/rockhound-sigextract`, binary `sigextract`. Reads
  `Data/Game2.dcb` from `Data.p4k` itself (ZIP64, CIG local headers,
  Zstandard via the pure-Rust `ruzstd`), parses DataCore and follows
  `Components -> SSCSignatureSystemParams -> radarProperties ->
  baseSignatureParams -> signatures[4]` for every entity.
- `update` merges the values into `data/signatures.json`: each row takes the
  most common value among the entities whose names match it, keeps names,
  tiers and `max_count`, and stamps `data_version`/`sc_version` only when
  something changed. The report lists changed rows, rows nothing matched,
  disagreeing entities and scannable entities no row claims.
- `list` prints every entity signature as TSV and `dump` prints a record as
  JSON, for tuning the name rules and following layout changes. `export`
  saves the decompressed Game2.dcb for offline debugging.
- The parser checks that every struct's fields add up to its declared size
  and that the data section ends at the end of the file, so a format change
  fails loudly instead of producing wrong values. That check caught the
  first real run: SC 4.7 ships DataCore version 8, whose records carry an
  extra 4-byte string offset after the file name (36 bytes, not 32).
- Checked against Alan's SC 4.7 LIVE Game2.dcb: 29,693 entities, 5,849 with
  signatures, and all 33 rows of the current table confirmed unchanged.
  Name rules follow the real entity names (`MineableRock_Asteroid<Tier>_*`,
  `MineableRock_Surface<Tier>_*`, `MineableRock_FPS_*`,
  `MineableRock_GroundVehicle_*`, salvage panels and `SalvageableDebris_<Ship>`);
  legacy rock types, tests and templates are ignored. A mineral the table
  lacks is added with its tier from the entity name.
- Made to survive patches (Alan asked for future-proof discovery): the
  record size is derived from the file and the field order verified against
  each record's struct size; a moved signature array is searched for below
  the signature component (and reported); unknown tiers become rows; array
  ranges and the decompressed size are checked up front. An ignored test
  (`SIGEXTRACT_GAME=... cargo test -- --ignored`) checks a new patch.
- CI builds `sigextract.exe` into the `rockhound-windows` artifact.

## 2026-10-18: License change, contributing guide and security policy

Alan asked for a license that stops others copying and monetizing forks, plus
contribution and security reporting rules. No version bump, so no release.

- `LICENSE` is now PolyForm Noncommercial 1.0.0 (Cargo `license` field
  updated to its SPDX id). Source-available, not OSI open source: free for
  noncommercial use and forks, no selling or commercial use.
- `data/signatures.json` stays MIT, credited to Mallachi in
  `THIRD_PARTY_NOTICES.md`; `LICENSE` says listed files keep their own license.
- `CONTRIBUTING.md`: contributions are licensed under the project license and
  the maintainer may relicense them.
- `SECURITY.md` and an issue-form contact link send vulnerability reports to
  GitHub private vulnerability reporting.

## 2026-10-18: v0.6.0, automatic releases

Alan asked for a package release with release notes on every merge to main.

- New CI job `release`: after all other jobs pass on a push to main, it
  publishes GitHub Release `v<version>` with the NSIS installer and the
  portable exe (`Rockhound_<version>_portable.exe`). Merges that don't bump
  the version (Dependabot) find the release already there and skip.
- Release notes are this version's DEVLOG entry followed by GitHub's list of
  PRs merged since the previous release. So every PR that bumps the version
  needs a `## <date>: v<version>, <title>` entry here.
- `app/scripts/release.mjs` reads the version, failing when Cargo.toml,
  package.json and tauri.conf.json disagree, and extracts the DEVLOG entry.
  The `check` job runs both on PRs, so a missed bump or entry fails the PR
  instead of the release.
- The release job uses `gh` rather than a third-party action, so the only
  write-scoped token runs no outside code. A release left without its files
  (or a draft left by a failed publish) gets them on the next run and is
  published, and pending release runs queue instead of replacing each
  other.

## 2026-10-15: v0.5.5, end-to-end tests without the game

Alan asked for app tests an agent or CI can run without the game, to work
towards continuous delivery (minimumcd.org).

- `ROCKHOUND_FAKE_GAME` plays a recorded game instead of capturing Star
  Citizen: a video (decoded by ffmpeg, which also crops the search band, as
  WGC copies only the band) or a folder of full-window screenshots. Frames
  go through the same `capture_rect` and `readout_crop` as live capture, so
  the locator is exercised too; only the frame grab differs.
- `npm run e2e`: WebdriverIO drives the debug build through tauri-driver.
  The spec starts a scan of a fake 5120x1440 game made from the two HUD
  band fixtures, waits for 17,200 and then 21,500 with their matches, stops,
  and saves a setting. Settings and data go to a fresh temp folder.
- New CI job `e2e` (Linux, WebKitWebDriver under xvfb). WebdriverIO 10 is
  used because 9 pulls in a vulnerable `basic-ftp` that fails `npm audit`.
- WebKitWebDriver sends keys faster than Mantine's number field re-renders
  and drops some, so the spec types at about 10 keys/s.
- A debug build reads slower than real time (the 2 s switch shows after
  about 6 s), so the spec allows 20 s for it.
- Second spec, `problem-values`: plays Alan's live crops of the values that
  have gone wrong (3,370 and 14,000 and 25,620 fail leave-one-out; 16,960
  and 17,080 had 6/8 swaps; 25,680 is the mixed cluster) through
  `ROCKHOUND_REPLAY_DIR` and checks each shows up, with 25,680 as "Mixed
  cluster". The bundled templates contain these crops, so it guards against
  regressions rather than measuring accuracy on unseen crops. Each spec gets
  its own settings folder.
- README: how to point an agent at the app, via WebDriver or, on Windows,
  Playwright over WebView2's DevTools port.

## 2026-10-15: v0.5.4, review fixes before merging the stack

From CodeRabbit's reviews when PRs #2 to #5 left draft:

- A window titled "Star Citizen" is only taken for the game when its exe
  path can't be read; a readable path must be StarCitizen.exe.
- "Save debug images" takes effect when settings are saved, not at the
  next scan start, so switching it off mid-scan stops the writes.
- `save_settings` keeps memory in sync with the saved file even when the
  overlay can't be shown (#4).
- CI runs with a read-only token that isn't left in the checkout (#3).

## 2026-10-15: v0.5.3, steady readout and a smaller search band

Alan's v0.5.2 session: 25,680 showed as a mixed cluster but blinked, and
the debug crops were full of rocks, stars and OUTPUT labels.

- The vote cleared the value on the first frame that didn't confirm it,
  and about half the live frames don't (the locator lands on something
  else). `Voter::hold` keeps a shown value through unconfirmed frames;
  the app holds it for 3 s. A different value still replaces it as soon
  as it wins the vote.
- The search band is now sized in window heights around the centre
  (0.3 h either side, 0.25 h to 0.5 h down): 864x360 at 5120x1440 instead
  of 2048x720. The HUD scales with height, so the old band in widths took
  in most of an ultrawide screen.

## 2026-10-15: v0.5.2, mixed clusters

Alan chose to show values only a mix of minerals explains (25,680 =
2 x 4,270 + 4 x 4,285) instead of hiding them.

- `SignatureDb::mixes` lists two-mineral ship-mining clusters of up to 12
  rocks that add up to a value, fewest rocks first, then same-tier pairs.
- `resolve` keeps such a value only when every glyph scores at least 0.8:
  almost any multiple of 5 is some mix, so the table can't catch a misread.
  Over both debug sessions' 119 crops, only the real 25,680 resolves this
  way.
- The readout shows "Mixed cluster" with the first three mixes and a count
  of the rest; the overlay and history say "Mixed cluster".

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
  rejected (handled in v0.5.2).

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
