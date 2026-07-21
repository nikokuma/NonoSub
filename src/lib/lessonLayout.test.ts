import { describe, expect, it } from "vitest";
import { calculateLessonStageLayout } from "./lessonLayout";

describe("lesson stage layout", () => {
  it("keeps the normal lesson board and controls inside 980 by 620", () => {
    const layout = calculateLessonStageLayout(980, 620, false);
    expect(layout.compact).toBe(false);
    expect(layout.left + layout.boardWidth + layout.right).toBeLessThanOrEqual(980);
    expect(layout.top + layout.boardHeight + 5 + 42 + layout.bottom).toBeLessThanOrEqual(620);
  });

  it("fits the full board and controls in the constrained Retina geometry", () => {
    const layout = calculateLessonStageLayout(720, 405, false);
    expect(layout.compact).toBe(true);
    expect(layout.left + layout.boardWidth + layout.right).toBeLessThanOrEqual(720);
    expect(layout.top + layout.boardHeight + 5 + 40 + layout.bottom).toBeLessThanOrEqual(405);
  });

  it("reserves additional space for the follow-up composer", () => {
    const closed = calculateLessonStageLayout(720, 405, false);
    const open = calculateLessonStageLayout(720, 405, true);
    expect(open.boardHeight).toBeLessThan(closed.boardHeight);
    expect(open.top + open.boardHeight + 5 + 54 + open.bottom).toBeLessThanOrEqual(405);
  });
});
