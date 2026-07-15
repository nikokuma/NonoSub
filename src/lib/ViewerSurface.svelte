<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { SessionState, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS } from "./fixtures";
  import { activeSegments, canResumeForCoverage, formatTime, reduceSession, shouldPauseForCoverage } from "./session";
  import { initialSession, loadPreferences, savePreferences, subscribePreferences, subscribeSession } from "./runtime";
  import SubtitleStack from "./SubtitleStack.svelte";

  let session = $state<SessionState>(FIXTURE_EVENTS.reduce(reduceSession, structuredClone(EMPTY_SESSION)));
  let preferences = $state(loadPreferences());
  let video = $state<HTMLVideoElement>();
  let stage = $state<HTMLDivElement>();
  let currentMs = $state(0);
  let durationMs = $state(0);
  let playing = $state(false);
  let catchingUp = $state(false);
  let controlsVisible = $state(true);
  let dragging = $state(false);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let wasPlayingBeforeLesson = false;

  const active = $derived(activeSegments(session.segments, currentMs));
  const mediaUrl = $derived(session.media?.path ? convertFileSrc(session.media.path) : undefined);

  onMount(() => {
    document.documentElement.dataset.surface = "viewer";
    const cleanup: Array<() => void> = [];
    void initialSession().then((value) => session = value);
    void subscribeSession(() => session, (value) => session = value).then((unlisten) => cleanup.push(unlisten));
    void subscribePreferences((value) => preferences = value).then((unlisten) => cleanup.push(unlisten));
    if (isTauri()) {
      void listen<string>("tray-action", ({ payload }) => {
        if (payload === "play_pause") togglePlayback();
        if (payload === "toggle_subtitles") controlsVisible = !controlsVisible;
      }).then((unlisten) => cleanup.push(unlisten));
      void listen("lesson-closed", () => {
        if (wasPlayingBeforeLesson && video?.paused) void video.play();
        wasPlayingBeforeLesson = false;
      }).then((unlisten) => cleanup.push(unlisten));
      void listen("lesson-opened", () => {
        if (session.mode !== "file") return;
        wasPlayingBeforeLesson = Boolean(video && !video.paused);
        video?.pause();
      }).then((unlisten) => cleanup.push(unlisten));
    }
    activity();
    return () => { cleanup.forEach((stop) => stop()); if (hideTimer) clearTimeout(hideTimer); };
  });

  $effect(() => {
    if ((session.phase === "ready" || session.phase === "complete") && video?.paused && currentMs === 0 && !catchingUp) void video.play();
  });

  $effect(() => {
    const translatedThroughMs = session.translatedThroughMs;
    if (!catchingUp || !video?.paused) return;
    if (session.phase === "complete" || canResumeForCoverage(currentMs, translatedThroughMs)) {
      catchingUp = false;
      void video.play();
    }
  });

  function activity() {
    controlsVisible = true;
    if (hideTimer) clearTimeout(hideTimer);
    if (playing) hideTimer = setTimeout(() => controlsVisible = false, 1500);
  }

  function togglePlayback() {
    if (!video) return;
    if (video.paused) void video.play(); else video.pause();
  }

  function updateTime() {
    if (!video) return;
    currentMs = video.currentTime * 1000;
    if (session.phase !== "complete" && !video.paused && shouldPauseForCoverage(currentMs, session.translatedThroughMs)) {
      video.pause();
      catchingUp = true;
    }
    if (catchingUp && canResumeForCoverage(currentMs, session.translatedThroughMs)) {
      catchingUp = false;
      void video.play();
    }
  }

  function updateDuration() {
    durationMs = video?.duration && Number.isFinite(video.duration) ? video.duration * 1000 : 0;
  }

  async function selectLine(segment: SubtitleSegment) {
    if (isTauri()) await invoke("select_lesson_segment", { segmentId: segment.id });
  }

  async function toggleFullscreen() {
    if (!isTauri()) return;
    const window = getCurrentWindow();
    await window.setFullscreen(!await window.isFullscreen());
  }

  function beginDrag(event: PointerEvent) {
    if (!stage) return;
    dragging = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    moveDrag(event);
  }

  function moveDrag(event: PointerEvent) {
    if (!dragging || !stage) return;
    const bounds = stage.getBoundingClientRect();
    preferences.style.position = {
      x: Math.min(.94, Math.max(.06, (event.clientX - bounds.left) / bounds.width)),
      y: Math.min(.94, Math.max(.1, (event.clientY - bounds.top) / bounds.height)),
    };
  }

  function finishDrag() {
    if (!dragging) return;
    dragging = false;
    void savePreferences(preferences);
  }
