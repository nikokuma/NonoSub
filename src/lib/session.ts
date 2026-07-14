import type { SessionEvent, SessionState, SubtitleSegment } from "./contracts";

export function reduceSession(state: SessionState, event: SessionEvent): SessionState {
  switch (event.type) {
    case "phase_changed":
      return { ...state, phase: event.phase };
    case "speaker_discovered":
      return { ...state, speakers: { ...state.speakers, [event.speaker.id]: event.speaker } };
    case "transcript_finalized": {
      const segments = [...state.segments.filter((segment) => segment.id !== event.segment.id), event.segment]
        .sort((left, right) => left.startMs - right.startMs);
      return { ...state, segments };
    }
    case "translation_finalized":
      return {
        ...state,
        segments: state.segments.map((segment) => segment.id === event.segmentId
          ? { ...segment, naturalEnglish: event.naturalEnglish, ambiguityNote: event.ambiguityNote, translationStatus: "complete" }
          : segment),
      };
    case "coverage_changed":
      return { ...state, translatedThroughMs: event.translatedThroughMs };
    case "recoverable_error":
      return { ...state, errors: [...state.errors, event.error] };
    case "fatal_error":
      return { ...state, fatalError: event.message };
    case "complete":
      return { ...state, phase: "complete" };
  }
}

export function activeSegments(segments: SubtitleSegment[], timeMs: number): SubtitleSegment[] {
  return segments
    .filter((segment) => timeMs >= segment.startMs && timeMs < segment.endMs)
    .slice(0, 2);
}

export function shouldPauseForCoverage(timeMs: number, translatedThroughMs: number): boolean {
  return translatedThroughMs - timeMs < 2_000;
}

export function canResumeForCoverage(timeMs: number, translatedThroughMs: number): boolean {
  return translatedThroughMs - timeMs >= 8_000;
}

export function formatTime(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1_000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
