import { DEFAULT_STYLE, type LearnerLevel, type SpeakerProfile, type StyleSettings, type SubtitleSegment } from "./contracts";

export interface Preferences {
  style: StyleSettings;
  level: LearnerLevel;
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
      style: {
        ...DEFAULT_STYLE,
        ...parsed.style,
        position: {
          x: clamp(parsed.style.position?.x ?? DEFAULT_STYLE.position.x, 0.08, 0.92),
          y: clamp(parsed.style.position?.y ?? DEFAULT_STYLE.position.y, 0.12, 0.92),
        },
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
