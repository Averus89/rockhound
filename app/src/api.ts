import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { check } from "@tauri-apps/plugin-updater";

export type Category = "ship_mining" | "ground_deposit" | "salvage";

/** Where the player is scanning; ranks colliding signatures. */
export type Location = "unknown" | "space" | "planet";

export interface SignatureMatch {
  name: string;
  category: Category;
  tier: string | null;
  base: number;
  count: number;
}

export interface Reading {
  value: number | null;
  raw: string;
  confidence: number;
  corrected: boolean;
  matches: SignatureMatch[];
  /** Two-mineral clusters adding up to the value, when no single source does. */
  mixes: [SignatureMatch, SignatureMatch][];
  at: number;
}

export interface Settings {
  region: [number, number, number, number] | null;
  intervalMs: number;
  overlay: boolean;
  debug: boolean;
  /** Keep read crops as glyph samples the player can send in. */
  glyphSamples: boolean;
  location: Location;
}

export interface Status {
  scanning: boolean;
  dataVersion: string;
  scVersion: string;
  settings: Settings;
}

export const getStatus = () => invoke<Status>("get_status");
export const saveSettings = (settings: Settings) =>
  invoke<void>("save_settings", { settings });
export const startScan = () => invoke<void>("start_scan");
export const stopScan = () => invoke<void>("stop_scan");

/** A readout crop kept as a glyph sample, labelled with what was read. */
export interface GlyphSample {
  file: string;
  value: number;
  /** PNG bytes. */
  png: number[];
}

export const listGlyphSamples = () => invoke<GlyphSample[]>("list_glyph_samples");
export const deleteGlyphSample = (file: string) => invoke<void>("delete_glyph_sample", { file });
/** Zips the samples with their checked values; returns the zip's path. */
export const exportGlyphPack = (samples: { file: string; value: number }[]) =>
  invoke<string>("export_glyph_pack", { samples });

export const onReading = (cb: (r: Reading) => void): Promise<UnlistenFn> =>
  listen<Reading>("reading", (e) => cb(e.payload));
export const onScannerStopped = (cb: (error: string | null) => void): Promise<UnlistenFn> =>
  listen<string | null>("scanner-stopped", (e) => cb(e.payload));
export const onSignaturesUpdated = (cb: (version: string) => void): Promise<UnlistenFn> =>
  listen<string>("signatures-updated", (e) => cb(e.payload));

/** A newer Rockhound release than the running one. */
export interface AvailableUpdate {
  version: string;
  /** Installed copies update in place; the portable exe can't. */
  installed: boolean;
  /** Downloads, verifies and runs the installer, which closes the app. */
  install: () => Promise<void>;
}

/** The newest release when it is newer than this copy, otherwise null. */
export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  const [update, installed] = await Promise.all([check(), invoke<boolean>("is_installed")]);
  if (!update) return null;
  return { version: update.version, installed, install: () => update.downloadAndInstall() };
}
