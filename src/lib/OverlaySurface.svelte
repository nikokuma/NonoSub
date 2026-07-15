<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { PhysicalPosition, PhysicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
  import type { SessionState, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS } from "./fixtures";
  import { reduceSession, visibleLiveSegments } from "./session";
  import { initialSession, loadPreferences, savePreferences, subscribePreferences, subscribeSession } from "./runtime";
  import LiveSubtitleStack from "./LiveSubtitleStack.svelte";

  let session = $state<SessionState>(FIXTURE_EVENTS.reduce(reduceSession, structuredClone(EMPTY_SESSION)));
  let preferences = $state(loadPreferences());
  let arranging = $state(false);
  let visible = $state(true);
  const captions = $derived(session.mode === "live" ? visibleLiveSegments(session.segments, session.liveSync) : session.segments.slice(-1));
  const waitingLabel = $derived(session.phase === "reconnecting"
    ? "Reconnecting to Nono…"
    : session.mode === "live" && session.segments.length > 0
      ? "Nono is coordinating subtitles…"
      : "Listening for speech…");

  onMount(() => {
    document.documentElement.dataset.surface = "overlay";
    const cleanup: Array<() => void> = [];
    void initialSession().then((value) => session = value);
    void subscribeSession(() => session, (value) => session = value).then((unlisten) => cleanup.push(unlisten));
    void subscribePreferences((value) => preferences = value).then((unlisten) => cleanup.push(unlisten));
    if (isTauri()) void listen<string>("tray-action", ({ payload }) => {
      if (payload === "arrange_overlay") arranging = !arranging;
      if (payload === "toggle_subtitles") visible = !visible;
    }).then((unlisten) => cleanup.push(unlisten));
    if (isTauri()) {
      const overlayWindow = getCurrentWindow();
      void restorePlacement();
      let placementTimer: ReturnType<typeof setTimeout> | undefined;
      void overlayWindow.onMoved(() => {
        if (placementTimer) clearTimeout(placementTimer);
        placementTimer = setTimeout(() => void rememberPlacement(), 180);
      }).then((unlisten) => cleanup.push(() => {
        if (placementTimer) clearTimeout(placementTimer);
        unlisten();
      }));
      void overlayWindow.onResized(() => {
        if (placementTimer) clearTimeout(placementTimer);
        placementTimer = setTimeout(() => void rememberPlacement(), 180);
      }).then((unlisten) => cleanup.push(unlisten));
    }
    return () => cleanup.forEach((stop) => stop());
  });

  async function selectLine(segment: SubtitleSegment) {
    if (isTauri()) await invoke("select_lesson_segment", { segmentId: segment.id });
  }

  async function startDragging() {
    if (isTauri()) await getCurrentWindow().startDragging();
  }

  async function restorePlacement() {
    const monitor = await currentMonitor();
    if (!monitor) return;
    const overlayWindow = getCurrentWindow();
    const width = Math.min(preferences.style.overlayWidth, monitor.size.width * 0.9);
    const height = Math.min(220, monitor.size.height * 0.3);
    await overlayWindow.setSize(new PhysicalSize(width, height));
    const x = monitor.position.x + preferences.style.overlayPosition.x * monitor.size.width - width / 2;
    const y = monitor.position.y + preferences.style.overlayPosition.y * monitor.size.height - height / 2;
    await overlayWindow.setPosition(new PhysicalPosition(
      Math.round(Math.min(monitor.position.x + monitor.size.width - width, Math.max(monitor.position.x, x))),
      Math.round(Math.min(monitor.position.y + monitor.size.height - height, Math.max(monitor.position.y, y))),
    ));
  }

  async function rememberPlacement() {
    const monitor = await currentMonitor();
    if (!monitor) return;
    const overlayWindow = getCurrentWindow();
    const [position, size] = await Promise.all([overlayWindow.outerPosition(), overlayWindow.outerSize()]);
    preferences.style.overlayPosition = {
      x: Math.min(.95, Math.max(.05, (position.x + size.width / 2 - monitor.position.x) / monitor.size.width)),
      y: Math.min(.95, Math.max(.05, (position.y + size.height / 2 - monitor.position.y) / monitor.size.height)),
    };
    preferences.style.overlayWidth = size.width;
    await savePreferences(preferences);
  }
</script>

<div class="overlay-shell" class:hidden={!visible} class:arranging>
  {#if arranging}<button class="grip" onpointerdown={startDragging}>⠿ MOVE NONOSUB</button>{/if}
  {#if captions.length > 0}<LiveSubtitleStack segment={captions[0]} speaker={captions[0].speakerId ? session.speakers[captions[0].speakerId] : undefined} style={preferences.style} sync={session.liveSync} onselect={selectLine} />{:else}<div class="waiting"><i></i>{waitingLabel}</div>{/if}
  {#if session.fatalError}<div class="error">{session.fatalError}</div>{/if}
</div>

<style>
  .overlay-shell{position:fixed;inset:0;display:grid;place-content:center;background:transparent;padding:20px}.overlay-shell.hidden{opacity:0;pointer-events:none}.overlay-shell.arranging{border:1px dashed #71e7df88;background:#0710161f}.grip{position:absolute;left:50%;top:4px;transform:translateX(-50%);border:1px solid #6de8df66;background:#08121bd9;color:#77e8df;border-radius:10px;padding:4px 10px;font-size:8px;letter-spacing:.14em}.waiting{display:flex;align-items:center;gap:9px;padding:8px 12px;border:1px solid #ffffff20;background:#080b11d9;border-radius:8px;color:#aeb5c0;font-size:10px}.waiting i{width:6px;height:6px;border-radius:50%;background:#70e7c6;box-shadow:0 0 10px #70e7c6}.error{position:absolute;left:50%;bottom:4px;transform:translateX(-50%);background:#280e18e8;color:#ffafd1;padding:5px 10px;border-radius:5px;font-size:8px}
</style>
