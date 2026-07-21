import { describe, expect, it } from "vitest";
import { clampSubtitlePosition, mediaEventIsCurrent } from "./viewerLayout";

describe("viewer layout", () => {
  it("keeps the complete subtitle panel inside every viewer edge", () => {
    const viewport = { width: 1_280, height: 720 };
    const panel = { width: 900, height: 220 };
    expect(clampSubtitlePosition({ x: 1, y: 1 }, viewport, panel)).toEqual({
      x: 822 / 1_280,
      y: 602 / 720,
    });
    expect(clampSubtitlePosition({ x: 0, y: 0 }, viewport, panel)).toEqual({
      x: 458 / 1_280,
      y: 118 / 720,
    });
  });

  it("centers an oversized panel instead of letting either edge escape", () => {
    expect(clampSubtitlePosition(
      { x: 0.9, y: 0.1 },
      { width: 640, height: 360 },
      { width: 700, height: 400 },
    )).toEqual({ x: 0.5, y: 0.5 });
  });

  it("rejects events from a replaced media element or session", () => {
    const current = {} as HTMLVideoElement;
    const stale = {} as HTMLVideoElement;
    expect(mediaEventIsCurrent(current, current, "session-2:file-b", "session-2:file-b")).toBe(true);
    expect(mediaEventIsCurrent(stale, current, "session-1:file-a", "session-2:file-b")).toBe(false);
    expect(mediaEventIsCurrent(current, current, "session-1:file-a", "session-2:file-b")).toBe(false);
  });
});
