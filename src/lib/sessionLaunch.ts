import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Preferences } from "./preferences";

export interface FileLaunchResult {
  path: string;
  fileName: string;
  durationMs: number;
  chunkCount: number;
}

export interface LaunchHooks {
  status?: (message: string) => void;
  analysisError?: (message: string) => void;
}

export function validateVideoPath(path: string): string | undefined {
  const normalized = path.trim().toLowerCase();
  if (!normalized) return "Choose a video file.";
  if (!normalized.endsWith(".mp4") && !normalized.endsWith(".mov")) {
    return "NonoSub currently supports MP4 and MOV video files.";
  }
  return undefined;
}

export async function cancelAndReplaceSession(): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_session");
  await Promise.all([
    invoke("hide_surface", { surface: "viewer" }),
    invoke("hide_surface", { surface: "overlay" }),
    invoke("hide_surface", { surface: "lesson" }),
  ]);
}

export async function startFileSession(
  path: string,
  preferences: Preferences,
  hooks: LaunchHooks = {},
): Promise<FileLaunchResult> {
  const validationError = validateVideoPath(path);
  if (validationError) throw new Error(validationError);
  if (!isTauri()) throw new Error("Local video playback requires the NonoSub desktop app.");

  await cancelAndReplaceSession();
  hooks.status?.("Opening video and preparing compatible playback…");
  const prepared = await invoke<{ path: string; file_name: string; generation: number }>("prepare_media", { path });
  hooks.status?.(`Decoding ${prepared.file_name} locally…`);
  const audio = await invoke<{ durationMs: number; chunkCount: number }>("prepare_audio", {
    generation: prepared.generation,
  });
  hooks.status?.(`${audio.chunkCount} audio chunk${audio.chunkCount === 1 ? "" : "s"} ready · analyzing`);
  await invoke("open_surface", { surface: "viewer" });
  void invoke("start_analysis", {
    generation: prepared.generation,
    languages: preferences.languages,
    processingMode: preferences.processingMode,
  }).catch((error) => hooks.analysisError?.(errorMessage(error)));
  return {
    path: prepared.path,
    fileName: prepared.file_name,
    durationMs: audio.durationMs,
    chunkCount: audio.chunkCount,
  };
}

export async function startLiveSession(
  preferences: Preferences,
  hooks: LaunchHooks = {},
): Promise<void> {
  if (!isTauri()) throw new Error("Live system audio requires the NonoSub macOS app.");
  await cancelAndReplaceSession();
  hooks.status?.("Choose a browser, window, or display in Apple’s sharing picker…");
  await invoke("open_surface", { surface: "overlay" });
  await invoke("start_live_capture", {
    languages: preferences.languages,
    syncMode: preferences.sync.liveMode,
    processingMode: preferences.processingMode,
  });
  hooks.status?.("Listening · live audio is sent to OpenAI and never saved.");
}

export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) return String(error.message);
  return String(error);
}
