<script lang="ts">
  import MomentoSubtitleCard from "./MomentoSubtitleCard.svelte";
  import CyberiaSubtitleCard from "./CyberiaSubtitleCard.svelte";
  import BroadcastSubtitleCard from "./BroadcastSubtitleCard.svelte";
  import ArcadeSubtitleCard from "./ArcadeSubtitleCard.svelte";
  import type { CaptionProcessingMode, LiveSyncState, SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { fitSubtitle } from "./subtitlePresentation";

  let {
    segment,
    speaker,
    style,
    sync,
    processingMode = "translated",
    onselect,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    sync?: LiveSyncState;
    processingMode?: CaptionProcessingMode;
    onselect: (segment: SubtitleSegment) => void;
  } = $props();

  let viewportWidth = $state(900);
  let viewportHeight = $state(220);
  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const baseFontSize = $derived(Math.min(style.fontSize, Math.max(18, viewportWidth / 30)));
  const maximumHeight = $derived(Math.min(184, Math.max(120, viewportHeight - 36)));
  const delayLabel = $derived(processingMode === "original_only"
    ? "LIVE · ORIGINAL"
    : sync ? `LIVE · ${(sync.targetDelayMs / 1_000).toFixed(1)}s BEHIND` : "LIVE");
  const contentKey = $derived(`${segment.id}:${source}:${translation}:${speaker?.displayName ?? ""}:${style.displayMode}:${style.preset}:${style.fontFamily}:${style.showSpeakerNames}:${processingMode}:${sync ? "timed" : "plain"}`);
</script>

<svelte:window bind:innerWidth={viewportWidth} bind:innerHeight={viewportHeight} />

<div
  class="live-subtitles preset-{style.preset} effect-{style.effect}"
  class:provisional={segment.isProvisional}
  style={`font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`}
  use:fitSubtitle={{ basePx: baseFontSize, minPx: 12, maxHeightPx: maximumHeight, contentKey }}
>
  {#if style.preset === "momento"}
    <MomentoSubtitleCard {segment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "cyberia"}
    <CyberiaSubtitleCard {segment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "classic-outline" || style.preset === "yellow-drop"}
    <BroadcastSubtitleCard {segment} {speaker} {style} variant={style.preset} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else if style.preset === "arcade"}
    <ArcadeSubtitleCard {segment} {speaker} {style} liveLabel={delayLabel} degraded={sync?.status === "degraded"} {onselect} />
  {:else}
    <button onclick={() => !segment.isProvisional && onselect(segment)} disabled={segment.isProvisional} aria-label={segment.isProvisional ? "Live caption in progress" : "Open this caption in Nono"}>
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
  .live-subtitles{width:min(92vw,900px);min-width:0;touch-action:none;user-select:none;font-size:var(--fit-font-size,28px);transform:scale(var(--fit-scale,1));transform-origin:center}.live-subtitles>button{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center;gap:3px}.signal{font-size:8px;font-weight:850;letter-spacing:.14em;color:#79e9cb;text-shadow:0 1px 8px #000}.signal.degraded{color:#ff9cc5}.speaker{display:flex;align-items:center;gap:5px;font-size:9px;text-transform:uppercase;font-weight:900;letter-spacing:.14em;margin-bottom:1px}.speaker i{width:4px;height:4px;border-radius:50%;background:currentColor;box-shadow:0 0 7px currentColor}.caption{width:fit-content;max-width:100%;padding:3px 12px;background:rgba(0,0,0,var(--sub-bg));overflow-wrap:anywhere;text-wrap:balance}.source{min-height:1.25em;font-weight:850;line-height:1.2}.translation{min-height:1.55em;font-size:.62em;line-height:1.28;color:#faf8ff}.waiting{color:#d4d2d9;font-style:italic;opacity:.7}.provisional{opacity:.9}.effect-outline .caption{text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000,0 3px 12px #000}.effect-shadow .caption{text-shadow:0 3px 10px #000}
</style>
