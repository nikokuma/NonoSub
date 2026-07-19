import { describe, expect, it } from "vitest";
import type { LessonOpenContext, SubtitleSegment } from "./contracts";
import { lessonThreadKey } from "./lessonIdentity";

const segment: SubtitleSegment = {
  id: "segment-1",
  origin: "file",
  startMs: 1_000,
  endMs: 2_000,
  sourceText: "今日はちょっと……",
  translationText: "Today is a little…",
  speakerId: "speaker-1",
  isProvisional: false,
  transcriptionStatus: "complete",
  translationStatus: "complete",
};

function context(overrides: Partial<LessonOpenContext> = {}): LessonOpenContext {
  return {
    selectionId: 1,
    sessionId: "session-1",
    sourceSurface: "viewer",
    segmentId: segment.id,
    selectedSegment: segment,
    cursorX: 20,
    cursorY: 30,
    externalMediaControl: "not_requested",
    ...overrides,
  };
}

describe("pinned lesson identity", () => {
  it("retains a thread when the same source revision is reopened", () => {
    expect(lessonThreadKey(context({ selectionId: 1 }))).toBe(
      lessonThreadKey(context({ selectionId: 8 })),
    );
  });

  it("isolates reused segment IDs in another session", () => {
    expect(lessonThreadKey(context())).not.toBe(
      lessonThreadKey(context({ sessionId: "session-2" })),
    );
  });

  it("isolates a revised source line without depending on its translation", () => {
    const revised = context({
      selectedSegment: { ...segment, sourceText: "今日はちょっと難しいです。" },
    });
    const translated = context({
      selectedSegment: { ...segment, translationText: "Today will not work." },
    });
    expect(lessonThreadKey(context())).not.toBe(lessonThreadKey(revised));
    expect(lessonThreadKey(context())).toBe(lessonThreadKey(translated));
  });
});
