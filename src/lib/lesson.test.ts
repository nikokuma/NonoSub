import { describe, expect, it } from "vitest";
import { FIXTURE_LESSON } from "./fixtures";
import { isLessonSkipped } from "./lesson";

describe("progressive chalkboard lessons", () => {
  it("keeps every teaching moment focused and bounded", () => {
    expect(FIXTURE_LESSON.moments).toHaveLength(3);
    for (const moment of FIXTURE_LESSON.moments) {
      expect(moment.title.trim()).not.toBe("");
      expect(moment.speechBubble.trim()).not.toBe("");
      expect(moment.boardSections.length).toBeLessThanOrEqual(2);
      expect(moment.boardSections.every((section) => section.lines.length <= 4)).toBe(true);
      expect(moment.demonstration.items.length).toBeLessThanOrEqual(5);
    }
  });

  it("uses deterministic demonstration primitives instead of model layout", () => {
    expect(FIXTURE_LESSON.moments.map((moment) => moment.demonstration.kind)).toEqual([
      "omitted_meaning",
      "literal_to_natural",
      "tone_scale",
    ]);
    expect(FIXTURE_LESSON.moments[0].demonstration.items[2]).toMatchObject({
      label: "[行けない]",
      accent: "missing",
    });
  });

  it("does not treat a fresh lesson with no card IDs as skipped", () => {
    expect(isLessonSkipped(undefined, undefined)).toBe(false);
    expect(isLessonSkipped("card-1", undefined)).toBe(false);
    expect(isLessonSkipped("card-1", "card-1")).toBe(true);
  });
});
