export function isLessonSkipped(activeCardKey?: string, skippedCardKey?: string): boolean {
  return Boolean(activeCardKey && activeCardKey === skippedCardKey);
}
