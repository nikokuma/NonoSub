import { DEFAULT_LIVE_SYNC } from "./contracts";
import type { LiveSyncMode, LiveSyncState, SequencedSessionEvent, SessionEvent, SessionState, SubtitleSegment } from "./contracts";

const MAX_RECOVERABLE_ERRORS = 50;

function compareSegments(left: SubtitleSegment, right: SubtitleSegment): number {
  return left.startMs - right.startMs || left.id.localeCompare(right.id);
}

export function upsertOrderedSegment(segments: SubtitleSegment[], segment: SubtitleSegment): SubtitleSegment[] {
  const next = segments.slice();
  const existing = next.findIndex((candidate) => candidate.id === segment.id);
  if (existing >= 0) next.splice(existing, 1);
  let low = 0;
  let high = next.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (compareSegments(next[middle], segment) <= 0) low = middle + 1;
    else high = middle;
  }
  next.splice(low, 0, segment);
  return next;
}

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
      const segments = upsertOrderedSegment(state.segments, event.segment);
      return { ...state, segments };
    }
    case "translation_finalized":
      return {
        ...state,
        segments: state.segments.map((segment) => segment.id === event.segmentId
          ? { ...segment, translationText: event.translationText, ambiguityNote: event.ambiguityNote, translationStatus: "complete" }
          : segment),
      };
    case "file_retranslation_applied": {
      const replacements = new Map(event.translations.map((translation) => [translation.segmentId, translation]));
      return {
        ...state,
        languages: event.languages,
        segments: state.segments.map((segment) => {
          const replacement = replacements.get(segment.id);
          return replacement
            ? {
                ...segment,
                translationText: replacement.translationText,
                ambiguityNote: replacement.ambiguityNote,
                translationStatus: "complete" as const,
              }
            : segment;
        }),
      };
    }
    case "coverage_changed":
      return { ...state, readyThroughMs: event.readyThroughMs };
    case "live_sync_changed":
      return {
        ...state,
        liveSync: {
          ...event.sync,
          visibleSegmentId: event.sync.visibleSegmentId ?? state.liveSync?.visibleSegmentId,
        },
      };
    case "live_audio_gap":
      return {
        ...state,
        errors: [...state.errors, {
          code: "live_audio_gap",
          message: `Live audio was unavailable for ${Math.max(0, event.endMs - event.startMs)} ms.`,
        }].slice(-MAX_RECOVERABLE_ERRORS),
      };
    case "lesson_selected":
      return { ...state, selectedSegmentId: event.segmentId };
    case "recoverable_error":
      return { ...state, errors: [...state.errors, event.error].slice(-MAX_RECOVERABLE_ERRORS) };
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

export function visibleLiveSegments(
  segments: SubtitleSegment[],
  sync?: LiveSyncState,
  mode: LiveSyncMode = "coordinated",
): SubtitleSegment[] {
  const requested = sync?.visibleSegmentId
    ? segments.find((segment) => segment.id === sync.visibleSegmentId)
    : undefined;
  const sourceComplete = (segment: SubtitleSegment) => !segment.isProvisional
    && segment.transcriptionStatus === "complete";
  if (requested && (mode === "fast_source" || sourceComplete(requested))) return [requested];
  if (mode === "fast_source" || !requested) return [];

  const requestedIndex = segments.findIndex((segment) => segment.id === requested.id);
  const retained = segments.slice(0, requestedIndex).reverse().find((segment) => sourceComplete(segment)
    && (segment.translationStatus === "complete"
      || segment.translationStatus === "failed"
      || segment.translationStatus === "skipped"));
  return retained ? [retained] : [];
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
