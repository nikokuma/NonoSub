<script lang="ts">
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { colorWithOpacity, readableAccentTextColor, subtitleRowVisibility } from "./subtitlePresentation";

  let {
    segment,
    speaker,
    style,
    liveLabel,
    degraded = false,
  }: {
    segment: SubtitleSegment;
    speaker?: SpeakerProfile;
    style: StyleSettings;
    liveLabel?: string;
    degraded?: boolean;
  } = $props();

  const accent = $derived(speaker?.color ?? "#35c7e8");
  const accentText = $derived(readableAccentTextColor(accent));
  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const visibility = $derived(subtitleRowVisibility(segment, style.displayMode));
  const showSource = $derived(visibility.showSource);
  const showTranslation = $derived(visibility.showTranslation);
  const sourcePanel = $derived(colorWithOpacity("#f4f7fb", style.backgroundOpacity));
  const translationPanel = $derived(colorWithOpacity("#05091e", style.backgroundOpacity));
</script>

<div
  class="momento-card"
  class:provisional={segment.isProvisional}
  class:degraded
  class:live={Boolean(liveLabel)}
  class:single-row={!showSource || !showTranslation}
  style={`--speaker-accent:${accent};--speaker-accent-text:${accentText};--source-panel:${sourcePanel};--translation-panel:${translationPanel}`}
  aria-label={segment.isProvisional ? "Caption in progress" : "Right-click this caption to ask Nono"}
