// End-to-end tests: WebdriverIO drives the real Rockhound app through
// tauri-driver, with a recorded game standing in for Star Citizen.
//
// Run with `npm run e2e` (builds a debug app first). Needs tauri-driver
// (`cargo install tauri-driver --locked`), ffmpeg, and on Linux
// WebKitWebDriver (`webkit2gtk-driver`) plus a display (xvfb-run in CI).

import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const exe = process.platform === "win32" ? ".exe" : "";
const fixtures = join(root, "crates/rockhound-core/tests/fixtures");
// Alan's 1440p crops (digit lines ~12 px tall).
const liveCrops = join(root, "data/glyphs/h12");

// Fresh settings, cache and template folders for every run. The config is
// loaded again in each worker process, which inherits the launcher's folder.
process.env.ROCKHOUND_E2E_HOME ??= mkdtempSync(join(tmpdir(), "rockhound-e2e-"));
const home = process.env.ROCKHOUND_E2E_HOME;
const video = join(home, "game.mkv");
const problemCrops = join(home, "problem-crops");

/**
 * Values that have misread in Alan's sessions or fail the leave-one-out
 * test (crates/rockhound-core/tests/real_hud.rs): 6 vs 8 (16,960, 17,080),
 * the 25,6x0 pair, the mixed cluster 25,680, and 3,370 and 14,000.
 */
export const PROBLEM_VALUES = [3370, 14000, 16960, 17080, 25620, 25680];

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

/**
 * A folder of Alan's live readout crops for each problem value, grouped by
 * value so the vote settles on each in turn. Every value gets at least two
 * crops, since the vote needs two agreeing reads.
 */
function makeProblemCrops() {
  mkdirSync(problemCrops, { recursive: true });
  const files = readdirSync(liveCrops);
  PROBLEM_VALUES.forEach((value, group) => {
    const crops = files.filter((f) => f.startsWith(`${value}-`)).sort();
    const copies = crops.length > 1 ? crops : [crops[0], crops[0]];
    copies.forEach((crop, i) => {
      copyFileSync(join(liveCrops, crop), join(problemCrops, `${group}-${i}.png`));
    });
  });
}

/**
 * Point the app tauri-driver is about to start at the recording a spec
 * needs, with its own settings and data folders (Linux XDG folders), so
 * one spec's saved settings don't change the next.
 */
function prepareApp(spec: string) {
  const name = spec.replace(/^.*[\\/]|\.e2e\.ts$/g, "");
  delete process.env.ROCKHOUND_FAKE_GAME;
  delete process.env.ROCKHOUND_REPLAY_DIR;
  if (name === "problem-values") process.env.ROCKHOUND_REPLAY_DIR = problemCrops;
  else process.env.ROCKHOUND_FAKE_GAME = video;
  process.env.XDG_CONFIG_HOME = join(home, name, "config");
  process.env.XDG_DATA_HOME = join(home, name, "data");
}

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

  onPrepare: () => {
    makeFakeGame();
    makeProblemCrops();
  },
  beforeSession: (_config, _capabilities, specs) => {
    prepareApp(specs[0]);
    // tauri-driver starts the app, which inherits the environment.
    tauriDriver = spawn(join(homedir(), ".cargo/bin/tauri-driver"), [], {
      stdio: [null, process.stdout, process.stderr],
    });
  },
  afterSession: () => {
    tauriDriver?.kill();
  },
  onComplete: () => rmSync(home, { recursive: true, force: true }),
};
