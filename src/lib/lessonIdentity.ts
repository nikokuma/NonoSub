import type { LessonOpenContext } from "./contracts";

export function lessonThreadKey(context: LessonOpenContext): string {
  const segment = context.selectedSegment;
  return JSON.stringify([
    context.sessionId,
    segment.id,
    segment.origin,
    segment.startMs,
    segment.endMs,
    segment.sourceText,
    segment.speakerId ?? null,
  ]);
}
