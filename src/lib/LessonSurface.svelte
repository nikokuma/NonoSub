<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { LogicalSize, PhysicalPosition, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
  import type { LessonCard, LessonMessage, LessonOpenContext, LessonSurfaceMode, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS, FIXTURE_LESSON } from "./fixtures";
  import { dominantChalkColor, isLessonSkipped, lessonStepOrder } from "./lesson";
  import { lessonThreadKey } from "./lessonIdentity";
  import { calculateLessonStageLayout, LESSON_WINDOW_TARGET } from "./lessonLayout";
  import { capLessonMessages, recallLessonThread, rememberLessonThread } from "./lessonThreads";
  import { fitLogicalWindowSize, fitLogicalWindowSizeProportionally, makeMonitorKey, normalizeLessonPlacement, resolveLessonPosition, shouldPersistLessonPlacement, type MonitorGeometry } from "./floatingPlacement";
  import { reduceSession } from "./session";
  import { loadPreferences, maintainSubscription, savePreferencePatch, subscribePreferences } from "./runtime";
  import ChalkDemo from "./ChalkDemo.svelte";
  import ChalkPhrase from "./ChalkPhrase.svelte";
  import ChalkStepNumber from "./ChalkStepNumber.svelte";
  import NonoScene from "./NonoScene.svelte";
  import type { NonoMood } from "./nonoToon";
  import LessonQuestionComposer from "./LessonQuestionComposer.svelte";
  import { IDLE_TAIL_PRESENTATION, tipUnderlineProgress, type TailPresentation, type TailPresentationPhase } from "./tailPresentation";

  type BoardPhase = "idle" | "erasing" | "thinking" | "writing";

  const UNDERLINE_LEAD_MS = 350;

  const fixtureSession = FIXTURE_EVENTS.reduce(reduceSession, structuredClone(EMPTY_SESSION));
  const emptySegment: SubtitleSegment = {
    id: "",
    origin: "file",
    startMs: 0,
    endMs: 0,
    sourceText: "",
    isProvisional: false,
    transcriptionStatus: "pending",
    translationStatus: "pending",
  };
  const emptyOpenContext: LessonOpenContext = {
    selectionId: 0,
    sessionId: "",
    sourceSurface: "workbench",
    segmentId: "",
    selectedSegment: isTauri() ? emptySegment : fixtureSession.segments[3],
    cursorX: 0,
    cursorY: 0,
    externalMediaControl: "not_requested",
  };
  let preferences = $state(loadPreferences());
  let messages = $state<LessonMessage[]>([]);
  let threads = $state<Record<string, LessonMessage[]>>({});
  let threadOrder = $state<string[]>([]);
  let threadSessionId = "";
  let loading = $state(false);
  let error = $state("");
  let activeSelectionId = 0;
  let activeThreadKey: string | undefined;
  let requestGeneration = 0;
  let boardPhase = $state<BoardPhase>("idle");
  let activeMomentIndex = $state(0);
  let activeCardKey = $state<string>();
  let skippedCardKey = $state<string>();
  let boardImageFailed = $state(false);
  let boardElement = $state<HTMLDivElement>();
  let tailPresentation = $state<TailPresentation>({ ...IDLE_TAIL_PRESENTATION });
  let tailRigAvailable = $state(false);
  let nonoGesture = $state<NonoMood | undefined>();
  let cueGeneration = 0;
  let underlineElementGeneration = -1;
  let activeUnderlineElement: HTMLElement | undefined;
  let underlineProgress = $state<Record<string, number>>({});
  let mode = $state<LessonSurfaceMode>("compose");
  let followupOpen = $state(false);
  let openContext = $state<LessonOpenContext>(emptyOpenContext);
  let placementTimer: ReturnType<typeof setTimeout> | undefined;
  let placementSuppressedUntil = 0;
  let viewportWidth = $state(typeof window === "undefined" ? 980 : window.innerWidth);
  let viewportHeight = $state(typeof window === "undefined" ? 620 : window.innerHeight);

  const selected = $derived(openContext.selectionId > 0 ? openContext.selectedSegment : (isTauri() ? undefined : fixtureSession.segments[3]));
  const latestAssistant = $derived(messages.findLast((message) => message.card?.selectedSegmentId === selected?.id));
  const latestCard = $derived(latestAssistant?.card ?? (isTauri() ? undefined : FIXTURE_LESSON));
  const latestCardKey = $derived(latestAssistant?.id ?? (isTauri() ? undefined : "fixture"));
  const lessonSkipped = $derived(isLessonSkipped(latestCardKey, skippedCardKey));
  const currentMoment = $derived(lessonSkipped ? undefined : latestCard?.moments[activeMomentIndex]);
  const stepOrder = $derived(lessonStepOrder(currentMoment, Boolean(selected)));
  const hasMoreMoments = $derived(Boolean(latestCard && activeMomentIndex < latestCard.moments.length - 1));
  const bubbleText = $derived(
    error && mode === "lesson"
      ? error
      : boardPhase === "erasing"
      ? "New question, new board. Mind the chalk dust!"
      : boardPhase === "thinking"
        ? "Hm… I’m choosing the useful part, not every fact in the textbook."
        : lessonSkipped
          ? "Okay, we’ll leave the rest there. Ask about the part you actually care about."
          : currentMoment?.speechBubble ?? "Pick what you want me to explain. I brought chalk."
  );
  const activePointCueId = $derived(tailPresentation.phase === "idle" ? undefined : tailPresentation.pointCueId);
  const nonoMood = $derived<NonoMood>(
    nonoGesture ?? (boardPhase === "thinking" ? "think" : boardPhase === "writing" ? "neutral" : "idle"),
  );
  const stageLayout = $derived(calculateLessonStageLayout(viewportWidth, viewportHeight, followupOpen));

  $effect(() => {
    if (boardPhase !== "idle") nonoGesture = undefined;
  });

  onMount(() => {
    document.documentElement.dataset.surface = "lesson";
    const cleanup: Array<() => void> = [];
    cleanup.push(maintainSubscription(
      () => subscribePreferences((value) => preferences = value),
      (message) => { if (message) error = message; },
    ));
    void tick().then(() => playCueSequence());
    if (isTauri()) {
      void invoke<LessonOpenContext | null>("get_lesson_open_context").then((context) => {
        if (context) applyOpenContext(context);
      });
      void listen<LessonOpenContext>("lesson-composer-opened", ({ payload }) => {
        applyOpenContext(payload);
      }).then((unlisten) => cleanup.push(unlisten));
      void listen("lesson-selection-invalidated", () => {
        openContext = emptyOpenContext;
      }).then((unlisten) => cleanup.push(unlisten));
      void getCurrentWindow().onMoved(() => {
        if (placementTimer) clearTimeout(placementTimer);
        placementTimer = setTimeout(() => void rememberLessonPlacement(), 180);
      }).then((unlisten) => cleanup.push(unlisten));
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (followupOpen) followupOpen = false;
      else void closeLesson();
    };
    window.addEventListener("keydown", escape);
    return () => {
      cleanup.forEach((stop) => stop());
      if (placementTimer) clearTimeout(placementTimer);
      window.removeEventListener("keydown", escape);
    };
  });

  function applyOpenContext(context: LessonOpenContext) {
    suppressPlacementEvents();
    if (context.sessionId && context.sessionId !== threadSessionId) {
      threads = {};
      threadOrder = [];
      threadSessionId = context.sessionId;
      activeThreadKey = undefined;
      activeSelectionId = 0;
      messages = [];
    }
    openContext = context;
    mode = "compose";
    followupOpen = false;
    error = "";
    void resizeLessonWindow("compose");
  }

  $effect(() => {
    const nextSelectionId = openContext.selectionId;
    if (nextSelectionId === activeSelectionId) return;
    if (activeThreadKey) {
      const stored = rememberLessonThread({ threads, order: threadOrder }, activeThreadKey, messages);
      threads = stored.threads;
      threadOrder = stored.order;
    }
    activeSelectionId = nextSelectionId;
    activeThreadKey = nextSelectionId > 0 ? lessonThreadKey(openContext) : undefined;
    if (activeThreadKey) {
      const recalled = recallLessonThread({ threads, order: threadOrder }, activeThreadKey);
      threads = recalled.store.threads;
      threadOrder = recalled.store.order;
      messages = recalled.messages;
    } else {
      messages = [];
    }
    error = "";
    loading = false;
    boardPhase = "idle";
    activeMomentIndex = 0;
    activeCardKey = undefined;
    skippedCardKey = undefined;
    followupOpen = false;
    requestGeneration += 1;
    nonoGesture = undefined;
    cancelCueSequence();
  });

  $effect(() => {
    const nextCardKey = latestCardKey;
    if (!nextCardKey || nextCardKey === activeCardKey) return;
    activeCardKey = nextCardKey;
    activeMomentIndex = 0;
    skippedCardKey = undefined;
    cancelCueSequence();
  });

  async function ask(question: string) {
    if (!selected || !question.trim() || loading) return;
    const normalizedQuestion = question.trim();
    if (Array.from(normalizedQuestion).length > 800) {
      error = "Questions for Nono can be at most 800 characters.";
      mode = "error";
      return;
    }
    cancelCueSequence();
    const requestSelectionId = openContext.selectionId;
    const requestId = ++requestGeneration;
    const eraseExistingBoard = Boolean(currentMoment);
    const priorThread: Array<{ role: "user" | "assistant"; text: string }> = [];
    let remainingThreadCharacters = 6_000;
    for (const message of messages.slice().reverse()) {
      if (priorThread.length >= 12 || remainingThreadCharacters === 0) break;
      const text = Array.from(message.text.trim()).slice(0, remainingThreadCharacters).join("");
      if (!text) continue;
      remainingThreadCharacters -= Array.from(text).length;
      priorThread.unshift({ role: message.role, text });
    }
    const userMessage: LessonMessage = { id: crypto.randomUUID(), role: "user", text: normalizedQuestion };
    messages = capLessonMessages([...messages, userMessage]);
    loading = true;
    error = "";
    followupOpen = false;
    mode = eraseExistingBoard ? "lesson" : "thinking";
    boardPhase = eraseExistingBoard ? "erasing" : "thinking";
    try {
      const request: Promise<LessonCard> = isTauri()
        ? invoke<LessonCard>("request_lesson", {
            selectionId: requestSelectionId,
            question: normalizedQuestion,
            learnerLevel: preferences.level,
            thread: priorThread,
          })
        : Promise.resolve(FIXTURE_LESSON);
      const eraseTransition = eraseExistingBoard
        ? sleep(560).then(() => {
            if (requestId === requestGeneration) boardPhase = "thinking";
          })
        : Promise.resolve();
      const [card] = await Promise.all([request, eraseTransition]);
      if (requestId !== requestGeneration || openContext.selectionId !== requestSelectionId) return;
      messages = capLessonMessages([...messages, { id: crypto.randomUUID(), role: "assistant", text: card.moments[0].speechBubble, card }]);
      mode = "lesson";
      await resizeLessonWindow("lesson");
      boardPhase = "writing";
      await sleep(720);
      if (requestId === requestGeneration) {
        boardPhase = "idle";
        if (!hasMoreMoments) nonoGesture = "thumbs_up";
        void playCueSequence();
      }
    } catch (requestError) {
      if (requestId !== requestGeneration) return;
      error = errorMessage(requestError);
      boardPhase = "idle";
      nonoGesture = "surprised";
      mode = eraseExistingBoard ? "lesson" : "error";
      await resizeLessonWindow(eraseExistingBoard ? "lesson" : "compose");
    } finally {
      if (requestId === requestGeneration) loading = false;
    }
  }

  async function nextMoment() {
    if (!latestCard || !hasMoreMoments || boardPhase !== "idle") return;
    cancelCueSequence();
    boardPhase = "erasing";
    await sleep(440);
    activeMomentIndex += 1;
    boardPhase = "writing";
    await sleep(620);
    boardPhase = "idle";
    if (!hasMoreMoments) nonoGesture = "thumbs_up";
    void playCueSequence();
  }

  async function skipRemaining() {
    if (!latestCardKey || boardPhase !== "idle") return;
    cancelCueSequence();
    boardPhase = "erasing";
    await sleep(440);
    skippedCardKey = latestCardKey;
    boardPhase = "idle";
    nonoGesture = "cheer";
  }

  async function closeLesson() {
    cancelCueSequence();
    if (isTauri()) {
      await invoke("close_lesson_surface");
    }
  }

  async function resumeExternalMedia() {
    if (!isTauri() || openContext.externalMediaControl !== "paused") return;
    const result = await invoke<LessonOpenContext["externalMediaControl"]>("post_media_play_pause");
    openContext = {
      ...openContext,
      externalMediaControl: result === "paused" ? "not_requested" : result,
    };
  }

  async function startWindowDrag(event: PointerEvent) {
    if (!isTauri() || event.button !== 0) return;
    event.preventDefault();
    await getCurrentWindow().startDragging();
  }

  function monitorGeometry(monitor: NonNullable<Awaited<ReturnType<typeof currentMonitor>>>): MonitorGeometry {
    const base = {
      x: monitor.position.x,
      y: monitor.position.y,
      width: monitor.size.width,
      height: monitor.size.height,
    };
    return { ...base, key: makeMonitorKey(monitor.name, base) };
  }

  async function restoreLessonPlacement() {
    const monitor = await currentMonitor();
    if (!monitor) return;
    const geometry = monitorGeometry(monitor);
    const lessonWindow = getCurrentWindow();
    suppressPlacementEvents();
    const scale = monitor.scaleFactor || 1;
    const { width, height } = fitLogicalWindowSizeProportionally(LESSON_WINDOW_TARGET, geometry, scale);
    await lessonWindow.setSize(new LogicalSize(width, height));
    const physicalWidth = Math.round(width * scale);
    const physicalHeight = Math.round(height * scale);
    const placement = preferences.lessonPlacements[geometry.key];
    const position = resolveLessonPosition(geometry, physicalWidth, physicalHeight, placement);
    await lessonWindow.setPosition(new PhysicalPosition(position.x, position.y));
  }

  async function resizeLessonWindow(targetMode: "compose" | "lesson") {
    if (!isTauri()) return;
    if (targetMode === "lesson") {
      await restoreLessonPlacement();
      return;
    }
    const monitor = await currentMonitor();
    if (!monitor) return;
    const geometry = monitorGeometry(monitor);
    const lessonWindow = getCurrentWindow();
    suppressPlacementEvents();
    const current = await lessonWindow.outerPosition();
    const scale = monitor.scaleFactor || 1;
    const desired = { width: 720, height: 210 };
    const { width, height } = fitLogicalWindowSize(desired, geometry, scale);
    await lessonWindow.setSize(new LogicalSize(width, height));
    const physicalWidth = Math.round(width * scale);
    const physicalHeight = Math.round(height * scale);
    const x = Math.min(geometry.x + geometry.width - physicalWidth - 18, Math.max(geometry.x + 18, current.x));
    const y = Math.min(geometry.y + geometry.height - physicalHeight - 18, Math.max(geometry.y + 18, current.y));
    await lessonWindow.setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
  }

  async function rememberLessonPlacement() {
    if (!isTauri() || !shouldPersistLessonPlacement(mode, placementSuppressedUntil, performance.now())) return;
    const monitor = await currentMonitor();
    if (!monitor) return;
    const geometry = monitorGeometry(monitor);
    const lessonWindow = getCurrentWindow();
    const [position, size] = await Promise.all([lessonWindow.outerPosition(), lessonWindow.outerSize()]);
    const nextPlacements = { ...preferences.lessonPlacements };
    delete nextPlacements[geometry.key];
    nextPlacements[geometry.key] = normalizeLessonPlacement(geometry, {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
      });
    preferences.lessonPlacements = Object.fromEntries(Object.entries(nextPlacements).slice(-8));
    preferences = await savePreferencePatch({
      lessonPlacements: { [geometry.key]: preferences.lessonPlacements[geometry.key] },
    });
  }

  function suppressPlacementEvents(durationMs = 550) {
    placementSuppressedUntil = performance.now() + durationMs;
    if (placementTimer) {
      clearTimeout(placementTimer);
      placementTimer = undefined;
    }
  }

  function errorMessage(value: unknown): string {
    return typeof value === "object" && value && "message" in value ? String(value.message) : String(value);
  }

  function sleep(milliseconds: number) {
    return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
  }

  function cueUnderlineProgress(cueId: string): number {
    return underlineProgress[cueId] ?? 0;
  }

  function handleTailTip(report: { cueId: string; x: number; y: number }) {
    if (
      !tailRigAvailable
      || underlineElementGeneration !== cueGeneration
      || tailPresentation.sequenceId !== cueGeneration
      || (tailPresentation.phase !== "underline" && tailPresentation.phase !== "retract")
      || tailPresentation.underlineCueId !== report.cueId
    ) return;
    if (activeUnderlineElement?.dataset.cueId !== report.cueId) {
      activeUnderlineElement = boardElement?.querySelector<HTMLElement>(`[data-cue-id="${report.cueId}"]`) ?? undefined;
    }
    if (!activeUnderlineElement) return;
    const rect = activeUnderlineElement.getBoundingClientRect();
    const rtl = getComputedStyle(activeUnderlineElement).direction === "rtl";
    underlineProgress = {
      ...underlineProgress,
      [report.cueId]: tipUnderlineProgress(rect, report.x, rtl, underlineProgress[report.cueId] ?? 0),
    };
  }

  function cancelCueSequence() {
    cueGeneration += 1;
    underlineElementGeneration = -1;
    activeUnderlineElement = undefined;
    underlineProgress = {};
    tailPresentation = { ...IDLE_TAIL_PRESENTATION, sequenceId: cueGeneration };
  }

  async function playCueSequence() {
    if (!currentMoment || !boardElement || boardPhase !== "idle") return;
    const generation = ++cueGeneration;
    underlineProgress = {};
    await tick();
    await sleep(250);
    if (generation !== cueGeneration || !boardElement) return;
    const point = boardElement.querySelector<HTMLElement>('[data-tail-cue="point"]');
    const underline = boardElement.querySelector<HTMLElement>('[data-tail-cue="underline"]');
    const pointCueId = point?.dataset.cueId;
    const underlineCueId = underline?.dataset.cueId;
    const underlineColor = underline?.dataset.chalkColor as TailPresentation["underlineColor"];
    if (!pointCueId && !underlineCueId) return;
    underlineElementGeneration = generation;
    activeUnderlineElement = underline ?? undefined;

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      if (underlineCueId) underlineProgress = { [underlineCueId]: 1 };
      return;
    }

    const cues = { pointCueId, underlineCueId, underlineColor };
    if (pointCueId) {
      await animatePresentation(generation, "point", 450, cues);
      if (generation !== cueGeneration) return;
    }
    if (underlineCueId) {
      if (!pointCueId) {
        await sleep(UNDERLINE_LEAD_MS);
        if (generation !== cueGeneration) return;
      }
      await animatePresentation(generation, "hold", 400, cues);
      if (generation !== cueGeneration) return;
      await animatePresentation(generation, "underline", 550, cues, (progress) => {
        if (!tailRigAvailable) underlineProgress = { ...underlineProgress, [underlineCueId]: progress };
      });
      if (generation !== cueGeneration) return;
      await sleep(320);
      if (generation !== cueGeneration) return;
      underlineProgress = { ...underlineProgress, [underlineCueId]: 1 };
      await animatePresentation(generation, "retract", 350, cues);
      if (generation !== cueGeneration) return;
    }
    if (pointCueId) tailPresentation = { sequenceId: generation, phase: "sustain", progress: 1, ...cues };
    else tailPresentation = { ...IDLE_TAIL_PRESENTATION, sequenceId: generation };
  }

  function animatePresentation(
    generation: number,
    phase: TailPresentationPhase,
    duration: number,
    cues: Pick<TailPresentation, "pointCueId" | "underlineCueId" | "underlineColor">,
    onProgress?: (progress: number) => void,
  ): Promise<void> {
    return new Promise((resolve) => {
      const startedAt = performance.now();
      const frame = (now: number) => {
        if (generation !== cueGeneration) return resolve();
        const progress = Math.max(0, Math.min(1, (now - startedAt) / duration));
        tailPresentation = { sequenceId: generation, phase, progress, ...cues };
        onProgress?.(progress);
        if (progress >= 1) resolve();
        else requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    });
  }
</script>

<svelte:window onresize={() => {
  viewportWidth = window.innerWidth;
  viewportHeight = window.innerHeight;
}} />

{#if selected && mode !== "lesson"}
  <div class="composer-shell">
    <LessonQuestionComposer
      segment={selected}
      style={preferences.style}
      {mode}
      {error}
      externalMediaControl={openContext.externalMediaControl}
      onsubmit={(question) => void ask(question)}
      oncancel={closeLesson}
      ondrag={startWindowDrag}
      onresumeexternal={() => void resumeExternalMedia()}
    />
  </div>
{:else}
<div class="lesson-shell">
  <section
    class="stage"
    class:compact={stageLayout.compact}
    style={`--stage-scale:${stageLayout.scale};--board-content-scale:${stageLayout.boardContentScale};--board-x:${stageLayout.board.x}px;--board-y:${stageLayout.board.y}px;--board-width:${stageLayout.board.width}px;--board-height:${stageLayout.board.height}px;--rail-x:${stageLayout.characterRail.x}px;--rail-y:${stageLayout.characterRail.y}px;--rail-width:${stageLayout.characterRail.width}px;--rail-height:${stageLayout.characterRail.height}px;--bubble-x:${stageLayout.bubble.x}px;--bubble-y:${stageLayout.bubble.y}px;--bubble-width:${stageLayout.bubble.width}px;--bubble-height:${stageLayout.bubble.height}px;--controls-x:${stageLayout.controls.x}px;--controls-y:${stageLayout.controls.y}px;--controls-width:${stageLayout.controls.width}px;--controls-height:${stageLayout.controls.height}px`}
  >
      <button class="floating-close" aria-label="Close Nono lesson" onclick={closeLesson}>×</button>
      <NonoScene presentation={tailPresentation} mood={nonoMood} viewport={stageLayout.characterViewport} onRigStatus={(available) => tailRigAvailable = available} onTailTip={handleTailTip} />
      <svg class="chalk-filter" width="0" height="0" aria-hidden="true">
        <filter id="chalk-roughen" x="-4%" y="-8%" width="108%" height="116%" color-interpolation-filters="sRGB">
          <feTurbulence type="fractalNoise" baseFrequency="0.72" numOctaves="1" seed="17" result="noise" />
          <feDisplacementMap in="SourceGraphic" in2="noise" scale="0.45" xChannelSelector="R" yChannelSelector="G" />
        </filter>
      </svg>
      <div class="bubble" class:thinking={boardPhase === "thinking"}>
        <button class="drag-handle bubble-drag" aria-label="Move lesson window" onpointerdown={startWindowDrag}></button>
        {bubbleText}
      </div>
      <div class="lesson-stack">
        <div class="board-prop" class:image-failed={boardImageFailed}>
          <button class="drag-handle board-drag" aria-label="Move lesson window" onpointerdown={startWindowDrag}></button>
          {#if !boardImageFailed}<img class="board-art" src="/assets/nono-chalkboard-anime.png" alt="" aria-hidden="true" onerror={() => boardImageFailed = true} />{/if}
          <div class="board" class:thinking={boardPhase === "thinking"} bind:this={boardElement}>
            {#if boardPhase === "erasing"}<div class="erase-sweep" aria-hidden="true"><span></span></div>{/if}
            <div class="board-top">
              <span data-teach-anchor="title">{currentMoment?.title ?? "Understanding the line"}</span>
              <div><i>{preferences.level}</i>{#if latestCard && currentMoment}<b>{activeMomentIndex + 1} / {latestCard.moments.length}</b>{/if}</div>
            </div>
            {#if selected}
              <div class="selected" data-teach-anchor="selected-source">
                <div class="numbered-source">
                  {#if stepOrder.source}<ChalkStepNumber number={stepOrder.source} label="Source sentence" accent={currentMoment?.sourceFocus.color ?? "white"} />{/if}
                  <ChalkPhrase
                    phrase={{ text: selected.sourceText, color: currentMoment?.sourceFocus.color ?? "white", mark: "none", tailCue: currentMoment?.sourceFocus.tailCue ?? "none" }}
                    cueId="source-focus"
                    underlineProgress={cueUnderlineProgress("source-focus")}
                    pointing={activePointCueId === "source-focus"}
                    rigAvailable={tailRigAvailable}
                  />
                </div>
                {#if selected.translationText}<small dir="auto">{selected.translationText}</small>{/if}
              </div>
            {/if}
            {#if boardPhase === "thinking"}
              <div class="board-empty waiting"><span></span><span></span><span></span><p>Choosing the next teaching moment…</p></div>
            {:else if currentMoment}
              <div class="board-content" class:composed={currentMoment.boardSections.length > 0 && currentMoment.demonstration.kind !== "none"} class:erasing={boardPhase === "erasing"} class:writing={boardPhase === "writing"}>
                <div class="sections">
                  {#each currentMoment.boardSections as section, sectionIndex}
                    <section style={`--delay:${sectionIndex * 100}ms`} data-teach-anchor={`section-${sectionIndex}`}>
                      <div class="section-heading">
                        <ChalkStepNumber number={stepOrder.sections[sectionIndex]} label={section.heading} accent={dominantChalkColor(section.lines.map((line) => line.color))} />
                        <h3 dir="auto">{section.heading}</h3>
                      </div>
                      {#each section.lines as line, lineIndex}
                        {@const cueId = `section-${sectionIndex}-line-${lineIndex}`}
                        <p><ChalkPhrase phrase={line} {cueId} underlineProgress={cueUnderlineProgress(cueId)} pointing={activePointCueId === cueId} rigAvailable={tailRigAvailable} /></p>
                      {/each}
                    </section>
                  {/each}
                </div>
                <ChalkDemo demo={currentMoment.demonstration} underlineProgressByCue={underlineProgress} pointCueId={activePointCueId} rigAvailable={tailRigAvailable} stepNumber={stepOrder.demonstration} />
                {#if currentMoment.ambiguityNote}
                  <div class="ambiguity" data-teach-anchor="ambiguity">
                    {#if stepOrder.ambiguity}<ChalkStepNumber number={stepOrder.ambiguity} label="Ambiguity note" accent={currentMoment.ambiguityNote.color} />{/if}
                    <b>?</b>
                    <ChalkPhrase phrase={currentMoment.ambiguityNote} cueId="ambiguity" underlineProgress={cueUnderlineProgress("ambiguity")} pointing={activePointCueId === "ambiguity"} rigAvailable={tailRigAvailable} />
                  </div>
                {/if}
              </div>
            {:else if lessonSkipped}<div class="board-empty">Lesson skipped. Ask about the part you actually care about.</div>
            {:else if !loading}<div class="board-empty">Ask a question and Nono will organize the answer here.</div>{/if}
          </div>
        </div>
        <nav class="deck-controls" aria-label="Lesson controls">
          <div class="progress-wrap">
            {#if latestCard && currentMoment && !lessonSkipped}
              <div class="progress" aria-label={`Teaching moment ${activeMomentIndex + 1} of ${latestCard.moments.length}`}>
                {#each latestCard.moments as _, index}<span class:active={index === activeMomentIndex} class:complete={index < activeMomentIndex}></span>{/each}
              </div>
              <small>Lesson {activeMomentIndex + 1} of {latestCard.moments.length}</small>
            {:else}<small>Ask Nono about this line</small>{/if}
          </div>
          {#if latestCard && currentMoment && latestCard.moments.length > 1 && !lessonSkipped}
            {#if hasMoreMoments}
              <button class="skip" onclick={skipRemaining} disabled={boardPhase !== "idle"}>Skip</button>
              <button class="next" onclick={nextMoment} disabled={boardPhase !== "idle"}>Next · {latestCard.moments[activeMomentIndex + 1].title}</button>
            {:else}<em>Complete</em>{/if}
          {/if}
          {#if followupOpen && selected}
            <div class="followup-composer">
              <LessonQuestionComposer segment={selected} style={preferences.style} mode="compose" compact onsubmit={(question) => void ask(question)} oncancel={() => followupOpen = false} />
            </div>
          {:else}
            <button class="ask-toggle" onclick={() => followupOpen = true} disabled={!selected || loading}>Ask Another</button>
          {/if}
        </nav>
      </div>
  </section>
</div>
{/if}

<style>
  .composer-shell{width:100vw;height:100vh;box-sizing:border-box;background:transparent}
  .lesson-shell{height:100vh;display:grid;grid-template-rows:minmax(0,1fr);align-content:start;color:#f7f5fb;background:transparent;overflow:hidden}
  .stage{position:relative;width:100%;height:100%;min-height:0;overflow:visible;background:transparent;box-sizing:border-box}
  .floating-close{position:absolute;z-index:30;left:calc(var(--rail-x) + var(--rail-width) - clamp(20px,calc(34px * var(--stage-scale)),34px));top:calc(var(--rail-y) + clamp(2px,calc(8px * var(--stage-scale)),8px));width:clamp(20px,calc(30px * var(--stage-scale)),30px);height:clamp(20px,calc(30px * var(--stage-scale)),30px);border:1px solid #fff7;background:#142019d9;color:#f4f0df;border-radius:50%;font-size:clamp(13px,calc(19px * var(--stage-scale)),19px);line-height:1;box-shadow:0 5px 16px #0007}.floating-close:hover{background:#9a315c;color:white}
  .chalk-filter{position:absolute;pointer-events:none}
  .bubble{position:absolute;z-index:9;left:var(--bubble-x);top:var(--bubble-y);width:var(--bubble-width);height:var(--bubble-height);display:flex;align-items:center;padding:clamp(6px,calc(18px * var(--stage-scale)),18px);padding-right:clamp(25px,calc(42px * var(--stage-scale)),42px);overflow:hidden;background:#fff;color:#24232a;border:clamp(1px,calc(2px * var(--stage-scale)),2px) solid #4f3b46;border-radius:clamp(10px,calc(20px * var(--stage-scale)),20px) clamp(10px,calc(20px * var(--stage-scale)),20px) clamp(10px,calc(20px * var(--stage-scale)),20px) clamp(3px,calc(5px * var(--stage-scale)),5px);font-size:clamp(6px,calc(13px * var(--stage-scale)),13px);line-height:1.4;box-shadow:clamp(2px,calc(6px * var(--stage-scale)),6px) clamp(2px,calc(7px * var(--stage-scale)),7px) 0 #291e2355;transition:color .2s,transform .2s;box-sizing:border-box}
  .bubble.thinking{color:#78576b;transform:translateY(2px)}
  .drag-handle{position:absolute;z-index:22;border:0;background:transparent;opacity:0;transition:opacity .15s}.drag-handle:hover{opacity:1}.drag-handle::after{content:"MOVE";display:block;padding:3px 7px;border:1px solid #f4f0df66;border-radius:8px;background:#173c2bcc;color:#f4f0df;font:700 6px/1 "JetBrains Mono",monospace;letter-spacing:.14em}.bubble-drag{left:12px;right:12px;top:-8px;height:15px}.bubble-drag::after{width:max-content;margin:auto}.board-drag{left:24%;right:19%;top:5.2%;height:9%}.board-drag::after{width:max-content;margin:auto}
  .lesson-stack{position:absolute;inset:0;z-index:2;min-height:0;pointer-events:none}
  .board-prop{position:absolute;left:var(--board-x);top:var(--board-y);width:var(--board-width);height:var(--board-height);margin:0;filter:drop-shadow(0 clamp(5px,calc(14px * var(--stage-scale)),14px) clamp(5px,calc(12px * var(--stage-scale)),12px) #20160f77);pointer-events:auto}
  .board-art{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;pointer-events:none;user-select:none}
  .board{position:absolute;inset:13.2% 12.8% 15.8%;padding:calc(8px * var(--board-content-scale)) calc(10px * var(--board-content-scale));display:grid;grid-template-rows:auto auto minmax(0,1fr);color:#f4f0df;font-family:"Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive;font-weight:600;overflow:hidden;text-shadow:0 1px 1px #00150b88;--chalk-scale:var(--board-content-scale)}
  .board.thinking::before{content:"";position:absolute;inset:0;z-index:-1;background:radial-gradient(circle at 50% 45%,#7ab09322,transparent 58%)}
  .board-prop.image-failed{min-height:0;filter:none}
  .board-prop.image-failed .board{inset:0;padding:20px 22px;background:#173c2b;border:9px solid #795438;border-radius:5px;box-shadow:inset 0 0 40px #051b11aa,0 18px 40px #0008}
  .board-top{display:flex;justify-content:space-between;align-items:center;position:relative;padding-bottom:calc(7px * var(--board-content-scale))}.board-top::after{content:"";position:absolute;left:0;right:0;bottom:1px;height:max(1px,calc(1px * var(--board-content-scale)));background:linear-gradient(90deg,#f4f0df80,#f4f0df35 62%,transparent);transform:rotate(-.2deg);box-shadow:0 1px #f4f0df18}
  .board-top>span{font-size:clamp(11px,calc(17px * var(--board-content-scale)),28px)}
  .board-top>div{display:flex;align-items:center;gap:calc(8px * var(--board-content-scale))}
  .board-top i,.board-top b{font-style:normal;font-family:Inter,sans-serif;text-transform:uppercase;font-size:clamp(6px,calc(7px * var(--board-content-scale)),12px);letter-spacing:.15em;color:#d5ccaa}
  .board-top b{padding:calc(3px * var(--board-content-scale)) calc(6px * var(--board-content-scale));border:1px solid #e8dfc044;border-radius:calc(8px * var(--board-content-scale));color:#7be4db}
  .selected{position:relative;display:grid;justify-items:start;gap:calc(2px * var(--board-content-scale));margin:calc(6px * var(--board-content-scale)) 0 calc(7px * var(--board-content-scale));padding:calc(4px * var(--board-content-scale)) calc(7px * var(--board-content-scale)) calc(6px * var(--board-content-scale))}.selected::after{content:"";position:absolute;left:2%;right:5%;bottom:0;height:max(1px,calc(1px * var(--board-content-scale)));background:linear-gradient(90deg,#f4f0df65,#f4f0df20,transparent);transform:rotate(.25deg)}
  .numbered-source{display:flex;align-items:flex-start;gap:calc(6px * var(--board-content-scale));min-width:0;max-width:100%}.numbered-source :global(.chalk-phrase){min-width:0}
  .selected>small{padding-inline-start:calc(1.58em + 6px * var(--board-content-scale))}
  .selected :global(.chalk-phrase){font-size:clamp(9px,calc(13px * var(--board-content-scale)),22px)}.selected small{color:#d6ceb7;font:500 clamp(6px,calc(7px * var(--board-content-scale)),12px)/1.35 "Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive}
  .board-content{position:relative;min-height:0;overflow:hidden;display:grid;grid-template-rows:auto minmax(0,1fr) auto;align-content:start}.board-content.erasing{animation:erase-board .44s ease-in forwards}.board-content.writing{animation:chalk .42s ease-out both}
  .board-content.composed{grid-template-columns:minmax(0,.62fr) minmax(0,1.38fr);grid-template-rows:auto minmax(0,1fr);align-items:start;gap:calc(4px * var(--board-content-scale)) calc(8px * var(--board-content-scale))}
  .board-content.composed .sections{grid-column:1;grid-row:1}
  .board-content.composed :global(.chalk-demo){grid-column:2;grid-row:1/3;margin-top:3px}
  .board-content.composed .ambiguity{grid-column:1;grid-row:2;align-self:end;justify-content:flex-start;margin:1px 0 0}
  .erase-sweep{position:absolute;z-index:8;inset:0;overflow:hidden;pointer-events:none}.erase-sweep span{position:absolute;top:51%;left:102%;width:44px;height:17px;border:2px solid #d8c7a5;border-radius:4px;background:linear-gradient(#9b6c4a 0 42%,#e4d7bb 43%);box-shadow:-18px 5px 18px #e8dfc066;animation:eraser-sweep .56s ease-in-out forwards}.erase-sweep::after{content:"";position:absolute;top:47%;left:0;width:100%;height:40px;background:linear-gradient(90deg,transparent,#eee6d233,transparent);filter:blur(8px);animation:dust-sweep .56s ease-out forwards}
  .sections{display:grid;grid-template-columns:repeat(auto-fit,minmax(calc(120px * var(--board-content-scale)),1fr));gap:calc(8px * var(--board-content-scale))}
  .sections section{animation:chalk .32s ease-out both;animation-delay:var(--delay)}
  .section-heading{display:flex;align-items:center;gap:calc(5px * var(--board-content-scale));margin:calc(2px * var(--board-content-scale)) 0 calc(5px * var(--board-content-scale))}.sections h3{position:relative;flex:1;min-width:0;font-size:clamp(8px,calc(12px * var(--board-content-scale)),20px);margin:0;padding-bottom:calc(2px * var(--board-content-scale));color:#f4f0df}.sections h3::after{content:"";position:absolute;left:0;right:8%;bottom:0;height:max(1px,calc(1px * var(--board-content-scale)));background:#f4f0df55;transform:rotate(-.5deg)}
  .sections p{font-size:clamp(7px,calc(9px * var(--board-content-scale)),15px);line-height:1.38;margin:calc(3px * var(--board-content-scale)) 0}
  .ambiguity{display:flex;align-items:center;justify-content:center;gap:calc(5px * var(--board-content-scale));margin-top:calc(4px * var(--board-content-scale));font-size:clamp(7px,calc(9px * var(--board-content-scale)),15px);line-height:1.35}
  .ambiguity>b{color:#f39bc4;font-size:clamp(11px,calc(13px * var(--board-content-scale)),21px);transform:rotate(-8deg);text-shadow:0 0 4px #f39bc455}
  .board-empty{display:grid;place-content:center;min-height:0;text-align:center;color:#b8b099;font-size:clamp(8px,calc(11px * var(--board-content-scale)),18px)}
  .board-empty.waiting{grid-template-columns:repeat(3,7px);gap:6px}.waiting span{width:7px;height:7px;background:#eee6d2;border-radius:50%;animation:think 1s ease-in-out infinite}.waiting span:nth-child(2){animation-delay:.15s}.waiting span:nth-child(3){animation-delay:.3s}.waiting p{grid-column:1/-1;margin:8px 0 0}
  .deck-controls{position:absolute;z-index:10;left:var(--controls-x);top:var(--controls-y);display:flex;align-items:center;justify-content:flex-end;gap:clamp(2px,calc(8px * var(--stage-scale)),8px);width:var(--controls-width);height:var(--controls-height);min-height:0;margin:0;padding:clamp(2px,calc(5px * var(--stage-scale)),5px) clamp(3px,calc(8px * var(--stage-scale)),8px);box-sizing:border-box;border:1px solid #c9bdac77;border-radius:clamp(5px,calc(14px * var(--stage-scale)),14px);background:#f7f1e9e8;color:#4b3d43;box-shadow:clamp(1px,calc(3px * var(--stage-scale)),3px) clamp(2px,calc(5px * var(--stage-scale)),5px) 0 #17100d55;font-family:Inter,sans-serif;pointer-events:auto;overflow:hidden}.progress-wrap{display:flex;align-items:center;gap:clamp(3px,calc(8px * var(--stage-scale)),8px);margin-right:auto;min-width:0}.progress-wrap small{font-size:clamp(5px,calc(8px * var(--stage-scale)),8px);color:#75646b;white-space:nowrap}
  .deck-controls button{border:1px solid #8d777f55;border-radius:clamp(4px,calc(8px * var(--stage-scale)),8px);padding:clamp(2px,calc(7px * var(--stage-scale)),7px) clamp(4px,calc(11px * var(--stage-scale)),11px);font-size:clamp(5px,calc(8px * var(--stage-scale)),8px);cursor:pointer}.deck-controls button:disabled{opacity:.45}.deck-controls .skip{background:transparent;color:#75646b}.deck-controls .next{max-width:clamp(110px,calc(360px * var(--stage-scale)),360px);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:#244d3c;color:#fff8e7;border-color:#244d3c;font-weight:700}.deck-controls .ask-toggle{background:#28212a;color:#fff;border-color:#28212a}.deck-controls em{font-style:normal;font-size:clamp(5px,calc(8px * var(--stage-scale)),8px);letter-spacing:.13em;color:#2e776b}
  .progress{display:flex;gap:clamp(2px,calc(4px * var(--stage-scale)),4px)}.progress span{width:clamp(8px,calc(22px * var(--stage-scale)),22px);height:clamp(2px,calc(5px * var(--stage-scale)),5px);background:#a8929c55;border-radius:2px}.progress span.active{background:#e3a924}.progress span.complete{background:#43a597}
  .followup-composer{flex:1;min-width:0;height:100%;z-index:40}
  @keyframes chalk{from{opacity:0;transform:translateY(5px);filter:blur(2px)}to{opacity:1;transform:none;filter:none}}
  @keyframes erase-board{0%{opacity:1;clip-path:inset(0)}55%{filter:blur(1px)}100%{opacity:.08;clip-path:inset(0 100% 0 0);filter:blur(3px)}}
  @keyframes eraser-sweep{0%{left:102%;transform:rotate(-7deg)}45%{transform:rotate(6deg)}100%{left:-54px;transform:rotate(-4deg)}}
  @keyframes dust-sweep{0%{opacity:0;transform:translateX(70%)}35%{opacity:1}100%{opacity:0;transform:translateX(-70%)}}
  @keyframes think{0%,100%{opacity:.25;transform:translateY(0)}50%{opacity:1;transform:translateY(-4px)}}
  .stage.compact .deck-controls .next{max-width:110px}
  .stage.compact .progress-wrap small{display:none}
  @media(max-width:800px){.progress-wrap small{display:none}.deck-controls .skip{display:none}}
</style>
