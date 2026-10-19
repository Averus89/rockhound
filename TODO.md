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
- [x] Sync works now the repo is public
- [x] App updates itself from GitHub Releases (Tauri updater, signed installer)
- [x] Updater key and pubkey set up (Alan)
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

## Glyph packs
- [x] Opt-in sample collection, review screen and zip export
- [x] Issue form + workflow that turns a pack into a PR
- [x] Shipped templates grouped by digit height
- [ ] Allow GitHub Actions to create pull requests in repo settings (Alan)
- [ ] First real pack from another resolution

## Known issues
- Lines are scaled to 48 px, so templates carry across HUD sizes (4K-sized
  39/46, 1080p-sized 35/46), but real packs should beat resized crops.
- Windows OCR quality on 11 px digits is unknown; it reads a 4x upscale.

## Website
- [x] One-page static site in `site/`
- [x] GitHub Pages deploy workflow (`pages.yml`)
- [ ] Switch Pages source to GitHub Actions in repo settings (Alan)
- [ ] Final tagline ("Know the rock before you land." is a placeholder)
- [ ] Social preview image for link cards
