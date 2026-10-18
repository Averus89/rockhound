import { describe, expect, it } from "vitest";
// @ts-expect-error plain JS module, run by Node in CI without a build step
import { extractNotes, readVersion } from "./release.mjs";

const DEVLOG = `# Devlog

## 2026-10-15: v0.5.10, later

Ten.

## 2026-10-15: v0.5.1, first

- One.

### Detail

More.

## 2026-10-14: v0.5.0, start

Zero.
`;

describe("extractNotes", () => {
  it("returns the entry with its subsections, up to the next entry", () => {
    expect(extractNotes(DEVLOG, "0.5.1")).toBe(
      "## 2026-10-15: v0.5.1, first\n\n- One.\n\n### Detail\n\nMore.\n",
    );
  });

  it("does not match a version that only shares a prefix", () => {
    expect(extractNotes(DEVLOG, "0.5")).toBe("");
    expect(extractNotes(DEVLOG, "0.5.1")).not.toContain("Ten.");
  });

  it("reads the last entry to the end of the file", () => {
    expect(extractNotes(DEVLOG, "0.5.0")).toBe(
      "## 2026-10-14: v0.5.0, start\n\nZero.\n",
    );
  });

  it("returns an empty string for a missing version", () => {
    expect(extractNotes(DEVLOG, "9.9.9")).toBe("");
  });
});

function files(cargo: string, pkg: string, tauri: string) {
  const map: Record<string, string> = {
    "Cargo.toml": `[workspace]\nmembers = ["a"]\n\n[workspace.package]\nversion = "${cargo}"\n`,
    "app/package.json": JSON.stringify({ version: pkg }),
    "app/src-tauri/tauri.conf.json": JSON.stringify({ version: tauri }),
  };
  return (path: string) => map[path];
}

describe("readVersion", () => {
  it("returns the version when all files agree", () => {
    expect(readVersion(files("1.2.3", "1.2.3", "1.2.3"))).toBe("1.2.3");
  });

  it("names every file when a bump missed one", () => {
    expect(() => readVersion(files("1.2.3", "1.2.3", "1.2.2"))).toThrow(
      "app/src-tauri/tauri.conf.json: 1.2.2",
    );
  });
});
