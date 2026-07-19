import { DEFAULT_LANGUAGES, DEFAULT_STYLE, DEFAULT_SYNC, type CaptionProcessingMode, type LanguageSettings, type LearnerLevel, type LessonPlacement, type SpeakerProfile, type StyleSettings, type SubtitleSegment, type SyncSettings } from "./contracts";

export interface Preferences {
  style: StyleSettings;
  level: LearnerLevel;
  languages: LanguageSettings;
  sync: SyncSettings;
  processingMode: CaptionProcessingMode;
  onboardingComplete: boolean;
  lessonPlacements: Record<string, LessonPlacement>;
  experimentalExternalPause: boolean;
}

type DeepPartial<T> = T extends object
  ? { [Key in keyof T]?: DeepPartial<T[Key]> }
  : T;

export type PreferencePatch = DeepPartial<Preferences>;

export interface PreferenceEnvelope {
  revision: number;
  preferences: unknown;
  rebased: boolean;
}

export function decodePreferenceEnvelope(
  envelope: PreferenceEnvelope,
  currentRevision: number,
): { revision: number; preferences: Preferences } | undefined {
  if (!Number.isSafeInteger(envelope.revision) || envelope.revision < 0 || envelope.revision <= currentRevision) return undefined;
  const preferences = parsePreferences(JSON.stringify(envelope.preferences));
  return preferences ? { revision: envelope.revision, preferences } : undefined;
}

export function mergePreferencePatch(preferences: Preferences, patch: PreferencePatch): Preferences {
  const merged = structuredClone(preferences) as unknown as Record<string, unknown>;
  mergeObjectPatch(merged, patch as Record<string, unknown>);
  return parsePreferences(JSON.stringify(merged)) ?? preferences;
}

export function preferencePatchBetween(before: Preferences, after: Preferences): PreferencePatch {
  return diffObjects(
    before as unknown as Record<string, unknown>,
    after as unknown as Record<string, unknown>,
  ) as PreferencePatch;
}

function diffObjects(before: Record<string, unknown>, after: Record<string, unknown>): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(after)) {
    const previous = before[key];
    if (isPlainObject(previous) && isPlainObject(value)) {
      const nested = diffObjects(previous, value);
      if (Object.keys(nested).length > 0) patch[key] = nested;
    } else if (!Object.is(previous, value)) {
      patch[key] = structuredClone(value);
    }
  }
  return patch;
}

function mergeObjectPatch(target: Record<string, unknown>, patch: Record<string, unknown>): void {
  for (const [key, value] of Object.entries(patch)) {
    if (isPlainObject(value) && isPlainObject(target[key])) {
      mergeObjectPatch(target[key], value);
    } else {
      target[key] = structuredClone(value);
    }
  }
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export function serializePreferences(preferences: Preferences): string {
  return JSON.stringify(preferences);
}

export function parsePreferences(serialized: string): Preferences | undefined {
  try {
    const parsed = JSON.parse(serialized) as Partial<Preferences>;
    if (!parsed.style || !["beginner", "intermediate", "advanced"].includes(parsed.level ?? "")) return undefined;
    const savedPreset = (parsed.style as { preset?: string }).preset;
    const preset = savedPreset === "nono-pop"
      ? "momento"
      : savedPreset === "cyberia"
        ? "wired"
        : savedPreset === "arcade"
          ? "fallout"
          : ["clean", "classic-outline", "yellow-drop", "fallout", "momento", "wired"].includes(savedPreset ?? "")
            ? savedPreset
            : DEFAULT_STYLE.preset;
    const legacyStyle = parsed.style as Partial<StyleSettings> & {
      cyberiaColors?: StyleSettings["wiredColors"];
      arcadeColors?: StyleSettings["falloutColors"];
    };
    return {
      level: parsed.level as LearnerLevel,
      languages: {
        source: parsed.languages?.source ?? DEFAULT_LANGUAGES.source,
        target: parsed.languages?.target ?? DEFAULT_LANGUAGES.target,
        explanation: parsed.languages?.explanation ?? parsed.languages?.target ?? DEFAULT_LANGUAGES.explanation,
      },
      onboardingComplete: parsed.onboardingComplete ?? false,
      lessonPlacements: parseLessonPlacements(parsed.lessonPlacements),
      experimentalExternalPause: parsed.experimentalExternalPause === true,
      processingMode: parsed.processingMode === "original_only" ? "original_only" : "translated",
      sync: {
        liveMode: parsed.sync?.liveMode === "fast_source" ? "fast_source" : DEFAULT_SYNC.liveMode,
      },
      style: {
        ...DEFAULT_STYLE,
        ...parsed.style,
        preset: preset as StyleSettings["preset"],
        position: {
          x: clamp(parsed.style.position?.x ?? DEFAULT_STYLE.position.x, 0.08, 0.92),
          y: clamp(parsed.style.position?.y ?? DEFAULT_STYLE.position.y, 0.12, 0.92),
        },
        overlayPosition: {
          x: clamp(parsed.style.overlayPosition?.x ?? DEFAULT_STYLE.overlayPosition.x, 0.05, 0.95),
          y: clamp(parsed.style.overlayPosition?.y ?? DEFAULT_STYLE.overlayPosition.y, 0.05, 0.95),
        },
        overlayWidth: clamp(parsed.style.overlayWidth ?? DEFAULT_STYLE.overlayWidth, 520, 1200),
        wiredColors: {
          ...DEFAULT_STYLE.wiredColors,
          ...(legacyStyle.cyberiaColors ?? {}),
          ...legacyStyle.wiredColors,
        },
        falloutColors: {
          ...DEFAULT_STYLE.falloutColors,
          ...(legacyStyle.arcadeColors ?? {}),
          ...legacyStyle.falloutColors,
        },
      },
    };
  } catch {
    return undefined;
  }
}

function parseLessonPlacements(value: unknown): Record<string, LessonPlacement> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const placements: Record<string, LessonPlacement> = {};
  for (const [key, candidate] of Object.entries(value)) {
    if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) continue;
    const placement = candidate as Partial<LessonPlacement>;
    if (!Number.isFinite(placement.x) || !Number.isFinite(placement.y)) continue;
    placements[key] = {
      monitorKey: typeof placement.monitorKey === "string" && placement.monitorKey ? placement.monitorKey : key,
      x: clamp(placement.x as number, 0, 1),
      y: clamp(placement.y as number, 0, 1),
    };
  }
  return placements;
}

