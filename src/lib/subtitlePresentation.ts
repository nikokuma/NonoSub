export interface SubtitleFitRequest {
  basePx: number;
  minPx: number;
  maxHeightPx: number;
  measureHeight: (fontSizePx: number) => number;
}

export interface SubtitleFitResult {
  fontSizePx: number;
  scale: number;
}

export interface SubtitleFitOptions {
  basePx: number;
  minPx: number;
  maxHeightPx: number;
  contentKey: string;
}

export interface LiveCaptionDensityRequest {
  basePx: number;
  viewportWidth: number;
  sourceText: string;
  translationText: string;
  showSource: boolean;
  showTranslation: boolean;
}

/**
 * Live text grows a fragment at a time. Measuring and scaling the whole card
 * for every fragment makes every existing word jump. Use a small number of
 * deterministic density steps instead; finalized file captions still use the
 * exact DOM fitter below.
 */
export function calculateLiveCaptionFontSize({
  basePx,
  viewportWidth,
  sourceText,
  translationText,
  showSource,
  showTranslation,
}: LiveCaptionDensityRequest): number {
  const sourceWeight = showSource ? Array.from(sourceText.trim()).length : 0;
  // Translation rows render at roughly two thirds of the source size.
  const translationWeight = showTranslation ? Array.from(translationText.trim()).length * 0.65 : 0;
  const density = sourceWeight + translationWeight;
  const widthCap = Math.max(18, viewportWidth / 30);
  const densityCap = density > 300
    ? 17
    : density > 240
      ? 19
      : density > 185
        ? 21
        : density > 130
          ? 23
          : density > 90
            ? 25
            : Number.POSITIVE_INFINITY;
  return Math.max(15, Math.min(basePx, widthCap, densityCap));
}

export function subtitleFitOptionsEqual(left: SubtitleFitOptions, right: SubtitleFitOptions): boolean {
  return left.basePx === right.basePx
    && left.minPx === right.minPx
    && left.maxHeightPx === right.maxHeightPx
    && left.contentKey === right.contentKey;
}

export function calculateSubtitleFit({
  basePx,
  minPx,
  maxHeightPx,
  measureHeight,
}: SubtitleFitRequest): SubtitleFitResult {
  let fontSizePx = Math.max(minPx, Math.round(basePx));
  let measuredHeight = measureHeight(fontSizePx);

  while (measuredHeight > maxHeightPx && fontSizePx > minPx) {
    fontSizePx -= 1;
    measuredHeight = measureHeight(fontSizePx);
  }

  return {
    fontSizePx,
    scale: measuredHeight > maxHeightPx ? maxHeightPx / measuredHeight : 1,
  };
}

export function fitSubtitle(node: HTMLElement, initialOptions: SubtitleFitOptions) {
  let options = initialOptions;
  let frame = 0;
  let fitting = false;
  let observedWidth = -1;

  const fit = (): boolean => {
    if (fitting || node.clientWidth === 0) return false;
    fitting = true;

    const result = calculateSubtitleFit({
      basePx: options.basePx,
      minPx: options.minPx,
      maxHeightPx: options.maxHeightPx,
      measureHeight: (fontSizePx) => {
        node.style.setProperty("--fit-font-size", `${fontSizePx}px`);
        node.style.setProperty("--fit-scale", "1");
        return node.scrollHeight;
      },
    });

    node.style.setProperty("--fit-font-size", `${result.fontSizePx}px`);
    node.style.setProperty("--fit-scale", result.scale.toFixed(4));
    node.dataset.fitFontSize = String(result.fontSizePx);
    node.dataset.fitScale = result.scale.toFixed(4);
    fitting = false;
    return true;
  };

  const schedule = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(fit);
  };

  const observer = new ResizeObserver(([entry]) => {
    const width = entry?.contentRect.width ?? node.clientWidth;
    if (Math.abs(width - observedWidth) < 0.5) return;
    observedWidth = width;
    schedule();
  });
  observer.observe(node);
  document.fonts?.addEventListener("loadingdone", schedule);
  void document.fonts?.ready.then(schedule);
  // Actions run after their element exists. Fit immediately so a newly added
  // overlap never paints for one frame at the previous caption's size.
  if (!fit()) schedule();

  return {
    update(nextOptions: SubtitleFitOptions) {
      if (subtitleFitOptionsEqual(options, nextOptions)) return;
      const contentChanged = options.contentKey !== nextOptions.contentKey;
      options = nextOptions;
      if (contentChanged) {
        cancelAnimationFrame(frame);
        if (!fit()) schedule();
      } else {
        schedule();
      }
    },
    destroy() {
      cancelAnimationFrame(frame);
      observer.disconnect();
      document.fonts?.removeEventListener("loadingdone", schedule);
    },
  };
}

export function readableAccentTextColor(color: string): "#05091e" | "#ffffff" {
  const normalized = color.trim().replace(/^#/, "");
  const expanded = normalized.length === 3
    ? normalized.split("").map((character) => `${character}${character}`).join("")
    : normalized;
  if (!/^[0-9a-f]{6}$/i.test(expanded)) return "#ffffff";

  const channels = [0, 2, 4].map((offset) => Number.parseInt(expanded.slice(offset, offset + 2), 16) / 255);
  const linear = channels.map((channel) => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  const luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
  const whiteContrast = 1.05 / (luminance + 0.05);
  const inkLuminance = 0.0029;
  const inkContrast = (luminance + 0.05) / (inkLuminance + 0.05);
  return inkContrast >= whiteContrast ? "#05091e" : "#ffffff";
}

export function colorWithOpacity(color: string, opacity: number): string {
  const normalized = color.trim().replace(/^#/, "");
  const expanded = normalized.length === 3
    ? normalized.split("").map((character) => `${character}${character}`).join("")
    : normalized;
  if (!/^[0-9a-f]{6}$/i.test(expanded)) return color;
  const channels = [0, 2, 4].map((offset) => Number.parseInt(expanded.slice(offset, offset + 2), 16));
  return `rgba(${channels.join(", ")}, ${Math.min(1, Math.max(0, opacity))})`;
}
