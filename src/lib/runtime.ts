import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  DEFAULT_LANGUAGES,
  DEFAULT_STYLE,
  DEFAULT_SYNC,
  EMPTY_SESSION,
  type SequencedSessionEvent,
  type SessionState,
} from "./contracts";
import { FIXTURE_EVENTS, LONG_LIVE_FIXTURE_EVENTS, ORIGINAL_ONLY_FIXTURE_EVENTS, OVERLAP_FILE_FIXTURE_EVENTS, PATHOLOGICAL_LIVE_FIXTURE_EVENTS } from "./fixtures";
import { decodePreferenceEnvelope, mergePreferencePatch, parsePreferences, serializePreferences, type PreferenceEnvelope, type PreferencePatch, type Preferences } from "./preferences";
import { reduceSession } from "./session";
import { SessionEventCoordinator } from "./sessionSync";

const PREFERENCES_KEY = "nonosub-preferences-v5";
let preferenceRevision = -1;
let canonicalPreferences: Preferences | undefined;
let preferenceInitialization: Promise<Preferences> | undefined;

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

export function initialSession(): SessionState {
  return isTauri() ? { ...structuredClone(EMPTY_SESSION), sessionId: "idle" } : fixtureSession();
}

export function loadPreferences(): Preferences {
  if (typeof localStorage === "undefined") return defaultPreferences();
  const saved = localStorage.getItem(PREFERENCES_KEY)
    ?? localStorage.getItem("nonosub-preferences-v4")
    ?? localStorage.getItem("nonosub-preferences-v3")
    ?? localStorage.getItem("nonosub-preferences-v2")
    ?? localStorage.getItem("nonosub-preferences");
  return saved ? parsePreferences(saved) ?? defaultPreferences() : defaultPreferences();
}

function storeCanonicalPreferences(preferences: Preferences): void {
  canonicalPreferences = preferences;
  if (typeof localStorage !== "undefined") localStorage.setItem(PREFERENCES_KEY, serializePreferences(preferences));
}

function acceptPreferenceEnvelope(envelope: PreferenceEnvelope): Preferences | undefined {
  const decoded = decodePreferenceEnvelope(envelope, preferenceRevision);
  if (!decoded) return undefined;
  preferenceRevision = decoded.revision;
  storeCanonicalPreferences(decoded.preferences);
  return decoded.preferences;
}

async function ensurePreferencesInitialized(): Promise<Preferences> {
  if (canonicalPreferences) return canonicalPreferences;
  if (!isTauri()) {
    const preferences = loadPreferences();
    storeCanonicalPreferences(preferences);
    return preferences;
  }
  preferenceInitialization ??= invoke<PreferenceEnvelope>("initialize_preferences", { preferences: loadPreferences() })
    .then((envelope) => acceptPreferenceEnvelope(envelope) ?? canonicalPreferences ?? loadPreferences())
    .finally(() => preferenceInitialization = undefined);
  return preferenceInitialization;
}

export async function savePreferencePatch(patch: PreferencePatch): Promise<Preferences> {
  const current = await ensurePreferencesInitialized();
  if (!isTauri()) {
    const merged = mergePreferencePatch(current, patch);
    preferenceRevision += 1;
    storeCanonicalPreferences(merged);
    return merged;
  }
  const envelope = await invoke<PreferenceEnvelope>("patch_preferences", {
    baseRevision: preferenceRevision,
    patch,
  });
  const preferences = acceptPreferenceEnvelope(envelope) ?? canonicalPreferences ?? current;
  return preferences;
}

export async function subscribePreferences(onPreferences: (preferences: Preferences) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    onPreferences(await ensurePreferencesInitialized());
    return () => undefined;
  }
  const unlisten = await listen<PreferenceEnvelope>("preferences-updated", ({ payload }) => {
    const preferences = acceptPreferenceEnvelope(payload);
    if (preferences) onPreferences(preferences);
  });
  try {
    onPreferences(await ensurePreferencesInitialized());
    return unlisten;
  } catch (error) {
    unlisten();
    throw error;
  }
}

export async function subscribeSession(
  update: (state: SessionState) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    update(fixtureSession());
    return () => undefined;
  }
  const refresh = () => invoke<SessionState>("get_session_snapshot");
  const coordinator = new SessionEventCoordinator(refresh, update);
  const unlisten = await listen<SequencedSessionEvent>("session-event", ({ payload }) => coordinator.enqueue(payload));
  try {
    await coordinator.initialize(await refresh());
    return () => {
      coordinator.stop();
      unlisten();
    };
  } catch (error) {
    coordinator.stop();
    unlisten();
    throw error;
  }
}

export function maintainSubscription(
  subscribe: () => Promise<UnlistenFn>,
  onError?: (message: string) => void,
): UnlistenFn {
  let stopped = false;
  let active: UnlistenFn | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let attempts = 0;

  const connect = async () => {
    try {
      const unlisten = await subscribe();
      if (stopped) {
        unlisten();
        return;
      }
      active = unlisten;
      attempts = 0;
      onError?.("");
    } catch (error) {
      if (stopped) return;
      const message = error instanceof Error ? error.message : String(error);
      onError?.(`NonoSub lost its app connection and is retrying. ${message}`);
      const delay = Math.min(10_000, 500 * 2 ** Math.min(attempts, 4));
      attempts += 1;
      retryTimer = setTimeout(() => void connect(), delay);
    }
  };
  void connect();

  return () => {
    stopped = true;
    if (retryTimer) clearTimeout(retryTimer);
    active?.();
  };
}

function fixtureSession(): SessionState {
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
