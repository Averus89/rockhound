import { MantineProvider } from "@mantine/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AvailableUpdate } from "./api";
import { RELEASES_URL, UpdateNotice } from "./UpdateNotice";

const show = (update: AvailableUpdate) =>
  render(
    <MantineProvider>
      <UpdateNotice update={update} />
    </MantineProvider>,
  );

describe("UpdateNotice", () => {
  it("installs the update when the app was installed", () => {
    const install = vi.fn(() => new Promise<void>(() => {}));
    show({ version: "0.8.0", installed: true, install });
    expect(screen.getByText("Rockhound v0.8.0 is available")).toBeTruthy();
    fireEvent.click(screen.getByText("Update and restart"));
    expect(install).toHaveBeenCalledOnce();
  });

  it("shows why an update failed", async () => {
    show({
      version: "0.8.0",
      installed: true,
      install: () => Promise.reject(new Error("signature mismatch")),
    });
    fireEvent.click(screen.getByText("Update and restart"));
    await waitFor(() =>
      expect(screen.getByText("Update failed: Error: signature mismatch")).toBeTruthy(),
    );
  });

  it("points the portable exe at the releases page", () => {
    show({ version: "0.8.0", installed: false, install: vi.fn() });
    expect(screen.getByText(`Download it from ${RELEASES_URL}.`)).toBeTruthy();
    expect(screen.queryByText("Update and restart")).toBeNull();
  });
});
