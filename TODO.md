# TODO

## Milestone 1: replay harness + recognizer (in progress)
- [x] Core pipeline: preprocess, segment, template match, resolve, vote
- [x] Replay tool: frames / learn / eval
- [ ] Run on Alan's reference videos and tune
- [ ] Decide: templates good enough, or add ONNX digit model

## Milestone 2: live capture
- [ ] Windows Graphics Capture of the SC window
- [ ] Locate the orange pin to find the readout automatically
- [ ] Timed pipeline loop

## Milestone 3: Tauri shell
- [ ] Tauri v2 + React + Mantine main window (simplified UI)
- [ ] Signature table sync (fetch, validate, cache, bundled fallback)

## Milestone 4: overlay
## Milestone 5: packaging + release workflow

## Known issues
- Template scale is fixed to the capture resolution; other HUD sizes need
  their own templates (resize-to-sample covers small differences only).
