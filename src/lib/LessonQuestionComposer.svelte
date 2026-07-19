<script lang="ts">
  import type { ExternalMediaControlResult, LessonSurfaceMode, StyleSettings, SubtitleSegment } from "./contracts";

  let {
    segment,
    style,
    mode,
    error = "",
    externalMediaControl = "not_requested",
    compact = false,
    onsubmit,
    oncancel,
    ondrag,
  }: {
    segment: SubtitleSegment;
    style: StyleSettings;
    mode: LessonSurfaceMode;
    error?: string;
    externalMediaControl?: ExternalMediaControlResult;
    compact?: boolean;
    onsubmit: (question: string) => void;
    oncancel?: () => void;
    ondrag?: (event: PointerEvent) => void;
  } = $props();

  const prompts = ["Break it down", "Translate this", "Cultural context"];
  let question = $state("");
  let input = $state<HTMLInputElement>();

  $effect(() => {
    if (mode === "compose") queueMicrotask(() => input?.focus());
  });

  function send(value = question) {
    const normalized = value.trim();
    if (!normalized || mode === "thinking") return;
    question = "";
    onsubmit(normalized);
  }

  const theme = $derived(style.preset === "momento" ? "momento" : style.preset === "wired" ? "wired" : style.preset === "fallout" ? "fallout" : "neutral");
  const accent = $derived(style.preset === "wired" ? style.wiredColors.fallbackAccent : style.preset === "fallout" ? style.falloutColors.text : style.preset === "yellow-drop" ? "#f5df26" : "#f06aaa");
</script>

