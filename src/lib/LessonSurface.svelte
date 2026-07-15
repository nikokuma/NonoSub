<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import type { LessonCard, LessonMessage, SessionState, SubtitleSegment } from "./contracts";
  import { EMPTY_SESSION } from "./contracts";
  import { FIXTURE_EVENTS, FIXTURE_LESSON, QUICK_PROMPTS } from "./fixtures";
  import { buildTutorContext } from "./preferences";
  import { reduceSession } from "./session";
  import { initialSession, loadPreferences, subscribePreferences, subscribeSession } from "./runtime";
  import NonoScene from "./NonoScene.svelte";

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

  const selected = $derived(session.segments.find((segment) => segment.id === session.selectedSegmentId) ?? session.segments[3]);
  const latestCard = $derived(messages.findLast((message) => message.card?.selectedSegmentId === selected?.id)?.card ?? (isTauri() ? undefined : FIXTURE_LESSON));

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
    shouldFollow = true;
    requestGeneration += 1;
  });

  function trackScroll() {
    if (!history) return;
    shouldFollow = history.scrollHeight - history.scrollTop - history.clientHeight < 60;
  }

  async function ask(question: string) {
    if (!selected || !question.trim() || loading) return;
    const requestSegment = selected;
    const requestId = ++requestGeneration;
    const userMessage: LessonMessage = { id: crypto.randomUUID(), role: "user", text: question.trim() };
    messages = [...messages, userMessage];
    input = "";
    loading = true;
    error = "";
    try {
      const card = isTauri()
        ? await invoke<LessonCard>("request_lesson", {
            question: question.trim(),
            selected: requestSegment,
            learnerLevel: preferences.level,
            context: buildTutorContext(session.segments, requestSegment.id),
            thread: messages.slice(-12).map(({ role, text }) => ({ role, text })),
          })
        : { ...FIXTURE_LESSON, speechBubble: question === "Tone & politeness" ? "The unfinished phrase softens the refusal and lets the listener infer the awkward part." : FIXTURE_LESSON.speechBubble };
      if (requestId !== requestGeneration || selected?.id !== requestSegment.id) return;
      messages = [...messages, { id: crypto.randomUUID(), role: "assistant", text: card.speechBubble, card }];
    } catch (requestError) {
      if (requestId !== requestGeneration) return;
      error = errorMessage(requestError);
    } finally {
      if (requestId === requestGeneration) loading = false;
    }
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
</script>

<div class="lesson-shell">
  <header data-tauri-drag-region><div><span>の</span><b>NONO / LANGUAGE ROOM</b></div><button onclick={closeLesson}>×</button></header>
  <main>
    <section class="stage">
      <div class="nono"><NonoScene /></div>
      <div class="bubble" class:thinking={loading}>{loading ? "Hm. Let me put this on the board…" : latestCard?.speechBubble ?? "Pick what you want me to explain. I brought chalk."}</div>
      <div class="board">
        <div class="board-top"><span>{latestCard?.title ?? "Understanding the line"}</span><i>{preferences.level}</i></div>
        {#if selected}<div class="selected"><b>{selected.sourceText}</b><small>{selected.translationText}</small></div>{/if}
        {#if latestCard}
          <div class="sections">{#each latestCard.boardSections as section, index}<section style={`--delay:${index * 100}ms`}><h3>{section.heading}</h3>{#each section.lines as line}<p>• {line}</p>{/each}</section>{/each}</div>
          {#if latestCard.ambiguityNote}<div class="ambiguity"><b>AMBIGUITY</b>{latestCard.ambiguityNote}</div>{/if}
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
      <form onsubmit={(event) => { event.preventDefault(); void ask(input); }}><textarea bind:value={input} placeholder="Ask about grammar, tone, or culture…" disabled={!selected || loading}></textarea><button disabled={!input.trim() || loading}>↑</button></form>
    </aside>
  </main>
</div>

<style>
  .lesson-shell{height:100vh;display:grid;grid-template-rows:42px 1fr;background:#0a0d13;color:#f7f5fb;border:1px solid #303846}header{display:flex;align-items:center;justify-content:space-between;padding:0 12px;border-bottom:1px solid #29313d;background:#0d1118}header>div{display:flex;align-items:center;gap:8px;font-size:8px;letter-spacing:.13em;color:#82909d}header span{width:23px;height:23px;display:grid;place-items:center;background:#ff70b7;color:white;border-radius:5px}header button{border:0;background:none;color:#76808c;font-size:20px}main{min-height:0;display:grid;grid-template-columns:minmax(400px,1.3fr) minmax(240px,.7fr)}.stage{position:relative;overflow:hidden;padding:148px 22px 20px;background:radial-gradient(circle at 5% 10%,#43263c55,transparent 36%),#10141b}.nono{position:absolute;left:6px;top:4px;width:190px;height:145px;overflow:hidden}.nono :global(.nono-scene){height:145px!important}.bubble{position:absolute;left:175px;right:20px;top:24px;min-height:72px;padding:15px 18px;background:#fff;color:#24232a;border-radius:20px 20px 20px 5px;font-size:11px;line-height:1.55;box-shadow:0 13px 30px #0007}.bubble.thinking{color:#78576b}.board{height:100%;min-height:330px;padding:20px 22px;background:#173c2b;border:9px solid #795438;border-radius:5px;box-shadow:inset 0 0 40px #051b11aa,0 18px 40px #0008;color:#f3ecd8;font-family:"Comic Sans MS","Bradley Hand",cursive;overflow-y:auto}.board-top{display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid #e8dfc044;padding-bottom:8px}.board-top span{font-size:17px}.board-top i{font-style:normal;font-family:Inter,sans-serif;text-transform:uppercase;font-size:7px;letter-spacing:.15em;color:#d5ccaa}.selected{display:grid;gap:4px;margin:12px 0;padding:9px;border:1px dashed #e9e0c055}.selected b{font-size:13px}.selected small{font-family:Inter,sans-serif;color:#d6ceb7;font-size:8px}.sections{display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:10px}.sections section{animation:chalk .32s ease-out both;animation-delay:var(--delay)}.sections h3{font-size:12px;margin:5px 0;border-bottom:1px solid #efe5c033;padding-bottom:3px}.sections p{font-size:9px;line-height:1.45;margin:3px 0}.ambiguity{margin-top:12px;padding:8px;border-left:2px solid #e6c45d;background:#081e1544;font-size:9px;line-height:1.5}.ambiguity b{display:block;color:#e6c45d;font-family:Inter,sans-serif;font-size:7px;letter-spacing:.12em}.board-empty{display:grid;place-content:center;height:190px;text-align:center;color:#b8b099;font-size:11px}.lesson-thread{min-height:0;display:grid;grid-template-rows:auto 1fr auto auto;border-left:1px solid #29313d;background:#0c1016}.quick,.suggestions{display:flex;gap:5px;padding:9px;overflow-x:auto;border-bottom:1px solid #242b35}.quick button,.suggestions button{white-space:nowrap;border:1px solid #2c3541;background:#131923;color:#bbc3ce;padding:6px 8px;border-radius:12px;font-size:7px}.history{min-height:0;overflow-y:auto;padding:12px;display:grid;align-content:start;gap:8px}.welcome{color:#687381;font-size:9px;line-height:1.6}.message{padding:9px 10px;background:#171e28;border:1px solid #26303d;border-radius:8px;font-size:9px;line-height:1.5;white-space:pre-wrap}.message span{display:block;color:#6ce1d9;font-size:7px;letter-spacing:.13em;margin-bottom:4px}.message.user{margin-left:18%;background:#211625;border-color:#462b42}.message.user span{color:#ff84bf}.message.error{border-color:#833953;color:#ffc0d9}.message.error button{display:block;margin-top:7px;border:0;background:#ff70b7;color:white;padding:4px 8px;border-radius:5px}.suggestions{border-top:1px solid #242b35;border-bottom:0}.lesson-thread form{display:grid;grid-template-columns:1fr 36px;margin:0 9px 9px;border:1px solid #2b3542;background:#111721;border-radius:8px}.lesson-thread textarea{height:48px;resize:none;border:0;background:none;color:white;padding:9px;font-size:9px;outline:0}.lesson-thread form button{margin:8px;width:28px;height:28px;border:0;border-radius:50%;background:#ff70b7;color:white}@keyframes chalk{from{opacity:0;transform:translateY(5px);filter:blur(2px)}to{opacity:1;transform:none;filter:none}}@media(max-width:680px){main{grid-template-columns:1fr}.lesson-thread{position:absolute;right:0;top:42px;bottom:0;width:42%;background:#0c1016f5}.stage{padding-right:44%}}
</style>
