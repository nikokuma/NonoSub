import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const infoPlist = readFileSync(new URL("../../src-tauri/Info.plist", import.meta.url), "utf8");

describe("macOS capture privacy packaging", () => {
  it("declares why ScreenCaptureKit needs screen and system-audio access", () => {
    expect(infoPlist).toContain("<key>NSScreenCaptureUsageDescription</key>");
    expect(infoPlist).toContain("<key>NSAudioCaptureUsageDescription</key>");
    expect(infoPlist).toContain("list the application, window, or display");
    expect(infoPlist).toContain("temporary live captions and translations");
  });
});
