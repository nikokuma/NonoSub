import { describe, expect, it } from "vitest";
import { fitLogicalWindowSize, normalizeLessonPlacement, resolveLessonPosition, type MonitorGeometry } from "./floatingPlacement";

const monitor: MonitorGeometry = { key: "main", x: 100, y: 50, width: 1600, height: 900 };

describe("floating lesson placement", () => {
  it("stores a normalized center", () => {
    expect(normalizeLessonPlacement(monitor, { x: 900, y: 275, width: 400, height: 450 })).toEqual({
      monitorKey: "main",
      x: 0.625,
      y: 0.5,
    });
  });

  it("clamps restored placement inside a changed display", () => {
    expect(resolveLessonPosition(monitor, 980, 620, { monitorKey: "missing", x: 1, y: 1 })).toEqual({
      x: 702,
      y: 312,
    });
  });

  it("keeps composer and lesson dimensions in logical points on Retina displays", () => {
    expect(fitLogicalWindowSize({ width: 720, height: 210 }, { width: 3024, height: 1964 }, 2)).toEqual({ width: 720, height: 210 });
    expect(fitLogicalWindowSize({ width: 980, height: 620 }, { width: 1600, height: 900 }, 2)).toEqual({ width: 720, height: 405 });
  });
});
