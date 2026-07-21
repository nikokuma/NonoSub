export type LearnerLevel = "beginner" | "intermediate" | "advanced";
export type SubtitleDisplayMode = "source" | "translation" | "both";
export type CaptionProcessingMode = "translated" | "original_only";
export type SegmentStatus = "pending" | "complete" | "failed" | "skipped";
export type SessionMode = "file" | "live";
export type LiveSyncMode = "coordinated" | "fast_source";
export type LiveTranslationEngine = "realtime" | "transcript_locked";
export type LiveSyncStatus = "steady" | "catching_up" | "degraded";
export type LiveCaptureLifecycle = "inactive" | "starting" | "active" | "reconnecting" | "stopping" | "failed";
export type EndSessionReason = "user_stop" | "replacement" | "quit" | "fatal_error";
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

export type SubtitlePreset =
  | "clean"
  | "classic-outline"
  | "yellow-drop"
  | "arcade"
  | "momento"
  | "wired";
export type AppSurface = "workbench" | "viewer" | "overlay" | "lesson" | "launcher";
export type LauncherMode = "file" | "live";
export type LauncherState = "idle" | "hovering" | "preparing" | "starting" | "error";
export type LiveCaptureSourceKind = "application" | "window" | "display";

export interface LiveCaptureSourceSelection {
  kind: LiveCaptureSourceKind;
  processId?: number;
  windowId?: number;
  displayId?: number;
}

export interface LiveCaptureSource extends LiveCaptureSourceSelection {
  id: string;
  title: string;
  detail: string;
  applicationName?: string;
  bundleIdentifier?: string;
}

export interface LiveCaptureSources {
  applications: LiveCaptureSource[];
  windows: LiveCaptureSource[];
  displays: LiveCaptureSource[];
}

export interface LessonPlacement {
  monitorKey: string;
  x: number;
  y: number;
}

export type LessonSurfaceMode = "compose" | "thinking" | "lesson" | "error";

export interface LessonOpenContext {
  selectionId: number;
  sessionId: string;
  sourceSurface: "viewer" | "overlay" | "workbench";
  segmentId: string;
  selectedSegment: SubtitleSegment;
  cursorX: number;
  cursorY: number;
  externalMediaControl: ExternalMediaControlResult;
}

export interface LessonClosedContext {
  selectionId: number;
  sessionId: string;
  sourceSurface: "viewer" | "overlay" | "workbench";
  segmentId: string;
  reason: "closed" | "invalidated";
}

export type ExternalMediaControlResult = "not_requested" | "paused" | "permission_required" | "failed" | "unsupported";

export interface LanguageSettings {
  source: "auto" | string;
  target: string;
  explanation: string;
}

export interface SyncSettings {
  liveMode: LiveSyncMode;
  translationEngine: LiveTranslationEngine;
}

export interface LiveSyncState {
  targetDelayMs: number;
  observedLagMs: number;
  status: LiveSyncStatus;
  visibleSegmentId?: string;
}

export interface LiveCaptureStatus {
  sessionId: string;
  lifecycle: LiveCaptureLifecycle;
  startedAtMs?: number;
  sourceLabel?: string;
}

