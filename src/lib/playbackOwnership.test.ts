import { describe, expect, it } from "vitest";
import type { LessonClosedContext, LessonOpenContext, SubtitleSegment } from "./contracts";
import { createPlaybackPauseLease, shouldResumePlayback } from "./playbackOwnership";

const segment: SubtitleSegment = {
  id: "line-1",
  origin: "file",
  startMs: 0,
  endMs: 1_000,
  sourceText: "何ですか？",
  translationText: "What is it?",
  isProvisional: false,
  transcriptionStatus: "complete",
  translationStatus: "complete",
};

const open: LessonOpenContext = {
  selectionId: 7,
  sessionId: "session-4",
  sourceSurface: "viewer",
  segmentId: segment.id,
  selectedSegment: segment,
  cursorX: 0,
  cursorY: 0,
  externalMediaControl: "not_requested",
};

const closed: LessonClosedContext = {
  selectionId: 7,
  sessionId: "session-4",
  sourceSurface: "viewer",
  segmentId: segment.id,
  reason: "closed",
};

function current(overrides: Partial<Parameters<typeof shouldResumePlayback>[2]> = {}) {
  return {
    sessionId: "session-4",
    mediaInstanceId: "/video.mov",
    playbackRevision: 3,
    paused: true,
    coverageReady: true,
    ...overrides,
  };
}

describe("file lesson playback ownership", () => {
  it("resumes only a matching lesson-owned pause", () => {
    const lease = createPlaybackPauseLease(open, "/video.mov", true, 3)!;
    expect(shouldResumePlayback(lease, closed, current())).toBe(true);
  });

  it("keeps a video paused when it was already paused", () => {
    const lease = createPlaybackPauseLease(open, "/video.mov", false, 3)!;
    expect(shouldResumePlayback(lease, closed, current())).toBe(false);
  });

  it("does not override a user or buffering playback revision", () => {
    const lease = createPlaybackPauseLease(open, "/video.mov", true, 3)!;
    expect(shouldResumePlayback(lease, closed, current({ playbackRevision: 4 }))).toBe(false);
    expect(shouldResumePlayback(lease, closed, current({ coverageReady: false }))).toBe(false);
  });

  it("rejects replacement media, stale selections, and invalidation", () => {
    const lease = createPlaybackPauseLease(open, "/video.mov", true, 3)!;
    expect(shouldResumePlayback(lease, closed, current({ sessionId: "session-5" }))).toBe(false);
    expect(shouldResumePlayback(lease, closed, current({ mediaInstanceId: "/other.mov" }))).toBe(false);
    expect(shouldResumePlayback(lease, { ...closed, selectionId: 8 }, current())).toBe(false);
    expect(shouldResumePlayback(lease, { ...closed, reason: "invalidated" }, current())).toBe(false);
  });

  it("does not create a file playback lease for live lessons", () => {
    expect(createPlaybackPauseLease({ ...open, sourceSurface: "overlay" }, "/video.mov", true, 3)).toBeUndefined();
  });
});
