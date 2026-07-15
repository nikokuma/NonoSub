<script lang="ts">
  import type { LiveSyncState, SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";

  let {
    segment,
    speaker,
    style,
    sync,
    onselect,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    sync?: LiveSyncState;
    onselect: (segment: SubtitleSegment) => void;
  } = $props();

  let viewportWidth = $state(900);
  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const contentLength = $derived(Array.from(`${source}${translation}`).length);
  const densityFontSize = $derived(contentLength > 180 ? 18 : contentLength > 120 ? 21 : contentLength > 80 ? 24 : style.fontSize);
  const liveFontSize = $derived(Math.min(style.fontSize, densityFontSize, Math.max(18, viewportWidth / 30)));
  const delayLabel = $derived(sync ? `LIVE · ${(sync.targetDelayMs / 1_000).toFixed(1)}s BEHIND` : "LIVE");
</script>

<svelte:window bind:innerWidth={viewportWidth} />

<div
  class="live-subtitles preset-{style.preset} effect-{style.effect}"
  class:provisional={segment.isProvisional}
  style={`font-size:${liveFontSize}px;font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`}
>
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
</div>

<style>
  .live-subtitles{width:min(92vw,900px);min-width:0;touch-action:none;user-select:none}.live-subtitles button{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center;gap:3px}.signal{font-size:8px;font-weight:850;letter-spacing:.14em;color:#79e9cb;text-shadow:0 1px 8px #000}.signal.degraded{color:#ff9cc5}.speaker{display:flex;align-items:center;gap:5px;font-size:9px;text-transform:uppercase;font-weight:900;letter-spacing:.14em;margin-bottom:1px}.speaker i{width:4px;height:4px;border-radius:50%;background:currentColor;box-shadow:0 0 7px currentColor}.caption{width:fit-content;max-width:100%;padding:3px 12px;background:rgba(0,0,0,var(--sub-bg));overflow-wrap:anywhere;text-wrap:balance}.source{min-height:1.25em;font-weight:850;line-height:1.2}.translation{min-height:1.55em;font-size:.62em;line-height:1.28;color:#faf8ff}.waiting{color:#d4d2d9;font-style:italic;opacity:.7}.provisional{opacity:.9}.effect-outline .caption{text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000,0 3px 12px #000}.effect-shadow .caption{text-shadow:0 3px 10px #000}.preset-cinema .caption{font-family:Georgia,serif}.preset-contrast .caption{background:#000;color:#fff}.preset-nono-pop .caption{background:rgba(163,51,126,var(--sub-bg));border-radius:9px}.preset-manga .caption{background:rgba(255,255,255,var(--sub-bg));color:#111;font-family:serif}.preset-retro .caption{font-family:monospace;letter-spacing:.04em}
</style>
