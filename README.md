# Rockhound

Fast Star Citizen signature scanner. Reads the signature value from the HUD
while you fly and tells you what's out there (which ore, how many rocks,
salvage), with a main window and an in-game overlay.

Rewritten from scratch in Rust with a Tauri + React (Mantine) UI. Instead of a general
OCR engine it matches the fixed HUD digit font against stored templates, so a
read takes a few milliseconds and the app stays small.

## Status

Early. What exists now:

- `crates/rockhound-core`: readout pipeline (cleanup, glyph split, template
  matching, signature table lookup, frame voting).
- `crates/rockhound-replay`: tool to test the pipeline against reference videos.
- `app/`: Tauri desktop app. Main window with the current reading, its best
  match and recent reads; settings for the readout box and read interval.
  Live game capture is the next milestone.
- `data/signatures.json`: signature table (SC 4.7). The app checks this repo
  for a newer copy at startup, caches it, and falls back to the bundled one.
  Once the file is publicly readable, a patch only needs a data commit; while
  the repo is private, installed apps keep their bundled table.

## Windows build

Every CI run builds `rockhound-app.exe` and an NSIS installer. Download them
from the run's **Artifacts** (`rockhound-windows`) on the Actions tab.

## Running the app

Needs Rust, Node 22 and, on Linux, the WebKitGTK dev packages.

```sh
cd app
npm install
npm run tauri dev
```

Until live capture lands, the app scans a folder of crops made by the replay
tool. Put templates from `learn` in the app data folder (`templates/`, e.g.
`%APPDATA%\io.github.averus89.rockhound\templates` on Windows) and start it
with `ROCKHOUND_REPLAY_DIR` pointing at a frames folder.

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
