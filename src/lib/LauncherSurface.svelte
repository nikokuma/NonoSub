<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { LauncherMode, LauncherState, LiveCaptureSource, LiveCaptureSourceKind, LiveCaptureSources } from "./contracts";
  import { captureSelection, captureSourceMonogram, EMPTY_CAPTURE_SOURCES, filterCaptureSources } from "./captureSources";
  import { loadPreferences } from "./runtime";
  import { errorMessage, startFileSession, startLiveSession, validateVideoPath } from "./sessionLaunch";

  let mode = $state<LauncherMode>("file");
  let launchState = $state<LauncherState>("idle");
  let message = $state("Drop one MP4 or MOV here.");
  let retryPath = $state<string>();
  let running = $state(false);
  let loadingSources = $state(false);
  let sources = $state<LiveCaptureSources>(structuredClone(EMPTY_CAPTURE_SOURCES));
  let sourceKind = $state<LiveCaptureSourceKind>("application");
  let sourceQuery = $state("");
  let selectedSource = $state<LiveCaptureSource>();
  let loadSequence = 0;
  const visibleSources = $derived(filterCaptureSources(sources, sourceKind, sourceQuery));
  const browserFixtureSources: LiveCaptureSources = {
    applications: [
      { id: "application:101", kind: "application", title: "Safari", detail: "2 visible windows", applicationName: "Safari", bundleIdentifier: "com.apple.Safari", processId: 101 },
      { id: "application:102", kind: "application", title: "QuickTime Player", detail: "1 visible window", applicationName: "QuickTime Player", bundleIdentifier: "com.apple.QuickTimePlayerX", processId: 102 },
    ],
    windows: [
      { id: "window:201", kind: "window", title: "Japanese livestream", detail: "Safari · 1280×720", applicationName: "Safari", processId: 101, windowId: 201 },
      { id: "window:202", kind: "window", title: "JpTestFemale.mov", detail: "QuickTime Player · 960×540", applicationName: "QuickTime Player", processId: 102, windowId: 202 },
    ],
    displays: [{ id: "display:1", kind: "display", title: "Display 1", detail: "3024×1964", displayId: 1 }],
  };

  onMount(() => {
    document.documentElement.dataset.surface = "launcher";
    const cleanup: Array<() => void> = [];
    if (isTauri()) {
      void invoke<LauncherMode>("get_launcher_mode").then((next) => setMode(next));
      void listen<LauncherMode>("launcher-action", ({ payload }) => setMode(payload)).then((unlisten) => cleanup.push(unlisten));
      void listen<{ generation: number; phase: string }>("media-preparation-progress", ({ payload }) => {
        if (!running || mode !== "file") return;
        message = ({
          inspecting: "Inspecting the selected video…",
          converting_video: "Preparing compatible video playback…",
          decoding_audio: "Decoding audio locally…",
          creating_chunks: "Creating secure transcription chunks…",
          ready: "Media preparation complete.",
        } as Record<string, string>)[payload.phase] ?? message;
      }).then((unlisten) => cleanup.push(unlisten));
      const launcherWindow = getCurrentWindow();
      void launcherWindow.onDragDropEvent(({ payload }) => {
        if (mode !== "file" || running) return;
        if (payload.type === "enter" || payload.type === "over") launchState = "hovering";
        else if (payload.type === "leave") launchState = "idle";
        else if (payload.type === "drop") {
          launchState = "idle";
          if (payload.paths.length !== 1) return showError("Drop exactly one MP4 or MOV file.");
          void launchFile(payload.paths[0]);
        }
      }).then((unlisten) => cleanup.push(unlisten));
    } else if (new URLSearchParams(window.location.search).get("launcherMode") === "live") {
      mode = "live";
      sources = browserFixtureSources;
      message = "Select the app playing the video. NonoSub captures its audio, not the screen image.";
    }
    return () => {
      loadSequence += 1;
      cleanup.forEach((stop) => stop());
    };
  });

  function setMode(next: LauncherMode) {
    mode = next;
    launchState = "idle";
    retryPath = undefined;
    sourceQuery = "";
    selectedSource = undefined;
    message = next === "file"
      ? "Drop one MP4 or MOV here."
      : "Choose exactly which application, window, or display NonoSub may listen to.";
    if (isTauri()) {
      const size = next === "live" ? new LogicalSize(720, 520) : new LogicalSize(420, 190);
      void getCurrentWindow().setSize(size);
    }
    if (next === "live") void loadLiveSources();
  }

  async function chooseVideo() {
    if (!isTauri() || running) return;
    const path = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mov"] }] });
    if (typeof path === "string") await launchFile(path);
  }

  async function launchFile(path: string) {
    const validation = validateVideoPath(path);
    if (validation) return showError(validation);
    running = true;
    retryPath = path;
    launchState = "preparing";
    try {
      await startFileSession(path, loadPreferences(), {
        status: (value) => message = value,
        analysisError: (value) => {
          showError(value);
          void invoke("open_surface", { surface: "launcher" });
        },
      });
      await invoke("hide_surface", { surface: "launcher" });
    } catch (error) {
      showError(errorMessage(error));
    } finally {
      running = false;
    }
  }

  async function loadLiveSources() {
    if (!isTauri() || loadingSources || running) return;
    const request = ++loadSequence;
    loadingSources = true;
    selectedSource = undefined;
    launchState = "starting";
    message = "Finding visible applications and windows…";
    try {
      const available = await invoke<LiveCaptureSources>("list_live_capture_sources");
      if (request !== loadSequence || mode !== "live") return;
      sources = available;
      launchState = "idle";
      message = available.applications.length
        ? "Select the app playing the video. NonoSub captures its audio, not the screen image."
        : "No visible applications were found. Try Windows or Displays, then refresh.";
    } catch (error) {
      if (request === loadSequence) showError(errorMessage(error));
    } finally {
      if (request === loadSequence) loadingSources = false;
    }
  }

  function chooseSource(source: LiveCaptureSource) {
    selectedSource = source;
    message = `${source.title} selected · only captured audio is sent to OpenAI.`;
  }

  async function launchLive() {
    if (running || loadingSources || !selectedSource) return;
    running = true;
    launchState = "starting";
    try {
      await startLiveSession(loadPreferences(), captureSelection(selectedSource), { status: (value) => message = value });
      await invoke("hide_surface", { surface: "launcher" });
    } catch (error) {
      showError(errorMessage(error));
    } finally {
      running = false;
    }
  }

  function selectSourceKind(kind: LiveCaptureSourceKind) {
    sourceKind = kind;
    selectedSource = undefined;
    sourceQuery = "";
  }

  function showError(value: string) {
    message = value;
    launchState = "error";
  }

  async function close() {
    if (!running && isTauri()) await invoke("hide_surface", { surface: "launcher" });
  }

  async function cancelPreparation() {
    if (!isTauri() || !running || mode !== "file") return;
    message = "Cancelling media preparation…";
    await invoke("cancel_media_preparation").catch(() => undefined);
  }

  async function openSettings() {
    if (!isTauri()) return;
    await invoke("open_surface", { surface: "workbench" });
    await invoke("hide_surface", { surface: "launcher" });
  }
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && void close()} />

