import { describe, expect, it } from "vitest";
import { FIXTURE_LESSON } from "./fixtures";
import { dominantChalkColor, isLessonSkipped, lessonStepOrder } from "./lesson";
import type { ChalkPhrase, TeachingMoment } from "./contracts";

function phrases(moment: TeachingMoment): ChalkPhrase[] {
  return [
    ...moment.boardSections.flatMap((section) => section.lines),
    ...(moment.demonstration.result ? [moment.demonstration.result] : []),
    ...(moment.ambiguityNote ? [moment.ambiguityNote] : []),
  ];
}

describe("progressive chalkboard lessons", () => {
  it("keeps every teaching moment focused and bounded", () => {
    expect(FIXTURE_LESSON.moments).toHaveLength(3);
    const fixtureCues: Array<"point" | "underline"> = [];
    for (const moment of FIXTURE_LESSON.moments) {
      expect(moment.title.trim()).not.toBe("");
      expect(moment.speechBubble.trim()).not.toBe("");
      expect(moment.boardSections.length).toBeLessThanOrEqual(2);
      expect(moment.boardSections.every((section) => section.lines.length <= 3)).toBe(true);
      expect(moment.demonstration.items.length).toBeLessThanOrEqual(4);
      if (moment.demonstration.kind !== "none") expect(moment.boardSections.length).toBeLessThanOrEqual(1);
      const allCues = [
        moment.sourceFocus.tailCue,
        ...phrases(moment).map((phrase) => phrase.tailCue),
        ...moment.demonstration.items.map((item) => item.tailCue),
      ];
      const activeCues = allCues.filter((cue) => cue !== "none");
      expect(activeCues).toHaveLength(1);
      fixtureCues.push(activeCues[0]);
    }
    expect(fixtureCues).toContain("point");
    expect(fixtureCues).toContain("underline");
  });

  it("uses deterministic demonstration primitives instead of model layout", () => {
    expect(FIXTURE_LESSON.moments.map((moment) => moment.demonstration.kind)).toEqual([
      "omitted_meaning",
      "literal_to_natural",
      "tone_scale",
    ]);
    expect(FIXTURE_LESSON.moments[0].demonstration.items[2]).toMatchObject({
      label: "[行けない]",
      color: "pink",
      mark: "bracket",
    });
  });

  it("uses the fixed teaching palette and reserves strikes for pink corrections", () => {
    const colors = new Set(["white", "baby_blue", "yellow", "pink"]);
    for (const moment of FIXTURE_LESSON.moments) {
      for (const phrase of phrases(moment)) {
        expect(colors.has(phrase.color)).toBe(true);
        if (phrase.mark === "strike") expect(phrase.color).toBe("pink");
      }
      for (const item of moment.demonstration.items) {
        expect(colors.has(item.color)).toBe(true);
        if (item.mark === "strike") expect(item.color).toBe("pink");
      }
    }
  });

  it("does not treat a fresh lesson with no card IDs as skipped", () => {
    expect(isLessonSkipped(undefined, undefined)).toBe(false);
    expect(isLessonSkipped("card-1", undefined)).toBe(false);
    expect(isLessonSkipped("card-1", "card-1")).toBe(true);
  });

  it("assigns a stable visual reading order across each board region", () => {
    expect(lessonStepOrder(FIXTURE_LESSON.moments[0])).toEqual({
      source: 1,
      sections: [2],
      demonstration: 3,
      ambiguity: 4,
    });
    expect(lessonStepOrder(FIXTURE_LESSON.moments[1])).toEqual({
      source: 1,
      sections: [],
      demonstration: 2,
    });
  });

  it("uses the most common lesson color for a step's subtle echo ring", () => {
    expect(dominantChalkColor(["baby_blue", "yellow", "baby_blue", "pink"])).toBe("baby_blue");
    expect(dominantChalkColor(["pink", "yellow"])).toBe("pink");
    expect(dominantChalkColor([])).toBe("white");
  });
});
