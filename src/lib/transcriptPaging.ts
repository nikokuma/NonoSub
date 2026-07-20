export const TRANSCRIPT_PAGE_SIZE = 200;

export function visibleTranscriptPage<T>(items: T[], visibleCount: number): T[] {
  return items.slice(-Math.max(TRANSCRIPT_PAGE_SIZE, visibleCount));
}

export function earlierTranscriptCount(total: number, visibleCount: number): number {
  return Math.max(0, total - Math.max(TRANSCRIPT_PAGE_SIZE, visibleCount));
}
