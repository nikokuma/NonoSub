<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import type { LessonCard, LessonMessage, SessionState, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS, FIXTURE_LESSON, QUICK_PROMPTS } from "./fixtures";
  import { dominantChalkColor, isLessonSkipped, lessonStepOrder } from "./lesson";
  import { buildTutorContext } from "./preferences";
  import { reduceSession } from "./session";
  import { initialSession, loadPreferences, subscribePreferences, subscribeSession } from "./runtime";
  import ChalkDemo from "./ChalkDemo.svelte";
  import ChalkPhrase from "./ChalkPhrase.svelte";
  import ChalkStepNumber from "./ChalkStepNumber.svelte";
  import NonoScene from "./NonoScene.svelte";
  import { IDLE_TAIL_PRESENTATION, type TailPresentation, type TailPresentationPhase } from "./tailPresentation";

  type BoardPhase = "idle" | "erasing" | "thinking" | "writing";

  let session = $state<SessionState>(FIXTURE_EVENTS.reduce(reduceSession, structuredClone(EMPTY_SESSION)));
  let preferences = $state(loadPreferences());
  let messages = $state<LessonMessage[]>([]);
  let threads = $state<Record<string, LessonMessage[]>>({});
  let input = $state("");
  let loading = $state(false);
  let error = $state("");
  let history = $state<HTMLDivElement>();
  let shouldFollow = true;
  let activeSegmentId: string | undefined;
  let requestGeneration = 0;
  let boardPhase = $state<BoardPhase>("idle");
  let activeMomentIndex = $state(0);
  let activeCardKey = $state<string>();
  let skippedCardKey = $state<string>();
  let boardImageFailed = $state(false);
  let boardElement = $state<HTMLDivElement>();
  let tailPresentation = $state<TailPresentation>({ ...IDLE_TAIL_PRESENTATION });
  let tailRigAvailable = $state(false);
  let cueGeneration = 0;
  let underlineProgress = $state<Record<string, number>>({});

  const selected = $derived(session.segments.find((segment) => segment.id === session.selectedSegmentId) ?? session.segments[3]);
  const latestAssistant = $derived(messages.findLast((message) => message.card?.selectedSegmentId === selected?.id));
  const latestCard = $derived(latestAssistant?.card ?? (isTauri() ? undefined : FIXTURE_LESSON));
  const latestCardKey = $derived(latestAssistant?.id ?? (isTauri() ? undefined : "fixture"));
  const lessonSkipped = $derived(isLessonSkipped(latestCardKey, skippedCardKey));
  const currentMoment = $derived(lessonSkipped ? undefined : latestCard?.moments[activeMomentIndex]);
  const stepOrder = $derived(lessonStepOrder(currentMoment, Boolean(selected)));
  const hasMoreMoments = $derived(Boolean(latestCard && activeMomentIndex < latestCard.moments.length - 1));
  const bubbleText = $derived(
    boardPhase === "erasing"
      ? "New question, new board. Mind the chalk dust!"
      : boardPhase === "thinking"
        ? "Hm… I’m choosing the useful part, not every fact in the textbook."
        : lessonSkipped
          ? "Okay, we’ll leave the rest there. Ask about the part you actually care about."
          : currentMoment?.speechBubble ?? "Pick what you want me to explain. I brought chalk."
  );
  const activePointCueId = $derived(tailPresentation.phase === "idle" ? undefined : tailPresentation.pointCueId);
  const nonoMood = $derived(boardPhase === "thinking" ? "think" : boardPhase === "writing" ? "present" : "idle");

  onMount(() => {
    document.documentElement.dataset.surface = "lesson";
    const cleanup: Array<() => void> = [];
    void initialSession().then((value) => session = value);
    void subscribeSession(() => session, (value) => session = value).then((unlisten) => cleanup.push(unlisten));
    void subscribePreferences((value) => preferences = value).then((unlisten) => cleanup.push(unlisten));
    void tick().then(() => playCueSequence());
    const escape = (event: KeyboardEvent) => event.key === "Escape" && void closeLesson();
    window.addEventListener("keydown", escape);
    return () => { cleanup.forEach((stop) => stop()); window.removeEventListener("keydown", escape); };
  });

  $effect(() => {
    const count = messages.length;
    if (count < 0 || !shouldFollow) return;
    void tick().then(() => history?.scrollTo({ top: history.scrollHeight, behavior: "smooth" }));
  });

  $effect(() => {
    const nextSegmentId = selected?.id;
    if (nextSegmentId === activeSegmentId) return;
    if (activeSegmentId) threads[activeSegmentId] = messages;
    activeSegmentId = nextSegmentId;
    messages = nextSegmentId ? [...(threads[nextSegmentId] ?? [])] : [];
    input = "";
    error = "";
    loading = false;
    boardPhase = "idle";
    activeMomentIndex = 0;
    activeCardKey = undefined;
    skippedCardKey = undefined;
    shouldFollow = true;
    requestGeneration += 1;
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

  function trackScroll() {
    if (!history) return;
    shouldFollow = history.scrollHeight - history.scrollTop - history.clientHeight < 60;
  }

  async function ask(question: string) {
    if (!selected || !question.trim() || loading) return;
    cancelCueSequence();
    const requestSegment = selected;
    const requestId = ++requestGeneration;
    const eraseExistingBoard = Boolean(currentMoment);
    const userMessage: LessonMessage = { id: crypto.randomUUID(), role: "user", text: question.trim() };
    messages = [...messages, userMessage];
    input = "";
    loading = true;
    error = "";
    boardPhase = eraseExistingBoard ? "erasing" : "thinking";
    try {
      const request: Promise<LessonCard> = isTauri()
        ? invoke<LessonCard>("request_lesson", {
            question: question.trim(),
            selected: requestSegment,
            learnerLevel: preferences.level,
            context: buildTutorContext(session.segments, requestSegment.id),
            thread: messages.slice(-12).map(({ role, text }) => ({ role, text })),
          })
        : Promise.resolve(FIXTURE_LESSON);
      const eraseTransition = eraseExistingBoard
        ? sleep(560).then(() => {
            if (requestId === requestGeneration) boardPhase = "thinking";
          })
        : Promise.resolve();
      const [card] = await Promise.all([request, eraseTransition]);
      if (requestId !== requestGeneration || selected?.id !== requestSegment.id) return;
      messages = [...messages, { id: crypto.randomUUID(), role: "assistant", text: card.moments[0].speechBubble, card }];
      boardPhase = "writing";
      await sleep(720);
      if (requestId === requestGeneration) {
        boardPhase = "idle";
        void playCueSequence();
      }
    } catch (requestError) {
      if (requestId !== requestGeneration) return;
      error = errorMessage(requestError);
      boardPhase = "idle";
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
    void playCueSequence();
  }

  async function skipRemaining() {
    if (!latestCardKey || boardPhase !== "idle") return;
    cancelCueSequence();
    boardPhase = "erasing";
    await sleep(440);
    skippedCardKey = latestCardKey;
    boardPhase = "idle";
  }

  async function closeLesson() {
    cancelCueSequence();
    if (isTauri()) {
      await invoke("hide_surface", { surface: "lesson" });
      await emit("lesson-closed");
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

  function cancelCueSequence() {
    cueGeneration += 1;
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

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      if (underlineCueId) underlineProgress = { [underlineCueId]: 1 };
      return;
    }

    if (pointCueId) {
      await animatePresentation(generation, "point", 450, { pointCueId, underlineCueId, underlineColor });
      if (generation !== cueGeneration) return;
    }
    if (underlineCueId) {
      await animatePresentation(generation, "hold", 400, { pointCueId, underlineCueId, underlineColor });
      if (generation !== cueGeneration) return;
      await animatePresentation(generation, "underline", 550, { pointCueId, underlineCueId, underlineColor }, (progress) => {
        underlineProgress = { ...underlineProgress, [underlineCueId]: progress };
      });
      underlineProgress = { ...underlineProgress, [underlineCueId]: 1 };
      await sleep(220);
    } else {
      tailPresentation = { sequenceId: generation, phase: "hold", progress: 1, pointCueId };
      await sleep(700);
    }
    if (generation !== cueGeneration) return;
    await animatePresentation(generation, "retract", 350, { pointCueId, underlineCueId, underlineColor });
    if (generation === cueGeneration) tailPresentation = { ...IDLE_TAIL_PRESENTATION, sequenceId: generation };
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

<div class="lesson-shell">
  <header data-tauri-drag-region><div><span>の</span><b>NONO / LANGUAGE ROOM</b></div><button onclick={closeLesson}>×</button></header>
  <main>
    <section class="stage">
      <NonoScene presentation={tailPresentation} mood={nonoMood} onRigStatus={(available) => tailRigAvailable = available} />
      <svg class="chalk-filter" width="0" height="0" aria-hidden="true">
        <filter id="chalk-roughen" x="-4%" y="-8%" width="108%" height="116%" color-interpolation-filters="sRGB">
          <feTurbulence type="fractalNoise" baseFrequency="0.72" numOctaves="1" seed="17" result="noise" />
          <feDisplacementMap in="SourceGraphic" in2="noise" scale="0.45" xChannelSelector="R" yChannelSelector="G" />
        </filter>
      </svg>
      <div class="bubble" class:thinking={boardPhase === "thinking"}>{bubbleText}</div>
      <div class="lesson-stack">
        <div class="board-prop" class:image-failed={boardImageFailed}>
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
        {#if latestCard && currentMoment && latestCard.moments.length > 1 && !lessonSkipped}
          <nav class="deck-controls" aria-label="Lesson moments">
            <div class="progress" aria-label={`Teaching moment ${activeMomentIndex + 1} of ${latestCard.moments.length}`}>
              {#each latestCard.moments as _, index}<span class:active={index === activeMomentIndex} class:complete={index < activeMomentIndex}></span>{/each}
            </div>
            {#if hasMoreMoments}
              <button class="skip" onclick={skipRemaining} disabled={boardPhase !== "idle"}>Skip remaining</button>
              <button class="next" onclick={nextMoment} disabled={boardPhase !== "idle"}>Next lesson · {latestCard.moments[activeMomentIndex + 1].title}</button>
            {:else}<em>Lesson complete</em>{/if}
          </nav>
        {/if}
      </div>
    </section>

    <aside class="lesson-thread">
      <div class="quick">{#each QUICK_PROMPTS as prompt}<button onclick={() => ask(prompt)} disabled={!selected || loading}>{prompt}</button>{/each}</div>
      <div class="history" bind:this={history} onscroll={trackScroll} aria-live="polite">
        {#if messages.length === 0}<p class="welcome">This lesson stays in memory only for the current session. Ask as many follow-ups as you like.</p>{/if}
        {#each messages as message}<div class="message {message.role}"><span>{message.role === "assistant" ? "NONO" : "YOU"}</span>{message.text}</div>{/each}
        {#if error}<div class="message error"><span>CONNECTION</span>{error}<button onclick={() => ask(messages.findLast((message) => message.role === "user")?.text ?? "Break it down")}>Retry</button></div>{/if}
      </div>
      {#if latestCard}<div class="suggestions">{#each latestCard.suggestedFollowUps as prompt}<button onclick={() => ask(prompt)}>{prompt}</button>{/each}</div>{/if}
      <form onsubmit={(event) => { event.preventDefault(); void ask(input); }}><textarea bind:value={input} placeholder="Ask for a translation, grammar, tone, or culture…" disabled={!selected || loading}></textarea><button disabled={!input.trim() || loading}>↑</button></form>
    </aside>
  </main>
</div>

<style>
  .lesson-shell{height:100vh;display:grid;grid-template-rows:42px 1fr;background:#0a0d13;color:#f7f5fb;border:1px solid #303846}
  header{display:flex;align-items:center;justify-content:space-between;padding:0 12px;border-bottom:1px solid #29313d;background:#0d1118}
  header>div{display:flex;align-items:center;gap:8px;font-size:8px;letter-spacing:.13em;color:#82909d}
  header span{width:23px;height:23px;display:grid;place-items:center;background:#ff70b7;color:white;border-radius:5px}
  header button{border:0;background:none;color:#76808c;font-size:20px}
  main{min-height:0;display:grid;grid-template-columns:minmax(400px,1.3fr) minmax(240px,.7fr)}
  .stage{position:relative;overflow:hidden;padding:148px 22px 20px;background:radial-gradient(circle at 8% 8%,#ffd7e8 0,transparent 32%),linear-gradient(145deg,#f5eee4 0%,#dfece7 50%,#d8e1ee 100%)}
  .chalk-filter{position:absolute;pointer-events:none}
  .bubble{position:absolute;z-index:9;left:175px;right:20px;top:24px;min-height:72px;padding:15px 18px;background:#fff;color:#24232a;border:2px solid #4f3b46;border-radius:20px 20px 20px 5px;font-size:11px;line-height:1.55;box-shadow:6px 7px 0 #b79fad55;transition:color .2s,transform .2s}
  .bubble.thinking{color:#78576b;transform:translateY(2px)}
  .lesson-stack{position:relative;z-index:2;display:grid;align-content:center;gap:8px;min-height:0}
  .board-prop{position:relative;width:100%;aspect-ratio:16/9;margin:auto;filter:drop-shadow(0 14px 12px #65452d44)}
  .board-art{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;pointer-events:none;user-select:none}
  .board{position:absolute;inset:13.2% 12.8% 15.8%;padding:8px 10px;display:grid;grid-template-rows:auto auto minmax(0,1fr);color:#f4f0df;font-family:"Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive;font-weight:600;overflow:hidden;text-shadow:0 1px 1px #00150b88}
  .board.thinking::before{content:"";position:absolute;inset:0;z-index:-1;background:radial-gradient(circle at 50% 45%,#7ab09322,transparent 58%)}
  .board-prop.image-failed{min-height:330px;aspect-ratio:auto;filter:none}
  .board-prop.image-failed .board{inset:0;padding:20px 22px;background:#173c2b;border:9px solid #795438;border-radius:5px;box-shadow:inset 0 0 40px #051b11aa,0 18px 40px #0008}
  .board-top{display:flex;justify-content:space-between;align-items:center;position:relative;padding-bottom:7px}.board-top::after{content:"";position:absolute;left:0;right:0;bottom:1px;height:1px;background:linear-gradient(90deg,#f4f0df80,#f4f0df35 62%,transparent);transform:rotate(-.2deg);box-shadow:0 1px #f4f0df18}
  .board-top>span{font-size:clamp(12px,1.5vw,17px)}
  .board-top>div{display:flex;align-items:center;gap:8px}
  .board-top i,.board-top b{font-style:normal;font-family:Inter,sans-serif;text-transform:uppercase;font-size:7px;letter-spacing:.15em;color:#d5ccaa}
  .board-top b{padding:3px 6px;border:1px solid #e8dfc044;border-radius:8px;color:#7be4db}
  .selected{position:relative;display:grid;justify-items:start;gap:2px;margin:6px 0 7px;padding:4px 7px 6px}.selected::after{content:"";position:absolute;left:2%;right:5%;bottom:0;height:1px;background:linear-gradient(90deg,#f4f0df65,#f4f0df20,transparent);transform:rotate(.25deg)}
  .numbered-source{display:flex;align-items:flex-start;gap:6px;min-width:0;max-width:100%}.numbered-source :global(.chalk-phrase){min-width:0}
  .selected>small{padding-inline-start:calc(1.58em + 6px)}
  .selected :global(.chalk-phrase){font-size:clamp(10px,1.12vw,13px)}.selected small{color:#d6ceb7;font:500 7px/1.35 "Klee One","Hiragino Maru Gothic ProN","Noto Sans",cursive}
  .board-content{position:relative;min-height:0;overflow:hidden;display:grid;grid-template-rows:auto minmax(0,1fr) auto;align-content:start}.board-content.erasing{animation:erase-board .44s ease-in forwards}.board-content.writing{animation:chalk .42s ease-out both}
  .board-content.composed{grid-template-columns:minmax(0,.62fr) minmax(0,1.38fr);grid-template-rows:auto minmax(0,1fr);align-items:start;gap:4px 8px}
  .board-content.composed .sections{grid-column:1;grid-row:1}
  .board-content.composed :global(.chalk-demo){grid-column:2;grid-row:1/3;margin-top:3px}
  .board-content.composed .ambiguity{grid-column:1;grid-row:2;align-self:end;justify-content:flex-start;margin:1px 0 0}
  .erase-sweep{position:absolute;z-index:8;inset:0;overflow:hidden;pointer-events:none}.erase-sweep span{position:absolute;top:51%;left:102%;width:44px;height:17px;border:2px solid #d8c7a5;border-radius:4px;background:linear-gradient(#9b6c4a 0 42%,#e4d7bb 43%);box-shadow:-18px 5px 18px #e8dfc066;animation:eraser-sweep .56s ease-in-out forwards}.erase-sweep::after{content:"";position:absolute;top:47%;left:0;width:100%;height:40px;background:linear-gradient(90deg,transparent,#eee6d233,transparent);filter:blur(8px);animation:dust-sweep .56s ease-out forwards}
  .sections{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:8px}
  .sections section{animation:chalk .32s ease-out both;animation-delay:var(--delay)}
  .section-heading{display:flex;align-items:center;gap:5px;margin:2px 0 5px}.sections h3{position:relative;flex:1;min-width:0;font-size:clamp(9px,1vw,12px);margin:0;padding-bottom:2px;color:#f4f0df}.sections h3::after{content:"";position:absolute;left:0;right:8%;bottom:0;height:1px;background:#f4f0df55;transform:rotate(-.5deg)}
  .sections p{font-size:clamp(7px,.78vw,9px);line-height:1.38;margin:3px 0}
  .ambiguity{display:flex;align-items:center;justify-content:center;gap:5px;margin-top:4px;font-size:clamp(7px,.72vw,9px);line-height:1.35}
  .ambiguity>b{color:#f39bc4;font-size:13px;transform:rotate(-8deg);text-shadow:0 0 4px #f39bc455}
  .board-empty{display:grid;place-content:center;min-height:0;text-align:center;color:#b8b099;font-size:11px}
  .board-empty.waiting{grid-template-columns:repeat(3,7px);gap:6px}.waiting span{width:7px;height:7px;background:#eee6d2;border-radius:50%;animation:think 1s ease-in-out infinite}.waiting span:nth-child(2){animation-delay:.15s}.waiting span:nth-child(3){animation-delay:.3s}.waiting p{grid-column:1/-1;margin:8px 0 0}
  .deck-controls{position:relative;z-index:10;display:grid;grid-template-columns:auto auto minmax(120px,1fr);align-items:center;gap:8px;min-height:35px;padding:5px 8px 5px 12px;border:1px solid #b7a59b66;border-radius:12px;background:#fff9;color:#4b3d43;box-shadow:3px 4px 0 #b79fad33;font-family:Inter,sans-serif}
  .deck-controls button{border:1px solid #8d777f55;border-radius:8px;padding:7px 10px;font-size:7px;cursor:pointer}.deck-controls button:disabled{opacity:.45}.deck-controls .skip{background:transparent;color:#75646b}.deck-controls .next{justify-self:end;max-width:250px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:#244d3c;color:#fff8e7;border-color:#244d3c;font-weight:700}.deck-controls em{grid-column:2/-1;justify-self:end;font-style:normal;font-size:7px;letter-spacing:.13em;color:#2e776b}
  .progress{display:flex;gap:4px}.progress span{width:18px;height:4px;background:#a8929c55;border-radius:2px}.progress span.active{background:#e3a924}.progress span.complete{background:#43a597}
  .lesson-thread{min-height:0;display:grid;grid-template-rows:auto 1fr auto auto;border-left:1px solid #29313d;background:#0c1016}
  .quick,.suggestions{display:flex;gap:5px;padding:9px;overflow-x:auto;border-bottom:1px solid #242b35}
  .quick button,.suggestions button{white-space:nowrap;border:1px solid #2c3541;background:#131923;color:#bbc3ce;padding:6px 8px;border-radius:12px;font-size:7px}
  .history{min-height:0;overflow-y:auto;padding:12px;display:grid;align-content:start;gap:8px}
  .welcome{color:#687381;font-size:9px;line-height:1.6}.message{padding:9px 10px;background:#171e28;border:1px solid #26303d;border-radius:8px;font-size:9px;line-height:1.5;white-space:pre-wrap}
  .message span{display:block;color:#6ce1d9;font-size:7px;letter-spacing:.13em;margin-bottom:4px}.message.user{margin-left:18%;background:#211625;border-color:#462b42}.message.user span{color:#ff84bf}.message.error{border-color:#833953;color:#ffc0d9}.message.error button{display:block;margin-top:7px;border:0;background:#ff70b7;color:white;padding:4px 8px;border-radius:5px}
  .suggestions{border-top:1px solid #242b35;border-bottom:0}.lesson-thread form{display:grid;grid-template-columns:1fr 36px;margin:0 9px 9px;border:1px solid #2b3542;background:#111721;border-radius:8px}.lesson-thread textarea{height:48px;resize:none;border:0;background:none;color:white;padding:9px;font-size:9px;outline:0}.lesson-thread form button{margin:8px;width:28px;height:28px;border:0;border-radius:50%;background:#ff70b7;color:white}
  @keyframes chalk{from{opacity:0;transform:translateY(5px);filter:blur(2px)}to{opacity:1;transform:none;filter:none}}
  @keyframes erase-board{0%{opacity:1;clip-path:inset(0)}55%{filter:blur(1px)}100%{opacity:.08;clip-path:inset(0 100% 0 0);filter:blur(3px)}}
  @keyframes eraser-sweep{0%{left:102%;transform:rotate(-7deg)}45%{transform:rotate(6deg)}100%{left:-54px;transform:rotate(-4deg)}}
  @keyframes dust-sweep{0%{opacity:0;transform:translateX(70%)}35%{opacity:1}100%{opacity:0;transform:translateX(-70%)}}
  @keyframes think{0%,100%{opacity:.25;transform:translateY(0)}50%{opacity:1;transform:translateY(-4px)}}
  @media(max-width:680px){main{grid-template-columns:1fr}.lesson-thread{position:absolute;right:0;top:42px;bottom:0;width:42%;background:#0c1016f5}.stage{padding-right:44%}.board-prop{margin-top:auto;margin-bottom:auto}.deck-controls{grid-template-columns:auto auto 1fr}}
</style>
