import { describe, expect, it } from "vitest";
import infoPlist from "../../src-tauri/Info.plist?raw";

describe("macOS capture privacy packaging", () => {
  it("declares why ScreenCaptureKit needs screen and system-audio access", () => {
    expect(infoPlist).toContain("<key>NSScreenCaptureUsageDescription</key>");
    expect(infoPlist).toContain("<key>NSAudioCaptureUsageDescription</key>");
    expect(infoPlist).toContain("list the application, window, or display");
    expect(infoPlist).toContain("temporary live captions and translations");
  });
});
