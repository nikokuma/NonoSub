<script lang="ts">
  import type { SpeakerProfile, StyleSettings, SubtitleSegment } from "./contracts";
  import { colorWithOpacity, readableAccentTextColor, subtitleRowVisibility } from "./subtitlePresentation";

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

  const colors = $derived(style.wiredColors);
  const accent = $derived(speaker?.color ?? colors.fallbackAccent);
  const accentText = $derived(readableAccentTextColor(accent));
  const source = $derived(segment.sourceText.trim());
  const translation = $derived(segment.translationText?.trim() ?? "");
  const visibility = $derived(subtitleRowVisibility(segment, style.displayMode));
  const showSource = $derived(visibility.showSource);
  const showTranslation = $derived(visibility.showTranslation);
  const timestamp = $derived(formatTime(segment.startMs));
  const status = $derived(liveLabel ?? (segment.isProvisional ? "BUFFERING" : "▶ ACTIVE"));
  const panel = $derived(colorWithOpacity(colors.panel, style.backgroundOpacity));
  const wash = $derived(colorWithOpacity(colors.wash, style.backgroundOpacity));

  function formatTime(milliseconds: number): string {
    const seconds = Math.max(0, milliseconds) / 1_000;
    const minutes = Math.floor(seconds / 60);
    return `${String(minutes).padStart(2, "0")}:${(seconds % 60).toFixed(1).padStart(4, "0")}`;
  }
</script>

<button
  class="cyberia-card"
  class:provisional={segment.isProvisional}
  class:degraded
  class:live={Boolean(liveLabel)}
  class:single-row={!showSource || !showTranslation}
  style={`--speaker-accent:${accent};--speaker-accent-text:${accentText};--cyberia-panel:${panel};--cyberia-wash:${wash};--cyberia-source:${colors.sourceText};--cyberia-translation:${colors.translationText};--cyberia-metadata:${colors.metadata}`}
  onclick={(event) => event.detail === 0 && !segment.isProvisional && onselect(segment)}
  disabled={segment.isProvisional}
  aria-label={segment.isProvisional ? "Caption in progress" : "Right-click this caption to ask Nono"}
