<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import type { LessonCard, LessonMessage, SessionState, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS, FIXTURE_LESSON, QUICK_PROMPTS } from "./fixtures";
  import { isLessonSkipped } from "./lesson";
  import { buildTutorContext } from "./preferences";
  import { reduceSession } from "./session";
  import { initialSession, loadPreferences, subscribePreferences, subscribeSession } from "./runtime";
  import ChalkDemo from "./ChalkDemo.svelte";
  import NonoScene from "./NonoScene.svelte";

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

  const selected = $derived(session.segments.find((segment) => segment.id === session.selectedSegmentId) ?? session.segments[3]);
  const latestAssistant = $derived(messages.findLast((message) => message.card?.selectedSegmentId === selected?.id));
  const latestCard = $derived(latestAssistant?.card ?? (isTauri() ? undefined : FIXTURE_LESSON));
  const latestCardKey = $derived(latestAssistant?.id ?? (isTauri() ? undefined : "fixture"));
  const lessonSkipped = $derived(isLessonSkipped(latestCardKey, skippedCardKey));
  const currentMoment = $derived(lessonSkipped ? undefined : latestCard?.moments[activeMomentIndex]);
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

  onMount(() => {
    document.documentElement.dataset.surface = "lesson";
    const cleanup: Array<() => void> = [];
    void initialSession().then((value) => session = value);
    void subscribeSession(() => session, (value) => session = value).then((unlisten) => cleanup.push(unlisten));
    void subscribePreferences((value) => preferences = value).then((unlisten) => cleanup.push(unlisten));
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
  });

  $effect(() => {
    const nextCardKey = latestCardKey;
    if (!nextCardKey || nextCardKey === activeCardKey) return;
    activeCardKey = nextCardKey;
    activeMomentIndex = 0;
    skippedCardKey = undefined;
  });

  function trackScroll() {
    if (!history) return;
    shouldFollow = history.scrollHeight - history.scrollTop - history.clientHeight < 60;
  }

  async function ask(question: string) {
    if (!selected || !question.trim() || loading) return;
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
      if (requestId === requestGeneration) boardPhase = "idle";
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
    boardPhase = "erasing";
    await sleep(440);
    activeMomentIndex += 1;
    boardPhase = "writing";
    await sleep(620);
    boardPhase = "idle";
  }

  async function skipRemaining() {
    if (!latestCardKey || boardPhase !== "idle") return;
    boardPhase = "erasing";
    await sleep(440);
    skippedCardKey = latestCardKey;
    boardPhase = "idle";
  }

  async function closeLesson() {
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
</script>

<div class="lesson-shell">
  <header data-tauri-drag-region><div><span>の</span><b>NONO / LANGUAGE ROOM</b></div><button onclick={closeLesson}>×</button></header>
  <main>
    <section class="stage">
      <div class="nono" class:erasing={boardPhase === "erasing"} class:thinking={boardPhase === "thinking"} class:writing={boardPhase === "writing"}><NonoScene /></div>
      <div class="bubble" class:thinking={boardPhase === "thinking"}>{bubbleText}</div>
      <div class="board" class:thinking={boardPhase === "thinking"}>
        {#if boardPhase === "erasing"}<div class="erase-sweep" aria-hidden="true"><span></span></div>{/if}
        <div class="board-top">
          <span>{currentMoment?.title ?? "Understanding the line"}</span>
          <div><i>{preferences.level}</i>{#if latestCard && currentMoment}<b>{activeMomentIndex + 1} / {latestCard.moments.length}</b>{/if}</div>
        </div>
        {#if selected}<div class="selected"><b>{selected.sourceText}</b>{#if selected.translationText}<small>{selected.translationText}</small>{/if}</div>{/if}
        {#if boardPhase === "thinking"}
          <div class="board-empty waiting"><span></span><span></span><span></span><p>Choosing the next teaching moment…</p></div>
        {:else if currentMoment}
          <div class="board-content" class:erasing={boardPhase === "erasing"} class:writing={boardPhase === "writing"}>
            <div class="sections">{#each currentMoment.boardSections as section, index}<section style={`--delay:${index * 100}ms`}><h3>{section.heading}</h3>{#each section.lines as line}<p>• {line}</p>{/each}</section>{/each}</div>
            <ChalkDemo demo={currentMoment.demonstration} />
            {#if currentMoment.ambiguityNote}<div class="ambiguity"><b>AMBIGUITY</b>{currentMoment.ambiguityNote}</div>{/if}
          </div>
          {#if latestCard && latestCard.moments.length > 1}
            <div class="deck-controls">
              <div class="progress" aria-label={`Teaching moment ${activeMomentIndex + 1} of ${latestCard.moments.length}`}>
                {#each latestCard.moments as _, index}<span class:active={index === activeMomentIndex} class:complete={index < activeMomentIndex}></span>{/each}
              </div>
              {#if hasMoreMoments}
                <button class="skip" onclick={skipRemaining} disabled={boardPhase !== "idle"}>Skip rest</button>
                <button class="next" onclick={nextMoment} disabled={boardPhase !== "idle"}>Next · {latestCard.moments[activeMomentIndex + 1].title}</button>
              {:else}<em>Lesson complete</em>{/if}
            </div>
          {/if}
        {:else if lessonSkipped}<div class="board-empty">Lesson skipped. Ask about the part you actually care about.</div>
        {:else if !loading}<div class="board-empty">Ask a question and Nono will organize the answer here.</div>{/if}
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
  .stage{position:relative;overflow:hidden;padding:148px 22px 20px;background:radial-gradient(circle at 5% 10%,#43263c55,transparent 36%),#10141b}
  .nono{position:absolute;left:6px;top:4px;width:190px;height:145px;overflow:hidden;transform-origin:68% 85%}
  .nono.erasing{animation:nono-erase .56s ease-in-out both}.nono.thinking{animation:nono-think 1.5s ease-in-out infinite}.nono.writing{animation:nono-present .7s ease-out both}
  .nono :global(.nono-scene){height:145px!important}
  .bubble{position:absolute;left:175px;right:20px;top:24px;min-height:72px;padding:15px 18px;background:#fff;color:#24232a;border-radius:20px 20px 20px 5px;font-size:11px;line-height:1.55;box-shadow:0 13px 30px #0007;transition:color .2s,transform .2s}
  .bubble.thinking{color:#78576b;transform:translateY(2px)}
  .board{position:relative;height:100%;min-height:330px;padding:20px 22px;display:grid;grid-template-rows:auto auto minmax(0,1fr) auto;background:#173c2b;border:9px solid #795438;border-radius:5px;box-shadow:inset 0 0 40px #051b11aa,0 18px 40px #0008;color:#f3ecd8;font-family:"Comic Sans MS","Bradley Hand",cursive;overflow:hidden}
  .board.thinking{background:radial-gradient(circle at 50% 45%,#285e45,#173c2b 58%)}
  .board-top{display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid #e8dfc044;padding-bottom:8px}
  .board-top>span{font-size:17px}
  .board-top>div{display:flex;align-items:center;gap:8px}
  .board-top i,.board-top b{font-style:normal;font-family:Inter,sans-serif;text-transform:uppercase;font-size:7px;letter-spacing:.15em;color:#d5ccaa}
  .board-top b{padding:3px 6px;border:1px solid #e8dfc044;border-radius:8px;color:#7be4db}
  .selected{display:grid;gap:4px;margin:12px 0;padding:9px;border:1px dashed #e9e0c055}
  .selected>b{font-size:13px}.selected small{font-family:Inter,sans-serif;color:#d6ceb7;font-size:8px}
  .board-content{position:relative;min-height:0;overflow-y:auto;padding-right:4px}.board-content.erasing{animation:erase-board .44s ease-in forwards}.board-content.writing{animation:chalk .42s ease-out both}
  .erase-sweep{position:absolute;z-index:8;inset:0;overflow:hidden;pointer-events:none}.erase-sweep span{position:absolute;top:51%;left:102%;width:44px;height:17px;border:2px solid #d8c7a5;border-radius:4px;background:linear-gradient(#9b6c4a 0 42%,#e4d7bb 43%);box-shadow:-18px 5px 18px #e8dfc066;animation:eraser-sweep .56s ease-in-out forwards}.erase-sweep::after{content:"";position:absolute;top:47%;left:0;width:100%;height:40px;background:linear-gradient(90deg,transparent,#eee6d233,transparent);filter:blur(8px);animation:dust-sweep .56s ease-out forwards}
  .sections{display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:10px}
  .sections section{animation:chalk .32s ease-out both;animation-delay:var(--delay)}
  .sections h3{font-size:12px;margin:5px 0;border-bottom:1px solid #efe5c033;padding-bottom:3px}
  .sections p{font-size:9px;line-height:1.45;margin:3px 0}
  .ambiguity{margin-top:12px;padding:8px;border-left:2px solid #e6c45d;background:#081e1544;font-size:9px;line-height:1.5}
  .ambiguity b{display:block;color:#e6c45d;font-family:Inter,sans-serif;font-size:7px;letter-spacing:.12em}
  .board-empty{display:grid;place-content:center;min-height:0;text-align:center;color:#b8b099;font-size:11px}
  .board-empty.waiting{grid-template-columns:repeat(3,7px);gap:6px}.waiting span{width:7px;height:7px;background:#eee6d2;border-radius:50%;animation:think 1s ease-in-out infinite}.waiting span:nth-child(2){animation-delay:.15s}.waiting span:nth-child(3){animation-delay:.3s}.waiting p{grid-column:1/-1;margin:8px 0 0}
  .deck-controls{display:grid;grid-template-columns:auto auto minmax(120px,1fr);align-items:center;gap:8px;margin-top:12px;padding-top:10px;border-top:1px solid #e8dfc033;font-family:Inter,sans-serif}
  .deck-controls button{border:1px solid #e8dfc044;border-radius:4px;padding:7px 9px;font-size:7px;cursor:pointer}.deck-controls button:disabled{opacity:.45}.deck-controls .skip{background:transparent;color:#b8b099}.deck-controls .next{justify-self:end;max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:#f3ecd8;color:#183c2c;font-weight:700}.deck-controls em{grid-column:2/-1;justify-self:end;font-style:normal;font-size:7px;letter-spacing:.13em;color:#7be4db}
  .progress{display:flex;gap:4px}.progress span{width:18px;height:3px;background:#e8dfc030;border-radius:2px}.progress span.active{background:#f3d675}.progress span.complete{background:#7be4db}
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
  @keyframes nono-erase{0%,100%{transform:translateX(0) rotate(0)}30%{transform:translateX(9px) rotate(2.5deg)}62%{transform:translateX(-4px) rotate(-2deg)}}
  @keyframes nono-think{0%,100%{transform:translateY(0) rotate(0)}50%{transform:translateY(-2px) rotate(-1deg)}}
  @keyframes nono-present{0%{transform:translateX(-6px) rotate(-2deg)}55%{transform:translateX(3px) rotate(1deg)}100%{transform:none}}
  @keyframes think{0%,100%{opacity:.25;transform:translateY(0)}50%{opacity:1;transform:translateY(-4px)}}
  @media(max-width:680px){main{grid-template-columns:1fr}.lesson-thread{position:absolute;right:0;top:42px;bottom:0;width:42%;background:#0c1016f5}.stage{padding-right:44%}.deck-controls{grid-template-columns:auto auto 1fr}}
</style>