</script>

<svelte:window onpointermove={activity} onkeydown={activity} />

<div class="viewer" bind:this={stage}>
  {#if mediaUrl}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} src={mediaUrl} onloadedmetadata={updateDuration} ondurationchange={updateDuration} ontimeupdate={updateTime} onplay={() => { playing = true; activity(); }} onpause={() => playing = false} onended={() => playing = false}></video>
  {:else}<div class="fixture-backdrop"><span>駅前</span><small>NONOSUB VIEWER FIXTURE</small></div>{/if}

  <div role="group" aria-label="Movable subtitle overlay" class="subtitle-position" style={`left:${preferences.style.position.x * 100}%;top:${preferences.style.position.y * 100}%`} onpointerdown={beginDrag} onpointermove={moveDrag} onpointerup={finishDrag} onpointercancel={finishDrag}>
    <SubtitleStack segments={active} speakers={session.speakers} style={preferences.style} movable onselect={selectLine} />
  </div>

  {#if catchingUp}<div class="catching"><i>の</i>Nono is catching up…</div>{/if}
  <div class="chrome" class:visible={controlsVisible}>
    <div class="topbar" data-tauri-drag-region><span>の</span><b>{session.media?.fileName ?? "NonoSub"}</b><button onclick={() => invoke("open_surface", { surface: "workbench" })}>Settings</button></div>
    <div class="controls"><button class="play" onclick={togglePlayback}>{playing ? "❚❚" : "▶"}</button><time>{formatTime(currentMs)}</time><input type="range" min="0" max={durationMs || 33_000} value={currentMs} oninput={(event) => { currentMs = Number(event.currentTarget.value); if (video) video.currentTime = currentMs / 1000; }} /><time>{formatTime(durationMs)}</time><button onclick={toggleFullscreen}>⛶</button></div>
  </div>
</div>

<style>
  .viewer{position:fixed;inset:0;background:#000;overflow:hidden;cursor:none}.viewer:has(.chrome.visible){cursor:default}video,.fixture-backdrop{position:absolute;inset:0;width:100%;height:100%;object-fit:contain}.fixture-backdrop{display:grid;place-content:center;text-align:center;background:radial-gradient(circle at 30% 20%,#54345e,#161827 44%,#07080d)}.fixture-backdrop span{font-family:serif;font-size:80px;color:#ffffff20}.fixture-backdrop small{color:#ff80be;letter-spacing:.2em}.subtitle-position{position:absolute;transform:translate(-50%,-50%);z-index:4}.catching{position:absolute;left:50%;top:44%;transform:translate(-50%,-50%);display:flex;align-items:center;gap:9px;padding:11px 15px;background:#0d1119e8;border:1px solid #ffffff20;border-radius:8px;font-size:11px}.catching i{font-style:normal;display:grid;place-items:center;width:28px;height:28px;border-radius:7px;background:#ff70b7}.chrome{position:absolute;inset:0;opacity:0;pointer-events:none;transition:opacity .2s;background:linear-gradient(#000a,transparent 18%,transparent 76%,#000c)}.chrome.visible{opacity:1}.topbar,.controls{pointer-events:auto;position:absolute;left:0;right:0;display:flex;align-items:center}.topbar{top:0;height:55px;padding:0 18px;gap:9px}.topbar span{display:grid;place-items:center;width:28px;height:28px;background:#ff70b7;border-radius:7px}.topbar b{font-size:11px}.topbar button{margin-left:auto;border:1px solid #ffffff24;background:#0e1119aa;padding:7px 10px;border-radius:6px}.controls{bottom:0;height:62px;padding:0 22px;gap:12px}.controls button{border:0;background:none}.controls .play{width:34px;height:34px;background:white;color:#10131a;border-radius:50%}.controls input{flex:1;accent-color:#ff70b7}.controls time{font-size:9px;color:#c0c4cc}
</style>
