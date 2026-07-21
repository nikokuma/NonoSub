import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_LANGUAGES, DEFAULT_STYLE, DEFAULT_SYNC } from "./contracts";

const mocks = vi.hoisted(() => ({
  emit: vi.fn(),
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: mocks.emit }));

import { startFileSession, startLiveSession, validateVideoPath } from "./sessionLaunch";

describe("session launcher", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it.each(["movie.mp4", "/Users/nico/clip.MOV"])("accepts %s", (path) => {
    expect(validateVideoPath(path)).toBeUndefined();
  });

  it.each(["", "movie.mkv", "video.mp4.txt"])("rejects %s", (path) => {
    expect(validateVideoPath(path)).toBeTruthy();
  });

  it("hands the prepared generation to audio preparation and analysis", async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "prepare_media") {
        return Promise.resolve({
          path: "/tmp/movie.mp4",
          file_name: "movie.mp4",
          generation: 42,
        });
      }
      if (command === "prepare_audio") {
        return Promise.resolve({ durationMs: 60_000, chunkCount: 2 });
      }
      return Promise.resolve();
    });
    const preferences = {
      style: structuredClone(DEFAULT_STYLE),
      level: "beginner" as const,
      languages: { ...DEFAULT_LANGUAGES },
      sync: { ...DEFAULT_SYNC },
      processingMode: "translated" as const,
      onboardingComplete: true,
      lessonPlacements: {},
      experimentalExternalPause: false,
    };

    await startFileSession("/tmp/movie.mp4", preferences);

    expect(mocks.invoke).toHaveBeenCalledWith("prepare_audio", { generation: 42 });
    expect(mocks.invoke).toHaveBeenCalledWith("start_analysis", {
      generation: 42,
      languages: preferences.languages,
      processingMode: "translated",
    });
  });

  it("passes only the selected native source identifiers into live capture", async () => {
    mocks.invoke.mockResolvedValue(undefined);
    const preferences = {
      style: structuredClone(DEFAULT_STYLE),
      level: "beginner" as const,
      languages: { ...DEFAULT_LANGUAGES },
      sync: { ...DEFAULT_SYNC },
      processingMode: "translated" as const,
      onboardingComplete: true,
      lessonPlacements: {},
      experimentalExternalPause: false,
    };
    const source = { kind: "application" as const, processId: 42 };

    await startLiveSession(preferences, source);

    expect(mocks.invoke).toHaveBeenCalledWith("start_live_capture", {
      languages: preferences.languages,
      syncMode: preferences.sync.liveMode,
      translationEngine: preferences.sync.translationEngine,
      processingMode: preferences.processingMode,
      source,
    });
  });

  it("hides the overlay when the selected live source cannot start", async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "start_live_capture") return Promise.reject(new Error("source vanished"));
      return Promise.resolve();
    });
    const preferences = {
      style: structuredClone(DEFAULT_STYLE),
      level: "beginner" as const,
      languages: { ...DEFAULT_LANGUAGES },
      sync: { ...DEFAULT_SYNC },
      processingMode: "translated" as const,
      onboardingComplete: true,
      lessonPlacements: {},
      experimentalExternalPause: false,
    };

    await expect(startLiveSession(preferences, { kind: "window", windowId: 99 })).rejects.toThrow("source vanished");
    expect(mocks.invoke).toHaveBeenCalledWith("hide_surface", { surface: "overlay" });
  });
});
