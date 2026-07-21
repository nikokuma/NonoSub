<script lang="ts">
  import type { ChalkColor } from "./contracts";

  const CHALK_COLORS: Record<ChalkColor, string> = {
    white: "#f4f0df",
    baby_blue: "#9edbf2",
    yellow: "#f2d66b",
    pink: "#f39bc4",
  };

  let { number, label, accent = "white" }: { number: number; label: string; accent?: ChalkColor } = $props();
  const accentColor = $derived(CHALK_COLORS[accent]);
</script>

<span class="chalk-step" aria-label={`Step ${number}: ${label}`} style={`--step-accent:${accentColor}`}>
  <span aria-hidden="true">{number}</span>
</span>

<style>
  .chalk-step {
    position: relative;
    flex: 0 0 auto;
    width: 1.68em;
    height: 1.68em;
    display: inline-grid;
    place-items: center;
    isolation: isolate;
    color: #f4f0df;
    font: 600 clamp(7px, calc(11px * var(--chalk-scale, 1)), 18px)/1 "Klee One", "Hiragino Maru Gothic ProN", cursive;
    text-shadow: .35px .25px 0 #f4f0df66, 0 0 4px #f4f0df33;
    transform: rotate(-2deg);
    filter: url(#chalk-roughen);
  }

  .chalk-step > span {
    position: relative;
    z-index: 2;
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    line-height: 1;
    transform: translateY(-.075em);
  }

  .chalk-step::before,
  .chalk-step::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 0;
    border: max(1px, calc(1.3px * var(--chalk-scale, 1))) solid #f4f0df;
    border-radius: 49% 45% 52% 47% / 46% 52% 45% 51%;
    box-shadow: 0 0 3px #f4f0df44;
    pointer-events: none;
  }

  .chalk-step::after {
    inset: .09em -.04em -.04em .08em;
    border-color: var(--step-accent);
    opacity: .5;
    transform: rotate(5deg);
  }
</style>
