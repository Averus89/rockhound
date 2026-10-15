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

## Milestone 4: overlay
## Milestone 5: packaging + release workflow
- [x] Windows exe + installer artifact in CI
- [x] Coverage gate, frontend tests, cargo/npm audit, Dependabot
- [ ] CodeQL if the repo goes public

## Known issues
- Lines are scaled to 24 px, so templates carry across HUD scales, but a
  different font weight (e.g. 4K UI scale) may still need its own samples.
- Windows OCR quality on 11 px digits is unknown; it reads a 4x upscale.
