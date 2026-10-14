import { MantineProvider } from "@mantine/core";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Reading } from "./api";
import { Readout } from "./Readout";

const reading = (overrides: Partial<Reading> = {}): Reading => ({
  value: 12720,
  raw: "12,720",
  confidence: 0.9,
  corrected: false,
  matches: [
    { name: "Copper", category: "ship_mining", tier: "common", base: 4240, count: 3 },
    { name: "Salvage Panels", category: "salvage", tier: null, base: 2120, count: 6 },
  ],
  at: 0,
  ...overrides,
});

const show = (r: Reading | null, scanning = true) =>
  render(
    <MantineProvider>
      <Readout reading={r} scanning={scanning} />
    </MantineProvider>,
  );

describe("Readout", () => {
  it("shows the value, best match and alternatives", () => {
    show(reading());
    expect(screen.getByText("12,720")).toBeTruthy();
    expect(screen.getByText("Copper ×3")).toBeTruthy();
    expect(screen.getByText("Ship mining")).toBeTruthy();
    expect(screen.getByText(/Could also be: Salvage Panels ×6/)).toBeTruthy();
    expect(screen.queryByText("corrected")).toBeNull();
  });

  it("flags corrected reads with the raw text", () => {
    show(reading({ corrected: true, raw: "12,780" }));
    expect(screen.getByText("corrected").closest("[title]")?.getAttribute("title")).toBe(
      "Read as 12,780",
    );
  });

  it("says when a value matches nothing", () => {
    show(reading({ matches: [] }));
    expect(screen.getByText("Unknown signature")).toBeTruthy();
  });

  it("prompts while waiting for a value", () => {
    show(null, false);
    expect(screen.getByText(/Press Start/)).toBeTruthy();
    show(null, true);
    expect(screen.getByText(/Looking for a signature/)).toBeTruthy();
  });
});
