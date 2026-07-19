import { describe, expect, it } from "vitest";
import { EMPTY_SESSION } from "./contracts";
import { FIXTURE_EVENTS, FIXTURE_SEGMENTS } from "./fixtures";
import { activeSegments, applySequencedEvent, canResumeForCoverage, latestLiveSegments, reduceSession, shouldPauseForCoverage, subtitleTimelineTime, visibleLiveSegments } from "./session";

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

  it("requires a snapshot refresh when a window misses an event", () => {
    const state = { ...EMPTY_SESSION, sessionId: "session-4", sequence: 7 };
    expect(applySequencedEvent(state, {
      sessionId: "session-4",
      sequence: 9,
      event: { type: "phase_changed", phase: "ready" },
    })).toBeUndefined();
    expect(applySequencedEvent(state, {
      sessionId: "session-4",
      sequence: 8,
      event: { type: "phase_changed", phase: "ready" },
    })?.phase).toBe("ready");
  });

  it("replaces a provisional live caption and keeps the newest final line", () => {
    const provisional = { ...FIXTURE_SEGMENTS[0], id: "live-2", origin: "live" as const, isProvisional: true };
    const finalized = { ...provisional, sourceText: "Final source", isProvisional: false };
    const withProvisional = reduceSession(EMPTY_SESSION, { type: "caption_upserted", segment: provisional });
    const withFinal = reduceSession(withProvisional, { type: "transcript_finalized", segment: finalized });
    expect(withFinal.segments).toHaveLength(1);
    expect(withFinal.segments[0].sourceText).toBe("Final source");
    expect(latestLiveSegments(withFinal.segments)).toEqual([finalized]);
  });

  it("updates a reconciled file boundary in place without orphaning selection", () => {
    const original = {
      ...FIXTURE_SEGMENTS[0],
      id: "stable-boundary-id",
      sourceText: "今日は",
      translationText: "As for today",
      translationStatus: "complete" as const,
    };
    const selected = {
      ...EMPTY_SESSION,
      segments: [original],
      selectedSegmentId: original.id,
    };
    const revised = {
      ...original,
      sourceText: "今日はちょっと",
      translationText: undefined,
      translationStatus: "failed" as const,
    };

    const next = reduceSession(selected, { type: "transcript_finalized", segment: revised });

    expect(next.segments).toEqual([revised]);
    expect(next.selectedSegmentId).toBe("stable-boundary-id");
  });

  it("shows only the current live caption while preserving finalized history", () => {
    const finalized = { ...FIXTURE_SEGMENTS[0], id: "live-1", origin: "live" as const };
    const provisional = { ...FIXTURE_SEGMENTS[1], id: "live-2", origin: "live" as const, isProvisional: true };
    expect(latestLiveSegments([finalized, provisional])).toEqual([provisional]);
  });

  it("applies VLC-style positive and negative subtitle offsets", () => {
    expect(subtitleTimelineTime(5_000, 1_000)).toBe(4_000);
    expect(subtitleTimelineTime(5_000, -500)).toBe(5_500);
    expect(subtitleTimelineTime(200, 1_000)).toBe(0);
  });

  it("shows only the live segment selected by canonical sync state", () => {
    const first = { ...FIXTURE_SEGMENTS[0], id: "live-1", origin: "live" as const };
    const second = { ...FIXTURE_SEGMENTS[1], id: "live-2", origin: "live" as const };
    expect(visibleLiveSegments([first, second], { targetDelayMs: 2_500, observedLagMs: 1_900, status: "steady", visibleSegmentId: "live-1" })).toEqual([first]);
    expect(visibleLiveSegments([first, second], { targetDelayMs: 2_500, observedLagMs: 1_900, status: "steady" })).toEqual([]);
  });

  it("reduces canonical live sync updates", () => {
    const sync = { targetDelayMs: 3_100, observedLagMs: 2_500, status: "catching_up" as const, visibleSegmentId: "live-2" };
    expect(reduceSession(EMPTY_SESSION, { type: "live_sync_changed", sync }).liveSync).toEqual(sync);
  });
});
