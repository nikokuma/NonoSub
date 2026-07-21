export interface LessonStageLayout {
  compact: boolean;
  top: number;
  right: number;
  bottom: number;
  left: number;
  boardWidth: number;
  boardHeight: number;
  controlsWidth: number;
}

export function calculateLessonStageLayout(
  viewportWidth: number,
  viewportHeight: number,
  followupOpen: boolean,
): LessonStageLayout {
  const width = Math.max(320, finite(viewportWidth, 980));
  const height = Math.max(240, finite(viewportHeight, 620));
  const compact = height < 520 || width < 800;
  const top = compact ? 52 : 102;
  const right = compact ? 8 : 20;
  const bottom = compact ? 4 : 6;
  const left = compact ? 80 : 148;
  const controlsHeight = followupOpen ? 54 : compact ? 40 : 42;
  const gap = 5;
  const availableWidth = Math.max(1, width - left - right);
  const availableHeight = Math.max(1, height - top - bottom - controlsHeight - gap);
  const boardWidth = Math.min(availableWidth, availableHeight * 16 / 9);
  const boardHeight = boardWidth * 9 / 16;

  return {
    compact,
    top,
    right,
    bottom,
    left,
    boardWidth,
    boardHeight,
    controlsWidth: compact ? boardWidth : boardWidth * 0.8,
  };
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}
