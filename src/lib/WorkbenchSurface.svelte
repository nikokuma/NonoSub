<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { ModelReadiness, SessionState, SpeakerProfile, SubtitlePreset, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS } from "./fixtures";
  import { formatTime, reduceSession } from "./session";
  import { initialSession, loadPreferences, savePreferences, subscribePreferences, subscribeSession } from "./runtime";

  const LANGUAGE_OPTIONS = [
    ["auto", "Auto-detect"], ["en", "English"], ["ja", "Japanese"], ["es", "Spanish"],
    ["fr", "French"], ["de", "German"], ["ko", "Korean"], ["zh", "Chinese"],
    ["pt", "Portuguese"], ["it", "Italian"], ["ru", "Russian"],
  ] as const;
  const PRESETS: SubtitlePreset[] = ["clean", "cinema", "contrast", "nono-pop", "manga", "retro"];

  let session = $state<SessionState>(FIXTURE_EVENTS.reduce(reduceSession, structuredClone(EMPTY_SESSION)));
  let preferences = $state(loadPreferences());
  let apiReady = $state(false);
  let liveReady = $state(false);
  let apiKey = $state("");
  let apiMessage = $state("");
  let mediaMessage = $state("Choose a local video or listen to another app.");
  let busy = $state(false);
  let onboarding = $state(false);
  let selectedId = $state<string | undefined>();
  let renaming = $state<string | undefined>();
  let renameValue = $state("");
  let hideWhenViewerReady = $state(false);

  $effect(() => {
    if (hideWhenViewerReady && session.mode === "file" && session.phase === "ready" && isTauri()) {
      hideWhenViewerReady = false;
      void invoke("hide_surface", { surface: "workbench" });
    }
  });

  onMount(() => {
    document.documentElement.dataset.surface = "workbench";
    const cleanup: Array<() => void> = [];
    void initialSession().then((value) => session = value);
    void subscribeSession(() => session, (value) => session = value).then((unlisten) => cleanup.push(unlisten));
    void subscribePreferences((value) => preferences = value).then((unlisten) => cleanup.push(unlisten));
    if (isTauri()) {
      void invoke<{ present: boolean }>("api_key_status").then((status) => {
        onboarding = !status.present;
        apiReady = status.present;
        liveReady = status.present;
      });
      void listen<string>("tray-action", ({ payload }) => {
        if (payload === "open_video") void chooseMedia();
        else if (payload === "start_live") void startLive();
        else if (payload === "languages") document.querySelector<HTMLElement>("#languages")?.focus();
        else if (payload.startsWith("level_")) {
          preferences.level = payload.slice(6) as typeof preferences.level;
          void persist();
        } else if (payload.startsWith("preset_")) {
          preferences.style.preset = payload.slice(7) as SubtitlePreset;
          void persist();
        }
      }).then((unlisten) => cleanup.push(unlisten));
    }
    return () => cleanup.forEach((stop) => stop());
  });

  async function persist() {
    await savePreferences(preferences);
  }

  async function chooseMedia() {
    if (!isTauri()) {
      mediaMessage = "The browser preview uses deterministic Japanese fixtures.";
      return;
    }
    const path = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mov"] }] });
    if (!path) return;
    busy = true;
    mediaMessage = "Opening video and preparing compatible playback…";
    try {
      const prepared = await invoke<{ path: string; file_name: string }>("prepare_media", { path });
      mediaMessage = `Decoding ${prepared.file_name} locally…`;
      const audio = await invoke<{ durationMs: number; chunkCount: number }>("prepare_audio");
      mediaMessage = `${audio.chunkCount} audio chunk${audio.chunkCount === 1 ? "" : "s"} ready · analyzing`;
      await persist();
      await invoke("open_surface", { surface: "viewer" });
      hideWhenViewerReady = true;
      void invoke("start_analysis", { languages: preferences.languages }).catch((error) => {
        hideWhenViewerReady = false;
        mediaMessage = errorMessage(error);
      });
    } catch (error) {
      mediaMessage = errorMessage(error);
    } finally {
      busy = false;
    }
  }

  async function startLive() {
    if (!isTauri()) {
      mediaMessage = "Live system audio is available in the macOS app.";
      return;
    }
    if (!liveReady) {
      mediaMessage = "This API project cannot access realtime translation yet.";
      return;
    }
    busy = true;
    mediaMessage = "Choose a browser, window, or display in Apple’s sharing picker…";
    try {
      await persist();
      await invoke("open_surface", { surface: "overlay" });
      await invoke("start_live_capture", { languages: preferences.languages, syncMode: preferences.sync.liveMode });
      mediaMessage = "Listening · live audio is sent to OpenAI and never saved.";
    } catch (error) {
      mediaMessage = errorMessage(error);
    } finally {
      busy = false;
    }
  }

  async function selectLine(segment: SubtitleSegment) {
    selectedId = segment.id;
    if (isTauri()) await invoke("select_lesson_segment", { segmentId: segment.id });
  }

  async function saveApiKey() {
    try {
      await invoke("save_api_key", { apiKey: apiKey.trim() });
      apiMessage = "Checking GPT‑5.6, diarized transcription, and realtime translation…";
      const readiness = await invoke<ModelReadiness>("validate_model_access");
      apiReady = readiness.file;
      liveReady = readiness.live;
      apiKey = "";
      preferences.onboardingComplete = true;
      await persist();
      onboarding = false;
      apiMessage = readiness.live ? "All models are ready." : "File mode is ready. Live translation is unavailable for this project.";
    } catch (error) { apiMessage = errorMessage(error); }
  }

  async function updateSpeaker(speaker: SpeakerProfile) {
    if (isTauri()) await invoke("update_speaker", { speaker });
    else session.speakers[speaker.id] = speaker;
  }

  function saveRename(speaker: SpeakerProfile) {
    const displayName = renameValue.trim();
    if (displayName) void updateSpeaker({ ...speaker, displayName });
    renaming = undefined;
  }

  function presetLabel(preset: SubtitlePreset) {
    return ({ clean: "Clean", cinema: "Cinema", contrast: "High Contrast", "nono-pop": "Nono Pop", manga: "Manga", retro: "Retro Pixel" })[preset];
  }

  function errorMessage(error: unknown): string {
    return typeof error === "object" && error && "message" in error ? String(error.message) : String(error);
  }
