import type { LessonOpenContext, LessonClosedContext } from "./contracts";

export interface PlaybackPauseLease {
  sessionId: string;
  mediaInstanceId: string;
  selectionId: number;
  segmentId: string;
  sourceSurface: "viewer";
  wasPlaying: boolean;
  playbackRevision: number;
}

export interface PlaybackResumeState {
  sessionId: string;
  mediaInstanceId: string;
  playbackRevision: number;
  paused: boolean;
  coverageReady: boolean;
}

export function createPlaybackPauseLease(
  context: LessonOpenContext,
  mediaInstanceId: string,
  wasPlaying: boolean,
  playbackRevision: number,
): PlaybackPauseLease | undefined {
  if (context.sourceSurface !== "viewer" || !context.sessionId || !mediaInstanceId) return undefined;
  return {
    sessionId: context.sessionId,
    mediaInstanceId,
    selectionId: context.selectionId,
    segmentId: context.segmentId,
    sourceSurface: "viewer",
    wasPlaying,
    playbackRevision,
  };
}

export function closeIdentifiesPlaybackLease(
  lease: PlaybackPauseLease,
  closed: LessonClosedContext,
): boolean {
  return closed.sourceSurface === lease.sourceSurface
    && closed.sessionId === lease.sessionId
    && closed.selectionId === lease.selectionId
    && closed.segmentId === lease.segmentId;
}

export function shouldResumePlayback(
  lease: PlaybackPauseLease,
  closed: LessonClosedContext,
  current: PlaybackResumeState,
): boolean {
  return closed.reason === "closed"
    && closeIdentifiesPlaybackLease(lease, closed)
    && lease.wasPlaying
    && current.paused
    && current.coverageReady
    && current.sessionId === lease.sessionId
    && current.mediaInstanceId === lease.mediaInstanceId
    && current.playbackRevision === lease.playbackRevision;
}
