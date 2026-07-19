<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { LauncherMode, LauncherState } from "./contracts";
  import { loadPreferences } from "./runtime";
  import { errorMessage, startFileSession, startLiveSession, validateVideoPath } from "./sessionLaunch";

  let mode = $state<LauncherMode>("file");
  let launchState = $state<LauncherState>("idle");
  let message = $state("Drop one MP4 or MOV here.");
  let retryPath = $state<string>();
  let running = $state(false);

  onMount(() => {
    document.documentElement.dataset.surface = "launcher";
    const cleanup: Array<() => void> = [];
    if (isTauri()) {
      void invoke<LauncherMode>("get_launcher_mode").then((next) => setMode(next));
      void listen<LauncherMode>("launcher-action", ({ payload }) => setMode(payload)).then((unlisten) => cleanup.push(unlisten));
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
    }
    return () => cleanup.forEach((stop) => stop());
  });

  function setMode(next: LauncherMode) {
    mode = next;
    launchState = "idle";
    retryPath = undefined;
    message = next === "file"
      ? "Drop one MP4 or MOV here."
      : "Apple will ask which app, window, or display NonoSub may listen to.";
    if (next === "live") void launchLive();
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

  async function launchLive() {
    if (running) return;
    running = true;
    launchState = "starting";
    try {
      await startLiveSession(loadPreferences(), { status: (value) => message = value });
      await invoke("hide_surface", { surface: "launcher" });
    } catch (error) {
      showError(errorMessage(error));
    } finally {
      running = false;
    }
  }

  function showError(value: string) {
    message = value;
    launchState = "error";
  }

  async function close() {
    if (!running && isTauri()) await invoke("hide_surface", { surface: "launcher" });
  }

  async function openSettings() {
    if (!isTauri()) return;
    await invoke("open_surface", { surface: "workbench" });
    await invoke("hide_surface", { surface: "launcher" });
  }
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && void close()} />

<main class="launcher" class:hovering={launchState === "hovering"} class:error={launchState === "error"}>
  <button class="close" aria-label="Close launcher" onclick={close}>×</button>
  <div class="mark" aria-hidden="true">の</div>
  <section>
    <span>{mode === "file" ? "OPEN LOCAL VIDEO" : "START LIVE CAPTIONS"}</span>
    <h1>{mode === "file" ? (launchState === "hovering" ? "Drop to open" : "Video → subtitles") : "Listen to another app"}</h1>
    <p>{message}</p>
    <div class="actions">
      {#if mode === "file"}
        <button class="primary" onclick={chooseVideo} disabled={running}>{launchState === "error" ? "Choose Another" : "Choose Video"}</button>
        {#if launchState === "error" && retryPath}<button onclick={() => retryPath && launchFile(retryPath)} disabled={running}>Retry</button>{/if}
      {:else if launchState === "error"}
        <button class="primary" onclick={launchLive} disabled={running}>Try Again</button>
      {/if}
      {#if launchState === "error"}<button onclick={openSettings}>Open Settings</button>{/if}
      {#if !running}<button onclick={close}>Cancel</button>{/if}
      {#if running}<i aria-label="Working"></i>{/if}
    </div>
  </section>
</main>

<style>
  .launcher{position:fixed;inset:8px;display:grid;grid-template-columns:76px 1fr;gap:16px;align-items:center;padding:20px 24px 18px 18px;border:1px solid #76e7df66;border-radius:18px;background:linear-gradient(135deg,#0a1017f5,#101522f2);box-shadow:0 22px 70px #0009;color:#f7f7fb;transition:border-color .15s,transform .15s}.launcher.hovering{border-color:#ff74b9;transform:scale(.985);background:linear-gradient(135deg,#111925f8,#241322f5)}.launcher.error{border-color:#ff74b966}.mark{width:64px;height:64px;display:grid;place-items:center;border-radius:18px;background:#ff6fb5;color:#fff;font-size:28px;font-weight:900;box-shadow:0 0 30px #ff6fb544}.close{position:absolute;right:10px;top:8px;border:0;background:none;color:#77808e;font-size:20px}.launcher span{color:#70e5de;font:800 8px/1 "JetBrains Mono",monospace;letter-spacing:.17em}.launcher h1{margin:7px 0 5px;font-size:20px;letter-spacing:-.03em}.launcher p{height:30px;margin:0;color:#9aa3af;font-size:9px;line-height:1.5;overflow:hidden}.actions{display:flex;align-items:center;gap:7px;margin-top:10px}.actions button{border:1px solid #33404f;background:#111923;color:#cbd2db;border-radius:7px;padding:7px 10px;font-size:8px}.actions button.primary{border-color:#6edfd766;color:#83eee6}.actions button:disabled{opacity:.45}.actions i{width:12px;height:12px;margin-left:3px;border:2px solid #70e5de44;border-top-color:#70e5de;border-radius:50%;animation:spin .7s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
</style>
