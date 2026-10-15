// Drives the main window like a player: start a scan of the fake game,
// watch the readout follow it, stop, and change a setting.

import { $, browser, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const settingsFile = join(
  process.env.XDG_CONFIG_HOME ?? "",
  "io.github.averus89.rockhound/settings.json",
);

/** XPath to the element whose own text is `text`, as the DOM has it (CSS
 * may upper-case what is shown). */
const ownText = (text: string) => `.//*[text()[normalize-space()='${text}']]`;
const byText = (text: string) => $(ownText(text));
const button = (label: string) => $(`//button[normalize-space()='${label}']`);
/** The big current reading; the history below repeats its labels. */
const readout = () => $(".rh-arrive");

describe("Rockhound main window", () => {
  it("opens idle and waits for Start", async () => {
    await expect($("h3=Rockhound")).toBeDisplayed();
    await expect(byText("Idle")).toBeDisplayed();
    await expect(byText("Press Start, then ping in game.")).toBeDisplayed();
  });

  it("reads the signatures the game shows", async () => {
    await button("Start").click();
    await expect(byText("Scanning")).toBeDisplayed();
    await expect(readout().$(".rh-value")).toHaveText("17,200");
    await expect(readout().$(ownText("Ice ×4"))).toBeDisplayed();
    // The fake game switches to the next rock 2 s in; a debug build reads
    // slower than real time, so that takes about 6 s.
    await expect(readout().$(".rh-value")).toHaveText("21,500", { wait: 20_000 });
    await expect(readout().$(ownText("Ice ×5"))).toBeDisplayed();
    await expect(byText("Recent")).toBeDisplayed();
  });

  it("stops scanning", async () => {
    await button("Stop").click();
    await expect(byText("Idle")).toBeDisplayed();
    await expect(button("Start")).toBeEnabled();
  });

  it("saves settings to disk", async () => {
    await $('[aria-label="Settings"]').click();
    const input = $("//label[normalize-space()='Read every (ms)']/following::input[1]");
    // Select and overtype, as a player would. Clearing the field directly
    // bypasses React, and keys sent all at once outrun the number field's
    // re-render and get dropped, so they go at typing speed.
    await input.click();
    await browser.keys(["Control", "a"]);
    for (const key of "500") {
      await browser.keys(key);
      await browser.pause(100);
    }
    await expect(input).toHaveValue("500");
    await button("Save").click();
    await browser.waitUntil(() => readSettings()?.intervalMs === 500, {
      timeoutMsg: "settings.json never got intervalMs 500",
    });
  });
});

function readSettings(): { intervalMs: number } | undefined {
  try {
    return JSON.parse(readFileSync(settingsFile, "utf8"));
  } catch {
    return undefined;
  }
}
