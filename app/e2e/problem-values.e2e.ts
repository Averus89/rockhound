// Plays Alan's live crops of the values that have misread before and checks
// the app shows each one, end to end through the scan loop and the UI.
//
// The bundled templates are learned from these same crops, so this guards
// the pipeline and the display against regressions; leave-one-out accuracy
// on unseen crops is measured in crates/rockhound-core/tests/real_hud.rs.

import { $, browser, expect } from "@wdio/globals";

import { PROBLEM_VALUES } from "./wdio.conf";

const hud = (value: number) => value.toLocaleString("en-US");

describe("Values that have misread before", () => {
  it("are each shown with what they are", async () => {
    await $("//button[normalize-space()='Start']").click();
    const shown = new Map<string, string>();
    const expected = PROBLEM_VALUES.map(hud);
    // The crops loop; collect what the readout shows until every value has
    // been on screen.
    await browser.waitUntil(
      async () => {
        const panel = await browser.execute(
          () => (document.querySelector(".rh-arrive") as HTMLElement | null)?.innerText ?? "",
        );
        const value = expected.find((v) => panel.includes(v));
        if (value) shown.set(value, panel);
        return expected.every((v) => shown.has(v));
      },
      {
        timeout: 60_000,
        interval: 100,
        timeoutMsg: `never shown: ${expected.filter((v) => !shown.has(v)).join(", ")}`,
      },
    );
    // Not a multiple of one mineral: 2 x 4,270 + 4 x 4,285.
    expect(shown.get("25,680")).toContain("Mixed cluster");
    for (const value of expected.filter((v) => v !== "25,680")) {
      expect(shown.get(value)).not.toContain("Unknown signature");
    }
  });
});
