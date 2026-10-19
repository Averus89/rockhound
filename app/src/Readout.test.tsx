import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Location, Reading } from "./api";
import { Readout } from "./Readout";

const reading = (overrides: Partial<Reading> = {}): Reading => ({
  value: 12720,
  raw: "12,720",
  confidence: 0.9,
  corrected: false,
  mixes: [],
  matches: [
    { name: "Copper", category: "ship_mining", tier: "common", base: 4240, count: 3 },
    { name: "Salvage Panels", category: "salvage", tier: null, base: 2120, count: 6 },
  ],
  at: 0,
  ...overrides,
});

const show = (r: Reading | null, scanning = true, location: Location = "unknown") =>
  render(
    <MantineProvider>
      <Readout reading={r} scanning={scanning} location={location} />
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

  it("lists the likeliest mixes of a mixed cluster", () => {
    const mix = (a: string, b: string): Reading["mixes"][number] => [
      { name: a, category: "ship_mining", tier: "common", base: 4270, count: 2 },
      { name: b, category: "ship_mining", tier: "common", base: 4285, count: 4 },
    ];
    const mixes = ["A", "B", "C", "D", "E"].map((n) => mix(`Iron ${n}`, "Ice"));
    show(reading({ value: 25680, matches: [], mixes }));
    expect(screen.getByText("Mixed cluster")).toBeTruthy();
    expect(screen.getByText("Iron A ×2 + Ice ×4")).toBeTruthy();
    expect(screen.queryByText("Iron D ×2 + Ice ×4")).toBeNull();
    expect(screen.getByText("and 2 other mixes")).toBeTruthy();
  });

  it("asks where the player is only when that would rank the matches", () => {
    const collision = reading({
      value: 3000,
      matches: [
        { name: "Small Ground Deposit", category: "ground_deposit", tier: null, base: 3000, count: 1 },
        { name: "Capital Wreck Debris", category: "salvage", tier: null, base: 3000, count: 1 },
      ],
    });
    show(collision);
    expect(screen.getByText(/Choose where you are/)).toBeTruthy();
    cleanup();
    show(collision, true, "space");
    expect(screen.queryByText(/Choose where you are/)).toBeNull();
    cleanup();
    show(reading());
    expect(screen.queryByText(/Choose where you are/)).toBeNull();
  });

  it("prompts while waiting for a value", () => {
    show(null, false);
    expect(screen.getByText(/Press Start/)).toBeTruthy();
    show(null, true);
    expect(screen.getByText(/Looking for a signature/)).toBeTruthy();
  });
});