>
  <span class="selected-shell">
    <span class="chrome-row">
      {#if style.showSpeakerNames && (speaker || segment.origin === "live")}
        <span class="speaker-tab">01 // {speaker?.displayName ?? "Live Audio"}</span>
      {/if}
      <span class="status" class:degraded>{status}</span>
    </span>
    <span class="selected-panel">
      <span class="active-wash">
        {#if showSource}
          <span class="source" class:waiting={!source}>{source || "Listening…"}</span>
        {/if}
        {#if showSource && showTranslation}<span class="divider" aria-hidden="true"></span>{/if}
        {#if showTranslation}
          <span class="translation" class:waiting={!translation}>
            <span>{translation || "Translation catching up…"}</span>
          </span>
        {/if}
        {#if !liveLabel}<span class="timestamp">{timestamp}</span>{/if}
      </span>
    </span>
  </span>
</button>

<style>
  .cyberia-card{width:100%;min-width:0;border:0;background:transparent;padding:.4em .62em .28em;color:white;text-align:left;cursor:pointer}.cyberia-card:disabled{cursor:default}.selected-shell{position:relative;width:100%;min-width:0;display:block;padding-top:.68em;isolation:isolate;transform-origin:center;transition:transform 80ms ease}.chrome-row{position:absolute;z-index:4;top:0;left:0;right:0;height:.82em;display:flex;align-items:start;justify-content:space-between;pointer-events:none}.speaker-tab{display:block;width:min(18%,158px);max-width:55%;padding:.18em .72em .2em;background:var(--speaker-accent);color:var(--speaker-accent-text);font-family:"JetBrains Mono",ui-monospace,monospace;font-size:.36em;font-weight:700;letter-spacing:.06em;line-height:1;text-transform:uppercase;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;box-shadow:0 0 .4em color-mix(in srgb,var(--speaker-accent) 30%,transparent)}.status{margin-left:auto;padding:.12em .35em;color:var(--speaker-accent);font-family:"JetBrains Mono",ui-monospace,monospace;font-size:max(8px,.31em);font-weight:700;letter-spacing:.06em;line-height:1.1;text-transform:uppercase;text-shadow:0 1px 7px #000}.status.degraded{filter:saturate(1.4) brightness(1.15)}.selected-panel{position:relative;display:block;width:100%;min-width:0;padding:.22em;background:var(--cyberia-panel);border:clamp(1px,.07em,2px) solid var(--speaker-accent);box-shadow:0 0 .6em color-mix(in srgb,var(--speaker-accent) 22%,transparent),inset 0 1px 0 color-mix(in srgb,var(--speaker-accent) 18%,transparent)}.active-wash{position:relative;display:grid;width:100%;min-width:0;padding:.3em .65em .24em;background:var(--cyberia-wash);overflow-wrap:anywhere;word-break:auto-phrase}.source,.translation{position:relative;z-index:1;display:block;max-width:100%;text-wrap:balance;-webkit-font-smoothing:none}.source{color:var(--cyberia-source);font-family:"DotGothic16",monospace;font-size:.94em;font-weight:400;line-height:1.18}.divider{display:block;width:100%;height:1px;margin:.08em 0;background:color-mix(in srgb,var(--cyberia-metadata) 48%,transparent)}.translation{padding-right:3.8em;color:var(--cyberia-translation);font-family:"JetBrains Mono",ui-monospace,monospace;font-size:.68em;font-weight:700;line-height:1.18}.translation>span{display:block}.timestamp{position:absolute;z-index:2;right:.5em;bottom:.3em;color:var(--speaker-accent);font-family:"JetBrains Mono",ui-monospace,monospace;font-size:.29em;font-weight:700;letter-spacing:.04em}.single-row .active-wash{min-height:2.1em;align-content:center}.single-row .translation{font-size:.78em;padding-right:3.8em}.waiting{font-style:italic;opacity:.72}.provisional{opacity:.88}.cyberia-card:hover:not(:disabled) .selected-panel{box-shadow:0 0 .8em color-mix(in srgb,var(--speaker-accent) 42%,transparent),inset 0 1px 0 color-mix(in srgb,var(--speaker-accent) 25%,transparent)}.cyberia-card:active:not(:disabled) .selected-shell{transform:scale(.985)}.cyberia-card:active:not(:disabled) .selected-panel,.cyberia-card:focus-visible .selected-panel{outline:clamp(2px,.09em,4px) solid var(--speaker-accent);outline-offset:.1em}.cyberia-card:focus-visible{outline:0}
  .source,.translation{text-shadow:0 1px 2px #05081c,-1px 0 1px #05081c,1px 0 1px #05081c}
  .live .source,.live .translation{text-wrap:wrap}
  .cyberia-card.live{max-height:180px;overflow:visible}
  .live .source,.live .translation{display:-webkit-box;-webkit-box-orient:vertical;overflow:hidden}
  .live .source{line-clamp:var(--live-source-lines);-webkit-line-clamp:var(--live-source-lines)}
  .live .translation{line-clamp:var(--live-translation-lines);-webkit-line-clamp:var(--live-translation-lines)}

  @media(prefers-reduced-motion:no-preference){.cyberia-card:not(.live) .speaker-tab{animation:tab-snap 70ms ease-out both}.cyberia-card:not(.live) .selected-panel{animation:panel-wipe 150ms cubic-bezier(.2,.8,.2,1) both}.cyberia-card:not(.live) .translation>span{animation:translation-in 180ms 35ms ease-out both}}
  @media(prefers-reduced-motion:reduce){.speaker-tab,.selected-panel,.translation>span{animation:none}.selected-shell{transition:none}}
  @keyframes tab-snap{from{opacity:0;transform:translateX(-.35em)}to{opacity:1;transform:none}}
  @keyframes panel-wipe{from{opacity:0;clip-path:inset(0 100% 0 0)}to{opacity:1;clip-path:inset(0)}}
  @keyframes translation-in{from{opacity:0;transform:translateX(-.3em)}to{opacity:1;transform:none}}
</style>
