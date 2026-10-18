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
  Live capture of the game window on Windows, and an overlay.
- `data/signatures.json`: signature table (SC 4.7). The app checks this repo
  for a newer copy at startup, caches it, and falls back to the bundled one.
  Once the file is publicly readable, a patch only needs a data commit; while
  the repo is private, installed apps keep their bundled table.

## Windows build and releases

Every CI run builds `rockhound-app.exe` and an NSIS installer. Download them
from the run's **Artifacts** (`rockhound-windows`) on the Actions tab.

A merge to main that bumps the version publishes a GitHub Release with the
installer, the portable exe and notes taken from that version's `DEVLOG.md`
entry plus the merged PRs. To release, bump the version in `Cargo.toml`,
`app/package.json` and `app/src-tauri/tauri.conf.json`, and add a
`## <date>: v<version>, <title>` entry to `DEVLOG.md`; CI checks both on the
PR.

## Running the app

Needs Rust, Node 22 and, on Linux, the WebKitGTK dev packages.

```sh
cd app
npm install
npm run tauri dev
```

On Windows the app captures the Star Citizen window. Anywhere else, or to
work without the game, point `ROCKHOUND_FAKE_GAME` at a recording before
starting it:

- a video of the game window (any format ffmpeg reads; needs `ffmpeg` and
  `ffprobe` on PATH), sampled once per read interval so it plays in real
  time, or
- a folder of full-window PNG screenshots, played in name order.

Both loop, and go through the same search band and pin locator as live
capture. `ROCKHOUND_REPLAY_DIR` still plays a folder of readout crops made by
the replay tool, skipping the locator.

## Automated tests

```sh
cargo test --workspace         # recognizer, scan loop, fake game
cd app && npm test             # React components
cd app && npm run e2e          # the whole app, driven through WebDriver
```

`npm run e2e` builds a debug app, turns two HUD screenshots from
`crates/rockhound-core/tests/fixtures` into a 5120x1440 fake game video, and
uses [WebdriverIO](https://webdriver.io) with
[tauri-driver](https://v2.tauri.app/develop/tests/webdriver/) to click
through the main window like a player: Start, see 17,200 and then 21,500
with their matches, Stop, change a setting. A second spec plays live crops
of the values that have misread before (3,370, 14,000, 16,960, 17,080,
25,620 and the mixed cluster 25,680) and checks each one is shown. Specs live
in `app/e2e/`. CI runs
them on Linux under a virtual display on every push.

It needs `tauri-driver` (`cargo install tauri-driver --locked`) and ffmpeg.
On Linux also `webkit2gtk-driver` and a display (`xvfb-run -a npm run e2e` on
a headless machine). On Windows it needs `msedgedriver` matching the
installed WebView2 version on PATH.

### Letting an agent drive the app

Two ways, both without the game:

- **WebDriver (Linux and Windows).** Start `tauri-driver`, then any WebDriver
  client can open a session with the capability
  `"tauri:options": {"application": "<path to rockhound-app>"}` and find,
  click and read elements, the way the e2e specs do.
- **Playwright over CDP (Windows).** WebView2 is Chromium, so it can expose
  the DevTools protocol:

  ```powershell
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
  $env:ROCKHOUND_FAKE_GAME = "C:\path\to\clip.mp4"
  cd app; npm run tauri dev
  ```

  Then attach Playwright with `chromium.connectOverCDP("http://localhost:9222")`,
  or give an agent the Playwright MCP server with
  `npx @playwright/mcp --cdp-endpoint http://localhost:9222`. Each window
  (main and overlay) shows up as a page.

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
