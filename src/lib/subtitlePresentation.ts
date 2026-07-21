import type { LiveSyncMode, SubtitleDisplayMode, SubtitleSegment } from "./contracts";

export interface SubtitleRowVisibility {
  showSource: boolean;
  showTranslation: boolean;
  sourceFallback: boolean;
}

/**
 * A terminal translation failure must remain useful. In translation-only mode
 * the source row temporarily wins rather than leaving an empty or permanent
 * "translating" card. Failed captions remain the same clickable segment.
 */
export function subtitleRowVisibility(
  segment: SubtitleSegment,
  displayMode: SubtitleDisplayMode,
): SubtitleRowVisibility {
  const sourceFallback = segment.translationStatus === "failed"
    && !segment.translationText?.trim();
  return {
    showSource: displayMode !== "translation" || sourceFallback,
    showTranslation: displayMode !== "source" && !sourceFallback,
    sourceFallback,
  };
}

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

export interface LiveCaptionEnvelopeRequest {
  viewportWidth: number;
  fontSizePx: number;
  sourceText: string;
  translationText: string;
  showSource: boolean;
  showTranslation: boolean;
}

export interface LiveCaptionEnvelope {
  sourceText: string;
  translationText: string;
  sourceLineLimit: number;
  translationLineLimit: number;
  sourceGraphemeBudget: number;
  translationGraphemeBudget: number;
}

const LIVE_CAPTION_MAX_GRAPHEMES = 180;
const liveGraphemeSegmenter = typeof Intl !== "undefined" && typeof Intl.Segmenter === "function"
  ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
  : undefined;

function liveGraphemes(text: string): string[] {
  if (!liveGraphemeSegmenter) return Array.from(text);
  return Array.from(liveGraphemeSegmenter.segment(text), ({ segment }) => segment);
}

function compactLiveWhitespace(text: string): string {
  return text.replace(/\s+/gu, " ").trim();
}

function boundedLiveTextTail(text: string, maximumGraphemes: number): string {
  const compact = compactLiveWhitespace(text);
  const graphemes = liveGraphemes(compact);
  const budget = Math.max(1, Math.floor(maximumGraphemes));
  if (graphemes.length <= budget) return compact;

  let tail = graphemes.slice(-(budget - 1)).join("");
  // Avoid beginning on a partial Latin word when a nearby boundary exists.
  const firstSpace = tail.search(/\s/u);
  if (firstSpace >= 0 && firstSpace <= Math.max(8, Math.floor(tail.length * 0.2))) {
    tail = tail.slice(firstSpace + 1).trimStart();
  }
  return `…${tail}`;
}

function liveRowBudget(
  text: string,
  viewportWidth: number,
  fontSizePx: number,
  rowScale: number,
  lineLimit: number,
): number {
  if (lineLimit === 0) return 0;
  const graphemes = liveGraphemes(compactLiveWhitespace(text));
  const cjkCount = graphemes.filter((grapheme) => /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u.test(grapheme)).length;
  const averageGlyphWidth = graphemes.length > 0 && cjkCount / graphemes.length >= 0.2 ? 0.96 : 0.58;
  const cardWidth = Math.min(900, Math.max(280, viewportWidth * 0.92));
  // Arcade has the widest horizontal frame. Using its inset keeps the text
  // budget safe for every preset without measuring on each realtime fragment.
  const usableWidth = Math.max(180, cardWidth - Math.max(64, fontSizePx * 5));
  const glyphWidth = Math.max(6, fontSizePx * rowScale * averageGlyphWidth);
  // Leave enough slack for Wired's timestamp gutter, Momento's angled cutout,
  // font fallback differences, and WebKit's final word wrapping. The CSS clamp
  // is a last-resort barrier, not the normal way the newest words are hidden.
  const estimated = Math.floor((usableWidth / glyphWidth) * lineLimit * 0.74);
  return Math.max(12, Math.min(LIVE_CAPTION_MAX_GRAPHEMES, estimated));
}

/**
 * Create a watching-only live caption. The canonical segment remains complete
 * for transcript history and Nono lessons; the overlay receives only the
 * newest text that can plausibly fit inside its fixed visual envelope.
 */
export function calculateLiveCaptionEnvelope({
  viewportWidth,
  fontSizePx,
  sourceText,
  translationText,
  showSource,
  showTranslation,
}: LiveCaptionEnvelopeRequest): LiveCaptionEnvelope {
  const sourceLineLimit = showSource ? (showTranslation ? 2 : 3) : 0;
  const translationLineLimit = showTranslation ? (showSource ? 2 : 3) : 0;
  const sourceGraphemeBudget = liveRowBudget(sourceText, viewportWidth, fontSizePx, 1, sourceLineLimit);
  const translationGraphemeBudget = liveRowBudget(translationText, viewportWidth, fontSizePx, 0.68, translationLineLimit);

  return {
    sourceText: showSource ? boundedLiveTextTail(sourceText, sourceGraphemeBudget) : sourceText,
    translationText: showTranslation ? boundedLiveTextTail(translationText, translationGraphemeBudget) : translationText,
    sourceLineLimit,
    translationLineLimit,
    sourceGraphemeBudget,
    translationGraphemeBudget,
  };
}

export function liveOverlaySegment(
  segment: SubtitleSegment,
  liveMode: LiveSyncMode,
): SubtitleSegment {
  if (liveMode !== "coordinated") {
    return segment;
  }
  const sourceComplete = !segment.isProvisional && segment.transcriptionStatus === "complete";
  if (sourceComplete && segment.translationStatus === "complete") return segment;
  return { ...segment, translationText: undefined };
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
