import { describe, expect, it } from "vitest";
import { EMPTY_SESSION } from "./contracts";
import { FIXTURE_EVENTS, FIXTURE_SEGMENTS } from "./fixtures";
import { activeSegments, canResumeForCoverage, reduceSession, shouldPauseForCoverage } from "./session";

describe("session contract", () => {
  it("reduces fixture events into one canonical session", () => {
    const state = FIXTURE_EVENTS.reduce(reduceSession, EMPTY_SESSION);
    expect(state.phase).toBe("ready");
    expect(state.segments).toHaveLength(6);
    expect(Object.keys(state.speakers)).toHaveLength(2);
  });

  it("selects no more than two overlapping subtitles", () => {
    const overlapping = FIXTURE_SEGMENTS.map((segment) => ({ ...segment, startMs: 0, endMs: 10_000 }));
    expect(activeSegments(overlapping, 5_000)).toHaveLength(2);
  });

  it("uses the two/eight second catch-up hysteresis", () => {
    expect(shouldPauseForCoverage(9_000, 10_500)).toBe(true);
    expect(canResumeForCoverage(9_000, 16_999)).toBe(false);
    expect(canResumeForCoverage(9_000, 17_000)).toBe(true);
  });
});
