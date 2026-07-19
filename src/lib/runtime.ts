import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  DEFAULT_LANGUAGES,
  DEFAULT_STYLE,
  DEFAULT_SYNC,
  EMPTY_SESSION,
  type LanguageSettings,
  type LearnerLevel,
  type SequencedSessionEvent,
  type SessionState,
  type StyleSettings,
} from "./contracts";
import { FIXTURE_EVENTS, LONG_LIVE_FIXTURE_EVENTS, ORIGINAL_ONLY_FIXTURE_EVENTS, OVERLAP_FILE_FIXTURE_EVENTS, PATHOLOGICAL_LIVE_FIXTURE_EVENTS } from "./fixtures";
import { parsePreferences, serializePreferences, type Preferences } from "./preferences";
import { applySequencedEvent, reduceSession } from "./session";

const PREFERENCES_KEY = "nonosub-preferences-v4";

export function defaultPreferences(): Preferences {
  return {
    style: structuredClone(DEFAULT_STYLE),
    level: "beginner",
    languages: { ...DEFAULT_LANGUAGES },
    sync: { ...DEFAULT_SYNC },
    processingMode: "translated",
    onboardingComplete: false,
    lessonPlacements: {},
    experimentalExternalPause: false,
  };
}

export function loadPreferences(): Preferences {
  if (typeof localStorage === "undefined") return defaultPreferences();
  const saved = localStorage.getItem(PREFERENCES_KEY)
    ?? localStorage.getItem("nonosub-preferences-v3")
    ?? localStorage.getItem("nonosub-preferences-v2")
    ?? localStorage.getItem("nonosub-preferences");
  return saved ? parsePreferences(saved) ?? defaultPreferences() : defaultPreferences();
}

export async function savePreferences(preferences: Preferences): Promise<void> {
  localStorage.setItem(PREFERENCES_KEY, serializePreferences(preferences));
  if (isTauri()) {
    await emit("preferences-updated", preferences);
    await invoke("update_languages", { languages: preferences.languages });
  }
}

export async function subscribePreferences(onPreferences: (preferences: Preferences) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<Preferences>("preferences-updated", ({ payload }) => onPreferences(payload));
}

export async function initialSession(): Promise<SessionState> {
  if (!isTauri()) {
    const fixtureName = typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("fixture") : undefined;
    const fixture = fixtureName === "live-long"
      ? LONG_LIVE_FIXTURE_EVENTS
      : fixtureName === "live-pathological"
        ? PATHOLOGICAL_LIVE_FIXTURE_EVENTS
        : fixtureName === "overlap-long"
          ? OVERLAP_FILE_FIXTURE_EVENTS
          : fixtureName === "original-only"
            ? ORIGINAL_ONLY_FIXTURE_EVENTS
            : FIXTURE_EVENTS;
    return fixture.reduce(reduceSession, structuredClone(EMPTY_SESSION));
  }
  return invoke<SessionState>("get_session_snapshot");
}

export async function subscribeSession(
  current: () => SessionState,
  update: (state: SessionState) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<SequencedSessionEvent>("session-event", async ({ payload }) => {
    const next = applySequencedEvent(current(), payload);
    update(next ?? await invoke<SessionState>("get_session_snapshot"));
  });
}

export function updatePreferenceStyle(preferences: Preferences, style: StyleSettings): Preferences {
  return { ...preferences, style };
}

export function updatePreferenceLevel(preferences: Preferences, level: LearnerLevel): Preferences {
  return { ...preferences, level };
}

export function updatePreferenceLanguages(preferences: Preferences, languages: LanguageSettings): Preferences {
  return { ...preferences, languages };
}
