import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Category = "ship_mining" | "ground_deposit" | "salvage";

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
  at: number;
}

export interface Settings {
  region: [number, number, number, number] | null;
  intervalMs: number;
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

export const onReading = (cb: (r: Reading) => void): Promise<UnlistenFn> =>
  listen<Reading>("reading", (e) => cb(e.payload));
export const onScannerStopped = (cb: (error: string | null) => void): Promise<UnlistenFn> =>
  listen<string | null>("scanner-stopped", (e) => cb(e.payload));
export const onSignaturesUpdated = (cb: (version: string) => void): Promise<UnlistenFn> =>
  listen<string>("signatures-updated", (e) => cb(e.payload));