{#if mode === "file"}
  <main class="launcher file" class:hovering={launchState === "hovering"} class:error={launchState === "error"}>
    <button class="close" aria-label="Close launcher" onclick={close}>×</button>
    <div class="mark" aria-hidden="true">の</div>
    <section>
      <span class="eyebrow">OPEN LOCAL VIDEO</span>
      <h1>{launchState === "hovering" ? "Drop to open" : "Video → subtitles"}</h1>
      <p class="status">{message}</p>
      <div class="actions">
        <button class="primary" onclick={chooseVideo} disabled={running}>{launchState === "error" ? "Choose Another" : "Choose Video"}</button>
        {#if launchState === "error" && retryPath}<button onclick={() => retryPath && launchFile(retryPath)} disabled={running}>Retry</button>{/if}
        {#if launchState === "error"}<button onclick={openSettings}>Open Settings</button>{/if}
        {#if !running}<button onclick={close}>Cancel</button>{/if}
        {#if running}<button onclick={cancelPreparation}>Cancel preparation</button>{/if}
        {#if running}<i aria-label="Working"></i>{/if}
      </div>
    </section>
  </main>
{:else}
  <main class="launcher live" class:error={launchState === "error"}>
    <button class="close" aria-label="Close source chooser" onclick={close}>×</button>
    <header>
      <div class="mark small" aria-hidden="true">の</div>
      <div><span class="eyebrow">LIVE CAPTURE SOURCE</span><h1>What should NonoSub listen to?</h1></div>
      <button class="refresh" onclick={loadLiveSources} disabled={loadingSources || running}>↻ Refresh</button>
    </header>
    <p class="privacy">The selected video image stays inside its app. NonoSub receives system audio and streams only temporary audio to OpenAI.</p>
    <nav class="source-tabs" aria-label="Capture source type">
      <button class:active={sourceKind === "application"} onclick={() => selectSourceKind("application")}>Applications <b>{sources.applications.length}</b></button>
      <button class:active={sourceKind === "window"} onclick={() => selectSourceKind("window")}>Windows <b>{sources.windows.length}</b></button>
      <button class:active={sourceKind === "display"} onclick={() => selectSourceKind("display")}>Displays <b>{sources.displays.length}</b></button>
    </nav>
    <label class="search"><span>⌕</span><input bind:value={sourceQuery} placeholder={`Search ${sourceKind}s`} disabled={loadingSources} /></label>
    <section class="source-list" aria-busy={loadingSources}>
      {#if loadingSources}
        <div class="empty"><i aria-label="Finding sources"></i><strong>Finding visible sources…</strong></div>
      {:else if visibleSources.length === 0}
        <div class="empty"><strong>No {sourceKind}s found</strong><span>Open the video first, then press Refresh.</span></div>
      {:else}
        {#each visibleSources as source (source.id)}
          <button class="source" class:selected={selectedSource?.id === source.id} onclick={() => chooseSource(source)}>
            <span class="source-icon">{captureSourceMonogram(source)}</span>
            <span class="source-copy"><strong>{source.title}</strong><small>{source.detail}</small></span>
            <span class="choice">{selectedSource?.id === source.id ? "✓" : ""}</span>
          </button>
        {/each}
      {/if}
    </section>
    <footer>
      <p class="status" class:error-text={launchState === "error"}>{message}</p>
      <div class="actions">
        {#if launchState === "error"}<button onclick={openSettings}>Open Settings</button>{/if}
        <button onclick={close} disabled={running}>Cancel</button>
        <button class="primary start" onclick={launchLive} disabled={!selectedSource || running || loadingSources}>{running ? "Starting…" : "Start Captions"}</button>
        {#if running}<i aria-label="Working"></i>{/if}
      </div>
    </footer>
  </main>
{/if}

<style>
  .launcher{position:fixed;inset:8px;border:1px solid #76e7df66;border-radius:18px;background:linear-gradient(135deg,#0a1017f8,#101522f6);box-shadow:0 22px 70px #0009;color:#f7f7fb;transition:border-color .15s,transform .15s;overflow:hidden}.launcher.error{border-color:#ff74b966}.launcher.file{display:grid;grid-template-columns:76px 1fr;gap:16px;align-items:center;padding:20px 24px 18px 18px}.launcher.file.hovering{border-color:#ff74b9;transform:scale(.985);background:linear-gradient(135deg,#111925f8,#241322f5)}.close{position:absolute;z-index:4;right:13px;top:10px;border:0;background:none;color:#77808e;font-size:20px;cursor:pointer}.mark{width:64px;height:64px;display:grid;place-items:center;border-radius:18px;background:#ff6fb5;color:#fff;font-size:28px;font-weight:900;box-shadow:0 0 30px #ff6fb544}.mark.small{width:42px;height:42px;border-radius:12px;font-size:19px}.eyebrow{color:#70e5de;font:800 9px/1 "JetBrains Mono",monospace;letter-spacing:.17em}.launcher h1{margin:7px 0 5px;font-size:20px;letter-spacing:-.03em}.status{margin:0;color:#9aa3af;font-size:10px;line-height:1.45}.file .status{height:30px;overflow:hidden}.actions{display:flex;align-items:center;gap:7px;margin-top:10px}.actions button,.refresh{border:1px solid #33404f;background:#111923;color:#cbd2db;border-radius:7px;padding:7px 10px;font-size:9px;cursor:pointer}.actions button.primary{border-color:#6edfd788;color:#83eee6;background:#102226}.actions button:disabled,.refresh:disabled{opacity:.4;cursor:default}.actions i,.empty i{width:12px;height:12px;margin-left:3px;border:2px solid #70e5de44;border-top-color:#70e5de;border-radius:50%;animation:spin .7s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
  .launcher.live{display:grid;grid-template-rows:auto auto auto auto 1fr auto;padding:22px 24px 18px}.live header{display:flex;align-items:center;gap:12px;padding-right:56px}.live header h1{margin:4px 0 0;font-size:20px}.refresh{margin-left:auto}.privacy{margin:13px 0 11px;padding:9px 11px;border:1px solid #293746;background:#0a111acc;color:#8f9aa7;font-size:9px;line-height:1.5}.source-tabs{display:flex;gap:6px}.source-tabs button{flex:1;border:1px solid #283746;background:#0c141e;color:#8e9aaa;padding:8px;border-radius:8px;font:700 9px/1 "JetBrains Mono",monospace;cursor:pointer}.source-tabs button.active{border-color:#66ddd4;color:#8ff7ef;background:#10242a}.source-tabs b{margin-left:5px;color:#ff75b8}.search{display:flex;align-items:center;gap:8px;margin:9px 0 8px;padding:0 11px;height:34px;border:1px solid #293746;border-radius:8px;background:#080e15;color:#647180}.search input{width:100%;border:0;outline:0;background:transparent;color:#f5f7fa;font:11px/1.2 "JetBrains Mono",monospace}.source-list{min-height:0;overflow:auto;display:grid;align-content:start;grid-template-columns:repeat(2,minmax(0,1fr));gap:7px;padding:1px 3px 4px 1px}.source{min-width:0;display:grid;grid-template-columns:38px 1fr 20px;align-items:center;gap:9px;padding:9px;border:1px solid #263543;border-radius:10px;background:#0b131d;color:#e7ebef;text-align:left;cursor:pointer}.source:hover{border-color:#4d7d86;background:#0d1923}.source.selected{border-color:#ff76b9;box-shadow:inset 0 0 0 1px #ff76b944;background:#1c1520}.source-icon{width:36px;height:36px;display:grid;place-items:center;border-radius:9px;background:linear-gradient(145deg,#173b46,#142239);color:#8df5ed;font:800 12px/1 "JetBrains Mono",monospace}.source-copy{min-width:0}.source-copy strong,.source-copy small{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.source-copy strong{font-size:11px}.source-copy small{margin-top:4px;color:#74808d;font:8px/1.2 "JetBrains Mono",monospace}.choice{color:#ff78ba;font-size:15px}.empty{grid-column:1/-1;min-height:132px;display:flex;flex-direction:column;justify-content:center;align-items:center;gap:10px;border:1px dashed #293746;border-radius:11px;color:#75818d}.empty strong{font-size:11px}.empty span{font-size:9px}.live footer{display:flex;align-items:center;gap:14px;padding-top:9px;border-top:1px solid #263441}.live footer .status{flex:1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.error-text{color:#ff93c6}.live footer .actions{margin:0}.start{min-width:98px}
</style>