<section class="composer {theme}" class:compact style={`--ask-accent:${accent};--wired-panel:${style.wiredColors.panel};--wired-wash:${style.wiredColors.wash};--fallout-panel:${style.falloutColors.panel};--fallout-text:${style.falloutColors.text}`} aria-label="Ask Nono">
  <button class="drag" aria-label="Move Ask Nono" onpointerdown={ondrag}></button>
  {#if oncancel}<button class="close" aria-label="Close Ask Nono" onclick={oncancel}>×</button>{/if}
  <header><span>NONO ASK</span><b>{mode === "thinking" ? "THINKING" : mode === "error" ? "TRY AGAIN" : compact ? "ASK ANOTHER" : "ABOUT THIS LINE"}</b></header>
  {#if !compact}
    <div class="selected-line">
      <p dir="auto">{segment.sourceText}</p>
      {#if segment.translationText}<small dir="auto">{segment.translationText}</small>{/if}
    </div>
  {/if}
  {#if mode === "thinking"}
    <div class="thinking-state" aria-live="polite"><i></i><i></i><i></i><span>Nono is organizing one clear teaching moment…</span></div>
  {:else}
    {#if mode === "error"}<div class="error" role="alert">{error || "Nono lost the chalk for a second. Please try again."}</div>{/if}
    <div class="prompt-row">{#each prompts as prompt}<button type="button" onclick={() => send(prompt)}>{prompt}</button>{/each}</div>
    <form onsubmit={(event) => { event.preventDefault(); send(); }}>
      <input bind:this={input} bind:value={question} placeholder="Ask about meaning, grammar, tone, or culture…" aria-label="Question for Nono" />
      <button class="send" disabled={!question.trim()}>Ask Nono <span>→</span></button>
    </form>
  {/if}
  {#if externalMediaControl === "permission_required" || externalMediaControl === "failed" || externalMediaControl === "unsupported"}
    <p class="media-notice">External media kept playing.</p>
  {/if}
</section>

<style>
  .composer{position:relative;width:100%;height:100%;box-sizing:border-box;display:grid;grid-template-columns:minmax(0,1fr);grid-template-rows:auto auto auto 1fr;gap:7px;padding:13px 15px 14px;color:#f9f8fb;background:#17191eee;border:1px solid #ffffff40;border-radius:16px;box-shadow:0 16px 44px #0009;overflow:hidden;isolation:isolate;font-family:Inter,sans-serif}
  .composer::before{content:"";position:absolute;inset:0;z-index:-1;background:linear-gradient(115deg,color-mix(in srgb,var(--ask-accent) 15%,transparent),transparent 45%)}
  header{display:flex;align-items:center;gap:8px;padding-right:28px;font-size:8px;letter-spacing:.14em}header span{color:var(--ask-accent);font-weight:900}header b{font-size:7px;color:#ffffff8c}.drag{position:absolute;z-index:3;left:42px;right:42px;top:0;height:12px;border:0;background:transparent}.drag:hover::after{content:"MOVE";position:absolute;top:2px;left:50%;transform:translateX(-50%);font:700 6px/1 monospace;color:#ffffff88}.close{position:absolute;z-index:4;right:9px;top:7px;width:24px;height:24px;border:0;border-radius:50%;background:#0005;color:#fff;font-size:17px}.selected-line{min-width:0;padding:6px 9px;background:#0005;border-left:3px solid var(--ask-accent)}.selected-line p,.selected-line small{margin:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.selected-line p{font-size:12px;font-weight:750}.selected-line small{display:block;margin-top:2px;color:#ffffffa8;font-size:9px}.prompt-row{display:flex;gap:5px;min-width:0}.prompt-row button{border:1px solid #ffffff32;border-radius:999px;background:#ffffff0b;color:#e8e8ed;padding:5px 9px;font-size:7px;white-space:nowrap}.prompt-row button:hover{border-color:var(--ask-accent);color:#fff}form{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:7px;align-self:end}input{min-width:0;border:1px solid #ffffff36;border-radius:9px;background:#080b10cc;color:#fff;padding:9px 11px;font-size:10px;outline:none;user-select:text;-webkit-user-select:text}input:focus{border-color:var(--ask-accent);box-shadow:0 0 0 2px color-mix(in srgb,var(--ask-accent) 20%,transparent)}.send{border:0;border-radius:9px;padding:0 13px;background:var(--ask-accent);color:#111;font-weight:900;font-size:8px}.send:disabled{opacity:.4}.send span{margin-left:5px}.thinking-state{grid-row:2/5;display:flex;align-items:center;justify-content:center;gap:6px;color:#ffffffbd;font-size:9px}.thinking-state i{width:6px;height:6px;border-radius:50%;background:var(--ask-accent);animation:pulse 1s ease-in-out infinite}.thinking-state i:nth-child(2){animation-delay:.13s}.thinking-state i:nth-child(3){animation-delay:.26s}.error{padding:5px 8px;border:1px solid #ff6f9f66;background:#47142699;color:#ffd3e2;font-size:8px}.media-notice{position:absolute;right:14px;bottom:2px;margin:0;color:#ffc989;font-size:7px}
  .momento{border-radius:4px 17px 5px 15px;background:#f5efe6;color:#18171b;border:3px solid #19171a;box-shadow:7px 7px 0 #19171a}.momento::after{content:"";position:absolute;z-index:-1;right:-18px;top:-25px;width:90px;height:65px;background:var(--ask-accent);transform:rotate(17deg)}.momento header b,.momento .selected-line small{color:#3a34388c}.momento .selected-line{background:#fff;border:2px solid #1a1719;border-left-width:7px;clip-path:polygon(0 0,98% 0,100% 82%,96% 100%,0 100%)}.momento .prompt-row button{border-color:#241f2288;background:#fff;color:#201b1e}.momento input{background:#fff;color:#18171b;border:2px solid #1b181a;border-radius:3px}.momento .close{background:#19171a}.momento .send{color:#fff}.momento header,.momento .selected-line,.momento form{font-family:"Avenir Next Condensed",Avenir,sans-serif}
  .wired{border-radius:2px;background:var(--wired-panel);border:1px solid var(--ask-accent);box-shadow:0 0 28px color-mix(in srgb,var(--ask-accent) 25%,transparent);font-family:"JetBrains Mono",monospace}.wired::after{content:"";position:absolute;inset:31px 8px 8px;z-index:-1;background:var(--wired-wash);opacity:.72}.wired header{border-bottom:1px solid var(--ask-accent);padding-bottom:5px}.wired .selected-line{background:#0003}.wired input{font-family:"JetBrains Mono",monospace}.wired .selected-line p{font-family:"DotGothic16",monospace;font-weight:400}
  .fallout{border-radius:1px;background:linear-gradient(90deg,transparent,var(--fallout-panel) 7%,var(--fallout-panel) 93%,transparent);border:0;color:var(--fallout-text);text-shadow:0 0 5px color-mix(in srgb,var(--fallout-text) 65%,transparent);font-family:"Share Tech Mono",monospace}.fallout::before{background:repeating-linear-gradient(0deg,transparent 0 3px,#fff 4px 4px);opacity:.025}.fallout header b,.fallout .selected-line small{color:color-mix(in srgb,var(--fallout-text) 65%,transparent)}.fallout .selected-line{border-color:var(--fallout-text);background:#0005}.fallout .prompt-row button,.fallout input{font-family:"Share Tech Mono",monospace;border-color:color-mix(in srgb,var(--fallout-text) 55%,transparent);color:var(--fallout-text)}.fallout .send{background:var(--fallout-text);color:#111}
  .compact{height:44px;display:block;padding:4px 34px 4px 5px;border-radius:10px;box-shadow:none}.compact header,.compact .prompt-row{display:none}.compact form{height:100%}.compact .thinking-state{height:100%}.compact .close{right:5px;top:7px;width:26px;height:26px}
  @keyframes pulse{0%,100%{opacity:.25;transform:translateY(0)}50%{opacity:1;transform:translateY(-3px)}}
  @media(prefers-reduced-motion:reduce){.thinking-state i{animation:none;opacity:.8}}
</style>
