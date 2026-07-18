import type { ChalkColor, TeachingMoment } from "./contracts";

export function isLessonSkipped(activeCardKey?: string, skippedCardKey?: string): boolean {
  return Boolean(activeCardKey && activeCardKey === skippedCardKey);
}

export interface LessonStepOrder {
  source?: number;
  sections: number[];
  demonstration?: number;
  ambiguity?: number;
}

export function lessonStepOrder(moment?: TeachingMoment, hasSelectedSource = true): LessonStepOrder {
  let next = 1;
  const order: LessonStepOrder = { sections: [] };
  if (!moment) return order;

  if (hasSelectedSource) order.source = next++;
  order.sections = moment.boardSections.map(() => next++);
  if (moment.demonstration.kind !== "none" && moment.demonstration.items.length > 0) {
    order.demonstration = next++;
  }
  if (moment.ambiguityNote) order.ambiguity = next;
  return order;
}

export function dominantChalkColor(colors: ChalkColor[]): ChalkColor {
  if (colors.length === 0) return "white";
  const counts = new Map<ChalkColor, number>();
  for (const color of colors) counts.set(color, (counts.get(color) ?? 0) + 1);
  return colors.reduce((dominant, color) =>
    (counts.get(color) ?? 0) > (counts.get(dominant) ?? 0) ? color : dominant
  );
}
