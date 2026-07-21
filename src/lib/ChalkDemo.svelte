<script lang="ts">
  import type { BoardDemo, ChalkPhrase as ChalkPhraseData } from "./contracts";
  import ChalkPhrase from "./ChalkPhrase.svelte";
  import ChalkStepNumber from "./ChalkStepNumber.svelte";
  import { dominantChalkColor } from "./lesson";

  let {
    demo,
    underlineProgressByCue = {},
    pointCueId,
    rigAvailable = true,
    stepNumber,
  }: {
    demo: BoardDemo;
    underlineProgressByCue?: Record<string, number>;
    pointCueId?: string;
    rigAvailable?: boolean;
    stepNumber?: number;
  } = $props();

  function itemPhrase(index: number): ChalkPhraseData {
    const item = demo.items[index];
    return { text: item.label, color: item.color, mark: item.mark, tailCue: item.tailCue };
  }

  function cueProgress(cueId: string): number {
    return underlineProgressByCue[cueId] ?? 0;
  }

  const stepAccent = $derived(dominantChalkColor([
    ...demo.items.map((item) => item.color),
    ...(demo.result ? [demo.result.color] : []),
  ]));
</script>

{#if demo.kind !== "none" && demo.items.length > 0}
  <figure class="chalk-demo {demo.kind}">
    {#if stepNumber}<div class="step-number"><ChalkStepNumber number={stepNumber} label="Demonstration" accent={stepAccent} /></div>{/if}
    {#if demo.kind === "literal_to_natural"}
      <div class="transform">
        {#each demo.items as item, index}
          {@const cueId = `demo-${index}`}
          <div class="transform-line">
            <ChalkPhrase phrase={itemPhrase(index)} {cueId} underlineProgress={cueProgress(cueId)} pointing={pointCueId === cueId} {rigAvailable} />
            <small dir="auto">{item.detail}</small>
          </div>
          {#if index < demo.items.length - 1}<i aria-hidden="true">⇢</i>{/if}
        {/each}
      </div>
    {:else if demo.kind === "tone_scale"}
      <div class="scale">
        <svg viewBox="0 0 100 8" preserveAspectRatio="none" aria-hidden="true"><path d="M2 4 C22 2.8 42 5.1 61 3.8 C77 2.9 89 4.7 98 3.5" /></svg>
        {#each demo.items as item, index}
          {@const cueId = `demo-${index}`}
          <div class="scale-point">
            <span aria-hidden="true"></span>
            <ChalkPhrase phrase={itemPhrase(index)} {cueId} underlineProgress={cueProgress(cueId)} pointing={pointCueId === cueId} {rigAvailable} />
            <small dir="auto">{item.detail}</small>
          </div>
        {/each}
      </div>
      <div class="scale-labels"><span>DIRECT</span><span>GENTLE</span></div>
    {:else if demo.kind === "mini_dialogue"}
      <div class="dialogue">
        {#each demo.items as item, index}
          {@const cueId = `demo-${index}`}
          <div class="dialogue-line">
            <ChalkPhrase phrase={itemPhrase(index)} {cueId} underlineProgress={cueProgress(cueId)} pointing={pointCueId === cueId} {rigAvailable} />
            <span class="guide" aria-hidden="true"></span>
            <small dir="auto">{item.detail}</small>
          </div>
        {/each}
      </div>
    {:else}
      <div class="breakdown" class:omission={demo.kind === "omitted_meaning"}>
        {#each demo.items as item, index}
          {@const cueId = `demo-${index}`}
          <div class="breakdown-part">
            <ChalkPhrase phrase={itemPhrase(index)} {cueId} underlineProgress={cueProgress(cueId)} pointing={pointCueId === cueId} {rigAvailable} />
            <small dir="auto">{item.detail}</small>
          </div>
          {#if index < demo.items.length - 1}<i aria-hidden="true">＋</i>{/if}
        {/each}
      </div>
    {/if}

    {#if demo.result}
      <div class="result">
        <span aria-hidden="true">∴</span>
        <ChalkPhrase phrase={demo.result} cueId="result" underlineProgress={cueProgress("result")} pointing={pointCueId === "result"} {rigAvailable} />
      </div>
    {/if}
    {#if demo.caption}<figcaption dir="auto">{demo.caption}</figcaption>{/if}
  </figure>
{/if}

<style>
  .chalk-demo{position:relative;min-height:0;margin:calc(2px * var(--chalk-scale,1)) 0;padding-inline-start:calc(20px * var(--chalk-scale,1));display:grid;align-content:start;gap:calc(4px * var(--chalk-scale,1));font-family:"Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive;animation:draw-in .42s ease-out both;overflow:visible}
  .step-number{position:absolute;inset-inline-start:0;top:0}
  .chalk-demo :global(.chalk-phrase){font-size:clamp(8px,calc(13px * var(--chalk-scale,1)),21px)}
  .transform{display:grid;grid-template-columns:minmax(0,1fr) auto minmax(0,1fr);align-items:center;gap:calc(6px * var(--chalk-scale,1))}.transform-line{display:grid;justify-items:center;gap:calc(2px * var(--chalk-scale,1));min-width:0;text-align:center}.transform>i{font-style:normal;color:#f2d66b;font-size:clamp(10px,calc(16px * var(--chalk-scale,1)),26px);transform:rotate(-2deg);text-shadow:0 0 4px #f2d66b55}
  small{display:block;max-width:100%;color:#d8d3c3;font:500 clamp(5px,calc(8px * var(--chalk-scale,1)),13px)/1.3 "Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive;overflow-wrap:anywhere}
  .breakdown{display:flex;align-items:center;justify-content:center;gap:calc(5px * var(--chalk-scale,1));min-width:0}.breakdown-part{display:grid;justify-items:center;gap:calc(2px * var(--chalk-scale,1));min-width:0;text-align:center}.breakdown>i{font-style:normal;color:#f4f0df99;font-size:clamp(7px,calc(10px * var(--chalk-scale,1)),16px)}.breakdown.omission .breakdown-part:last-of-type{padding-inline:calc(5px * var(--chalk-scale,1));border-bottom:max(1px,calc(1px * var(--chalk-scale,1))) dashed #f39bc4;transform:translateY(1px)}
  .scale{position:relative;display:flex;justify-content:space-between;align-items:start;gap:6px;padding-top:8px}.scale>svg{position:absolute;left:4%;right:4%;top:1px;width:92%;height:8px;overflow:visible}.scale>svg path{fill:none;stroke:#f4f0df99;stroke-width:1.3;stroke-linecap:round}.scale-point{position:relative;display:grid;justify-items:center;gap:2px;flex:1;min-width:0;text-align:center}.scale-point>span{position:absolute;top:-8px;width:4px;height:4px;border-radius:50%;background:#f4f0df;box-shadow:0 0 4px #fff8}.scale-labels{display:flex;justify-content:space-between;color:#c6c0ae;font-size:6px;letter-spacing:.14em}
  .dialogue{display:grid;gap:3px}.dialogue-line{display:grid;grid-template-columns:auto minmax(12px,1fr) minmax(0,2fr);align-items:center;gap:6px}.dialogue-line .guide{height:1px;background:linear-gradient(90deg,#f4f0df99,#f4f0df22);transform:rotate(-.4deg)}.dialogue-line small{text-align:left}
  .result{display:flex;align-items:center;justify-content:center;gap:calc(7px * var(--chalk-scale,1));margin-top:calc(2px * var(--chalk-scale,1));padding-top:calc(4px * var(--chalk-scale,1));border-top:max(1px,calc(1px * var(--chalk-scale,1))) solid #f4f0df33}.result :global(.chalk-phrase){font-size:clamp(9px,calc(14px * var(--chalk-scale,1)),23px)}.result>span{color:#f2d66b;font-size:clamp(9px,calc(13px * var(--chalk-scale,1)),21px);transform:rotate(-4deg)}
  figcaption{text-align:center;color:#bcb5a3;font:500 clamp(5px,calc(6px * var(--chalk-scale,1)),10px)/1.3 "Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive}
  @keyframes draw-in{from{opacity:0;clip-path:inset(0 100% 0 0);filter:blur(1px)}to{opacity:1;clip-path:inset(0);filter:none}}
</style>
