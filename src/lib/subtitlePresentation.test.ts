import { describe, expect, it } from "vitest";
import { calculateLiveCaptionFontSize, calculateSubtitleFit, colorWithOpacity, liveOverlaySegment, readableAccentTextColor, subtitleFitOptionsEqual } from "./subtitlePresentation";
import type { SubtitleSegment } from "./contracts";

describe("subtitle presentation", () => {
  it("keeps partial translation in the transcript but hides it from coordinated overlays", () => {
    const segment: SubtitleSegment = {
      id: "live-1",
      origin: "live",
      startMs: 0,
      endMs: 1_000,
      sourceText: "今日はちょっと。",
      translationText: "Today is a little",
      speakerId: "live-audio",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "pending",
    };
    expect(liveOverlaySegment(segment, "coordinated").translationText).toBeUndefined();
    expect(liveOverlaySegment(segment, "fast_source")).toBe(segment);
    expect(segment.translationText).toBe("Today is a little");
  });

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

  it("uses stable density steps for growing live captions", () => {
    expect(calculateLiveCaptionFontSize({
      basePx: 28,
      viewportWidth: 900,
      sourceText: "短い字幕です。",
      translationText: "This is short.",
      showSource: true,
      showTranslation: true,
    })).toBe(28);

    expect(calculateLiveCaptionFontSize({
      basePx: 28,
      viewportWidth: 900,
      sourceText: "配信についてもう少し長く話しているので、字幕が複数行になっても安定して表示される必要があります。".repeat(2),
      translationText: "This is a long explanation that continues while the live caption is growing and should remain visually stable instead of being measured and scaled on every incoming fragment.".repeat(2),
      showSource: true,
      showTranslation: true,
    })).toBe(17);
  });

  it("does not count hidden translation text toward live density", () => {
    const request = {
      basePx: 28,
      viewportWidth: 900,
      sourceText: "今日はちょっと……。",
      translationText: "A very long translation that should not affect source-only mode. ".repeat(8),
      showSource: true,
    };
    expect(calculateLiveCaptionFontSize({ ...request, showTranslation: false })).toBe(28);
    expect(calculateLiveCaptionFontSize({ ...request, showTranslation: true })).toBeLessThan(28);
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
