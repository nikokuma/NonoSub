<script lang="ts">
  import { onMount, tick } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { Channel, convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
  import NonoScene from "$lib/NonoScene.svelte";
  import {
    DEFAULT_STYLE,
    EMPTY_SESSION,
    type LearnerLevel,
    type SessionState,
    type SpeakerProfile,
    type StyleSettings,
    type SubtitlePreset,
    type SubtitleSegment,
    type TutorMessage,
  } from "$lib/contracts";
  import { FIXTURE_EVENTS, FIXTURE_SEGMENTS, QUICK_PROMPTS } from "$lib/fixtures";
  import { buildTutorContext, parsePreferences, serializePreferences } from "$lib/preferences";
  import { activeSegments, canResumeForCoverage, formatTime, reduceSession, shouldPauseForCoverage } from "$lib/session";

  type PreparedMedia = { path: string; assetUrl: string; fileName: string };

  let session = $state<SessionState>(FIXTURE_EVENTS.reduce(reduceSession, EMPTY_SESSION));
  let style = $state<StyleSettings>({ ...DEFAULT_STYLE, position: { ...DEFAULT_STYLE.position } });
  let level = $state<LearnerLevel>("beginner");
  let currentMs = $state(15_000);
  let playing = $state(false);
  let selected = $state<SubtitleSegment | undefined>(FIXTURE_SEGMENTS[3]);
  let media = $state<PreparedMedia | undefined>();
  let mediaError = $state("");
  let video = $state<HTMLVideoElement | undefined>();
  let videoStage = $state<HTMLDivElement | undefined>();
  let tutorInput = $state("");
  let tutorChat = $state<HTMLDivElement | undefined>();
  let tutorMessages = $state<TutorMessage[]>([
    { id: "welcome", role: "assistant", text: "Pick any subtitle and I’ll explain what the speaker really meant. Context matters, you know." },
  ]);
  let settingsOpen = $state(false);
  let apiReady = $state(false);
  let onboardingOpen = $state(false);
  let apiKey = $state("");
  let apiMessage = $state("");
  let renamingSpeaker = $state<string | undefined>();
  let renameValue = $state("");
  let analysisStatus = $state("Fixture ready");
  let liveSession = $state(false);
  let catchingUp = $state(false);
  let draggingOverlay = $state(false);

  const active = $derived(activeSegments(session.segments, currentMs));
  const progressMax = $derived(media && video?.duration ? video.duration * 1000 : 33_000);

  onMount(async () => {
    const saved = localStorage.getItem("nonosub-preferences");
    if (saved) {
      const preferences = parsePreferences(saved);
      if (preferences) {
        style = preferences.style;
        level = preferences.level;
      } else localStorage.removeItem("nonosub-preferences");
    }
    if (isTauri()) {
      try {
        const status = await invoke<{ present: boolean }>("api_key_status");
        if (status.present) {
          await invoke("validate_model_access");
          apiReady = true;
        }
      } catch (error) {
        apiMessage = String(error);
      }
    }
  });

  $effect(() => {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("nonosub-preferences", serializePreferences({ style, level }));
    }
  });

  $effect(() => {
    const latestText = tutorMessages.at(-1)?.text;
    if (latestText === undefined) return;
    void tick().then(() => {
      tutorChat?.scrollTo({ top: tutorChat.scrollHeight, behavior: "smooth" });
    });
  });

  async function chooseMedia() {
    mediaError = "";
    if (!isTauri()) {
      mediaError = "File selection is available in the desktop app. The browser preview uses the deterministic demo.";
      return;
    }
    const selectedPath = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mov"] }] });
    if (!selectedPath) return;
    try {
      const prepared = await invoke<{ path: string; file_name: string }>("prepare_media", { path: selectedPath });
      media = { path: prepared.path, fileName: prepared.file_name, assetUrl: convertFileSrc(prepared.path) };
      currentMs = 0;
      selected = undefined;
      liveSession = true;
      analysisStatus = "Decoding AAC locally…";
      const audio = await invoke<{ durationMs: number; chunkCount: number }>("prepare_audio");
      analysisStatus = `${audio.chunkCount} audio chunk${audio.chunkCount === 1 ? "" : "s"} ready`;
      if (apiReady) await startLiveAnalysis();
      else analysisStatus = "Audio ready · add API key to transcribe";
    } catch (error) {
      mediaError = String(error);
    }
  }

  async function startLiveAnalysis() {
    session = { ...EMPTY_SESSION, phase: "preparing", segments: [], speakers: {}, errors: [] };
    analysisStatus = "Transcribing Japanese…";
    const onEvent = new Channel<import("$lib/contracts").SessionEvent>();
    onEvent.onmessage = (event) => {
      session = reduceSession(session, event);
      if (event.type === "phase_changed") analysisStatus = event.phase === "ready" ? "Ready to watch" : `${event.phase[0].toUpperCase()}${event.phase.slice(1)}…`;
      if (event.type === "coverage_changed" && catchingUp && canResumeForCoverage(currentMs, event.translatedThroughMs) && video) {
        catchingUp = false;
        void video.play();
      }
      if (event.type === "phase_changed" && event.phase === "ready" && video?.paused) void video.play();
      if (event.type === "fatal_error") analysisStatus = "Analysis needs attention";
      if (event.type === "complete") analysisStatus = "Analysis complete";
    };
    try {
      await invoke("start_analysis", { onEvent });
    } catch (error) {
      analysisStatus = "Analysis stopped";
      mediaError = typeof error === "object" && error && "message" in error ? String(error.message) : String(error);
    }
  }

  function togglePlayback() {
    if (media && video) {
      if (video.paused) void video.play(); else video.pause();
      return;
    }
    playing = !playing;
  }

  function seek(milliseconds: number) {
    currentMs = milliseconds;
    if (media && video) video.currentTime = milliseconds / 1000;
  }

  function updatePlaybackTime() {
    if (!video) return;
    currentMs = video.currentTime * 1_000;
    if (liveSession && session.phase !== "complete" && !video.paused && shouldPauseForCoverage(currentMs, session.translatedThroughMs)) {
      video.pause();
      catchingUp = true;
    }
  }

  function selectLine(segment: SubtitleSegment) {
    if (media && video) video.pause();
    playing = false;
    selected = segment;
    seek(segment.startMs + 30);
  }

  function answerFor(prompt: string, segment: SubtitleSegment): string {
    if (segment.id === "seg-4") {
      if (prompt === "Literal vs natural") return "Literally, 今日はちょっと is only “today is a little…”. Naturally, it means “Today won’t work for me.” Japanese listeners complete the uncomfortable ending from context.";
      if (prompt === "Tone & politeness") return "The speaker softens the refusal twice: 〜わけじゃない avoids sounding unwilling, and ちょっと trails off instead of stating できません. It is polite, hesitant, and apologetic.";
      if (prompt === "What is omitted?") return "After ちょっと, something like 難しいです (“is difficult”) or 都合が悪いです (“is inconvenient”) is omitted. The pause lets the listener infer it without forcing a blunt ‘no.’";
      return "行きたくない means “don’t want to go.” わけじゃない denies that interpretation: “it’s not that…”. んですけど introduces an explanation but leaves room for the listener. 今日はちょっと trails off as a conventional refusal. Cute little sentence; surprisingly elaborate escape route.";
    }
    return `In this line, “${segment.sourceText}” is best understood as “${segment.naturalEnglish ?? "translation pending"}”. The surrounding exchange tells us what the Japanese leaves implicit. Live GPT‑5.6 tutoring activates after an API key is validated.`;
  }

  async function askTutor(prompt = tutorInput.trim()) {
    if (!prompt || !selected) return;
    const userMessage: TutorMessage = { id: crypto.randomUUID(), role: "user", text: prompt };
    if (isTauri() && apiReady) {
      const assistantId = crypto.randomUUID();
      tutorMessages = [...tutorMessages, userMessage, { id: assistantId, role: "assistant", text: "" }];
      const onDelta = new Channel<string>();
      onDelta.onmessage = (delta) => {
        tutorMessages = tutorMessages.map((message) => message.id === assistantId ? { ...message, text: message.text + delta } : message);
      };
      const context = buildTutorContext(session.segments, selected.id);
      try {
        await invoke("ask_nono", { question: prompt, selected, learnerLevel: level, context, thread: tutorMessages.slice(-12).map(({ role, text }) => ({ role, text })), onDelta });
      } catch (error) {
        tutorMessages = tutorMessages.map((message) => message.id === assistantId ? { ...message, text: `I couldn't reach GPT‑5.6: ${String(error)}` } : message);
      }
    } else {
      tutorMessages = [...tutorMessages, userMessage, { id: crypto.randomUUID(), role: "assistant", text: answerFor(prompt, selected) }];
    }
    tutorInput = "";
  }

  function updateSpeaker(id: string, patch: Partial<SpeakerProfile>) {
    const speaker = session.speakers[id];
    if (!speaker) return;
    session = { ...session, speakers: { ...session.speakers, [id]: { ...speaker, ...patch } } };
  }

  function saveRename(id: string) {
    const displayName = renameValue.trim();
    if (displayName) updateSpeaker(id, { displayName });
    renamingSpeaker = undefined;
  }

  async function saveApiKey() {
    if (apiKey.trim().length < 20) {
      apiMessage = "That key looks too short.";
      return;
    }
    if (isTauri()) {
      try {
        await invoke("save_api_key", { apiKey: apiKey.trim() });
        apiMessage = "Validating access to GPT‑5.6 and diarized transcription…";
        await invoke("validate_model_access");
      } catch (error) {
        apiMessage = String(error);
        return;
      }
    }
    apiKey = "";
    apiReady = true;
    apiMessage = "Stored securely. GPT‑5.6 and diarized transcription are ready.";
  }

  function cycleMode() {
    style.displayMode = style.displayMode === "both" ? "source" : style.displayMode === "source" ? "translation" : "both";
  }

  function presetLabel(preset: SubtitlePreset): string {
    return ({ clean: "Clean", cinema: "Cinema", contrast: "High Contrast", "nono-pop": "Nono Pop", manga: "Manga", retro: "Retro Pixel" })[preset];
  }

  function moveOverlay(event: PointerEvent) {
    if (!draggingOverlay || !videoStage) return;
    const bounds = videoStage.getBoundingClientRect();
    style.position = {
      x: Math.min(0.92, Math.max(0.08, (event.clientX - bounds.left) / bounds.width)),
      y: Math.min(0.92, Math.max(0.12, (event.clientY - bounds.top) / bounds.height)),
    };
  }

  function beginOverlayDrag(event: PointerEvent) {
    draggingOverlay = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    moveOverlay(event);
  }
