export type LearnerLevel = "beginner" | "intermediate" | "advanced";
export type SubtitleDisplayMode = "source" | "translation" | "both";
export type SegmentStatus = "pending" | "complete" | "failed";
export type SessionMode = "file" | "live";
export type LiveSyncMode = "coordinated" | "fast_source";
export type LiveSyncStatus = "steady" | "catching_up" | "degraded";
export type SessionPhase =
  | "idle"
  | "preparing"
  | "transcribing"
  | "buffering"
  | "ready"
  | "playing"
  | "paused"
  | "reconnecting"
  | "complete";

export type SubtitlePreset = "clean" | "cinema" | "contrast" | "nono-pop" | "manga" | "retro";
export type AppSurface = "workbench" | "viewer" | "overlay" | "lesson";

export interface LanguageSettings {
  source: "auto" | string;
  target: string;
  explanation: string;
}

export interface SyncSettings {
  liveMode: LiveSyncMode;
}

export interface LiveSyncState {
  targetDelayMs: number;
  observedLagMs: number;
  status: LiveSyncStatus;
  visibleSegmentId?: string;
}

export interface SubtitleSegment {
  id: string;
  origin: SessionMode;
  startMs: number;
  endMs: number;
  sourceText: string;
  translationText?: string;
  ambiguityNote?: string;
  speakerId?: string;
  isProvisional: boolean;
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
  overlayPosition: { x: number; y: number };
  overlayWidth: number;
  fontFamily: string;
  fontSize: number;
  backgroundOpacity: number;
  effect: "none" | "outline" | "shadow";
  displayMode: SubtitleDisplayMode;
  showSpeakerNames: boolean;
}

export interface BoardSection {
  heading: string;
  lines: string[];
}

export type BoardDemoKind = "none" | "sentence_breakdown" | "omitted_meaning" | "literal_to_natural" | "tone_scale" | "mini_dialogue";
export type BoardDemoAccent = "source" | "meaning" | "missing" | "tone";

export interface BoardDemoItem {
  label: string;
  detail: string;
  accent: BoardDemoAccent;
}

export interface BoardDemo {
  kind: BoardDemoKind;
  caption?: string;
  items: BoardDemoItem[];
  result?: string;
}

export interface TeachingMoment {
  title: string;
  speechBubble: string;
  boardSections: BoardSection[];
  demonstration: BoardDemo;
  ambiguityNote?: string;
}

export interface LessonCard {
  selectedSegmentId: string;
  moments: TeachingMoment[];
  suggestedFollowUps: string[];
}

export interface LessonMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  card?: LessonCard;
}

export interface RecoverableError {
  code: string;
  message: string;
  segmentId?: string;
}

export type SessionEvent =
  | { type: "session_reset"; mode: SessionMode; languages: LanguageSettings }
  | { type: "phase_changed"; phase: SessionPhase }
  | { type: "caption_upserted"; segment: SubtitleSegment }
  | { type: "transcript_finalized"; segment: SubtitleSegment }
  | { type: "translation_finalized"; segmentId: string; translationText: string; ambiguityNote?: string }
  | { type: "speaker_discovered"; speaker: SpeakerProfile }
  | { type: "coverage_changed"; translatedThroughMs: number }
  | { type: "live_sync_changed"; sync: LiveSyncState }
  | { type: "lesson_selected"; segmentId?: string }
  | { type: "recoverable_error"; error: RecoverableError }
  | { type: "fatal_error"; message: string }
  | { type: "complete" };

export interface SequencedSessionEvent {
  sessionId: string;
  sequence: number;
  event: SessionEvent;
}

export interface PreparedMediaInfo {
  path: string;
  fileName: string;
}

export interface SessionState {
  sessionId: string;
  sequence: number;
  mode?: SessionMode;
  languages: LanguageSettings;
  phase: SessionPhase;
  segments: SubtitleSegment[];
  speakers: Record<string, SpeakerProfile>;
  translatedThroughMs: number;
  liveSync?: LiveSyncState;
  errors: RecoverableError[];
  fatalError?: string;
  selectedSegmentId?: string;
  media?: PreparedMediaInfo;
}

export interface ModelReadiness {
  file: boolean;
  live: boolean;
}

export const DEFAULT_LANGUAGES: LanguageSettings = {
  source: "auto",
  target: "en",
  explanation: "en",
};

export const DEFAULT_SYNC: SyncSettings = {
  liveMode: "coordinated",
};

export const DEFAULT_LIVE_SYNC: LiveSyncState = {
  targetDelayMs: 2_500,
  observedLagMs: 0,
  status: "steady",
};

export const DEFAULT_STYLE: StyleSettings = {
  preset: "clean",
  position: { x: 0.5, y: 0.82 },
  overlayPosition: { x: 0.5, y: 0.78 },
  overlayWidth: 900,
  fontFamily: "Inter",
  fontSize: 28,
  backgroundOpacity: 0.58,
  effect: "outline",
  displayMode: "both",
  showSpeakerNames: true,
};

export const EMPTY_SESSION: SessionState = {
  sessionId: "fixture",
  sequence: 0,
  languages: { ...DEFAULT_LANGUAGES },
  phase: "idle",
  segments: [],
  speakers: {},
  translatedThroughMs: 0,
  errors: [],
};
