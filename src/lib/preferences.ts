import { DEFAULT_LANGUAGES, DEFAULT_STYLE, DEFAULT_SYNC, type LanguageSettings, type LearnerLevel, type SpeakerProfile, type StyleSettings, type SubtitleSegment, type SyncSettings } from "./contracts";

export interface Preferences {
  style: StyleSettings;
  level: LearnerLevel;
  languages: LanguageSettings;
  sync: SyncSettings;
  onboardingComplete: boolean;
}

export function serializePreferences(preferences: Preferences): string {
  return JSON.stringify(preferences);
}

export function parsePreferences(serialized: string): Preferences | undefined {
  try {
    const parsed = JSON.parse(serialized) as Partial<Preferences>;
    if (!parsed.style || !["beginner", "intermediate", "advanced"].includes(parsed.level ?? "")) return undefined;
    return {
      level: parsed.level as LearnerLevel,
      languages: {
        source: parsed.languages?.source ?? DEFAULT_LANGUAGES.source,
        target: parsed.languages?.target ?? DEFAULT_LANGUAGES.target,
        explanation: parsed.languages?.explanation ?? parsed.languages?.target ?? DEFAULT_LANGUAGES.explanation,
      },
      onboardingComplete: parsed.onboardingComplete ?? false,
      sync: {
        liveMode: parsed.sync?.liveMode === "fast_source" ? "fast_source" : DEFAULT_SYNC.liveMode,
      },
      style: {
        ...DEFAULT_STYLE,
        ...parsed.style,
        position: {
          x: clamp(parsed.style.position?.x ?? DEFAULT_STYLE.position.x, 0.08, 0.92),
          y: clamp(parsed.style.position?.y ?? DEFAULT_STYLE.position.y, 0.12, 0.92),
        },
        overlayPosition: {
          x: clamp(parsed.style.overlayPosition?.x ?? DEFAULT_STYLE.overlayPosition.x, 0.05, 0.95),
          y: clamp(parsed.style.overlayPosition?.y ?? DEFAULT_STYLE.overlayPosition.y, 0.05, 0.95),
        },
        overlayWidth: clamp(parsed.style.overlayWidth ?? DEFAULT_STYLE.overlayWidth, 520, 1200),
      },
    };
  } catch {
    return undefined;
  }
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

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