export function renameSpeaker(
  speakers: Record<string, SpeakerProfile>,
  id: string,
  displayName: string,
): Record<string, SpeakerProfile> {
  const speaker = speakers[id];
  const normalized = displayName.trim();
  if (!speaker || !normalized) return speakers;
  return { ...speakers, [id]: { ...speaker, displayName: normalized } };
}

export function buildTutorContext(
  segments: SubtitleSegment[],
  selectedId: string,
  precedingLimit = 80,
  followingLimit = 5,
): SubtitleSegment[] {
  const selectedIndex = segments.findIndex((segment) => segment.id === selectedId);
  if (selectedIndex < 0) return [];
  return segments.slice(Math.max(0, selectedIndex - precedingLimit), selectedIndex + followingLimit + 1);
}

export function effectiveStyle(style: StyleSettings, processingMode: CaptionProcessingMode): StyleSettings {
  return processingMode === "original_only" ? { ...style, displayMode: "source" } : style;
}

export function applyPreferenceAction(preferences: Preferences, action: string): Preferences | undefined {
  if (action.startsWith("preset_")) {
    const preset = action.slice(7);
    if (!["clean", "classic-outline", "yellow-drop", "fallout", "momento", "wired"].includes(preset)) return undefined;
    return { ...preferences, style: { ...preferences.style, preset: preset as StyleSettings["preset"] } };
  }
  if (action.startsWith("level_")) {
    const level = action.slice(6);
    if (!["beginner", "intermediate", "advanced"].includes(level)) return undefined;
    return { ...preferences, level: level as LearnerLevel };
  }
  if (action.startsWith("display_")) {
    const displayMode = action.slice(8);
    if (!["source", "translation", "both"].includes(displayMode)) return undefined;
    return { ...preferences, style: { ...preferences.style, displayMode: displayMode as StyleSettings["displayMode"] } };
  }
  if (action === "live_mode_coordinated" || action === "live_mode_fast_source") {
    return {
      ...preferences,
      sync: { liveMode: action === "live_mode_fast_source" ? "fast_source" : "coordinated" },
    };
  }
  if (action === "toggle_speaker_names") {
    return {
      ...preferences,
      style: { ...preferences.style, showSpeakerNames: !preferences.style.showSpeakerNames },
    };
  }
  if (action === "external_pause_on" || action === "external_pause_off" || action === "toggle_external_pause") {
    return {
      ...preferences,
      experimentalExternalPause: action === "toggle_external_pause"
        ? !preferences.experimentalExternalPause
        : action === "external_pause_on",
    };
  }
  return undefined;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
