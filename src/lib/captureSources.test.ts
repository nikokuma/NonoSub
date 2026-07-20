import { describe, expect, it } from "vitest";
import type { LiveCaptureSources } from "./contracts";
import { captureSelection, captureSourceMonogram, filterCaptureSources, sourcesForKind } from "./captureSources";

const sources: LiveCaptureSources = {
  applications: [{
    id: "application:12",
    kind: "application",
    title: "Safari",
    detail: "2 visible windows",
    applicationName: "Safari",
    bundleIdentifier: "com.apple.Safari",
    processId: 12,
  }],
  windows: [{
    id: "window:44",
    kind: "window",
    title: "Japanese livestream",
    detail: "Safari · 1280×720",
    applicationName: "Safari",
    processId: 12,
    windowId: 44,
  }],
  displays: [{
    id: "display:1",
    kind: "display",
    title: "Display 1",
    detail: "3024×1964",
    displayId: 1,
  }],
};

describe("capture source chooser", () => {
  it("keeps source kinds separate", () => {
    expect(sourcesForKind(sources, "application")).toEqual(sources.applications);
    expect(sourcesForKind(sources, "window")).toEqual(sources.windows);
    expect(sourcesForKind(sources, "display")).toEqual(sources.displays);
  });

  it("searches titles, apps, details, and bundle identifiers", () => {
    expect(filterCaptureSources(sources, "application", "APPLE")).toEqual(sources.applications);
    expect(filterCaptureSources(sources, "window", "livestream")).toEqual(sources.windows);
    expect(filterCaptureSources(sources, "display", "3024")).toEqual(sources.displays);
    expect(filterCaptureSources(sources, "window", "Firefox")).toEqual([]);
  });

  it("produces compact stable monograms without needing protected thumbnails", () => {
    expect(captureSourceMonogram(sources.applications[0])).toBe("S");
    expect(captureSourceMonogram(sources.windows[0])).toBe("S");
    expect(captureSourceMonogram(sources.displays[0])).toBe("▣");
  });

  it("removes display labels before invoking privileged capture", () => {
    expect(captureSelection(sources.windows[0])).toEqual({
      kind: "window",
      processId: 12,
      windowId: 44,
      displayId: undefined,
    });
  });
});
