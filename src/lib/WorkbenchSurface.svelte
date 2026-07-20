<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { ModelReadiness, SessionState, SpeakerProfile, SubtitlePreset, SubtitleSegment } from "./contracts";
  import { formatTime } from "./session";
  import { applyPreferenceAction, preferencePatchBetween } from "./preferences";
  import { initialSession, loadPreferences, maintainSubscription, savePreferencePatch, subscribePreferences, subscribeSession } from "./runtime";
  import type { PreferencePatch } from "./preferences";
  import { errorMessage, startFileSession, startLiveSession } from "./sessionLaunch";
  import SubtitleStylePreview from "./SubtitleStylePreview.svelte";
  import { earlierTranscriptCount, TRANSCRIPT_PAGE_SIZE, visibleTranscriptPage } from "./transcriptPaging";

  const LANGUAGE_OPTIONS = [
    ["auto", "Auto-detect"], ["en", "English"], ["ja", "Japanese"], ["es", "Spanish"],
    ["fr", "French"], ["de", "German"], ["ko", "Korean"], ["zh", "Chinese"],
    ["pt", "Portuguese"], ["it", "Italian"], ["ru", "Russian"],
  ] as const;
  const PRESETS: SubtitlePreset[] = ["clean", "classic-outline", "yellow-drop", "fallout", "momento", "wired"];

  let session = $state<SessionState>(initialSession());
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
  let surfacedFatalError = $state<string>();
  let retryingTranslationId = $state<string>();
  let transcriptVisibleCount = $state(TRANSCRIPT_PAGE_SIZE);
  let transcriptSessionId = $state("");
  let visibleTranscriptSegments = $derived(visibleTranscriptPage(session.segments, transcriptVisibleCount));
  let hiddenTranscriptCount = $derived(earlierTranscriptCount(session.segments.length, transcriptVisibleCount));

  $effect(() => {
    if (session.sessionId !== transcriptSessionId) {
      transcriptSessionId = session.sessionId;
      transcriptVisibleCount = TRANSCRIPT_PAGE_SIZE;
    }
  });

  $effect(() => {
    if (hideWhenViewerReady && session.mode === "file" && session.phase === "ready" && isTauri()) {
      hideWhenViewerReady = false;
      void invoke("hide_surface", { surface: "workbench" });
    }
  });

  $effect(() => {
    if (!isTauri() || !session.fatalError || session.fatalError === surfacedFatalError) return;
    surfacedFatalError = session.fatalError;
    mediaMessage = session.fatalError;
    void invoke("open_surface", { surface: "workbench" });
  });

  onMount(() => {
    document.documentElement.dataset.surface = "workbench";
    void Promise.all([
      document.fonts.load('400 24px "DotGothic16"', "行きたくないわけじゃないんですけど、今日はちょっと予定があって難しいかもしれません。"),
      document.fonts.load('700 18px "JetBrains Mono"', "It’s not that I don’t want to go, but I already have plans today."),
      document.fonts.load('400 18px "Share Tech Mono"', "FALLOUT SUBTITLE PREVIEW"),
    ]);
    const cleanup: Array<() => void> = [];
    cleanup.push(maintainSubscription(
      () => subscribeSession((value) => session = value),
      (message) => { if (message) mediaMessage = message; },
    ));
    cleanup.push(maintainSubscription(
      () => subscribePreferences((value) => preferences = value),
      (message) => { if (message) mediaMessage = message; },
    ));
    if (isTauri()) {
      void invoke<{ present: boolean }>("api_key_status").then((status) => {
        onboarding = !status.present;
        apiReady = status.present;
        liveReady = status.present;
      });
      void listen<string>("tray-action", ({ payload }) => {
        if (payload === "languages") document.querySelector<HTMLElement>("#languages")?.focus();
        const updated = applyPreferenceAction(preferences, payload);
        if (updated) {
          const patch = preferencePatchBetween(preferences, updated);
          preferences = updated;
          if (payload === "external_pause_on") void invoke("request_media_key_permission").catch(() => false);
          void persist(patch);
        }
      }).then((unlisten) => cleanup.push(unlisten));
    }
    return () => cleanup.forEach((stop) => stop());
  });

  async function persist(patch: PreferencePatch) {
    preferences = await savePreferencePatch(patch);
  }

  async function updateExternalPause() {
    if (preferences.experimentalExternalPause && isTauri()) {
      await invoke("request_media_key_permission").catch(() => false);
    }
    await persist({ experimentalExternalPause: preferences.experimentalExternalPause });
  }

  async function chooseMedia() {
    if (!isTauri()) {
      mediaMessage = "The browser preview uses deterministic Japanese fixtures.";
      return;
    }
    const path = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mov"] }] });
    if (!path) return;
    busy = true;
    try {
      await startFileSession(path, preferences, {
        status: (message) => mediaMessage = message,
        analysisError: (message) => {
          hideWhenViewerReady = false;
          mediaMessage = message;
        },
      });
      hideWhenViewerReady = true;
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
      await startLiveSession(preferences, { status: (message) => mediaMessage = message });
    } catch (error) {
      mediaMessage = errorMessage(error);
    } finally {
      busy = false;
    }
  }

  async function selectLine(segment: SubtitleSegment) {
    selectedId = segment.id;
    if (isTauri()) await invoke("open_lesson_composer", {
      segmentId: segment.id,
      sourceSurface: "workbench",
      cursorX: window.innerWidth * .72,
      cursorY: window.innerHeight * .35,
      experimentalExternalPause: false,
    });
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
      await persist({ onboardingComplete: true });
      onboarding = false;
      apiMessage = readiness.live ? "All models are ready." : "File mode is ready. Live translation is unavailable for this project.";
      if (isTauri()) window.setTimeout(() => void invoke("hide_surface", { surface: "workbench" }), 700);
    } catch (error) { apiMessage = errorMessage(error); }
  }

  async function updateSpeaker(speaker: SpeakerProfile) {
    if (isTauri()) await invoke("update_speaker", { sessionId: session.sessionId, speaker });
    else session.speakers[speaker.id] = speaker;
  }

  async function retryTranslation(segment: SubtitleSegment) {
    if (!isTauri() || retryingTranslationId) return;
    retryingTranslationId = segment.id;
    mediaMessage = "Retrying this subtitle translation…";
    try {
      await invoke("retry_translation", { segmentId: segment.id });
      mediaMessage = "Translation recovered.";
    } catch (error) {
      mediaMessage = errorMessage(error);
    } finally {
      retryingTranslationId = undefined;
    }
  }

  function saveRename(speaker: SpeakerProfile) {
    const displayName = renameValue.trim();
    if (displayName) void updateSpeaker({ ...speaker, displayName });
    renaming = undefined;
  }

  function presetLabel(preset: SubtitlePreset) {
    return ({ clean: "Clean", "classic-outline": "Classic Outline", "yellow-drop": "Yellow Drop", fallout: "Fallout", momento: "Momento", wired: "Wired" })[preset];
  }

