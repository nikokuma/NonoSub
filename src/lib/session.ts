import { DEFAULT_LIVE_SYNC } from "./contracts";
import type { LiveSyncState, SequencedSessionEvent, SessionEvent, SessionState, SubtitleSegment } from "./contracts";

export function reduceSession(state: SessionState, event: SessionEvent): SessionState {
  switch (event.type) {
    case "session_reset":
      return {
        ...state,
        mode: event.mode,
        processingMode: event.processingMode,
        languages: event.languages,
        phase: "preparing",
        segments: [],
        speakers: {},
        readyThroughMs: 0,
        liveSync: event.mode === "live" ? { ...DEFAULT_LIVE_SYNC } : undefined,
        errors: [],
        fatalError: undefined,
        selectedSegmentId: undefined,
      };
    case "phase_changed":
      return { ...state, phase: event.phase };
    case "speaker_discovered":
      return { ...state, speakers: { ...state.speakers, [event.speaker.id]: event.speaker } };
    case "caption_upserted":
    case "transcript_finalized": {
      const segments = [...state.segments.filter((segment) => segment.id !== event.segment.id), event.segment]
        .sort((left, right) => left.startMs - right.startMs);
      return { ...state, segments };
    }
    case "translation_finalized":
      return {
        ...state,
        segments: state.segments.map((segment) => segment.id === event.segmentId
          ? { ...segment, translationText: event.translationText, ambiguityNote: event.ambiguityNote, translationStatus: "complete" }
          : segment),
      };
    case "coverage_changed":
      return { ...state, readyThroughMs: event.readyThroughMs };
    case "live_sync_changed":
      return { ...state, liveSync: event.sync };
    case "lesson_selected":
      return { ...state, selectedSegmentId: event.segmentId };
    case "recoverable_error":
      return { ...state, errors: [...state.errors, event.error] };
    case "fatal_error":
      return { ...state, fatalError: event.message };
    case "complete":
      return { ...state, phase: "complete" };
  }
}

export function applySequencedEvent(state: SessionState, envelope: SequencedSessionEvent): SessionState | undefined {
  if (envelope.sessionId !== state.sessionId || envelope.sequence !== state.sequence + 1) return undefined;
  return { ...reduceSession(state, envelope.event), sequence: envelope.sequence };
}

export function activeSegments(segments: SubtitleSegment[], timeMs: number): SubtitleSegment[] {
  return segments
    .filter((segment) => timeMs >= segment.startMs && timeMs < segment.endMs)
    .slice(0, 2);
}

export function subtitleTimelineTime(videoTimeMs: number, manualOffsetMs: number): number {
  return Math.max(0, videoTimeMs - manualOffsetMs);
}

export function visibleLiveSegments(segments: SubtitleSegment[], sync?: LiveSyncState): SubtitleSegment[] {
  if (!sync?.visibleSegmentId) return [];
  const visible = segments.find((segment) => segment.id === sync.visibleSegmentId);
  return visible ? [visible] : [];
}

export function latestLiveSegments(segments: SubtitleSegment[]): SubtitleSegment[] {
  const provisional = segments.filter((segment) => segment.isProvisional).slice(-1);
  if (provisional.length > 0) return provisional;
  return segments.filter((segment) => !segment.isProvisional).slice(-1);
}

export function captionTail(text: string, maxCharacters: number): string {
  const normalized = text.replace(/\s+/g, " ").trim();
  const characters = Array.from(normalized);
  if (characters.length <= maxCharacters) return normalized;

  const start = characters.length - maxCharacters;
  const rawTail = characters.slice(start).join("");
  let tail = rawTail.trimStart();
  const firstSpace = tail.indexOf(" ");
  const startsInsideWord = start > 0 && !/^\s/u.test(rawTail);
  if (startsInsideWord && /^[\p{L}\p{N}]/u.test(tail) && firstSpace > 0 && firstSpace < maxCharacters / 3) {
    tail = tail.slice(firstSpace + 1);
  }
  return `…${tail}`;
}

export function shouldPauseForCoverage(timeMs: number, readyThroughMs: number): boolean {
  return readyThroughMs - timeMs < 2_000;
}

export function canResumeForCoverage(timeMs: number, readyThroughMs: number): boolean {
  return readyThroughMs - timeMs >= 8_000;
}

export function formatTime(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1_000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
