export interface NormalizedPosition {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export function clampSubtitlePosition(
  position: NormalizedPosition,
  viewport: Size,
  panel: Size,
  margin = 8,
): NormalizedPosition {
  const width = Math.max(1, viewport.width);
  const height = Math.max(1, viewport.height);
  const halfWidth = Math.max(0, panel.width / 2);
  const halfHeight = Math.max(0, panel.height / 2);
  const minimumX = halfWidth + margin;
  const maximumX = width - halfWidth - margin;
  const minimumY = halfHeight + margin;
  const maximumY = height - halfHeight - margin;

  return {
    x: clampAxis(position.x * width, minimumX, maximumX, width / 2) / width,
    y: clampAxis(position.y * height, minimumY, maximumY, height / 2) / height,
  };
}

export function mediaEventIsCurrent(
  eventElement: HTMLVideoElement | undefined,
  currentElement: HTMLVideoElement | undefined,
  eventInstanceId: string,
  currentInstanceId: string,
): boolean {
  return Boolean(eventElement)
    && eventElement === currentElement
    && eventInstanceId === currentInstanceId;
}

function clampAxis(value: number, minimum: number, maximum: number, fallback: number): number {
  if (!Number.isFinite(value) || maximum < minimum) return fallback;
  return Math.min(maximum, Math.max(minimum, value));
}
