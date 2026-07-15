<script lang="ts">
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";

  let {
    segments,
    speakers,
    style,
    movable = false,
    onselect,
    onmove,
  }: {
    segments: SubtitleSegment[];
    speakers: Record<string, SpeakerProfile>;
    style: StyleSettings;
    movable?: boolean;
    onselect: (segment: SubtitleSegment) => void;
    onmove?: (event: PointerEvent) => void;
  } = $props();
</script>

<div
  role="group"
  aria-label="Interactive subtitles"
  class="subtitles preset-{style.preset}"
  class:movable
  style={`font-size:${style.fontSize}px;font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`}
  onpointerdown={onmove}
>
  {#each segments as segment (segment.id)}
    {@const speaker = segment.speakerId ? speakers[segment.speakerId] : undefined}
    <button class="subtitle-line effect-{style.effect}" class:provisional={segment.isProvisional} onclick={() => !segment.isProvisional && onselect(segment)} disabled={segment.isProvisional}>
      {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
        <span class="speaker" style={`color:${speaker?.color ?? "#79e9cb"}`}>{speaker?.displayName ?? "Live Audio"}</span>
      {/if}
      {#if style.displayMode !== "translation"}<span class="source">{segment.sourceText}</span>{/if}
      {#if style.displayMode !== "source"}<span class="translation">{segment.translationText ?? "Nono is translating…"}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .subtitles{width:min(92vw,900px);display:grid;gap:9px;touch-action:none;user-select:none}.subtitles.movable{cursor:grab}.subtitle-line{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center}.subtitle-line:disabled{cursor:default}.subtitle-line span:not(.speaker){padding:3px 12px;background:rgba(0,0,0,var(--sub-bg));line-height:1.25}.speaker{font-size:.36em;text-transform:uppercase;font-weight:900;letter-spacing:.14em;margin-bottom:3px}.source{font-weight:850}.translation{font-size:.62em;color:#faf8ff}.provisional{opacity:.72}.effect-outline span:not(.speaker){text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000,0 3px 12px #000}.effect-shadow span:not(.speaker){text-shadow:0 3px 10px #000}.preset-cinema .source,.preset-cinema .translation{font-family:Georgia,serif}.preset-contrast .source,.preset-contrast .translation{background:#000;color:#fff}.preset-nono-pop .source,.preset-nono-pop .translation{background:rgba(163,51,126,var(--sub-bg));border-radius:9px}.preset-manga .source,.preset-manga .translation{background:rgba(255,255,255,var(--sub-bg));color:#111;font-family:serif}.preset-retro .source,.preset-retro .translation{font-family:monospace;letter-spacing:.04em}
</style>
