// End-to-end tests: WebdriverIO drives the real Rockhound app through
// tauri-driver, with a recorded game standing in for Star Citizen.
//
// Run with `npm run e2e` (builds a debug app first). Needs tauri-driver
// (`cargo install tauri-driver --locked`), ffmpeg, and on Linux
// WebKitWebDriver (`webkit2gtk-driver`) plus a display (xvfb-run in CI).

import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const exe = process.platform === "win32" ? ".exe" : "";
const fixtures = join(root, "crates/rockhound-core/tests/fixtures");

// Fresh settings, cache and template folders for every run. The config is
// loaded again in each worker process, which inherits the launcher's folder.
process.env.ROCKHOUND_E2E_HOME ??= mkdtempSync(join(tmpdir(), "rockhound-e2e-"));
const home = process.env.ROCKHOUND_E2E_HOME;
const video = join(home, "game.mkv");

/**
 * A 5120x1440 "game" showing 17,200 for 2 s, then 21,500 for 2 s, with each
 * HUD crop placed at the middle of the readout search band. Lossless, so the
 * 11 px digits survive.
 */
function makeFakeGame() {
  const result = spawnSync(
    "ffmpeg",
    [
      ...["-hide_banner", "-loglevel", "error", "-y"],
      ...["-loop", "1", "-t", "2", "-i", join(fixtures, "hud_band_17200.png")],
      ...["-loop", "1", "-t", "2", "-i", join(fixtures, "hud_band_21500.png")],
      "-filter_complex",
      "[0][1]concat=n=2:v=1,pad=5120:1440:2260:420:black,fps=8,format=rgb24",
      ...["-c:v", "ffv1", video],
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) throw new Error("ffmpeg could not build the fake game video");
}

// tauri-driver starts the app, which inherits these.
process.env.ROCKHOUND_FAKE_GAME = video;
// Keep the run away from the player's own settings (Linux XDG folders).
process.env.XDG_CONFIG_HOME = join(home, "config");
process.env.XDG_DATA_HOME = join(home, "data");

let tauriDriver: ChildProcess | undefined;

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // tauri-driver launches this binary instead of a browser.
      "tauri:options": { application: join(root, `target/debug/rockhound-app${exe}`) },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 60_000 },
  waitforTimeout: 10_000,

  onPrepare: makeFakeGame,
  beforeSession: () => {
    tauriDriver = spawn(join(homedir(), ".cargo/bin/tauri-driver"), [], {
      stdio: [null, process.stdout, process.stderr],
    });
  },
  afterSession: () => {
    tauriDriver?.kill();
  },
  onComplete: () => rmSync(home, { recursive: true, force: true }),
};
