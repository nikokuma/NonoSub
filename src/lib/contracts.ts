export type LearnerLevel = "beginner" | "intermediate" | "advanced";
export type SubtitleDisplayMode = "source" | "translation" | "both";
export type SegmentStatus = "pending" | "complete" | "failed";
export type SessionPhase =
  | "idle"
  | "preparing"
  | "transcribing"
  | "buffering"
  | "ready"
  | "playing"
  | "paused"
  | "complete";

export type SubtitlePreset = "clean" | "cinema" | "contrast" | "nono-pop" | "manga" | "retro";

export interface SubtitleSegment {
  id: string;
  startMs: number;
  endMs: number;
  sourceText: string;
  naturalEnglish?: string;
  ambiguityNote?: string;
  speakerId: string;
  transcriptionStatus: SegmentStatus;
  translationStatus: SegmentStatus;
}

export interface SpeakerProfile {
  id: string;
  displayName: string;
  color: string;
  reference?: { startMs: number; endMs: number };
}

export interface StyleSettings {
  preset: SubtitlePreset;
  position: { x: number; y: number };
  fontFamily: string;
  fontSize: number;
  backgroundOpacity: number;
  effect: "none" | "outline" | "shadow";
  displayMode: SubtitleDisplayMode;
  showSpeakerNames: boolean;
}

export interface RecoverableError {
  code: string;
  message: string;
  segmentId?: string;
}

export type SessionEvent =
  | { type: "phase_changed"; phase: SessionPhase }
  | { type: "transcript_finalized"; segment: SubtitleSegment }
  | { type: "translation_finalized"; segmentId: string; naturalEnglish: string; ambiguityNote?: string }
  | { type: "speaker_discovered"; speaker: SpeakerProfile }
  | { type: "coverage_changed"; translatedThroughMs: number }
  | { type: "recoverable_error"; error: RecoverableError }
  | { type: "fatal_error"; message: string }
  | { type: "complete" };

export interface SessionState {
  phase: SessionPhase;
  segments: SubtitleSegment[];
  speakers: Record<string, SpeakerProfile>;
  translatedThroughMs: number;
  errors: RecoverableError[];
  fatalError?: string;
}

export interface TutorMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
}

export const DEFAULT_STYLE: StyleSettings = {
  preset: "clean",
  position: { x: 0.5, y: 0.82 },
  fontFamily: "Inter",
  fontSize: 28,
  backgroundOpacity: 0.58,
  effect: "outline",
  displayMode: "both",
  showSpeakerNames: true,
};

export const EMPTY_SESSION: SessionState = {
  phase: "idle",
  segments: [],
  speakers: {},
  translatedThroughMs: 0,
  errors: [],
};
