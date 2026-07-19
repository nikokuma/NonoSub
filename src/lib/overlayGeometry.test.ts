import { describe, expect, it } from "vitest";
import { resolveOverlayGeometry } from "./overlayGeometry";

describe("resolveOverlayGeometry", () => {
  it("keeps logical dimensions intact on a Retina display", () => {
    const geometry = resolveOverlayGeometry(
      { x: 0, y: 0, width: 3024, height: 1964, scaleFactor: 2 },
      {
        normalizedPosition: { x: 0.5, y: 0.78 },
        preferredLogicalWidth: 900,
        contentLogicalHeight: 180,
      },
    );

    expect(geometry.logicalWidth).toBe(900);
    expect(geometry.logicalHeight).toBe(220);
    expect(geometry.physicalWidth).toBe(1800);
    expect(geometry.physicalHeight).toBe(440);
  });

  it("grows around long captions and keeps a bottom overlay on-screen", () => {
    const geometry = resolveOverlayGeometry(
      { x: 100, y: 50, width: 1600, height: 900, scaleFactor: 1 },
      {
        normalizedPosition: { x: 0.5, y: 0.95 },
        preferredLogicalWidth: 900,
        contentLogicalHeight: 330,
      },
    );

    expect(geometry.logicalHeight).toBe(370);
    expect(geometry.physicalY).toBeGreaterThanOrEqual(62);
    expect(geometry.physicalY + geometry.physicalHeight).toBeLessThanOrEqual(938);
  });

  it("caps pathological captions to the safe monitor region", () => {
    const geometry = resolveOverlayGeometry(
      { x: -1920, y: 0, width: 1920, height: 1080, scaleFactor: 1 },
      {
        normalizedPosition: { x: 0.5, y: 0.5 },
        preferredLogicalWidth: 1200,
        contentLogicalHeight: 2_000,
      },
    );

    expect(geometry.logicalHeight).toBeCloseTo(885.6);
    expect(geometry.physicalX).toBeGreaterThanOrEqual(-1908);
    expect(geometry.physicalY).toBeGreaterThanOrEqual(12);
    expect(geometry.physicalY + geometry.physicalHeight).toBeLessThanOrEqual(1068);
  });

  it("honors the compact live-overlay height cap independently of content", () => {
    const geometry = resolveOverlayGeometry(
      { x: 0, y: 0, width: 3024, height: 1964, scaleFactor: 2 },
      {
        normalizedPosition: { x: 0.5, y: 0.78 },
        preferredLogicalWidth: 900,
        contentLogicalHeight: 20_000,
        maximumLogicalHeight: 220,
      },
    );

    expect(geometry.logicalHeight).toBe(220);
    expect(geometry.physicalHeight).toBe(440);
    expect(geometry.physicalY).toBeGreaterThanOrEqual(24);
    expect(geometry.physicalY + geometry.physicalHeight).toBeLessThanOrEqual(1_940);
  });

  it("reserves transparent vertical bleed for decorative subtitle frames", () => {
    const geometry = resolveOverlayGeometry(
      { x: 0, y: 0, width: 3024, height: 1964, scaleFactor: 2 },
      {
        normalizedPosition: { x: 0.5, y: 0.78 },
        preferredLogicalWidth: 900,
        contentLogicalHeight: 180,
        maximumLogicalHeight: 240,
        verticalMargin: 30,
      },
    );

    expect(geometry.logicalHeight).toBe(240);
    expect(geometry.physicalHeight).toBe(480);
  });
});
