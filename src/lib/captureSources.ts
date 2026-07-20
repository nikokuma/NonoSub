import type { LiveCaptureSource, LiveCaptureSourceKind, LiveCaptureSourceSelection, LiveCaptureSources } from "./contracts";

export const EMPTY_CAPTURE_SOURCES: LiveCaptureSources = {
  applications: [],
  windows: [],
  displays: [],
};

export function sourcesForKind(
  sources: LiveCaptureSources,
  kind: LiveCaptureSourceKind,
): LiveCaptureSource[] {
  if (kind === "application") return sources.applications;
  if (kind === "window") return sources.windows;
  return sources.displays;
}

export function filterCaptureSources(
  sources: LiveCaptureSources,
  kind: LiveCaptureSourceKind,
  query: string,
): LiveCaptureSource[] {
  const needle = query.trim().toLocaleLowerCase();
  const candidates = sourcesForKind(sources, kind);
  if (!needle) return candidates;
  return candidates.filter((source) => [
    source.title,
    source.detail,
    source.applicationName,
    source.bundleIdentifier,
  ].some((value) => value?.toLocaleLowerCase().includes(needle)));
}

export function captureSourceMonogram(source: LiveCaptureSource): string {
  if (source.kind === "display") return "▣";
  const words = (source.applicationName ?? source.title)
    .trim()
    .split(/\s+/u)
    .filter(Boolean);
  return words.slice(0, 2).map((word) => word[0]?.toLocaleUpperCase() ?? "").join("") || "•";
}

export function captureSelection(source: LiveCaptureSource): LiveCaptureSourceSelection {
  return {
    kind: source.kind,
    processId: source.processId,
    windowId: source.windowId,
    displayId: source.displayId,
  };
}
