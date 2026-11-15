import { describe, expect, it } from "vitest";
import { canCancel, equipmentTone, intervalsOverlap, nextActions } from "./domain";

describe("reservation rules displayed by the UI", () => {
  it("uses half-open intervals so an adjacent booking is allowed", () => {
    expect(intervalsOverlap(10, 12, 12, 14)).toBe(false);
    expect(intervalsOverlap(10, 13, 12, 14)).toBe(true);
  });

  it("only lets the owner or an administrator cancel an unstarted booking", () => {
    expect(canCancel("pending", "member", true)).toBe(true);
    expect(canCancel("approved", "member", false)).toBe(false);
    expect(canCancel("checked_out", "admin", true)).toBe(false);
  });

  it("shows only the next administrator action for each open state", () => {
    expect(nextActions("pending", "admin")).toEqual(["approve", "reject"]);
    expect(nextActions("approved", "admin")).toEqual(["checkout"]);
    expect(nextActions("checked_out", "admin")).toEqual(["return"]);
    expect(nextActions("pending", "member")).toEqual([]);
  });

  it("marks unavailable equipment as paused", () => {
    expect(equipmentTone("active")).toBe("ready");
    expect(equipmentTone("maintenance")).toBe("paused");
  });
});
