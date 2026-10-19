import { MantineProvider } from "@mantine/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as api from "./api";
import { GlyphPackModal } from "./GlyphPackModal";

vi.mock("./api", () => ({
  listGlyphSamples: vi.fn(),
  deleteGlyphSample: vi.fn(),
  exportGlyphPack: vi.fn(),
}));

const show = () =>
  render(
    <MantineProvider>
      <GlyphPackModal opened onClose={() => {}} />
    </MantineProvider>,
  );

describe("GlyphPackModal", () => {
  beforeEach(() => {
    vi.mocked(api.listGlyphSamples).mockResolvedValue([
      { file: "17200-1.png", value: 17200, png: [137, 80, 78, 71] },
      { file: "3000-2.png", value: 3000, png: [137, 80, 78, 71] },
    ]);
    vi.mocked(api.deleteGlyphSample).mockResolvedValue();
    vi.mocked(api.exportGlyphPack).mockResolvedValue("C:\\Downloads\\rockhound-glyphs-1.zip");
    globalThis.URL.createObjectURL = vi.fn(() => "blob:x");
    globalThis.URL.revokeObjectURL = vi.fn();
  });

  it("sends the samples with corrected numbers, minus removed ones", async () => {
    show();
    await waitFor(() => expect(screen.getByText("Send 2 samples")).toBeTruthy());
    fireEvent.click(screen.getAllByLabelText("Remove sample")[1]);
    await waitFor(() => expect(screen.getByText("Send 1 sample")).toBeTruthy());
    expect(api.deleteGlyphSample).toHaveBeenCalledWith("3000-2.png");

    fireEvent.change(screen.getByLabelText("Number shown"), { target: { value: "17300" } });
    fireEvent.click(screen.getByText("Send 1 sample"));
    await waitFor(() => expect(screen.getByText(/rockhound-glyphs-1\.zip/)).toBeTruthy());
    expect(api.exportGlyphPack).toHaveBeenCalledWith([{ file: "17200-1.png", value: 17300 }]);
  });

  it("says how to collect when there are no samples", async () => {
    vi.mocked(api.listGlyphSamples).mockResolvedValue([]);
    show();
    await waitFor(() => expect(screen.getByText(/No samples yet/)).toBeTruthy());
  });
});
