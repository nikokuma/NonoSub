<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { LogicalSize, PhysicalPosition, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
  import type { SessionState, StyleSettings, SubtitleDisplayMode, SubtitlePreset, SubtitleSegment } from "./contracts";
  import { visibleLiveSegments } from "./session";
  import { effectiveStyle } from "./preferences";
  import { initialSession, loadPreferences, maintainSubscription, savePreferencePatch, subscribePreferences, subscribeSession } from "./runtime";
  import { resolveOverlayGeometry } from "./overlayGeometry";
  import LiveSubtitleStack from "./LiveSubtitleStack.svelte";

  let session = $state<SessionState>(initialSession());
  let preferences = $state(loadPreferences());
  let arranging = $state(false);
  let visible = $state(true);
  let suppressSelection = false;
  let dragCandidate: { pointerId: number; startX: number; startY: number; target: HTMLElement } | null = null;
  let dragging = $state(false);
  let captionHost: HTMLDivElement;
  let fitTimer: ReturnType<typeof setTimeout> | undefined;
  let suppressPlacementUntil = 0;
  let connectionIssue = $state("");
  let gapNotice = $state("");
  let shownGapMessage = "";
  let gapNoticeTimer: ReturnType<typeof setTimeout> | undefined;
  const fixturePreset = typeof window !== "undefined" && !isTauri()
    ? parseFixturePreset(new URLSearchParams(window.location.search).get("preset"))
    : undefined;
  const fixtureBackdrop = typeof window !== "undefined" && !isTauri()
    && new URLSearchParams(window.location.search).get("backdrop") === "split";
  const fixtureDisplayMode = typeof window !== "undefined" && !isTauri()
    ? parseFixtureDisplayMode(new URLSearchParams(window.location.search).get("display"))
    : undefined;
  const fixtureWaiting = typeof window !== "undefined" && !isTauri()
    && new URLSearchParams(window.location.search).get("state") === "waiting";
  const captions = $derived(fixtureWaiting
    ? []
    : session.mode === "live"
      ? visibleLiveSegments(session.segments, session.liveSync, preferences.sync.liveMode)
      : session.segments.slice(-1));
  const sessionStyle = $derived(effectiveStyle(preferences.style, session.processingMode));
  const activeStyle = $derived({
    ...sessionStyle,
    preset: fixturePreset ?? sessionStyle.preset,
    displayMode: fixtureDisplayMode ?? sessionStyle.displayMode,
  });
  const waitingLabel = $derived(connectionIssue || (session.phase === "reconnecting"
    ? "Reconnecting to Nono…"
    : session.processingMode === "original_only"
      ? "Listening for original speech…"
    : session.mode === "live" && session.segments.length > 0
      ? "Nono is coordinating subtitles…"
      : "Listening for speech…"));
  const waitingStyle = $derived<StyleSettings>({
    ...activeStyle,
    displayMode: activeStyle.displayMode === "translation" ? "translation" : "source",
    showSpeakerNames: false,
  });
  const waitingSegment = $derived<SubtitleSegment>({
    id: "__live-waiting__",
    origin: "live",
    startMs: 0,
    endMs: 0,
    sourceText: activeStyle.displayMode === "translation" ? "" : waitingLabel,
    translationText: activeStyle.displayMode === "translation" ? waitingLabel : undefined,
    isProvisional: true,
    transcriptionStatus: "pending",
    translationStatus: activeStyle.displayMode === "translation" ? "complete" : "pending",
  });
  const displayedSegment = $derived(captions[0] ?? waitingSegment);
  const displayedStyle = $derived(captions.length > 0 ? activeStyle : waitingStyle);
  const latestAudioGap = $derived(session.errors.findLast((error) => error.code === "live_audio_gap"));

  $effect(() => {
    const message = latestAudioGap?.message ?? "";
    if (!message || message === shownGapMessage) return;
    shownGapMessage = message;
    gapNotice = message;
    if (gapNoticeTimer) clearTimeout(gapNoticeTimer);
    gapNoticeTimer = setTimeout(() => gapNotice = "", 4_000);
  });

  function parseFixturePreset(value: string | null): SubtitlePreset | undefined {
    return value && ["clean", "classic-outline", "yellow-drop", "fallout", "momento", "wired"].includes(value)
      ? value as SubtitlePreset
      : undefined;
  }

  function parseFixtureDisplayMode(value: string | null): SubtitleDisplayMode | undefined {
    return value && ["source", "translation", "both"].includes(value)
      ? value as SubtitleDisplayMode
      : undefined;
  }

  onMount(() => {
    document.documentElement.dataset.surface = "overlay";
    const cleanup: Array<() => void> = [];
    cleanup.push(maintainSubscription(() => subscribeSession((value) => session = value), (message) => connectionIssue = message));
    cleanup.push(maintainSubscription(() => subscribePreferences((value) => preferences = value), (message) => connectionIssue = message));
    if (isTauri()) void listen<string>("tray-action", ({ payload }) => {
      if (payload === "arrange_overlay") arranging = !arranging;
      if (payload === "show_subtitles") visible = true;
      if (payload === "toggle_subtitles") visible = !visible;
    }).then((unlisten) => cleanup.push(unlisten));
    if (isTauri()) {
      const overlayWindow = getCurrentWindow();
      void restorePlacement().then(() => scheduleContentFit());
      let placementTimer: ReturnType<typeof setTimeout> | undefined;
      void overlayWindow.onMoved(() => {
        if (performance.now() < suppressPlacementUntil) return;
        if (placementTimer) clearTimeout(placementTimer);
        placementTimer = setTimeout(() => void rememberPlacement(), 180);
      }).then((unlisten) => cleanup.push(() => {
        if (placementTimer) clearTimeout(placementTimer);
        unlisten();
      }));
      void overlayWindow.onResized(() => {
        if (performance.now() < suppressPlacementUntil) return;
        if (placementTimer) clearTimeout(placementTimer);
        placementTimer = setTimeout(() => void rememberPlacement(), 180);
      }).then((unlisten) => cleanup.push(unlisten));

      const contentObserver = new ResizeObserver(() => scheduleContentFit());
      if (captionHost) contentObserver.observe(captionHost);
      cleanup.push(() => {
        contentObserver.disconnect();
        if (fitTimer) clearTimeout(fitTimer);
        if (gapNoticeTimer) clearTimeout(gapNoticeTimer);
      });
    }
    return () => cleanup.forEach((stop) => stop());
  });

  async function openComposer(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    const segmentId = (event.target as HTMLElement | null)?.closest<HTMLElement>("[data-segment-id]")?.dataset.segmentId;
    const segment = session.segments.find((candidate) => candidate.id === segmentId);
    if (!isTauri() || !segment || segment.isProvisional) return;
    await invoke("open_lesson_composer", {
      segmentId: segment.id,
      sourceSurface: "overlay",
      cursorX: event.clientX,
      cursorY: event.clientY,
      experimentalExternalPause: preferences.experimentalExternalPause,
    });
  }

  function suppressLookup(node: HTMLElement) {
    const prevent = (event: Event) => event.preventDefault();
    for (const name of ["selectstart", "dragstart", "webkitmouseforcewillbegin"]) node.addEventListener(name, prevent);
    return { destroy: () => {
      for (const name of ["selectstart", "dragstart", "webkitmouseforcewillbegin"]) node.removeEventListener(name, prevent);
    } };
  }

  function beginDragging(event: PointerEvent) {
    if (!isTauri() || event.button !== 0) return;
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);
    dragCandidate = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      target,
    };
  }

  function moveDragging(event: PointerEvent) {
    if (!dragCandidate || dragCandidate.pointerId !== event.pointerId || dragging) return;
    if (Math.hypot(event.clientX - dragCandidate.startX, event.clientY - dragCandidate.startY) < 6) return;
    dragging = true;
    suppressSelection = true;
    void getCurrentWindow().startDragging().finally(() => {
      dragging = false;
      dragCandidate = null;
      window.setTimeout(() => suppressSelection = false, 350);
    });
  }

  function finishDragging(event: PointerEvent) {
    if (dragCandidate?.target.hasPointerCapture(event.pointerId)) dragCandidate.target.releasePointerCapture(event.pointerId);
    if (!dragging) dragCandidate = null;
  }

  async function restorePlacement() {
    const monitor = await currentMonitor();
    if (!monitor) return;
    const overlayWindow = getCurrentWindow();
    const geometry = overlayGeometry(monitor, 180);
    suppressPlacementUntil = performance.now() + 500;
    await overlayWindow.setSize(new LogicalSize(geometry.logicalWidth, geometry.logicalHeight));
    await overlayWindow.setPosition(new PhysicalPosition(geometry.physicalX, geometry.physicalY));
  }

  function scheduleContentFit() {
    if (!isTauri()) return;
    if (fitTimer) clearTimeout(fitTimer);
    fitTimer = setTimeout(() => void fitWindowToContent(), 34);
  }

  async function fitWindowToContent() {
    const monitor = await currentMonitor();
    if (!monitor || !captionHost) return;
    const contentHeight = Math.ceil(captionHost.getBoundingClientRect().height);
    if (!Number.isFinite(contentHeight) || contentHeight <= 0) return;
    const geometry = overlayGeometry(monitor, contentHeight);
    const overlayWindow = getCurrentWindow();
    const currentSize = await overlayWindow.innerSize();
    const sizeChanged = Math.abs(currentSize.width - geometry.physicalWidth) > 2
      || Math.abs(currentSize.height - geometry.physicalHeight) > 2;
    suppressPlacementUntil = performance.now() + 500;
    if (sizeChanged) await overlayWindow.setSize(new LogicalSize(geometry.logicalWidth, geometry.logicalHeight));
    await overlayWindow.setPosition(new PhysicalPosition(geometry.physicalX, geometry.physicalY));
  }

  function overlayGeometry(
    monitor: NonNullable<Awaited<ReturnType<typeof currentMonitor>>>,
    contentHeight: number,
  ) {
    return resolveOverlayGeometry({
      x: monitor.position.x,
      y: monitor.position.y,
      width: monitor.size.width,
      height: monitor.size.height,
      scaleFactor: monitor.scaleFactor || 1,
    }, {
      normalizedPosition: preferences.style.overlayPosition,
      preferredLogicalWidth: preferences.style.overlayWidth,
      contentLogicalHeight: contentHeight,
      maximumLogicalHeight: 240,
      verticalMargin: 30,
    });
  }

  async function rememberPlacement() {
    const monitor = await currentMonitor();
    if (!monitor) return;
    const overlayWindow = getCurrentWindow();
    const [position, size] = await Promise.all([overlayWindow.outerPosition(), overlayWindow.outerSize()]);
    const scale = monitor.scaleFactor || 1;
    preferences.style.overlayPosition = {
      x: Math.min(.95, Math.max(.05, (position.x + size.width / 2 - monitor.position.x) / monitor.size.width)),
      y: Math.min(.95, Math.max(.05, (position.y + size.height / 2 - monitor.position.y) / monitor.size.height)),
    };
    preferences.style.overlayWidth = size.width / scale;
    preferences = await savePreferencePatch({
      style: {
        overlayPosition: preferences.style.overlayPosition,
        overlayWidth: preferences.style.overlayWidth,
      },
    });
  }
