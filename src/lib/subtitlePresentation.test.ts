import { describe, expect, it } from "vitest";
import { calculateSubtitleFit, colorWithOpacity, readableAccentTextColor, subtitleFitOptionsEqual } from "./subtitlePresentation";

describe("subtitle presentation", () => {
  it("shrinks long captions until they fit the available height", () => {
    const result = calculateSubtitleFit({
      basePx: 28,
      minPx: 12,
      maxHeightPx: 180,
      measureHeight: (fontSizePx) => fontSizePx * 10,
    });

    expect(result).toEqual({ fontSizePx: 18, scale: 1 });
  });

  it("scales an extreme caption rather than clipping it at the minimum font size", () => {
    const result = calculateSubtitleFit({
      basePx: 28,
      minPx: 12,
      maxHeightPx: 180,
      measureHeight: (fontSizePx) => fontSizePx * 20,
    });

    expect(result.fontSizePx).toBe(12);
    expect(result.scale).toBe(0.75);
  });

  it("chooses readable text for light and dark speaker colors", () => {
    expect(readableAccentTextColor("#35c7e8")).toBe("#05091e");
    expect(readableAccentTextColor("#311640")).toBe("#ffffff");
    expect(readableAccentTextColor("not-a-color")).toBe("#ffffff");
  });

  it("applies bounded opacity to customizable subtitle colors", () => {
    expect(colorWithOpacity("#05081c", 0.58)).toBe("rgba(5, 8, 28, 0.58)");
    expect(colorWithOpacity("#abc", 2)).toBe("rgba(170, 187, 204, 1)");
    expect(colorWithOpacity("currentColor", 0.5)).toBe("currentColor");
  });

  it("does not refit when paint-only state leaves the layout options unchanged", () => {
    const options = { basePx: 28, minPx: 13, maxHeightPx: 158, contentKey: "caption:both:momento" };
    expect(subtitleFitOptionsEqual(options, { ...options })).toBe(true);
  });

  it.each([
    ["requested size", { basePx: 32 }],
    ["minimum size", { minPx: 12 }],
    ["available height", { maxHeightPx: 184 }],
    ["layout content", { contentKey: "new-caption:both:momento" }],
  ])("refits after a %s change", (_label, change) => {
    const options = { basePx: 28, minPx: 13, maxHeightPx: 158, contentKey: "caption:both:momento" };
    expect(subtitleFitOptionsEqual(options, { ...options, ...change })).toBe(false);
  });
});
