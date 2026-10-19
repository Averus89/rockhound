# TODO

## Milestone 1: replay harness + recognizer (in progress)
- [x] Core pipeline: preprocess, line layout, template alignment, resolve, vote
- [x] Replay tool: frames / learn / eval
- [ ] Run on Alan's reference videos and tune
- [ ] Decide: templates good enough, or add ONNX digit model

## Milestone 2: live capture
- [x] Windows Graphics Capture of the SC window
- [x] Locate the orange pin to find the readout automatically
- [x] Timed pipeline loop
- [x] Windows OCR teaches templates on a fresh install
- [ ] Verify on a real game session (Alan)

## Milestone 3: Tauri shell
- [x] Tauri v2 + React + Mantine main window (simplified UI)
- [x] Signature table sync (fetch, validate, cache, bundled fallback)
- [ ] Sync only works once the repo (or the data file) is publicly readable
- [ ] Ship templates with the app instead of the app data folder
- [x] `sigextract`: refresh signatures.json from Data.p4k after a patch
- [x] First real run of `sigextract` (SC 4.7 LIVE): table confirmed

## Milestone 4: overlay
- [x] Click-through always-on-top overlay window with the current value
- [ ] Let the player move the overlay (drag mode or position setting)
## Milestone 5: packaging + release workflow
- [x] Windows exe + installer artifact in CI
- [x] GitHub Release with notes on every version bump merged to main
- [x] Coverage gate, frontend tests, cargo/npm audit, Dependabot
- [ ] CodeQL if the repo goes public
- [x] End-to-end UI tests on a fake game (WebdriverIO + tauri-driver, CI)
- [ ] Overlay e2e check (needs a second window handle in the spec)
- [ ] Run e2e on Windows too (msedgedriver must match the runner's WebView2)

## Location context
- [x] Manual space / planet switch ranks colliding signatures
- [ ] Detect space vs planet automatically (Game.log or HUD cue)

## Known issues
- Lines are scaled to 24 px, so templates carry across HUD scales, but a
  different font weight (e.g. 4K UI scale) may still need its own samples.
- Windows OCR quality on 11 px digits is unknown; it reads a 4x upscale.