</script>

<div
  class="overlay-shell"
  class:hidden={!visible}
  class:arranging
  class:dragging
  class:fixture-backdrop={fixtureBackdrop}
  use:suppressLookup
  oncontextmenu={openComposer}
  onpointerdown={beginDragging}
  onpointermove={moveDragging}
  onpointerup={finishDragging}
  onpointercancel={finishDragging}
  role="group"
  aria-label="Live subtitle overlay. Drag to reposition; right-click to ask Nono."
>
  {#if arranging}
    <div class="grip" aria-hidden="true">⠿ DRAG NONOSUB</div>
    <button class="stop-live" onclick={(event) => { event.stopPropagation(); void invoke("end_session", { reason: "user_stop" }); }}>Stop Live</button>
  {/if}
  <div class="caption-host" bind:this={captionHost}>
    <LiveSubtitleStack segment={displayedSegment} speaker={displayedSegment.speakerId ? session.speakers[displayedSegment.speakerId] : undefined} style={displayedStyle} sync={session.liveSync} liveMode={preferences.sync.liveMode} processingMode={session.processingMode} />
    {#if gapNotice}<div class="gap-notice">{gapNotice}</div>{/if}
    {#if session.fatalError}<div class="error">{session.fatalError}</div>{/if}
  </div>
</div>

<style>
  .overlay-shell{position:fixed;inset:0;display:grid;place-content:center;background:transparent;padding:30px 20px;cursor:grab;touch-action:none}.caption-host{width:100%;min-width:0;max-height:180px;overflow:visible;display:grid;place-items:center}.overlay-shell.dragging{cursor:grabbing}.overlay-shell.hidden{opacity:0;pointer-events:none}.overlay-shell.arranging{border:1px dashed #71e7df88;background:#0710161f}.overlay-shell.fixture-backdrop{background:linear-gradient(105deg,#d9e8ec 0 48%,#263b4b 48% 52%,#101421 52%)}.grip{position:absolute;left:50%;top:4px;transform:translateX(-50%);border:1px solid #6de8df66;background:#08121bd9;color:#77e8df;border-radius:10px;padding:4px 10px;font-size:8px;letter-spacing:.14em;pointer-events:none}.stop-live{position:absolute;right:8px;top:5px;z-index:5;border:1px solid #ff82ad88;background:#2b0a17e8;color:#ffd3e3;border-radius:8px;padding:4px 9px;font-size:8px;cursor:pointer}.gap-notice{margin-top:4px;padding:3px 7px;border-radius:5px;background:#25160ddd;color:#ffc787;font-size:7px}.error{margin-top:6px;background:#280e18e8;color:#ffafd1;padding:5px 10px;border-radius:5px;font-size:8px}
</style>
