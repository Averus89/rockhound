import { describe, expect, it } from "vitest";

import { formatSignature } from "./format";

describe("formatSignature", () => {
  it("groups thousands like the HUD", () => {
    expect(formatSignature(17200)).toBe("17,200");
    expect(formatSignature(950)).toBe("950");
  });
});