export interface SessionEnding {
  sessionId: string;
  reason: EndSessionReason;
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

export interface WiredColors {
  panel: string;
  wash: string;
  sourceText: string;
  translationText: string;
  metadata: string;
  fallbackAccent: string;
}

export interface ArcadeColors {
  text: string;
  panel: string;
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
  wiredColors: WiredColors;
  arcadeColors: ArcadeColors;
}

export interface BoardSection {
  heading: string;
  lines: ChalkPhrase[];
}

export type BoardDemoKind = "none" | "sentence_breakdown" | "omitted_meaning" | "literal_to_natural" | "tone_scale" | "mini_dialogue";
export type ChalkColor = "white" | "baby_blue" | "yellow" | "pink";
export type ChalkMark = "none" | "box" | "bracket" | "strike";
export type TailCue = "none" | "point" | "underline";

export interface ChalkPhrase {
  text: string;
  color: ChalkColor;
  mark: ChalkMark;
  tailCue: TailCue;
}

export interface SourceFocus {
  color: ChalkColor;
  tailCue: TailCue;
}

export interface BoardDemoItem {
  label: string;
  detail: string;
  color: ChalkColor;
  mark: ChalkMark;
  tailCue: TailCue;
}

export interface BoardDemo {
  kind: BoardDemoKind;
  caption?: string;
  items: BoardDemoItem[];
  result?: ChalkPhrase;
}

export interface TeachingMoment {
  title: string;
  speechBubble: string;
  sourceFocus: SourceFocus;
  boardSections: BoardSection[];
  demonstration: BoardDemo;
  ambiguityNote?: ChalkPhrase;
}

export interface LessonCard {
  schemaVersion: 2;
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

export interface RetranslatedSegment {
  segmentId: string;
  translationText: string;
  ambiguityNote?: string;
}

export type SessionEvent =
  | { type: "session_reset"; mode: SessionMode; languages: LanguageSettings; processingMode: CaptionProcessingMode; liveTranslationEngine?: LiveTranslationEngine }
  | { type: "phase_changed"; phase: SessionPhase }
  | { type: "caption_upserted"; segment: SubtitleSegment }
  | { type: "transcript_finalized"; segment: SubtitleSegment }
  | { type: "translation_finalized"; segmentId: string; translationText: string; ambiguityNote?: string }
  | { type: "file_retranslation_applied"; languages: LanguageSettings; translations: RetranslatedSegment[] }
  | { type: "speaker_discovered"; speaker: SpeakerProfile }
  | { type: "coverage_changed"; readyThroughMs: number }
  | { type: "live_sync_changed"; sync: LiveSyncState }
  | { type: "live_audio_gap"; startMs: number; endMs: number }
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
  processingMode: CaptionProcessingMode;
  liveTranslationEngine?: LiveTranslationEngine;
  languages: LanguageSettings;
  phase: SessionPhase;
  segments: SubtitleSegment[];
  speakers: Record<string, SpeakerProfile>;
  readyThroughMs: number;
  liveSync?: LiveSyncState;
  errors: RecoverableError[];
  fatalError?: string;
  selectedSegmentId?: string;
  media?: PreparedMediaInfo;
}

export type CapabilityAvailability = "available" | "unavailable" | "unknown";

export interface ApiConfigurationStatus {
  configured: boolean;
  validatedAt?: number;
  validationSchema: number;
  languageModel: CapabilityAvailability;
  fileTranscription: CapabilityAvailability;
  realtimeTranslation: CapabilityAvailability;
  realtimeOriginalOnly: CapabilityAvailability;
  liveTextTranslation: CapabilityAvailability;
}

/** @deprecated Use ApiConfigurationStatus. */
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
  translationEngine: "realtime",
};

export const DEFAULT_LIVE_SYNC: LiveSyncState = {
  targetDelayMs: 2_500,
  observedLagMs: 0,
  status: "steady",
};

export const DEFAULT_STYLE: StyleSettings = {
  preset: "momento",
  position: { x: 0.5, y: 0.82 },
  overlayPosition: { x: 0.5, y: 0.78 },
  overlayWidth: 900,
  fontFamily: "Inter",
  fontSize: 28,
  backgroundOpacity: 0.58,
  effect: "outline",
  displayMode: "both",
  showSpeakerNames: true,
  wiredColors: {
    panel: "#05081c",
    wash: "#0b2944",
    sourceText: "#c9e6fa",
    translationText: "#ffffff",
    metadata: "#5fa8dc",
    fallbackAccent: "#4ac8ff",
  },
  arcadeColors: {
    text: "#f0a14a",
    panel: "#0b0d08",
  },
};

export const EMPTY_SESSION: SessionState = {
  sessionId: "fixture",
  sequence: 0,
  processingMode: "translated",
  languages: { ...DEFAULT_LANGUAGES },
  phase: "idle",
  segments: [],
  speakers: {},
  readyThroughMs: 0,
  errors: [],
};