</script>

<div class="workbench-shell">
  <header>
    <div class="brand"><span>の</span><div><b>NonoSub</b><small>Understand why they said it that way.</small></div></div>
    <div class="model-state"><i class:ready={apiReady}></i>{apiReady ? "FILE MODE READY" : "API SETUP NEEDED"}<i class:ready={liveReady}></i>{liveReady ? "LIVE READY" : "LIVE UNAVAILABLE"}</div>
  </header>

  <main>
    <section class="command-deck">
      <div class="intro"><span class="eyebrow">NONOSUB / CONTROL DECK</span><h1>Watch normally.<br><em>Understand everything.</em></h1><p>NonoSub disappears into the subtitles, then brings Nono back when a line deserves an explanation.</p></div>
      <div class="launch-grid">
        <button class="launch file" onclick={chooseMedia} disabled={busy}><span>LOCAL VIDEO</span><b>Open MP4 or MOV</b><small>Diarized · contextual · submission-ready</small></button>
        <button class="launch live" onclick={startLive} disabled={busy || !liveReady}><span>LIVE CAPTIONS</span><b>Listen to another app</b><small>Apple system-audio picker · macOS 14+</small></button>
      </div>
      <div class="status-line"><i></i><span>{mediaMessage}</span><b>{session.phase.toUpperCase()}</b></div>

      <section class="language-panel" id="languages" tabindex="-1">
        <div><span class="eyebrow">LANGUAGE ROUTING</span><h2>Any language → any language</h2></div>
        <label>Source<select bind:value={preferences.languages.source} onchange={persist}>{#each LANGUAGE_OPTIONS as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <span class="arrow">→</span>
        <label>Subtitles<select bind:value={preferences.languages.target} onchange={() => { preferences.languages.explanation = preferences.languages.target; void persist(); }}>{#each LANGUAGE_OPTIONS.filter(([code]) => code !== "auto") as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <label class="explanation">Nono explains in<select bind:value={preferences.languages.explanation} onchange={persist}>{#each LANGUAGE_OPTIONS.filter(([code]) => code !== "auto") as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <label class="live-timing">Live timing<select bind:value={preferences.sync.liveMode} onchange={persist}><option value="coordinated">Coordinated bilingual</option><option value="fast_source">Fast source</option></select></label>
        <p class="language-note">File mode honors the source hint. Live Captions auto-detects the speaker and uses the selected subtitle language.</p>
      </section>

      <section class="styles">
        <div class="section-head"><div><span class="eyebrow">SUBTITLE SIGNAL</span><h2>Make every line land.</h2></div><label>Size <input type="range" min="18" max="44" bind:value={preferences.style.fontSize} onchange={persist} /></label></div>
        <div class="preset-row">{#each PRESETS as preset}<button class:chosen={preferences.style.preset === preset} onclick={() => { preferences.style.preset = preset; void persist(); }}>{presetLabel(preset)}</button>{/each}</div>
        <div class="fine-controls">
          <label>Background <input type="range" min="0" max="0.9" step="0.05" bind:value={preferences.style.backgroundOpacity} onchange={persist} /></label>
          <label>Display <select bind:value={preferences.style.displayMode} onchange={persist}><option value="both">Source + translation</option><option value="source">Source only</option><option value="translation">Translation only</option></select></label>
          <label>Effect <select bind:value={preferences.style.effect} onchange={persist}><option value="outline">Outline</option><option value="shadow">Shadow</option><option value="none">None</option></select></label>
          <label class="check"><input type="checkbox" bind:checked={preferences.style.showSpeakerNames} onchange={persist} /> Speaker names</label>
        </div>
      </section>
    </section>

    <aside class="transcript-rail">
      <div class="rail-head"><div><span class="eyebrow">CURRENT SESSION</span><h2>Transcript</h2></div><span>{session.mode === "live" && session.liveSync ? `LIVE · ${(session.liveSync.targetDelayMs / 1_000).toFixed(1)}s BEHIND` : `${session.segments.length} LINES`}</span></div>
      <div class="transcript">
        {#if session.segments.length === 0}<div class="empty">Your transcript will collect here while you watch.</div>{/if}
        {#each session.segments as segment}
          {@const speaker = segment.speakerId ? session.speakers[segment.speakerId] : undefined}
          <button class:current={selectedId === segment.id} onclick={() => selectLine(segment)} disabled={segment.isProvisional}>
            <time>{formatTime(segment.startMs)}</time><div><strong style={`color:${speaker?.color ?? "#79e9cb"}`}>{speaker?.displayName ?? (segment.origin === "live" ? "Live Audio" : "Speaker")}</strong><p>{segment.sourceText}</p><span>{segment.translationText ?? "Translating…"}</span></div>
          </button>
        {/each}
      </div>
      {#if Object.keys(session.speakers).length > 0}
        <div class="speakers"><span class="eyebrow">SPEAKERS / SESSION ONLY</span>{#each Object.values(session.speakers) as speaker}<div><input type="color" value={speaker.color} onchange={(event) => updateSpeaker({ ...speaker, color: event.currentTarget.value })} />{#if renaming === speaker.id}<input class="rename" bind:value={renameValue} onblur={() => saveRename(speaker)} onkeydown={(event) => event.key === "Enter" && saveRename(speaker)} />{:else}<button onclick={() => { renaming = speaker.id; renameValue = speaker.displayName; }}>{speaker.displayName} ✎</button>{/if}</div>{/each}</div>
      {/if}
      <button class="privacy" onclick={() => onboarding = true}>Privacy, API key & permissions</button>
    </aside>
  </main>
</div>

{#if onboarding}
  <div class="modal-backdrop"><section class="modal" role="dialog" aria-modal="true"><button class="close" onclick={() => apiReady && (onboarding = false)}>×</button><span class="nono-mark">の</span><span class="eyebrow">WELCOME TO NONOSUB</span><h1>Your media stays yours.</h1><p>Local video never leaves this Mac. Extracted audio goes to OpenAI transcription. For Live Captions, only audio from the source you select in Apple’s picker is streamed to OpenAI. Transcript context and questions go to GPT‑5.6.</p><div class="privacy-grid"><div><b>STAYS LOCAL</b><span>Video file, lesson history, preferences</span></div><div><b>SENT TO OPENAI</b><span>Extracted or selected audio, transcript context, questions</span></div></div><p class="fine">No accounts, analytics, NonoSub cloud, or saved transcript. Your API key lives in the operating-system credential vault and never enters a webview after saving.</p><label>OpenAI API key<input type="password" bind:value={apiKey} placeholder="sk-…" autocomplete="off" /></label><button class="save" onclick={saveApiKey}>Save securely & validate</button>{#if apiMessage}<p class="api-message">{apiMessage}</p>{/if}</section></div>
{/if}

<style>
  .workbench-shell{height:100vh;display:grid;grid-template-rows:64px 1fr;background:#080a0f;color:#f8f8fc}.workbench-shell:before{content:"";position:fixed;inset:64px auto 0 0;width:3px;background:linear-gradient(#5fe8e1,#ff70b7 48%,transparent 90%)}header{display:flex;align-items:center;justify-content:space-between;padding:0 24px;border-bottom:1px solid #222833;background:#0b0e14}.brand{display:flex;align-items:center;gap:11px}.brand>span,.nono-mark{width:37px;height:37px;display:grid;place-items:center;background:#ff70b7;color:white;border-radius:8px;font-weight:900;box-shadow:0 0 24px #ff70b744}.brand b{display:block}.brand small{display:block;color:#767e8c;font-size:9px}.model-state{display:flex;align-items:center;gap:7px;color:#707986;font-size:8px;letter-spacing:.12em}.model-state i{width:6px;height:6px;border-radius:50%;background:#6b3141;margin-left:10px}.model-state i.ready{background:#68e7c5;box-shadow:0 0 10px #68e7c5}main{min-height:0;display:grid;grid-template-columns:minmax(620px,1fr) 390px}.command-deck{overflow-y:auto;padding:44px clamp(30px,5vw,74px);border-right:1px solid #222833;background:radial-gradient(circle at 7% 0,#182032 0,transparent 34%),linear-gradient(135deg,#080b10,#0c0e15)}.intro{max-width:720px}.eyebrow{font-size:8px;font-weight:900;letter-spacing:.18em;color:#69ddd9}.intro h1{font-size:43px;line-height:1.02;letter-spacing:-1.9px;margin:12px 0}.intro h1 em{font-style:normal;color:#ff70b7}.intro p{max-width:590px;color:#9098a7;font-size:12px;line-height:1.65}.launch-grid{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin:30px 0 10px}.launch{min-height:120px;text-align:left;padding:20px;border:1px solid #29313e;background:#10151e;color:white;position:relative;overflow:hidden}.launch:after{content:"";position:absolute;right:-30px;bottom:-40px;width:100px;height:100px;border:1px solid #6ce9e433;transform:rotate(45deg)}.launch.live{border-color:#ff70b755;background:#17121c}.launch span{display:block;color:#69ddd9;font-size:8px;letter-spacing:.16em;font-weight:900}.launch.live span{color:#ff83c1}.launch b{display:block;font-size:18px;margin:9px 0}.launch small{color:#727c89}.launch:disabled{opacity:.45}.status-line{height:34px;display:flex;align-items:center;gap:9px;border:1px solid #222a35;padding:0 12px;background:#0c1017;color:#8b94a1;font-size:9px}.status-line i{width:5px;height:5px;background:#69ddd9;box-shadow:0 0 8px #69ddd9}.status-line b{margin-left:auto;color:#ff83c1;font-size:8px}.language-panel,.styles{margin-top:28px;border:1px solid #252d39;background:#0d1118;padding:20px}.language-panel{display:grid;grid-template-columns:1.2fr 1fr auto 1fr;gap:12px;align-items:end}.language-panel h2,.styles h2,.rail-head h2{font-size:17px;margin:4px 0}.language-panel .explanation{grid-column:2/4}.language-panel .live-timing{grid-column:4}.language-note{grid-column:2/5;margin:0;color:#606b79;font-size:8px;line-height:1.45}.language-panel label,.fine-controls label,.section-head label,.modal label{display:grid;gap:6px;color:#77818d;font-size:8px;text-transform:uppercase;letter-spacing:.1em}.language-panel select,.fine-controls select,.modal input{border:1px solid #2b3441;background:#090d13;color:white;padding:9px}.arrow{padding-bottom:10px;color:#ff70b7}.section-head{display:flex;justify-content:space-between;align-items:end}.section-head input{accent-color:#ff70b7}.preset-row{display:grid;grid-template-columns:repeat(6,1fr);gap:6px;margin:15px 0}.preset-row button{border:1px solid #29313c;background:#121720;padding:9px 4px;font-size:8px}.preset-row button.chosen{border-color:#ff70b7;color:#ff9acb;background:#251522}.fine-controls{display:grid;grid-template-columns:1fr 1fr 1fr auto;gap:10px;align-items:end}.fine-controls input[type=range]{accent-color:#ff70b7}.check{display:flex!important;align-items:center!important;padding-bottom:9px}.transcript-rail{min-height:0;display:grid;grid-template-rows:70px 1fr auto 42px;background:#0b0e14}.rail-head{display:flex;align-items:center;justify-content:space-between;padding:0 17px;border-bottom:1px solid #222833}.rail-head>span{color:#697381;font-size:8px}.transcript{min-height:0;overflow-y:auto;padding:8px}.transcript>button{width:100%;display:grid;grid-template-columns:42px 1fr;gap:8px;text-align:left;border:1px solid transparent;background:none;color:inherit;padding:10px;border-radius:4px}.transcript>button:hover,.transcript>button.current{background:#111720;border-color:#283342}.transcript time{font-size:9px;color:#697381}.transcript strong{font-size:8px;text-transform:uppercase;letter-spacing:.09em}.transcript p{font-size:11px;margin:4px 0}.transcript span{display:block;color:#858d99;font-size:9px;line-height:1.4}.empty{margin:30px 18px;color:#626b78;font-size:11px;line-height:1.6}.speakers{border-top:1px solid #222833;padding:12px 16px}.speakers>div{display:flex;align-items:center;gap:7px;margin-top:8px}.speakers input[type=color]{width:15px;height:15px;border:0;padding:0;background:none}.speakers button{border:0;background:none;font-size:9px}.rename{width:110px;background:#090d13;color:white;border:1px solid #ff70b7}.privacy{border:0;border-top:1px solid #222833;background:#0e1219;color:#7c8591;font-size:8px;text-transform:uppercase;letter-spacing:.12em}.modal-backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;background:#04060bd9;backdrop-filter:blur(12px)}.modal{width:min(560px,90vw);position:relative;padding:30px;background:#10151e;border:1px solid #354050;box-shadow:0 30px 100px #000}.modal .close{position:absolute;right:14px;top:10px;border:0;background:none;color:#7d8590;font-size:24px}.nono-mark{margin-bottom:15px}.modal h1{font-size:29px;margin:8px 0}.modal>p{color:#969daa;font-size:11px;line-height:1.6}.privacy-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin:18px 0}.privacy-grid div{display:grid;gap:5px;border:1px solid #293340;padding:13px}.privacy-grid b{color:#69ddd9;font-size:8px}.privacy-grid span{color:#818995;font-size:9px}.fine{background:#0a0e14;padding:12px}.modal label{margin-top:14px}.save{width:100%;margin-top:10px;border:0;background:linear-gradient(90deg,#db4e99,#795bea);padding:11px;color:white;font-weight:800}.api-message{color:#69ddd9!important}@media(max-width:1050px){main{grid-template-columns:1fr 340px}.command-deck{padding:30px}.preset-row{grid-template-columns:repeat(3,1fr)}.language-panel{grid-template-columns:1fr 1fr}.language-panel .explanation,.language-panel .live-timing,.language-note{grid-column:auto}.arrow{display:none}}
</style>