>
  {#if liveLabel}<span class="live-signal">{liveLabel}</span>{/if}
  <span class="cutout">
    {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
      <span class="speaker-tab"><span>{speaker?.displayName ?? "Live Audio"}</span></span>
    {/if}
    {#if showSource}
      <span class="source-card" class:waiting={!source}>{source || "Listening…"}</span>
    {/if}
    {#if showTranslation}
      <span class="translation-card" class:waiting={!translation}>
        <span>{translation || "Translation catching up…"}</span>
      </span>
    {/if}
  </span>
</div>

<style>
  .momento-card { width: 100%; min-width: 0; display: grid; justify-items: center; gap: .18em; border: 0; background: transparent; padding: .32em .62em .45em; color: white; text-align: center; cursor: pointer; }
  .momento-card:disabled { cursor: default; }
  .live-signal { font-family: "Avenir Next Condensed", Inter, sans-serif; font-size: max(8px, .3em); font-weight: 800; letter-spacing: .14em; color: var(--speaker-accent); text-shadow: 0 1px 7px #000; text-transform: uppercase; }
  .degraded .live-signal { color: #ff8ebf; }
  .cutout { position: relative; width: 100%; min-width: 0; display: grid; justify-items: center; padding: .72em .48em .2em; isolation: isolate; transform-origin: center; transition: transform 80ms ease; }
  .speaker-tab { position: absolute; z-index: 4; top: 0; left: max(.8em, 7%); max-width: 48%; padding: .16em .8em .2em; background: var(--speaker-accent); color: var(--speaker-accent-text); box-shadow: -.18em .16em 0 #f4f7fb; transform: skewX(-12deg); font-family: "Avenir Next Condensed", Inter, sans-serif; font-size: .38em; font-weight: 900; letter-spacing: .12em; line-height: 1; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .speaker-tab span { display: block; transform: skewX(12deg); }
  .source-card, .translation-card { position: relative; display: block; width: fit-content; max-width: 100%; overflow-wrap: anywhere; word-break: auto-phrase; text-wrap: balance; }
  .live .source-card, .live .translation-card { text-wrap: wrap; }
  .momento-card.live { max-height: 180px; overflow: visible; }
  .live .source-card, .live .translation-card { display: -webkit-box; -webkit-box-orient: vertical; overflow: hidden; }
  .live .source-card { line-clamp: var(--live-source-lines); -webkit-line-clamp: var(--live-source-lines); }
  .live .translation-card { line-clamp: var(--live-translation-lines); -webkit-line-clamp: var(--live-translation-lines); }
  .source-card { z-index: 2; padding: .2em .78em .26em 1em; background: var(--source-panel); color: #05091e; border: clamp(1px, .07em, 2px) solid #05091e; box-shadow: .2em .2em 0 #05091e; font-family: "Hiragino Sans", "Noto Sans CJK JP", "Noto Sans", Inter, sans-serif; font-size: 1em; font-weight: 900; line-height: 1.18; -webkit-text-stroke: clamp(1px, .04em, 1.5px) #f4f7fb; paint-order: stroke fill; }
  .source-card::before { content: ""; position: absolute; z-index: -1; left: -.48em; top: -.04em; bottom: -.04em; width: .58em; background: var(--speaker-accent); border: clamp(1px, .07em, 2px) solid #05091e; transform: skewX(-12deg); }
  .translation-card { z-index: 1; margin-top: .16em; margin-left: .72em; padding: .24em .82em .28em; background: var(--translation-panel); color: #fff; border-bottom: clamp(2px, .1em, 4px) solid var(--speaker-accent); box-shadow: .18em .16em 0 var(--speaker-accent); font-family: "Avenir Next Condensed", Inter, sans-serif; font-size: .68em; font-weight: 800; line-height: 1.18; text-shadow: 0 1px 2px #05091e, -1px 0 1px #05091e, 1px 0 1px #05091e; }
  .translation-card > span { display: block; }
  .single-row .cutout { width: fit-content; max-width: 100%; padding-top: .48em; }
  .single-row .source-card, .single-row .translation-card { margin-top: 0; margin-left: 0; }
  .waiting { font-style: italic; opacity: .82; }
  .provisional { opacity: .9; }
  .momento-card:hover:not(:disabled) .cutout { filter: drop-shadow(0 0 .24em color-mix(in srgb, var(--speaker-accent) 55%, transparent)); }
  .momento-card:active:not(:disabled) .cutout { transform: scale(.98); }
  .momento-card:active:not(:disabled) .source-card, .momento-card:active:not(:disabled) .translation-card { outline: clamp(2px, .09em, 4px) solid var(--speaker-accent); outline-offset: .08em; }
  .momento-card:focus-visible { outline: 0; }
  .momento-card:focus-visible .cutout { filter: drop-shadow(0 0 .35em var(--speaker-accent)); }
  .momento-card:focus-visible .source-card { outline: clamp(2px, .09em, 4px) solid var(--speaker-accent); outline-offset: .1em; }

  @media(prefers-reduced-motion:no-preference) { .momento-card:not(.live) .speaker-tab { animation: accent-snap 70ms ease-out both; } .momento-card:not(.live) .source-card { animation: source-wipe 160ms cubic-bezier(.2,.8,.2,1) both; } .momento-card:not(.live) .translation-card { animation: target-wipe 190ms 40ms cubic-bezier(.2,.8,.2,1) both; } .momento-card:not(.live) .translation-card > span { animation: target-text-in 150ms ease-out both; } }
  @media(prefers-reduced-motion:reduce) { .speaker-tab, .source-card, .translation-card, .translation-card > span { animation: none; } .cutout { transition: none; } }
  @keyframes accent-snap { from { opacity: 0; transform: translateX(-.35em) skewX(-12deg); } to { opacity: 1; transform: translateX(0) skewX(-12deg); } }
  @keyframes source-wipe { from { opacity: 0; clip-path: inset(0 100% 0 0); transform: translateY(.18em); } to { opacity: 1; clip-path: inset(0); transform: none; } }
  @keyframes target-wipe { from { opacity: 0; clip-path: inset(0 100% 0 0); } to { opacity: 1; clip-path: inset(0); } }
  @keyframes target-text-in { from { opacity: 0; transform: translateX(-.3em); } to { opacity: 1; transform: none; } }
</style>
