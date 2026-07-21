import { describe, expect, it } from "vitest";
import { calculateLessonStageLayout, LESSON_BOARD_TARGET } from "./lessonLayout";

function expectContained(layout: ReturnType<typeof calculateLessonStageLayout>, width: number, height: number) {
  for (const rect of [layout.board, layout.characterRail, layout.bubble, layout.controls]) {
    expect(rect.x).toBeGreaterThanOrEqual(0);
    expect(rect.y).toBeGreaterThanOrEqual(0);
    expect(rect.x + rect.width).toBeLessThanOrEqual(width);
    expect(rect.y + rect.height).toBeLessThanOrEqual(height);
  }
  expect(layout.characterRail.x).toBeGreaterThan(layout.board.x + layout.board.width);
  expect(layout.bubble.x).toBeGreaterThanOrEqual(layout.board.x + layout.board.width);
  expect(layout.controls.y).toBeGreaterThan(layout.board.y + layout.board.height);
}

describe("lesson stage layout", () => {
  it("renders the literal doubled board at the full lesson target", () => {
    const layout = calculateLessonStageLayout(2048, 1024, false);
    expect(layout.scale).toBe(1);
    expect(layout.board.width).toBe(LESSON_BOARD_TARGET.width);
    expect(layout.board.height).toBe(LESSON_BOARD_TARGET.height);
    expect(layout.boardContentScale).toBe(1.65);
    expectContained(layout, 2048, 1024);
  });

  it.each([
    [2048, 1024],
    [1728, 864],
    [1360, 680],
    [720, 360],
    [720, 405],
  ])("keeps the board, Nono rail, bubble, and controls contained at %d by %d", (width, height) => {
    expectContained(calculateLessonStageLayout(width, height, false), width, height);
  });

  it("shrinks every major rectangle by the same ratio", () => {
    const full = calculateLessonStageLayout(2048, 1024, false);
    const half = calculateLessonStageLayout(1024, 512, false);
    expect(half.scale).toBe(0.5);
    expect(half.board.width).toBe(full.board.width / 2);
    expect(half.board.height).toBe(full.board.height / 2);
    expect(half.characterRail.width).toBe(full.characterRail.width / 2);
    expect(half.bubble.height).toBe(full.bubble.height / 2);
  });

  it("does not move or shrink the board when Ask Another opens", () => {
    const closed = calculateLessonStageLayout(1360, 680, false);
    const open = calculateLessonStageLayout(1360, 680, true);
    expect(open.board).toEqual(closed.board);
    expect(open.controls).toEqual(closed.controls);
  });
});
