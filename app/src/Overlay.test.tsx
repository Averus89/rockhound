import { MantineProvider } from "@mantine/core";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Reading } from "./api";
import { OverlayCard } from "./Overlay";

const reading: Reading = {
  value: 16960,
  raw: "16,960",
  confidence: 0.95,
  corrected: false,
  mixes: [],
  matches: [{ name: "Copper", category: "ship_mining", tier: "common", base: 4240, count: 4 }],
  at: 0,
};

const show = (r: Reading | null) =>
  render(
    <MantineProvider>
      <OverlayCard reading={r} />
    </MantineProvider>,
  );

describe("OverlayCard", () => {
  it("shows the value and best match", () => {
    show(reading);
    expect(screen.getByText("16,960")).toBeTruthy();
    expect(screen.getByText("Copper ×4")).toBeTruthy();
    expect(screen.getByText("common")).toBeTruthy();
  });

  it("draws nothing without a value", () => {
    show({ ...reading, value: null });
    expect(screen.queryByText("Copper", { exact: false })).toBeNull();
    expect(screen.queryByText("16,960")).toBeNull();
  });
});
