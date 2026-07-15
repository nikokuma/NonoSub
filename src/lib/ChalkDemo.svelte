<script lang="ts">
  import type { BoardDemo } from "./contracts";

  let { demo }: { demo: BoardDemo } = $props();
</script>

{#if demo.kind !== "none" && demo.items.length > 0}
  <figure class="chalk-demo {demo.kind}">
    {#if demo.kind === "literal_to_natural"}
      <div class="transform">
        {#each demo.items as item, index}
          <div class="demo-item {item.accent}"><b>{item.label}</b><span>{item.detail}</span></div>
          {#if index < demo.items.length - 1}<i aria-hidden="true">↓</i>{/if}
        {/each}
      </div>
    {:else if demo.kind === "tone_scale"}
      <div class="scale"><div class="scale-line"></div>{#each demo.items as item}<div class="demo-item {item.accent}"><b>{item.label}</b><span>{item.detail}</span></div>{/each}</div>
      <div class="scale-labels"><span>DIRECT</span><span>GENTLE</span></div>
    {:else if demo.kind === "mini_dialogue"}
      <div class="dialogue">{#each demo.items as item}<div class="demo-item {item.accent}"><b>{item.label}</b><span>{item.detail}</span></div>{/each}</div>
    {:else}
      <div class="tokens">{#each demo.items as item}<div class="demo-item {item.accent}"><b>{item.label}</b><span>{item.detail}</span></div>{/each}</div>
    {/if}

    {#if demo.result}<div class="result"><span>SO…</span><b>{demo.result}</b></div>{/if}
    {#if demo.caption}<figcaption>{demo.caption}</figcaption>{/if}
  </figure>
{/if}

<style>
  .chalk-demo{margin:12px 0 8px;padding:12px;border:1px solid #efe5c044;background:#081e153b;border-radius:4px;animation:draw-in .55s ease-out both}.tokens{display:flex;align-items:stretch;justify-content:center;gap:7px;flex-wrap:wrap}.demo-item{position:relative;display:grid;gap:3px;min-width:82px;padding:8px 9px;text-align:center;border:1px solid #e8dfc055;border-radius:3px;background:#ffffff08}.demo-item b{font-size:12px;line-height:1.25}.demo-item span{font-family:Inter,sans-serif;font-size:7px;line-height:1.35;color:#d4cdbb}.demo-item.source{border-color:#7be4db99}.demo-item.source b{color:#a7f2ec}.demo-item.meaning{border-color:#f3d67588}.demo-item.meaning b{color:#f6e4a6}.demo-item.missing{border-style:dashed;border-color:#ff91c5aa;background:#52223b30}.demo-item.missing b{color:#ffb6d8}.demo-item.tone{border-color:#c7a8ef88}.demo-item.tone b{color:#d9c4f5}.transform{display:grid;justify-items:center;gap:4px}.transform .demo-item{width:min(100%,260px)}.transform i{font-style:normal;font-size:18px;color:#f3d675}.scale{position:relative;display:flex;justify-content:space-between;align-items:flex-start;gap:7px;padding-top:7px}.scale-line{position:absolute;left:8%;right:8%;top:2px;height:2px;background:linear-gradient(90deg,#ff91c5,#f3d675,#7be4db)}.scale .demo-item{flex:1;min-width:0}.scale-labels{display:flex;justify-content:space-between;margin-top:5px;font-family:Inter,sans-serif;font-size:6px;letter-spacing:.15em;color:#aea790}.dialogue{display:grid;gap:7px}.dialogue .demo-item{text-align:left;grid-template-columns:70px 1fr}.dialogue .demo-item b{padding-right:8px;border-right:1px solid #ffffff25}.result{display:flex;align-items:center;justify-content:center;gap:9px;margin-top:10px;padding-top:9px;border-top:1px dashed #e8dfc044}.result span{font-family:Inter,sans-serif;font-size:6px;letter-spacing:.16em;color:#f3d675}.result b{font-size:12px;text-align:center;color:#fff5d9}figcaption{margin-top:7px;text-align:center;font-family:Inter,sans-serif;font-size:7px;line-height:1.4;color:#aead9d}@keyframes draw-in{from{opacity:0;clip-path:inset(0 100% 0 0);filter:blur(1px)}to{opacity:1;clip-path:inset(0);filter:none}}
</style>
