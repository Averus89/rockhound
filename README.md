# Rockhound

Fast Star Citizen signature scanner. Reads the signature value from the HUD
while you fly and tells you what's out there (which ore, how many rocks,
salvage), with a main window and an in-game overlay.

Rewritten from scratch in Rust (Tauri UI coming next). Instead of a general
OCR engine it matches the fixed HUD digit font against stored templates, so a
read takes a few milliseconds and the app stays small.

## Status

Early. What exists now:

- `crates/rockhound-core`: readout pipeline (cleanup, glyph split, template
  matching, signature table lookup, frame voting).
- `crates/rockhound-replay`: tool to test the pipeline against reference videos.
- `data/signatures.json`: signature table (SC 4.7). Will be synced from this
  repo by the app so a patch only needs a data commit.

## Testing against a reference video

1. Write a labels file next to the video, one range per line:

   ```text
   0:12-0:19 16,960
   0:19-0:25 -
   ```

   `-` means no readout is on screen.

2. Run (needs `ffmpeg` on PATH):

   ```sh
   cargo run --release -p rockhound-replay -- frames clip.mp4 --region 2540,490,85,40 --out replay/clip
   cargo run --release -p rockhound-replay -- learn replay/clip --labels clip.txt --out replay/templates
   cargo run --release -p rockhound-replay -- eval replay/clip --labels clip.txt --templates replay/templates
   ```

   `--region` is the readout box in video pixels (x,y,width,height). `eval`
   prints every wrong frame plus per-frame and voted accuracy and read time.
   Learn from one clip and evaluate on another to get an honest number.

## Credits

Maintained by Averus89. Inspired by SC Signature Scanner, originally by
Mallachi; the signature table is converted from it (see
`THIRD_PARTY_NOTICES.md`).

## License

MIT
