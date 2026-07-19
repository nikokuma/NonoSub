export interface OverlayMonitorGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
}

export interface OverlayGeometry {
  logicalWidth: number;
  logicalHeight: number;
  physicalX: number;
  physicalY: number;
  physicalWidth: number;
  physicalHeight: number;
}

interface ResolveOverlayGeometryOptions {
  normalizedPosition: { x: number; y: number };
  preferredLogicalWidth: number;
  contentLogicalHeight: number;
  minimumLogicalHeight?: number;
  maximumHeightRatio?: number;
  horizontalMargin?: number;
  verticalMargin?: number;
}

const clamp = (value: number, minimum: number, maximum: number) =>
  Math.min(maximum, Math.max(minimum, value));

/**
 * Resolve a Retina-safe overlay rectangle. Tauri monitor geometry is physical
 * pixels, while webview measurements and saved overlay width are logical
 * points. Keeping that conversion here prevents a 900×220 overlay from
 * accidentally becoming 450×110 CSS points on a 2× display.
 */
export function resolveOverlayGeometry(
  monitor: OverlayMonitorGeometry,
  options: ResolveOverlayGeometryOptions,
): OverlayGeometry {
  const scale = Math.max(1, monitor.scaleFactor || 1);
  const monitorLogicalWidth = monitor.width / scale;
  const monitorLogicalHeight = monitor.height / scale;
  const horizontalMargin = options.horizontalMargin ?? 20;
  const verticalMargin = options.verticalMargin ?? 20;
  const maximumLogicalWidth = Math.max(1, monitorLogicalWidth * 0.9);
  const minimumLogicalWidth = Math.min(520, maximumLogicalWidth);
  const logicalWidth = clamp(options.preferredLogicalWidth, minimumLogicalWidth, maximumLogicalWidth);
  const minimumLogicalHeight = Math.min(options.minimumLogicalHeight ?? 130, monitorLogicalHeight * 0.9);
  const maximumLogicalHeight = Math.max(
    minimumLogicalHeight,
    monitorLogicalHeight * (options.maximumHeightRatio ?? 0.82),
  );
  const logicalHeight = clamp(
    options.contentLogicalHeight + verticalMargin * 2,
    minimumLogicalHeight,
    maximumLogicalHeight,
  );
  const physicalWidth = Math.round(logicalWidth * scale);
  const physicalHeight = Math.round(logicalHeight * scale);
  const physicalMargin = Math.round(12 * scale);
  const desiredCenterX = monitor.x + clamp(options.normalizedPosition.x, 0, 1) * monitor.width;
  const desiredCenterY = monitor.y + clamp(options.normalizedPosition.y, 0, 1) * monitor.height;
  const minimumX = monitor.x + physicalMargin;
  const maximumX = monitor.x + monitor.width - physicalWidth - physicalMargin;
  const minimumY = monitor.y + physicalMargin;
  const maximumY = monitor.y + monitor.height - physicalHeight - physicalMargin;

  return {
    logicalWidth,
    logicalHeight,
    physicalX: Math.round(clamp(desiredCenterX - physicalWidth / 2, minimumX, Math.max(minimumX, maximumX))),
    physicalY: Math.round(clamp(desiredCenterY - physicalHeight / 2, minimumY, Math.max(minimumY, maximumY))),
    physicalWidth,
    physicalHeight,
  };
}
