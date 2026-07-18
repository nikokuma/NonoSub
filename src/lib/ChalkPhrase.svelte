<script lang="ts">
  import type { ChalkPhrase } from "./contracts";

  const CHALK_COLORS = {
    white: "#f4f0df",
    baby_blue: "#9edbf2",
    yellow: "#f2d66b",
    pink: "#f39bc4",
  } as const;

  let {
    phrase,
    cueId,
    underlineProgress = 0,
    pointing = false,
    rigAvailable = true,
  }: {
    phrase: ChalkPhrase;
    cueId: string;
    underlineProgress?: number;
    pointing?: boolean;
    rigAvailable?: boolean;
  } = $props();

  const chalk = $derived(CHALK_COLORS[phrase.color]);
  const visibleUnderline = $derived(phrase.tailCue === "underline" && underlineProgress > 0);
</script>

<span
  class="chalk-phrase mark-{phrase.mark}"
  class:pointing={pointing}
  data-chalk-color={phrase.color}
  data-tail-cue={phrase.tailCue}
  data-cue-id={cueId}
  dir="auto"
  style={`--chalk:${chalk};--underline-progress:${Math.max(0, Math.min(1, underlineProgress))}`}
>
  {#if pointing && !rigAvailable}<span class="fallback-pointer" aria-hidden="true">›</span>{/if}
  <span class="chalk-text" data-text={phrase.text}>{phrase.text}</span>
  {#if phrase.mark === "strike"}<span class="strike-strokes" aria-hidden="true"></span>{/if}
  {#if visibleUnderline}
    <svg class="tail-underline" viewBox="0 0 100 8" preserveAspectRatio="none" aria-hidden="true">
      <path d="M2 4.8 C22 2.9 41 5.6 61 3.8 C76 2.5 88 4.4 98 3.2" />
      <path class="dust" d="M3 6 C24 4.5 43 6.4 62 4.9 C78 3.8 90 5.4 97 4.4" />
    </svg>
  {/if}
</span>

<style>
  .chalk-phrase{position:relative;display:inline-flex;align-items:baseline;max-width:100%;padding:.04em .12em;color:var(--chalk);isolation:isolate}
  .chalk-text{position:relative;z-index:1;display:inline-block;max-width:100%;color:var(--chalk);overflow-wrap:anywhere;text-shadow:.35px .25px 0 color-mix(in srgb,var(--chalk) 48%,transparent),0 0 5px color-mix(in srgb,var(--chalk) 13%,transparent);filter:url(#chalk-roughen)}
  .chalk-text::after{content:attr(data-text);position:absolute;inset:0;z-index:-1;color:var(--chalk);opacity:.12;transform:translate(.45px,.35px);filter:blur(.25px);pointer-events:none}
  .mark-box{margin:.08em .18em;padding:.1em .38em}
  .mark-box::before,.mark-box::after{content:"";position:absolute;inset:-.08em -.16em;border:1.4px solid color-mix(in srgb,var(--chalk) 88%,transparent);border-radius:45% 52% 46% 50% / 12% 15% 11% 14%;transform:rotate(-.35deg);pointer-events:none}
  .mark-box::after{inset:-.02em -.1em;opacity:.32;transform:rotate(.45deg)}
  .mark-bracket{margin-inline:.28em;padding-inline:.22em}
  .mark-bracket::before,.mark-bracket::after{position:absolute;top:-.08em;bottom:-.08em;width:.22em;color:var(--chalk);font-family:"Klee One",cursive;font-weight:600;line-height:1.1;opacity:.9}
  .mark-bracket::before{content:"[";left:-.2em}.mark-bracket::after{content:"]";right:-.2em}
  .strike-strokes{position:absolute;z-index:3;left:-.08em;right:-.08em;top:48%;height:4px;pointer-events:none;transform:rotate(-1.2deg)}
  .strike-strokes::before,.strike-strokes::after{content:"";position:absolute;left:0;right:0;height:1.5px;background:var(--chalk);border-radius:50%;box-shadow:0 0 2px color-mix(in srgb,var(--chalk) 40%,transparent)}
  .strike-strokes::after{top:2px;left:4%;right:-2%;opacity:.55;transform:rotate(1.4deg)}
  .tail-underline{position:absolute;z-index:4;left:-.06em;right:-.06em;bottom:-.34em;width:calc(100% + .12em);height:.5em;overflow:visible;pointer-events:none}
  .tail-underline path{fill:none;stroke:var(--chalk);stroke-width:1.55;stroke-linecap:round;stroke-dasharray:104;stroke-dashoffset:calc(104 * (1 - var(--underline-progress)));transition:stroke-dashoffset 40ms linear;filter:drop-shadow(0 0 1px color-mix(in srgb,var(--chalk) 30%,transparent))}
  .tail-underline .dust{stroke-width:.75;opacity:.32;stroke-dasharray:102;stroke-dashoffset:calc(102 * (1 - var(--underline-progress)))}
  :dir(rtl)>.tail-underline{transform:scaleX(-1)}
  .fallback-pointer{position:absolute;z-index:5;right:calc(100% + .18em);top:50%;color:var(--chalk);font-size:1.45em;line-height:1;transform:translateY(-52%);text-shadow:0 0 4px var(--chalk);animation:pointer-nod .55s ease-in-out infinite alternate}
  @keyframes pointer-nod{to{transform:translate(.16em,-52%)}}
  @media(prefers-reduced-motion:reduce){.tail-underline path{transition:none}.fallback-pointer{animation:none}}
</style>