</script>

<svelte:head><title>NonoSub — Interactive language subtitles</title></svelte:head>

<div class="app-shell">
  <header>
    <div class="brand"><div class="mark">の</div><div><strong>NonoSub</strong><span>Understand why they said it that way.</span></div></div>
    <div class="header-actions">
      <div class:ready={apiReady} class="status-dot"><i></i>{apiReady ? "Models ready" : "API setup needed"}</div>
      <select bind:value={level} aria-label="Learner level"><option value="beginner">Beginner</option><option value="intermediate">Intermediate</option><option value="advanced">Advanced</option></select>
      <button class="quiet" onclick={() => onboardingOpen = true}>Privacy & API</button>
    </div>
  </header>

  <main>
    <section class="workspace">
      <div class="media-toolbar">
        <div><span class="eyebrow">NOW LEARNING FROM</span><strong>{media?.fileName ?? "NonoSub Japanese Demo"}</strong><small class="analysis-status">{analysisStatus}</small></div>
        <div class="toolbar-actions">{#if liveSession && session.errors.some((error) => error.code === "translation_failed")}<button class="quiet retry" onclick={() => void startLiveAnalysis()}>↻ Retry failed</button>{/if}<button class="quiet" onclick={cycleMode}>{style.displayMode === "both" ? "JP + EN" : style.displayMode === "source" ? "JP only" : "EN only"}</button><button class="quiet" onclick={() => settingsOpen = !settingsOpen}>Aa Style</button><button class="primary" onclick={chooseMedia}>＋ Choose video</button></div>
      </div>

      {#if mediaError}<div class="notice">{mediaError}</div>{/if}

      <div class="video-stage" bind:this={videoStage}>
        {#if media}
          <!-- Subtitles are rendered as NonoSub's interactive synchronized overlay. -->
          <!-- svelte-ignore a11y_media_has_caption -->
          <video bind:this={video} src={media.assetUrl} ontimeupdate={updatePlaybackTime} onplay={() => playing = true} onpause={() => playing = false} onended={() => playing = false}></video>
        {:else}
          <div class="demo-visual"><div class="orb one"></div><div class="orb two"></div><div class="station"><span>駅前</span><small>EVENING · 6:24 PM</small></div><div class="people"><div><i>A</i><b>Aiko</b></div><div><i>S</i><b>Sato</b></div></div></div>
        {/if}

        <div class="subtitles preset-{style.preset}" class:dragging={draggingOverlay} role="group" aria-label="Movable subtitles" style={`left:${style.position.x * 100}%;top:${style.position.y * 100}%;font-size:${style.fontSize}px;font-family:${style.fontFamily};--sub-bg:${style.backgroundOpacity}`} onpointerdown={beginOverlayDrag} onpointermove={moveOverlay} onpointerup={() => draggingOverlay = false} onpointercancel={() => draggingOverlay = false}>
          {#each active as segment (segment.id)}
            {@const speaker = session.speakers[segment.speakerId]}
            <button class="subtitle-line effect-{style.effect}" onclick={() => selectLine(segment)}>
              {#if style.showSpeakerNames}<span class="speaker" style={`color:${speaker?.color ?? "white"}`}>{speaker?.displayName ?? segment.speakerId}</span>{/if}
              {#if style.displayMode !== "translation"}<span class="jp">{segment.sourceText}</span>{/if}
              {#if style.displayMode !== "source"}<span class="en">{segment.naturalEnglish ?? "Nono is translating…"}</span>{/if}
            </button>
          {/each}
        </div>

        {#if catchingUp}<div class="catching-up"><span>の</span>Nono is catching up…</div>{/if}

        {#if settingsOpen}
          <aside class="style-popover">
            <div class="popover-head"><strong>Subtitle style</strong><button onclick={() => settingsOpen = false}>×</button></div>
            <div class="preset-grid">{#each (["clean", "cinema", "contrast", "nono-pop", "manga", "retro"] as SubtitlePreset[]) as preset}<button class:chosen={style.preset === preset} onclick={() => style.preset = preset}>{presetLabel(preset)}</button>{/each}</div>
            <label>Text size <output>{style.fontSize}px</output><input type="range" min="18" max="42" bind:value={style.fontSize} /></label>
            <label>Background <output>{Math.round(style.backgroundOpacity * 100)}%</output><input type="range" min="0" max="0.9" step="0.05" bind:value={style.backgroundOpacity} /></label>
            <label>Text effect <select bind:value={style.effect}><option value="outline">Outline</option><option value="shadow">Shadow</option><option value="none">None</option></select></label>
            <label class="check"><input type="checkbox" bind:checked={style.showSpeakerNames} /> Show speaker names</label>
          </aside>
        {/if}
      </div>

      <div class="controls">
        <button class="play" onclick={togglePlayback}>{playing ? "❚❚" : "▶"}</button>
        <span>{formatTime(currentMs)}</span>
        <input class="timeline" type="range" min="0" max={progressMax} step="50" value={currentMs} oninput={(event) => seek(Number(event.currentTarget.value))} aria-label="Video timeline" />
        <span>{formatTime(progressMax)}</span>
        <button class="icon-button" aria-label="Volume">◖))</button>
        <button class="icon-button" aria-label="Fullscreen">⛶</button>
      </div>

      <div class="speaker-bar">
        <span class="eyebrow">SPEAKERS</span>
        {#each Object.values(session.speakers) as speaker}
          <div class="speaker-chip">
            <input class="color-dot" type="color" value={speaker.color} onchange={(event) => updateSpeaker(speaker.id, { color: event.currentTarget.value })} aria-label={`Color for ${speaker.displayName}`} />
            {#if renamingSpeaker === speaker.id}
              <input class="rename" bind:value={renameValue} onkeydown={(event) => event.key === "Enter" && saveRename(speaker.id)} onblur={() => saveRename(speaker.id)} />
            {:else}
              <button onclick={() => { renamingSpeaker = speaker.id; renameValue = speaker.displayName; }}>{speaker.displayName} <span>✎</span></button>
            {/if}
          </div>
        {/each}
        <span class="local-badge">● SESSION ONLY</span>
      </div>
    </section>

    <aside class="right-rail">
      <div class="rail-tabs"><button class="active">Transcript</button><button>Nono tutor</button><span>● LIVE</span></div>
      <div class="transcript">
        {#each session.segments as segment}
          {@const speaker = session.speakers[segment.speakerId]}
          <button class:current={selected?.id === segment.id} onclick={() => selectLine(segment)}>
            <time>{formatTime(segment.startMs)}</time>
            <div><strong style={`color:${speaker?.color ?? "white"}`}>{speaker?.displayName ?? segment.speakerId}</strong><p>{segment.sourceText}</p><span>{segment.naturalEnglish}</span>{#if session.errors.some((error) => error.segmentId === segment.id)}<em>Translation unavailable · use Retry failed</em>{/if}</div>
          </button>
        {/each}
      </div>

      <section class="tutor">
        <NonoScene />
        <div class="tutor-title"><div><span class="eyebrow">ASK NONO</span><strong>{selected ? "Let’s unpack this line" : "Choose a subtitle"}</strong></div><span class="level">{level}</span></div>
        {#if selected}
          <div class="selected-line"><span style={`color:${session.speakers[selected.speakerId]?.color}`}>{session.speakers[selected.speakerId]?.displayName}</span><b>{selected.sourceText}</b><small>{selected.naturalEnglish}</small></div>
          <div class="quick-prompts">{#each QUICK_PROMPTS as prompt}<button onclick={() => void askTutor(prompt)}>{prompt}</button>{/each}</div>
        {/if}
        <div class="chat" bind:this={tutorChat} aria-live="polite">
          {#each tutorMessages as message}<div class:assistant={message.role === "assistant"} class="message">{message.text}</div>{/each}
        </div>
        <form onsubmit={(event) => { event.preventDefault(); void askTutor(); }}><textarea bind:value={tutorInput} placeholder="Ask about grammar, tone, or culture…" disabled={!selected}></textarea><button type="submit" disabled={!selected || !tutorInput.trim()}>↑</button></form>
        <p class="context-note">Nono sees this line + nearby dialogue · answers at {level} level</p>
      </section>
    </aside>
  </main>
</div>

{#if onboardingOpen}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && (onboardingOpen = false)}>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="privacy-title">
      <button class="close" onclick={() => onboardingOpen = false}>×</button><span class="modal-mark">の</span><span class="eyebrow">WELCOME TO NONOSUB</span><h1 id="privacy-title">Your video stays yours.</h1>
      <p>The video remains on this Mac. NonoSub extracts audio locally and sends audio chunks to OpenAI for transcription. Transcript context and your questions go to GPT‑5.6 for translation and teaching.</p>
      <div class="privacy-grid"><div><i>⌂</i><b>Stays local</b><span>Video, transcript history, tutor chat</span></div><div><i>↗</i><b>Sent to OpenAI</b><span>Extracted audio, transcript context, questions</span></div></div>
      <p class="fineprint">OpenAI API data is not used for training by default. Standard Responses requests may be retained for abuse monitoring for up to 30 days, even with <code>store:false</code>. No account, analytics, or NonoSub cloud database.</p>
      <label class="api-field">OpenAI API key<input type="password" bind:value={apiKey} placeholder="sk-…" autocomplete="off" /></label>
      <button class="primary full" onclick={saveApiKey}>Save securely & validate access</button>
      {#if apiMessage}<p class="api-message">{apiMessage}</p>{/if}
      <small>Your key is stored in the operating system credential vault and never enters the webview after saving.</small>
    </div>
  </div>
{/if}

<style>
  .app-shell { height: 100vh; display: grid; grid-template-rows: 66px 1fr; }
  header { display:flex; align-items:center; justify-content:space-between; padding: 0 22px; border-bottom:1px solid var(--line); background:rgba(9,8,18,.76); backdrop-filter:blur(20px); }
  .brand,.header-actions,.toolbar-actions,.speaker-bar,.speaker-chip,.tutor-title { display:flex; align-items:center; }
  .mark { width:38px; height:38px; display:grid; place-items:center; border-radius:12px; margin-right:11px; background:linear-gradient(145deg,var(--pink),#c15bed 65%,var(--violet)); box-shadow:0 8px 24px rgba(255,114,182,.25); font-weight:900; }
  .brand strong { display:block; font-size:16px; letter-spacing:-.2px; }.brand span { display:block; color:var(--muted); font-size:10px; }
  .header-actions { gap:10px; } select,.quiet { border:1px solid var(--line); background:rgba(255,255,255,.045); border-radius:9px; padding:8px 10px; font-size:11px; }
  .status-dot { color:#c0b9ce; font-size:10px; letter-spacing:.05em; text-transform:uppercase; display:flex; gap:6px; align-items:center; }.status-dot i { width:7px;height:7px;border-radius:50%;background:#ffb454;box-shadow:0 0 10px #ffb454;}.status-dot.ready i{background:var(--mint)}
  main { min-height:0; display:grid; grid-template-columns:minmax(580px,1fr) 380px; }
  .workspace { position:relative; min-width:0; display:grid; grid-template-rows:60px minmax(300px,1fr) 54px 46px; border-right:1px solid var(--line); }
  .media-toolbar { display:flex; align-items:center; justify-content:space-between; padding:0 18px; }.media-toolbar strong{display:block;font-size:13px;margin-top:3px}.eyebrow{color:#8f899f;font-size:9px;font-weight:800;letter-spacing:.15em}.toolbar-actions{gap:8px}.primary{border:0;border-radius:9px;background:linear-gradient(135deg,#eb60a7,#8d72ee);padding:9px 14px;font-size:11px;font-weight:750;box-shadow:0 6px 18px rgba(235,96,167,.18)}
  .analysis-status{display:inline-block;margin-left:8px;color:#767080;font-size:8px}
  .notice{position:absolute;z-index:10;top:60px;left:18px;right:18px;font-size:11px;color:#ffd39e;padding:8px 12px;background:rgba(31,25,23,.94);border:1px solid rgba(255,179,84,.2);border-radius:0 0 8px 8px}
  .video-stage { position:relative; min-height:0; overflow:hidden; margin:0 18px; border-radius:16px; background:#12101c; box-shadow:0 18px 50px rgba(0,0,0,.34); }
  video,.demo-visual{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;display:block}.demo-visual{overflow:hidden;background:linear-gradient(180deg,#2f294c 0%,#171827 58%,#0e1017);}.demo-visual:after{content:"";position:absolute;inset:54% 0 0;background:linear-gradient(165deg,transparent 0 16%,rgba(121,233,203,.07) 16% 17%,transparent 17% 28%,rgba(255,255,255,.08) 28% 29%,transparent 29%),linear-gradient(#131722,#080a10)}.orb{position:absolute;border-radius:50%;filter:blur(1px)}.orb.one{width:360px;height:360px;left:-90px;top:-150px;background:rgba(255,118,190,.18)}.orb.two{width:250px;height:250px;right:-60px;top:10px;background:rgba(111,129,255,.14)}.station{position:absolute;left:8%;top:17%;padding:12px 18px;border-left:2px solid var(--pink);z-index:1}.station span{display:block;font-family:serif;font-size:28px}.station small{font-size:8px;letter-spacing:.2em;color:#b7afc6}.people{position:absolute;inset:auto 9% 25%;display:flex;justify-content:space-between;z-index:2}.people>div{text-align:center}.people i{width:76px;height:76px;display:grid;place-items:center;border-radius:50%;font-style:normal;font-size:24px;background:linear-gradient(145deg,#674f7b,#2c2841);border:2px solid rgba(255,255,255,.15);box-shadow:0 13px 35px rgba(0,0,0,.4)}.people>div:last-child i{background:linear-gradient(145deg,#315a5a,#1c2d3a)}.people b{display:block;margin-top:6px;font-size:10px;color:#cac4d3}
  .subtitles{position:absolute;transform:translate(-50%,-50%);width:min(88%,780px);z-index:4;display:grid;gap:8px;cursor:grab;touch-action:none}.subtitles.dragging{cursor:grabbing}.subtitle-line{width:100%;border:0;background:transparent;padding:0;color:white;text-align:center;display:grid;justify-items:center}.subtitle-line span:not(.speaker){padding:2px 10px;background:rgba(0,0,0,var(--sub-bg));line-height:1.25}.speaker{font-size:.38em;text-transform:uppercase;font-weight:900;letter-spacing:.1em;margin-bottom:3px}.jp{font-weight:800}.en{font-size:.62em;color:#f7f4fb}.effect-outline span:not(.speaker){text-shadow:-1px -1px 0 #000,1px -1px 0 #000,-1px 1px 0 #000,1px 1px 0 #000}.effect-shadow span:not(.speaker){text-shadow:0 3px 8px #000}.preset-cinema .jp,.preset-cinema .en{font-family:Georgia,serif}.preset-contrast .jp,.preset-contrast .en{background:#000;color:#fff}.preset-nono-pop .jp,.preset-nono-pop .en{background:rgba(163,51,126,var(--sub-bg));border-radius:8px}.preset-manga .jp,.preset-manga .en{background:rgba(255,255,255,var(--sub-bg));color:#111;font-family:serif}.preset-retro .jp,.preset-retro .en{font-family:monospace;letter-spacing:.04em}.catching-up{position:absolute;z-index:6;left:50%;top:42%;transform:translate(-50%,-50%);display:flex;align-items:center;gap:8px;padding:10px 14px;border:1px solid var(--line);border-radius:12px;background:rgba(15,12,24,.9);font-size:11px}.catching-up span{display:grid;place-items:center;width:24px;height:24px;border-radius:8px;background:linear-gradient(135deg,var(--pink),var(--violet))}
  .style-popover{position:absolute;z-index:8;right:12px;top:12px;width:265px;padding:14px;background:rgba(19,17,31,.96);border:1px solid var(--line);border-radius:12px;box-shadow:0 20px 50px #000}.popover-head{display:flex;justify-content:space-between}.popover-head button{border:0;background:none;color:#999;font-size:20px}.preset-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px;margin:12px 0}.preset-grid button{border:1px solid var(--line);background:#252138;border-radius:7px;padding:7px;font-size:9px}.preset-grid button.chosen{border-color:var(--pink);background:rgba(255,114,182,.14)}.style-popover label{display:grid;grid-template-columns:1fr auto;gap:6px;margin-top:10px;font-size:10px;color:#c7c1d3}.style-popover input[type=range]{grid-column:1/-1;width:100%;accent-color:var(--pink)}.style-popover label.check{display:flex}.style-popover select{padding:3px 6px}
  .controls{display:flex;align-items:center;gap:12px;padding:0 20px;color:#aaa4b6;font-size:10px}.controls button{border:0;background:none}.play{width:31px;height:31px;border-radius:50%!important;background:white!important;color:#191624!important;font-size:11px}.timeline{flex:1;accent-color:var(--pink);height:3px}.icon-button{font-size:15px;color:#aaa4b6}
  .speaker-bar{gap:10px;padding:0 18px;border-top:1px solid var(--line)}.speaker-chip{gap:4px;background:rgba(255,255,255,.04);border:1px solid var(--line);padding:4px 8px;border-radius:20px}.speaker-chip button{background:none;border:0;font-size:10px}.speaker-chip button span{color:#777}.color-dot{width:13px;height:13px;padding:0;border:0;border-radius:50%;overflow:hidden}.color-dot::-webkit-color-swatch-wrapper{padding:0}.color-dot::-webkit-color-swatch{border:0;border-radius:50%}.rename{width:74px;background:#11101b;color:white;border:1px solid var(--pink);border-radius:4px;font-size:10px;padding:3px}.local-badge{margin-left:auto;font-size:8px;color:#756f80}
  .right-rail{min-height:0;display:grid;grid-template-rows:45px minmax(120px,.45fr) minmax(470px,1.55fr);background:rgba(14,13,25,.76)}.rail-tabs{display:flex;align-items:end;padding:0 14px;border-bottom:1px solid var(--line)}.rail-tabs button{height:44px;border:0;background:none;color:#777181;font-size:10px;padding:0 10px}.rail-tabs button.active{color:white;border-bottom:2px solid var(--pink)}.rail-tabs span{margin-left:auto;align-self:center;color:var(--mint);font-size:8px}
  .transcript{min-height:0;overflow-y:auto;padding:8px}.transcript>button{width:100%;display:grid;grid-template-columns:38px 1fr;gap:8px;text-align:left;border:1px solid transparent;background:none;color:inherit;padding:8px;border-radius:9px}.transcript>button:hover,.transcript>button.current{background:rgba(255,255,255,.045);border-color:rgba(255,114,182,.17)}.transcript time{font-size:9px;color:#6f6978}.transcript strong{font-size:9px;text-transform:uppercase}.transcript p{margin:2px 0;font-size:11px;line-height:1.3}.transcript span{display:block;color:#85808f;font-size:9px;line-height:1.3}.transcript em{display:inline-block;margin-top:4px;color:#ffb2d5;font-size:8px;font-style:normal}
  .tutor{min-height:0;border-top:1px solid var(--line);display:flex;flex-direction:column;overflow:hidden}.tutor-title{padding:0 14px 8px;justify-content:space-between}.tutor-title strong{display:block;font-size:13px}.level{text-transform:uppercase;font-size:8px;background:rgba(155,140,255,.14);color:#b9adff;border:1px solid rgba(155,140,255,.25);border-radius:10px;padding:4px 7px}.selected-line{margin:0 14px;padding:9px 10px;border-radius:9px;background:rgba(255,255,255,.045);display:grid;gap:3px}.selected-line span{font-size:8px;text-transform:uppercase;font-weight:bold}.selected-line b{font-size:11px}.selected-line small{font-size:9px;color:#8e8898}.quick-prompts{display:flex;gap:5px;padding:8px 14px;overflow-x:auto;flex:none}.quick-prompts button{white-space:nowrap;border:1px solid var(--line);border-radius:14px;background:#242035;padding:5px 8px;font-size:8px}.chat{flex:1;min-height:80px;padding:0 14px;overflow-y:auto;display:grid;align-content:start;gap:6px;scroll-behavior:smooth}.message{margin-left:18%;background:#2b263d;border-radius:9px;padding:8px;font-size:9px;line-height:1.4;white-space:pre-wrap}.message.assistant{margin:0 18% 0 0;background:rgba(255,114,182,.09);border:1px solid rgba(255,114,182,.12)}.tutor form{flex:none;margin:8px 14px 0;display:grid;grid-template-columns:1fr 30px;border:1px solid var(--line);border-radius:9px;background:#181522}.tutor textarea{height:40px;resize:none;border:0;background:none;color:white;padding:8px;font-size:9px;outline:0}.tutor form button{margin:6px;width:25px;height:25px;border:0;border-radius:50%;background:var(--pink);color:#17101a}.context-note{flex:none;text-align:center;color:#686271;font-size:7px;margin:5px 0 8px}
  .modal-backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;background:rgba(4,3,9,.78);backdrop-filter:blur(12px)}.modal{position:relative;width:510px;padding:28px;background:#161321;border:1px solid rgba(255,255,255,.13);border-radius:18px;box-shadow:0 28px 90px #000}.close{position:absolute;right:14px;top:12px;border:0;background:none;color:#999;font-size:24px}.modal-mark{display:grid;place-items:center;width:43px;height:43px;border-radius:13px;background:linear-gradient(135deg,var(--pink),var(--violet));margin-bottom:14px}.modal h1{font-size:27px;margin:5px 0 9px;letter-spacing:-.8px}.modal>p{color:#aaa3b4;font-size:11px;line-height:1.55}.privacy-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin:15px 0}.privacy-grid>div{padding:12px;border:1px solid var(--line);border-radius:10px;background:rgba(255,255,255,.025);display:grid;grid-template-columns:26px 1fr;gap:1px}.privacy-grid i{grid-row:1/3;font-style:normal;color:var(--mint)}.privacy-grid b{font-size:10px}.privacy-grid span{font-size:8px;color:#817b8a}.fineprint{padding:10px;border-radius:8px;background:rgba(255,255,255,.035);font-size:9px!important}.api-field{display:grid;gap:5px;font-size:9px;color:#999}.api-field input{border:1px solid var(--line);background:#0e0c16;border-radius:8px;color:white;padding:10px}.full{width:100%;margin-top:10px}.api-message{color:var(--mint)!important;margin:7px 0!important}.modal>small{display:block;text-align:center;color:#665f70;font-size:8px;margin-top:8px}
  @media(max-width:1100px){main{grid-template-columns:minmax(560px,1fr) 340px}.right-rail{font-size:90%}}
</style>
