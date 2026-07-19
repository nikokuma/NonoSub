<script lang="ts">
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { colorWithOpacity } from "./subtitlePresentation";

  let {
    segment,
    speaker,
    style,
    liveLabel,
    degraded = false,
    onselect,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    liveLabel?: string;
    degraded?: boolean;
    onselect: (segment: SubtitleSegment) => void;
  } = $props();

  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const showSource = $derived(style.displayMode !== "translation");
  const showTranslation = $derived(style.displayMode !== "source");
  const panel = $derived(colorWithOpacity(style.falloutColors.panel, style.backgroundOpacity));
</script>

<button
  class="arcade-card"
  class:provisional={segment.isProvisional}
  class:degraded
  class:live={Boolean(liveLabel)}
  style={`--arcade-text:${style.falloutColors.text};--arcade-panel:${panel}`}
  onclick={(event) => event.detail === 0 && !segment.isProvisional && onselect(segment)}
  disabled={segment.isProvisional}
  aria-label={segment.isProvisional ? "Caption in progress" : "Right-click this caption to ask Nono"}
>
  <span class="dialogue-strip">
    <span class="metadata">
      {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
        <span>{speaker?.displayName ?? "Live Audio"}</span>
      {/if}
      {#if liveLabel}<span class="live-signal">{liveLabel}</span>{/if}
    </span>
    <span class="caption-stack">
      {#if showSource}
        <span class="caption source" class:waiting={!source}>{source || "Listening…"}</span>
      {/if}
      {#if showTranslation}
        <span class="caption translation" class:waiting={!translation}>{translation || "Translation catching up…"}</span>
      {/if}
    </span>
  </span>
</button>

<style>
  .arcade-card {
    width: 100%;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: .24em 0 .3em;
    color: var(--arcade-text);
    text-align: left;
    cursor: pointer;
    font-family: "Share Tech Mono", "JetBrains Mono", Menlo, "Courier New", monospace;
    font-synthesis: none;
    font-weight: 400;
  }

  .arcade-card:disabled { cursor: default; }
  .dialogue-strip {
    position: relative;
    display: grid;
    gap: .12em;
    width: 100%;
    min-width: 0;
    padding: .4em 2.5em .44em;
    background: linear-gradient(90deg, transparent, var(--arcade-panel) 9%, var(--arcade-panel) 91%, transparent);
    filter: drop-shadow(0 .08em .12em #000b);
    isolation: isolate;
  }

  .dialogue-strip::before {
    content: "";
    position: absolute;
    z-index: 0;
    inset: 0 7%;
    pointer-events: none;
    background: linear-gradient(180deg, #ffffff05, transparent 36%, #0000001f);
  }

  .dialogue-strip::after {
    content: "";
    position: absolute;
    inset: 0 7%;
    pointer-events: none;
    background: repeating-linear-gradient(0deg, transparent 0 3px, #ffffff0e 4px);
    mix-blend-mode: screen;
  }

  .caption-stack, .metadata { position: relative; z-index: 1; }
  .caption-stack { display: grid; gap: .08em; min-width: 0; }
  .caption { display: block; max-width: 100%; overflow-wrap: anywhere; word-break: auto-phrase; }
  .arcade-card.live { max-height: 180px; overflow: visible; }
  .live .caption { display: -webkit-box; -webkit-box-orient: vertical; overflow: hidden; }
  .live .source { line-clamp: var(--live-source-lines); -webkit-line-clamp: var(--live-source-lines); }
  .live .translation { line-clamp: var(--live-translation-lines); -webkit-line-clamp: var(--live-translation-lines); }
  .source, .translation, .metadata {
    font-family: inherit;
    font-weight: 400;
    -webkit-text-stroke: clamp(.4px, .02em, .8px) #1b1208;
    paint-order: stroke fill;
    text-shadow: 0 0 .065em currentColor, 0 0 .14em currentColor, .08em .1em .06em #000;
  }
  .source { font-size: .86em; line-height: 1.1; letter-spacing: .015em; }
  .translation { font-size: .65em; line-height: 1.14; letter-spacing: .02em; opacity: .94; }
  .metadata { display: flex; justify-content: space-between; min-height: .8em; font-size: max(9px, .3em); letter-spacing: .14em; line-height: 1; text-transform: uppercase; }
  .waiting { font-style: italic; opacity: .68; }
  .provisional { opacity: .84; }
  .degraded .live-signal { color: #ff9cc5; }
  .arcade-card:hover:not(:disabled) .dialogue-strip { filter: drop-shadow(0 .08em .16em #000) brightness(1.08); }
  .arcade-card:active:not(:disabled) .dialogue-strip { transform: scale(.988); }
  .arcade-card:focus-visible { outline: 0; }
  .arcade-card:focus-visible .dialogue-strip { outline: 1px solid var(--arcade-text); outline-offset: -.05em; }

  @media (prefers-reduced-motion: no-preference) {
    .arcade-card:not(.live) .caption-stack { animation: phosphor-in 120ms steps(3, end) both; }
  }

  @keyframes phosphor-in {
    from { opacity: 0; filter: blur(2px); }
    to { opacity: 1; filter: none; }
  }
</style>
