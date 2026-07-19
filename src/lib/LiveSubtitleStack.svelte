<script lang="ts">
  import MomentoSubtitleCard from "./MomentoSubtitleCard.svelte";
  import CyberiaSubtitleCard from "./CyberiaSubtitleCard.svelte";
  import BroadcastSubtitleCard from "./BroadcastSubtitleCard.svelte";
  import ArcadeSubtitleCard from "./ArcadeSubtitleCard.svelte";
  import type { CaptionProcessingMode, LiveSyncMode, LiveSyncState, SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { calculateLiveCaptionFontSize, liveOverlaySegment } from "./subtitlePresentation";

  let {
    segment,
    speaker,
    style,
    sync,
    liveMode = "coordinated",
    processingMode = "translated",
    onselect,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    sync?: LiveSyncState;
    liveMode?: LiveSyncMode;
    processingMode?: CaptionProcessingMode;
    onselect: (segment: SubtitleSegment) => void;
  } = $props();

  let viewportWidth = $state(900);
  const renderedSegment = $derived(liveOverlaySegment(segment, liveMode));
  const source = $derived(renderedSegment.sourceText.trim());
  const translation = $derived(renderedSegment.translationText?.trim() ?? "");
  const showSource = $derived(style.displayMode !== "translation");
  const showTranslation = $derived(style.displayMode !== "source");
  const liveFontSize = $derived(calculateLiveCaptionFontSize({
    basePx: style.fontSize,
    viewportWidth,
    sourceText: source,
    translationText: translation,
    showSource,
    showTranslation,
  }));
  const delayLabel = $derived(processingMode === "original_only"
    ? "LIVE · ORIGINAL"
    : sync ? `LIVE · ${(sync.targetDelayMs / 1_000).toFixed(1)}s BEHIND` : "LIVE");
</script>

<svelte:window bind:innerWidth={viewportWidth} />

<div
  class="live-subtitles preset-{style.preset} effect-{style.effect}"
  data-segment-id={segment.id}
  class:provisional={segment.isProvisional}
  style={`font-size:${liveFontSize}px;font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`}
>
  {#if style.preset === "momento"}
    <MomentoSubtitleCard segment={renderedSegment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "wired"}
    <CyberiaSubtitleCard segment={renderedSegment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "classic-outline" || style.preset === "yellow-drop"}
    <BroadcastSubtitleCard segment={renderedSegment} {speaker} {style} variant={style.preset} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "fallout"}
    <ArcadeSubtitleCard segment={renderedSegment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else}
    <button
      onclick={(event) => event.detail === 0 && !renderedSegment.isProvisional && onselect(renderedSegment)}
      disabled={segment.isProvisional}
      aria-label={segment.isProvisional ? "Live caption in progress" : "Right-click this caption to ask Nono"}
    >
      <span class="signal" class:degraded={sync?.status === "degraded"}>{delayLabel}</span>
      {#if style.showSpeakerNames}<span class="speaker" style={`color:${speaker?.color ?? "#79e9cb"}`}><i></i>{speaker?.displayName ?? "Live Audio"}</span>{/if}
      {#if style.displayMode !== "translation"}
        <span class="caption source" class:waiting={!source}>{source || "Listening…"}</span>
      {/if}
      {#if style.displayMode !== "source"}
        <span class="caption translation" class:waiting={!translation}>{translation || "Translation catching up…"}</span>
      {/if}
    </button>
  {/if}
</div>

<style>
  .live-subtitles{width:min(92vw,900px);min-width:0;touch-action:none;user-select:none;-webkit-user-select:none;-webkit-touch-callout:none}.live-subtitles>button{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center;gap:3px}.signal{font-size:8px;font-weight:850;letter-spacing:.14em;color:#79e9cb;text-shadow:0 1px 8px #000}.signal.degraded{color:#ff9cc5}.speaker{display:flex;align-items:center;gap:5px;font-size:9px;text-transform:uppercase;font-weight:900;letter-spacing:.14em;margin-bottom:1px}.speaker i{width:4px;height:4px;border-radius:50%;background:currentColor;box-shadow:0 0 7px currentColor}.caption{width:fit-content;max-width:100%;padding:3px 12px;background:rgba(0,0,0,var(--sub-bg));overflow-wrap:anywhere;text-wrap:wrap}.source{min-height:1.25em;font-weight:850;line-height:1.2}.translation{min-height:1.55em;font-size:.62em;line-height:1.28;color:#faf8ff}.waiting{color:#d4d2d9;font-style:italic;opacity:.7}.provisional{opacity:.9}.effect-outline .caption{text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000,0 3px 12px #000}.effect-shadow .caption{text-shadow:0 3px 10px #000}
</style>
