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

  it("inserts and replaces segments without reordering equal timestamps unpredictably", () => {
    const late = { ...FIXTURE_SEGMENTS[0], id: "late", startMs: 3_000 };
    const early = { ...FIXTURE_SEGMENTS[0], id: "early", startMs: 1_000 };
    const middle = { ...FIXTURE_SEGMENTS[0], id: "middle", startMs: 2_000 };
    let state = { ...EMPTY_SESSION, segments: [early, late] };
    state = reduceSession(state, { type: "caption_upserted", segment: middle });
    state = reduceSession(state, {
      type: "transcript_finalized",
      segment: { ...middle, sourceText: "final" },
    });
    expect(state.segments.map((segment) => segment.id)).toEqual(["early", "middle", "late"]);
    expect(state.segments[1].sourceText).toBe("final");
  });

  it("retains only the newest fifty recoverable errors", () => {
    let state = EMPTY_SESSION;
    for (let index = 0; index < 75; index += 1) {
      state = reduceSession(state, {
        type: "recoverable_error",
        error: { code: `error-${index}`, message: "recoverable" },
      });
    }
    expect(state.errors).toHaveLength(50);
    expect(state.errors[0].code).toBe("error-25");
    expect(state.errors.at(-1)?.code).toBe("error-74");
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

  it("applies a complete file target-language replacement in one reducer step", () => {
    const before = {
      ...EMPTY_SESSION,
      mode: "file" as const,
      languages: { source: "ja", target: "en", explanation: "en" },
      segments: FIXTURE_SEGMENTS.slice(0, 2).map((segment, index) => ({
        ...segment,
        translationText: `old-${index}`,
        translationStatus: "complete" as const,
      })),
    };

    const after = reduceSession(before, {
      type: "file_retranslation_applied",
      languages: { source: "ja", target: "es", explanation: "es" },
      translations: before.segments.map((segment, index) => ({
        segmentId: segment.id,
        translationText: `nuevo-${index}`,
        ambiguityNote: index === 0 ? "context-dependent" : undefined,
      })),
    });

    expect(after.languages.target).toBe("es");
    expect(after.segments.map((segment) => segment.translationText)).toEqual(["nuevo-0", "nuevo-1"]);
    expect(after.segments[0].ambiguityNote).toBe("context-dependent");
    expect(after.segments.map((segment) => segment.sourceText)).toEqual(before.segments.map((segment) => segment.sourceText));
    expect(before.segments.map((segment) => segment.translationText)).toEqual(["old-0", "old-1"]);
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

  it("retains the last released live caption when a catch-up event omits visibility", () => {
    const visible = { targetDelayMs: 2_500, observedLagMs: 1_800, status: "steady" as const, visibleSegmentId: "live-1" };
    const state = reduceSession(EMPTY_SESSION, { type: "live_sync_changed", sync: visible });
    const catchingUp = reduceSession(state, {
      type: "live_sync_changed",
      sync: { targetDelayMs: 3_200, observedLagMs: 2_900, status: "catching_up" },
    });
    expect(catchingUp.liveSync?.visibleSegmentId).toBe("live-1");
  });

  it("keeps the previous complete pair when coordinated sync selects provisional source", () => {
    const previous = { ...FIXTURE_SEGMENTS[0], id: "live-1", origin: "live" as const };
    const provisional = {
      ...FIXTURE_SEGMENTS[1],
      id: "live-2",
      origin: "live" as const,
      isProvisional: true,
      transcriptionStatus: "pending" as const,
      translationStatus: "complete" as const,
    };
    expect(visibleLiveSegments(
      [previous, provisional],
      { targetDelayMs: 2_500, observedLagMs: 1_900, status: "steady", visibleSegmentId: "live-2" },
      "coordinated",
    )).toEqual([previous]);
  });
});
