<script lang="ts">
  import MomentoSubtitleCard from "./MomentoSubtitleCard.svelte";
  import CyberiaSubtitleCard from "./CyberiaSubtitleCard.svelte";
  import BroadcastSubtitleCard from "./BroadcastSubtitleCard.svelte";
  import ArcadeSubtitleCard from "./ArcadeSubtitleCard.svelte";
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { fitSubtitle } from "./subtitlePresentation";

  let {
    segments,
    speakers,
    style,
    movable = false,
    preview = false,
    onselect,
    onmove,
  }: {
    segments: SubtitleSegment[];
    speakers: Record<string, SpeakerProfile>;
    style: StyleSettings;
    movable?: boolean;
    preview?: boolean;
    onselect: (segment: SubtitleSegment) => void;
    onmove?: (event: PointerEvent) => void;
  } = $props();

  let viewportHeight = $state(720);
  const contentKey = $derived(
    segments.map((segment) => `${segment.id}:${segment.sourceText}:${segment.translationText ?? ""}:${segment.speakerId ? speakers[segment.speakerId]?.displayName ?? "" : ""}`).join("|")
      + `:${style.displayMode}:${style.preset}:${style.fontFamily}:${style.showSpeakerNames}`,
  );
  const maximumHeight = $derived(preview ? 158 : Math.min(260, Math.max(150, viewportHeight * 0.36)));
</script>

<svelte:window bind:innerHeight={viewportHeight} />

<div
  role="group"
  aria-label="Interactive subtitles"
  class="subtitles preset-{style.preset}"
  class:movable
  class:preview
  inert={preview}
  aria-hidden={preview}
  style={`font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`}
  use:fitSubtitle={{ basePx: style.fontSize, minPx: 13, maxHeightPx: maximumHeight, contentKey }}
  onpointerdown={onmove}
>
  {#each segments as segment (segment.id)}
    {@const speaker = segment.speakerId ? speakers[segment.speakerId] : undefined}
    <div class="segment-wrap" data-segment-id={segment.id}>
    {#if style.preset === "momento"}
      <MomentoSubtitleCard {segment} {speaker} {style} {onselect} />
    {:else if style.preset === "wired"}
      <CyberiaSubtitleCard {segment} {speaker} {style} {onselect} />
    {:else if style.preset === "classic-outline" || style.preset === "yellow-drop"}
      <BroadcastSubtitleCard {segment} {speaker} {style} variant={style.preset} {onselect} />
    {:else if style.preset === "fallout"}
      <ArcadeSubtitleCard {segment} {speaker} {style} {onselect} />
    {:else}
      <button
        class="subtitle-line effect-{style.effect}"
        class:provisional={segment.isProvisional}
        onclick={(event) => event.detail === 0 && !segment.isProvisional && onselect(segment)}
        disabled={segment.isProvisional}
        aria-label={segment.isProvisional ? "Caption in progress" : "Right-click this caption to ask Nono"}
      >
        {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
          <span class="speaker" style={`color:${speaker?.color ?? "#79e9cb"}`}>{speaker?.displayName ?? "Live Audio"}</span>
        {/if}
        {#if style.displayMode !== "translation"}<span class="source">{segment.sourceText}</span>{/if}
        {#if style.displayMode !== "source"}<span class="translation">{segment.translationText ?? "Nono is translating…"}</span>{/if}
      </button>
    {/if}
    </div>
  {/each}
</div>

<style>
  .subtitles{width:min(92vw,900px);display:grid;gap:9px;touch-action:none;user-select:none;-webkit-user-select:none;-webkit-touch-callout:none;font-size:var(--fit-font-size,28px);transform:scale(var(--fit-scale,1));transform-origin:center}.segment-wrap{display:contents}.subtitles.preview{width:100%;max-width:900px}.subtitles.movable{cursor:grab}.subtitle-line{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center}.subtitle-line:disabled{cursor:default}.subtitle-line span:not(.speaker){padding:3px 12px;background:rgba(0,0,0,var(--sub-bg));line-height:1.25;max-width:100%;overflow-wrap:anywhere;text-wrap:balance}.speaker{font-size:.36em;text-transform:uppercase;font-weight:900;letter-spacing:.14em;margin-bottom:3px}.source{font-weight:850}.translation{font-size:.62em;color:#faf8ff}.provisional{opacity:.72}.effect-outline span:not(.speaker){text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000,0 3px 12px #000}.effect-shadow span:not(.speaker){text-shadow:0 3px 10px #000}
</style>
