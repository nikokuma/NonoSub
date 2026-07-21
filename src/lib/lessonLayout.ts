export interface StageRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CharacterViewport extends StageRect {
  bottom: number;
}

export interface LessonStageLayout {
  compact: boolean;
  scale: number;
  board: StageRect;
  characterRail: StageRect;
  characterViewport: CharacterViewport;
  bubble: StageRect;
  controls: StageRect;
  boardContentScale: number;
}

export const LESSON_WINDOW_TARGET = { width: 2048, height: 1024 } as const;
export const LESSON_BOARD_TARGET = { width: 1624, height: 914 } as const;

const BASE_LAYOUT = {
  boardX: 16,
  boardY: 16,
  railX: 1656,
  railY: 16,
  railWidth: 340,
  railHeight: 914,
  bubbleHeight: 185,
  characterY: 217,
  characterHeight: 713,
  controlsY: 944,
  controlsHeight: 56,
} as const;

export function calculateLessonStageLayout(
  viewportWidth: number,
  viewportHeight: number,
  _followupOpen: boolean,
): LessonStageLayout {
  const width = Math.max(320, finite(viewportWidth, 980));
  const height = Math.max(180, finite(viewportHeight, 620));
  const scale = Math.min(1, width / LESSON_WINDOW_TARGET.width, height / LESSON_WINDOW_TARGET.height);
  const renderedWidth = LESSON_WINDOW_TARGET.width * scale;
  const renderedHeight = LESSON_WINDOW_TARGET.height * scale;
  const originX = (width - renderedWidth) / 2;
  const originY = (height - renderedHeight) / 2;

  const rect = (x: number, y: number, rectWidth: number, rectHeight: number): StageRect => ({
    x: originX + x * scale,
    y: originY + y * scale,
    width: rectWidth * scale,
    height: rectHeight * scale,
  });
  const board = rect(BASE_LAYOUT.boardX, BASE_LAYOUT.boardY, LESSON_BOARD_TARGET.width, LESSON_BOARD_TARGET.height);
  const characterRail = rect(BASE_LAYOUT.railX, BASE_LAYOUT.railY, BASE_LAYOUT.railWidth, BASE_LAYOUT.railHeight);
  const bubble = rect(BASE_LAYOUT.railX, BASE_LAYOUT.railY, BASE_LAYOUT.railWidth, BASE_LAYOUT.bubbleHeight);
  const character = rect(BASE_LAYOUT.railX, BASE_LAYOUT.characterY, BASE_LAYOUT.railWidth, BASE_LAYOUT.characterHeight);

  return {
    compact: scale < 0.55,
    scale,
    board,
    characterRail,
    characterViewport: { ...character, bottom: character.y + character.height },
    bubble,
    controls: rect(BASE_LAYOUT.boardX, BASE_LAYOUT.controlsY, LESSON_BOARD_TARGET.width, BASE_LAYOUT.controlsHeight),
    boardContentScale: clamp(board.width / 812, 0.7, 1.65),
  };
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
