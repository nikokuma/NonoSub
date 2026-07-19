<script lang="ts">
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";

  let {
    segment,
    speaker,
    style,
    variant,
    liveLabel,
    degraded = false,
    onselect,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    variant: "classic-outline" | "yellow-drop";
    liveLabel?: string;
    degraded?: boolean;
    onselect: (segment: SubtitleSegment) => void;
  } = $props();

  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const showSource = $derived(style.displayMode !== "translation");
  const showTranslation = $derived(style.displayMode !== "source");
</script>

<button
  class="broadcast-card variant-{variant}"
  class:provisional={segment.isProvisional}
  class:degraded
  class:live={Boolean(liveLabel)}
  style={`--broadcast-panel:rgba(0,0,0,${style.backgroundOpacity})`}
  onclick={(event) => event.detail === 0 && !segment.isProvisional && onselect(segment)}
  disabled={segment.isProvisional}
  aria-label={segment.isProvisional ? "Caption in progress" : "Right-click this caption to ask Nono"}
>
  {#if liveLabel}<span class="live-signal">{liveLabel}</span>{/if}
  {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
    <span class="speaker">{speaker?.displayName ?? "Live Audio"}</span>
  {/if}
  <span class="caption-stack">
    {#if showSource}
      <span class="caption source" class:waiting={!source}>{source || "Listening…"}</span>
    {/if}
    {#if showTranslation}
      <span class="caption translation" class:waiting={!translation}>{translation || "Translation catching up…"}</span>
    {/if}
  </span>
</button>

<style>
  .broadcast-card {
    width: 100%;
    min-width: 0;
    display: grid;
    justify-items: center;
    gap: .12em;
    border: 0;
    background: transparent;
    padding: .28em .7em .42em;
    color: white;
    text-align: center;
    cursor: pointer;
    font-synthesis: none;
    font-kerning: normal;
  }

  .broadcast-card:disabled { cursor: default; }
  .caption-stack { display: grid; justify-items: center; gap: .08em; width: fit-content; max-width: 100%; min-width: 0; padding: .2em .62em .24em; background: var(--broadcast-panel); }
  .caption { display: block; width: fit-content; max-width: 100%; overflow-wrap: anywhere; word-break: auto-phrase; text-wrap: balance; }
  .live .caption { text-wrap: wrap; }
  .broadcast-card.live { max-height: 180px; overflow: visible; }
  .live .caption { display: -webkit-box; -webkit-box-orient: vertical; overflow: hidden; }
  .live .source { line-clamp: var(--live-source-lines); -webkit-line-clamp: var(--live-source-lines); }
  .live .translation { line-clamp: var(--live-translation-lines); -webkit-line-clamp: var(--live-translation-lines); }
  .source { font-size: 1em; font-weight: 700; line-height: 1.16; }
  .translation { font-size: .7em; font-weight: 700; line-height: 1.18; }
  .speaker, .live-signal { font-size: max(9px, .38em); font-weight: 700; letter-spacing: .1em; line-height: 1.05; text-transform: uppercase; }
  .live-signal { margin-bottom: .08em; }

  .variant-classic-outline,
  .variant-yellow-drop {
    /* Arial is intentional: it matches the clean broadcast references. */
    font-family: Arial, "Hiragino Sans", "Noto Sans CJK JP", "Noto Sans", sans-serif;
  }

  .variant-classic-outline .caption,
  .variant-classic-outline .speaker,
  .variant-classic-outline .live-signal {
    color: #f7f7f5;
    -webkit-text-stroke: clamp(2px, .085em, 3px) #060606;
    paint-order: stroke fill;
    text-shadow:
      -.055em -.055em .03em #000,
       .055em -.055em .03em #000,
      -.055em  .055em .03em #000,
       .055em  .055em .03em #000,
       0 .14em .11em #000e;
  }

  .variant-yellow-drop .caption,
  .variant-yellow-drop .speaker,
  .variant-yellow-drop .live-signal {
    color: #fff200;
    -webkit-text-stroke: clamp(1.5px, .055em, 2px) #080808;
    paint-order: stroke fill;
    text-shadow: none;
  }

  .waiting { font-style: italic; opacity: .76; }
  .provisional { opacity: .84; }
  .degraded .live-signal { color: #ff9cc5; }
  .broadcast-card:focus-visible { outline: 2px solid currentColor; outline-offset: .12em; }
  .broadcast-card:active:not(:disabled) .caption-stack { transform: scale(.985); }

  @media (prefers-reduced-motion: no-preference) {
    .broadcast-card:not(.live) .caption-stack { animation: caption-in 110ms ease-out both; }
  }

  @keyframes caption-in {
    from { opacity: 0; transform: translateY(.12em); }
    to { opacity: 1; transform: none; }
  }
</style>