</script>

<div class="workbench-shell">
  <header>
    <div class="brand"><span>の</span><div><b>NonoSub</b><small>Understand why they said it that way.</small></div></div>
    <div class="model-state"><i class:ready={apiReady}></i>{apiReady ? "FILE MODE READY" : "API SETUP NEEDED"}<i class:ready={liveReady}></i>{liveReady ? "LIVE READY" : "LIVE UNAVAILABLE"}</div>
  </header>

  <main>
    <section class="command-deck">
      <div class="intro"><span class="eyebrow">NONOSUB / SETTINGS & TRANSCRIPT</span><h1>Configure quietly.<br><em>Watch without an app.</em></h1><p>This diagnostic view holds setup, styling, language routing, transcript inspection, and recovery. Normal watching stays in the viewer or floating subtitles.</p></div>
      <div class="launch-grid">
        <button class="launch file" onclick={chooseMedia} disabled={busy}><span>LOCAL VIDEO</span><b>Open MP4 or MOV</b><small>Diarized · contextual · submission-ready</small></button>
        <button class="launch live" onclick={startLive} disabled={busy || !liveReady}><span>LIVE CAPTIONS</span><b>Listen to another app</b><small>Apple system-audio picker · macOS 14+</small></button>
      </div>
      <div class="status-line"><i></i><span>{mediaMessage}</span><b>{session.phase.toUpperCase()}</b></div>

      <section class="language-panel" id="languages" tabindex="-1">
        <div><span class="eyebrow">LANGUAGE ROUTING</span><h2>Any language → any language</h2></div>
        <label>Source<select bind:value={preferences.languages.source} onchange={() => void persist({ languages: { source: preferences.languages.source } })}>{#each LANGUAGE_OPTIONS as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <span class="arrow">→</span>
        <label>Subtitles<select bind:value={preferences.languages.target} disabled={preferences.processingMode === "original_only"} onchange={() => { preferences.languages.explanation = preferences.languages.target; void persist({ languages: { target: preferences.languages.target, explanation: preferences.languages.explanation } }); }}>{#each LANGUAGE_OPTIONS.filter(([code]) => code !== "auto") as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <label class="processing">Caption processing<select bind:value={preferences.processingMode} onchange={() => void persist({ processingMode: preferences.processingMode })}><option value="translated">Translated</option><option value="original_only">Original only (fast)</option></select></label>
        <label class="explanation">Nono explains in<select bind:value={preferences.languages.explanation} onchange={() => void persist({ languages: { explanation: preferences.languages.explanation } })}>{#each LANGUAGE_OPTIONS.filter(([code]) => code !== "auto") as language}<option value={language[0]}>{language[1]}</option>{/each}</select></label>
        <label class="live-timing">Live timing<select bind:value={preferences.sync.liveMode} disabled={preferences.processingMode === "original_only"} onchange={() => void persist({ sync: { liveMode: preferences.sync.liveMode } })}><option value="coordinated">Coordinated bilingual</option><option value="fast_source">Fast source</option></select></label>
        <p class="language-note">Display changes what you see. Caption processing controls whether NonoSub requests a translation. Original-only subtitles remain clickable for translation, language, and culture questions.</p>
        <label class="external-pause"><input type="checkbox" bind:checked={preferences.experimentalExternalPause} onchange={updateExternalPause} /> Experimental: pause external media when Ask Nono opens</label>
        <p class="external-note">Best effort. macOS may require Accessibility permission and does not guarantee which media app receives the play/pause key.</p>
        {#if session.mode && session.phase !== "complete" && session.processingMode !== preferences.processingMode}<p class="next-session">Processing change applies to the next session.</p>{/if}
      </section>

      <section class="styles">
        <div class="section-head"><div><span class="eyebrow">SUBTITLE SIGNAL</span><h2>Make every line land.</h2></div><label>Size <input type="range" min="18" max="44" bind:value={preferences.style.fontSize} onchange={() => void persist({ style: { fontSize: preferences.style.fontSize } })} /></label></div>
        <div class="preset-row">{#each PRESETS as preset}<button class:chosen={preferences.style.preset === preset} onclick={() => { preferences.style.preset = preset; void persist({ style: { preset } }); }}>{presetLabel(preset)}</button>{/each}</div>
        <SubtitleStylePreview style={preferences.style} processingMode={preferences.processingMode} />
        <div class="fine-controls">
          <label>Background <input type="range" min="0" max="0.9" step="0.05" bind:value={preferences.style.backgroundOpacity} onchange={() => void persist({ style: { backgroundOpacity: preferences.style.backgroundOpacity } })} /></label>
          <label>Display {#if preferences.processingMode === "original_only"}<select disabled><option>Source only</option></select>{:else}<select bind:value={preferences.style.displayMode} onchange={() => void persist({ style: { displayMode: preferences.style.displayMode } })}><option value="both">Source + translation</option><option value="source">Source only</option><option value="translation">Translation only</option></select>{/if}</label>
          <label>Effect <select bind:value={preferences.style.effect} onchange={() => void persist({ style: { effect: preferences.style.effect } })}><option value="outline">Outline</option><option value="shadow">Shadow</option><option value="none">None</option></select></label>
          <label class="check"><input type="checkbox" bind:checked={preferences.style.showSpeakerNames} onchange={() => void persist({ style: { showSpeakerNames: preferences.style.showSpeakerNames } })} /> Speaker names</label>
        </div>
        {#if preferences.style.preset === "wired"}
          <div class="cyberia-colors" aria-label="Wired colors">
            <span>WIRED COLORS</span>
            <label>Panel <input type="color" bind:value={preferences.style.wiredColors.panel} onchange={() => void persist({ style: { wiredColors: { panel: preferences.style.wiredColors.panel } } })} /></label>
            <label>Selected wash <input type="color" bind:value={preferences.style.wiredColors.wash} onchange={() => void persist({ style: { wiredColors: { wash: preferences.style.wiredColors.wash } } })} /></label>
            <label>Source text <input type="color" bind:value={preferences.style.wiredColors.sourceText} onchange={() => void persist({ style: { wiredColors: { sourceText: preferences.style.wiredColors.sourceText } } })} /></label>
            <label>Translation <input type="color" bind:value={preferences.style.wiredColors.translationText} onchange={() => void persist({ style: { wiredColors: { translationText: preferences.style.wiredColors.translationText } } })} /></label>
            <label>Metadata <input type="color" bind:value={preferences.style.wiredColors.metadata} onchange={() => void persist({ style: { wiredColors: { metadata: preferences.style.wiredColors.metadata } } })} /></label>
            <label>Fallback speaker <input type="color" bind:value={preferences.style.wiredColors.fallbackAccent} onchange={() => void persist({ style: { wiredColors: { fallbackAccent: preferences.style.wiredColors.fallbackAccent } } })} /></label>
          </div>
        {:else if preferences.style.preset === "fallout"}
          <div class="cyberia-colors arcade-colors" aria-label="Fallout colors">
            <span>FALLOUT COLORS</span>
            <label>Terminal text <input type="color" bind:value={preferences.style.falloutColors.text} onchange={() => void persist({ style: { falloutColors: { text: preferences.style.falloutColors.text } } })} /></label>
            <label>Dialogue strip <input type="color" bind:value={preferences.style.falloutColors.panel} onchange={() => void persist({ style: { falloutColors: { panel: preferences.style.falloutColors.panel } } })} /></label>
          </div>
        {/if}
      </section>
    </section>

    <aside class="transcript-rail">
      <div class="rail-head"><div><span class="eyebrow">CURRENT SESSION</span><h2>Transcript</h2></div><span>{session.mode === "live" && session.processingMode === "original_only" ? "LIVE · ORIGINAL" : session.mode === "live" && session.liveSync ? `LIVE · ${(session.liveSync.targetDelayMs / 1_000).toFixed(1)}s BEHIND` : `${session.segments.length} LINES`}</span></div>
      <div class="transcript">
        {#if session.segments.length === 0}<div class="empty">Your transcript will collect here while you watch.</div>{/if}
        {#if hiddenTranscriptCount > 0}<button class="load-earlier" onclick={() => transcriptVisibleCount += TRANSCRIPT_PAGE_SIZE}>Load earlier · {hiddenTranscriptCount} hidden</button>{/if}
        {#each visibleTranscriptSegments as segment}
          {@const speaker = segment.speakerId ? session.speakers[segment.speakerId] : undefined}
          <div class="transcript-entry" class:current={selectedId === segment.id}>
            <button class="transcript-line" onclick={() => selectLine(segment)} disabled={segment.isProvisional}>
              <time>{formatTime(segment.startMs)}</time><div><strong style={`color:${speaker?.color ?? "#79e9cb"}`}>{speaker?.displayName ?? (segment.origin === "live" ? "Live Audio" : "Speaker")}</strong><p>{segment.sourceText}</p>{#if session.processingMode !== "original_only"}<span>{segment.translationText ?? (segment.translationStatus === "failed" ? "Translation unavailable · source shown" : "Translating…")}</span>{/if}</div>
            </button>
            {#if session.mode === "file" && session.processingMode === "translated" && segment.translationStatus === "failed"}
              <button class="retry-translation" onclick={() => retryTranslation(segment)} disabled={Boolean(retryingTranslationId)}>{retryingTranslationId === segment.id ? "Retrying…" : "Retry translation"}</button>
            {/if}
          </div>
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
  .workbench-shell{height:100vh;display:grid;grid-template-rows:64px 1fr;background:#080a0f;color:#f8f8fc}.workbench-shell:before{content:"";position:fixed;inset:64px auto 0 0;width:3px;background:linear-gradient(#5fe8e1,#ff70b7 48%,transparent 90%)}header{display:flex;align-items:center;justify-content:space-between;padding:0 24px;border-bottom:1px solid #222833;background:#0b0e14}.brand{display:flex;align-items:center;gap:11px}.brand>span,.nono-mark{width:37px;height:37px;display:grid;place-items:center;background:#ff70b7;color:white;border-radius:8px;font-weight:900;box-shadow:0 0 24px #ff70b744}.brand b{display:block}.brand small{display:block;color:#767e8c;font-size:9px}.model-state{display:flex;align-items:center;gap:7px;color:#707986;font-size:8px;letter-spacing:.12em}.model-state i{width:6px;height:6px;border-radius:50%;background:#6b3141;margin-left:10px}.model-state i.ready{background:#68e7c5;box-shadow:0 0 10px #68e7c5}main{min-height:0;display:grid;grid-template-columns:minmax(620px,1fr) 390px}.command-deck{overflow-y:auto;padding:44px clamp(30px,5vw,74px);border-right:1px solid #222833;background:radial-gradient(circle at 7% 0,#182032 0,transparent 34%),linear-gradient(135deg,#080b10,#0c0e15)}.intro{max-width:720px}.eyebrow{font-size:8px;font-weight:900;letter-spacing:.18em;color:#69ddd9}.intro h1{font-size:43px;line-height:1.02;letter-spacing:-1.9px;margin:12px 0}.intro h1 em{font-style:normal;color:#ff70b7}.intro p{max-width:590px;color:#9098a7;font-size:12px;line-height:1.65}.launch-grid{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin:30px 0 10px}.launch{min-height:120px;text-align:left;padding:20px;border:1px solid #29313e;background:#10151e;color:white;position:relative;overflow:hidden}.launch:after{content:"";position:absolute;right:-30px;bottom:-40px;width:100px;height:100px;border:1px solid #6ce9e433;transform:rotate(45deg)}.launch.live{border-color:#ff70b755;background:#17121c}.launch span{display:block;color:#69ddd9;font-size:8px;letter-spacing:.16em;font-weight:900}.launch.live span{color:#ff83c1}.launch b{display:block;font-size:18px;margin:9px 0}.launch small{color:#727c89}.launch:disabled{opacity:.45}.status-line{height:34px;display:flex;align-items:center;gap:9px;border:1px solid #222a35;padding:0 12px;background:#0c1017;color:#8b94a1;font-size:9px}.status-line i{width:5px;height:5px;background:#69ddd9;box-shadow:0 0 8px #69ddd9}.status-line b{margin-left:auto;color:#ff83c1;font-size:8px}.language-panel,.styles{margin-top:28px;border:1px solid #252d39;background:#0d1118;padding:20px}.language-panel{display:grid;grid-template-columns:1.2fr 1fr auto 1fr;gap:12px;align-items:end}.language-panel h2,.styles h2,.rail-head h2{font-size:17px;margin:4px 0}.language-panel .explanation{grid-column:2/4}.language-panel .live-timing{grid-column:4}.language-note{grid-column:2/5;margin:0;color:#606b79;font-size:8px;line-height:1.45}.language-panel label,.fine-controls label,.section-head label,.modal label{display:grid;gap:6px;color:#77818d;font-size:8px;text-transform:uppercase;letter-spacing:.1em}.language-panel select,.fine-controls select,.modal input{border:1px solid #2b3441;background:#090d13;color:white;padding:9px}.arrow{padding-bottom:10px;color:#ff70b7}.section-head{display:flex;justify-content:space-between;align-items:end}.section-head input{accent-color:#ff70b7}.preset-row{display:grid;grid-template-columns:repeat(auto-fit,minmax(92px,1fr));gap:6px;margin:15px 0}.preset-row button{border:1px solid #29313c;background:#121720;padding:9px 4px;font-size:8px}.preset-row button.chosen{border-color:#ff70b7;color:#ff9acb;background:#251522}.fine-controls{display:grid;grid-template-columns:1fr 1fr 1fr auto;gap:10px;align-items:end}.fine-controls input[type=range]{accent-color:#ff70b7}.check{display:flex!important;align-items:center!important;padding-bottom:9px}.cyberia-colors{display:grid;grid-template-columns:auto repeat(6,minmax(66px,1fr));gap:9px;align-items:end;margin-top:15px;padding-top:14px;border-top:1px solid #252d39}.arcade-colors{grid-template-columns:auto repeat(2,minmax(90px,160px))}.cyberia-colors>span{align-self:center;color:#4ac8ff;font-size:8px;font-weight:900;letter-spacing:.14em}.cyberia-colors label{display:grid;gap:5px;color:#77818d;font-size:7px;text-transform:uppercase;letter-spacing:.07em}.cyberia-colors input{width:100%;height:28px;border:1px solid #2b3441;background:#090d13;padding:2px}.transcript-rail{min-height:0;display:grid;grid-template-rows:70px 1fr auto 42px;background:#0b0e14}.rail-head{display:flex;align-items:center;justify-content:space-between;padding:0 17px;border-bottom:1px solid #222833}.rail-head>span{color:#697381;font-size:8px}.transcript{min-height:0;overflow-y:auto;padding:8px}.transcript-entry{border:1px solid transparent;border-radius:4px}.transcript-entry:hover,.transcript-entry.current{background:#111720;border-color:#283342}.transcript-line{width:100%;display:grid;grid-template-columns:42px 1fr;gap:8px;text-align:left;border:0;background:none;color:inherit;padding:10px}.retry-translation{margin:0 10px 9px 60px;border:1px solid #ff70b766;background:#231522;color:#ff9acb;padding:5px 8px;font-size:8px;text-transform:uppercase;letter-spacing:.08em}.retry-translation:disabled{opacity:.5}.transcript time{font-size:9px;color:#697381}.transcript strong{font-size:8px;text-transform:uppercase;letter-spacing:.09em}.transcript p{font-size:11px;margin:4px 0}.transcript span{display:block;color:#858d99;font-size:9px;line-height:1.4}.empty{margin:30px 18px;color:#626b78;font-size:11px;line-height:1.6}.speakers{border-top:1px solid #222833;padding:12px 16px}.speakers>div{display:flex;align-items:center;gap:7px;margin-top:8px}.speakers input[type=color]{width:15px;height:15px;border:0;padding:0;background:none}.speakers button{border:0;background:none;font-size:9px}.rename{width:110px;background:#090d13;color:white;border:1px solid #ff70b7}.privacy{border:0;border-top:1px solid #222833;background:#0e1219;color:#7c8591;font-size:8px;text-transform:uppercase;letter-spacing:.12em}.modal-backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;background:#04060bd9;backdrop-filter:blur(12px)}.modal{width:min(560px,90vw);position:relative;padding:30px;background:#10151e;border:1px solid #354050;box-shadow:0 30px 100px #000}.modal .close{position:absolute;right:14px;top:10px;border:0;background:none;color:#7d8590;font-size:24px}.nono-mark{margin-bottom:15px}.modal h1{font-size:29px;margin:8px 0}.modal>p{color:#969daa;font-size:11px;line-height:1.6}.privacy-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin:18px 0}.privacy-grid div{display:grid;gap:5px;border:1px solid #293340;padding:13px}.privacy-grid b{color:#69ddd9;font-size:8px}.privacy-grid span{color:#818995;font-size:9px}.fine{background:#0a0e14;padding:12px}.modal label{margin-top:14px}.save{width:100%;margin-top:10px;border:0;background:linear-gradient(90deg,#db4e99,#795bea);padding:11px;color:white;font-weight:800}.api-message{color:#69ddd9!important}@media(max-width:1050px){main{grid-template-columns:1fr 340px}.command-deck{padding:30px}.preset-row{grid-template-columns:repeat(3,1fr)}.cyberia-colors{grid-template-columns:repeat(4,1fr)}.cyberia-colors>span{grid-column:1/-1}.arcade-colors{grid-template-columns:repeat(2,1fr)}.language-panel{grid-template-columns:1fr 1fr}.language-panel .explanation,.language-panel .live-timing,.language-note{grid-column:auto}.arrow{display:none}}
  .language-panel .processing{grid-column:2}.language-panel .explanation{grid-column:3}.language-note,.next-session{grid-column:2/5;margin:0;font-size:8px;line-height:1.45}.language-note{color:#606b79}.next-session{color:#ff9acb}.language-panel select:disabled,.fine-controls select:disabled{opacity:.52}
  .load-earlier{width:100%;border:1px solid #29313c;background:#111720;color:#69ddd9;padding:9px;font-size:8px;text-transform:uppercase;letter-spacing:.1em}
  .language-panel .external-pause{grid-column:2/5;display:flex;align-items:center;gap:7px;text-transform:none;letter-spacing:0;color:#a8b0bc}.external-note{grid-column:2/5;margin:0;color:#6f7885;font-size:7px;line-height:1.4}
  @media(max-width:1050px){.language-panel .processing,.language-panel .explanation,.language-panel .live-timing,.language-note,.next-session{grid-column:auto}}
</style>
