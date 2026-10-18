// Release helpers for CI: the app version and its DEVLOG entry.
//
//   node app/scripts/release.mjs version         prints the version
//   node app/scripts/release.mjs notes <version> prints that version's notes

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ROOT = new URL("../../", import.meta.url);

/**
 * Returns the workspace version, failing when the files that carry it
 * disagree: the installer is named from tauri.conf.json, the exe from
 * Cargo.toml, so a half-done bump would ship mismatched files.
 * @param {(path: string) => string} read reads a file relative to the repo root
 * @returns {string}
 */
export function readVersion(read) {
  const cargo = read("Cargo.toml").match(
    /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m,
  )?.[1];
  const versions = {
    "Cargo.toml": cargo,
    "app/package.json": JSON.parse(read("app/package.json")).version,
    "app/src-tauri/tauri.conf.json": JSON.parse(
      read("app/src-tauri/tauri.conf.json"),
    ).version,
  };
  const distinct = new Set(Object.values(versions));
  if (distinct.size !== 1 || !cargo) {
    const list = Object.entries(versions)
      .map(([file, v]) => `${file}: ${v ?? "missing"}`)
      .join(", ");
    throw new Error(`Version mismatch: ${list}`);
  }
  return cargo;
}

/**
 * Returns the `## <date>: v<version>, <title>` entry of DEVLOG.md as
 * release notes, or "" when there is none.
 * @param {string} devlog
 * @param {string} version without the leading "v"
 * @returns {string}
 */
export function extractNotes(devlog, version) {
  const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const heading = new RegExp(`^## [^\\n]*\\bv${escaped}\\b(?![.\\d])[^\\n]*$`, "m");
  const match = heading.exec(devlog);
  if (!match) return "";
  const start = match.index + match[0].length;
  const next = devlog.slice(start).search(/^## /m);
  const body = devlog.slice(start, next === -1 ? undefined : start + next);
  const title = match[0].replace(/^## /, "").trim();
  return `## ${title}\n\n${body.trim()}\n`;
}

function main([command, version]) {
  const read = (path) => readFileSync(new URL(path, ROOT), "utf8");
  if (command === "version") {
    console.log(readVersion(read));
  } else if (command === "notes" && version) {
    const notes = extractNotes(read("DEVLOG.md"), version);
    // A release without its changelog entry is a missed DEVLOG update.
    if (!notes) throw new Error(`DEVLOG.md has no entry for v${version}`);
    process.stdout.write(notes);
  } else {
    throw new Error("usage: release.mjs version | notes <version>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
