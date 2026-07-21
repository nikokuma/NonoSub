import type { LessonCard, SessionEvent, SubtitleSegment } from "./contracts";

export const FIXTURE_SEGMENTS: SubtitleSegment[] = [
  { id: "seg-1", origin: "file", startMs: 1_000, endMs: 5_600, sourceText: "佐藤さん、昨日のメッセージ、見ました？", translationText: "Sato, did you see my message yesterday?", speakerId: "speaker-1", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-2", origin: "file", startMs: 6_200, endMs: 8_800, sourceText: "え、何ですか？", translationText: "Huh? What message?", speakerId: "speaker-2", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-3", origin: "file", startMs: 9_400, endMs: 13_600, sourceText: "駅前の店、今日までなんです。", translationText: "That place by the station closes today.", speakerId: "speaker-1", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-4", origin: "file", startMs: 14_500, endMs: 20_600, sourceText: "行きたくないわけじゃないんですけど、今日はちょっと……。", translationText: "It's not that I don't want to go, but today is… a little difficult.", ambiguityNote: "The unfinished phrase is a conventional, indirect refusal.", speakerId: "speaker-2", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-5", origin: "file", startMs: 21_100, endMs: 25_700, sourceText: "「ちょっと」って、今日は行かないってこと？", translationText: "By ‘a little,’ do you mean you're not going today?", speakerId: "speaker-1", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-6", origin: "file", startMs: 26_400, endMs: 30_600, sourceText: "まあ、そういう感じです。すみません。", translationText: "Well… that's basically it. Sorry.", speakerId: "speaker-2", isProvisional: false, transcriptionStatus: "complete", translationStatus: "complete" },
];

export const FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "file", languages: { source: "ja", target: "en", explanation: "en" }, processingMode: "translated" },
  { type: "phase_changed", phase: "ready" },
  { type: "speaker_discovered", speaker: { id: "speaker-1", displayName: "Aiko", color: "#ff83bd" } },
  { type: "speaker_discovered", speaker: { id: "speaker-2", displayName: "Sato", color: "#78dfc2" } },
  ...FIXTURE_SEGMENTS.map((segment): SessionEvent => ({ type: "transcript_finalized", segment })),
  { type: "coverage_changed", readyThroughMs: 60_000 },
];

export const LONG_LIVE_FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "live", languages: { source: "auto", target: "en", explanation: "en" }, processingMode: "translated" },
  { type: "phase_changed", phase: "ready" },
  {
    type: "transcript_finalized",
    segment: {
      id: "live-long",
      origin: "live",
      startMs: 84_000,
      endMs: 108_000,
      sourceText: "配信とかはしたいと思ってるし、なんならあの動画がまだ編集中なんですけど、字幕だけ、あと字幕だけちょっと待って、あとちょっと、ちょっとあの音が聞き取りづらいところがあったからそこだけちょっと",
      translationText: "Um, I do want to do streams and things, and if anything, the video is still being edited, but only the captions—just wait a bit, and just a little longer—there were a few spots where the audio was hard to hear, just that part.",
      speakerId: "live-audio",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "complete",
    },
  },
  { type: "live_sync_changed", sync: { targetDelayMs: 2_800, observedLagMs: 2_200, status: "steady", visibleSegmentId: "live-long" } },
];

// Deliberately exceeds every backend clause limit. R2 uses this to prove the
// watching surface remains safe even if a provider or future regression sends
// a pathological segment; transcript state intentionally keeps the full text.
export const PATHOLOGICAL_LIVE_FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "live", languages: { source: "auto", target: "en", explanation: "en" }, processingMode: "translated" },
  { type: "phase_changed", phase: "ready" },
  {
    type: "transcript_finalized",
    segment: {
      id: "live-pathological",
      origin: "live",
      startMs: 120_000,
      endMs: 180_000,
      sourceText: `${"これは表示してはいけない古い字幕履歴です。".repeat(160)}ここが現在聞こえている最後の日本語です。`,
      translationText: `${"This is old caption history that must never cover the screen. ".repeat(160)}This is the newest translated sentence.`,
      speakerId: "live-audio",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "complete",
    },
  },
  { type: "live_sync_changed", sync: { targetDelayMs: 6_000, observedLagMs: 5_400, status: "degraded", visibleSegmentId: "live-pathological" } },
];

export const ORIGINAL_ONLY_FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "live", languages: { source: "ja", target: "en", explanation: "en" }, processingMode: "original_only" },
  { type: "phase_changed", phase: "ready" },
  {
    type: "transcript_finalized",
    segment: {
      id: "live-original-fixture",
      origin: "live",
      startMs: 1_000,
      endMs: 3_400,
      sourceText: "今日はちょっと……。",
      speakerId: "live-audio",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "skipped",
    },
  },
  { type: "live_sync_changed", sync: { targetDelayMs: 0, observedLagMs: 0, status: "steady", visibleSegmentId: "live-original-fixture" } },
];

export const OVERLAP_FILE_FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "file", languages: { source: "ja", target: "en", explanation: "en" }, processingMode: "translated" },
  { type: "phase_changed", phase: "ready" },
  { type: "speaker_discovered", speaker: { id: "speaker-1", displayName: "Aiko", color: "#ff83bd" } },
  { type: "speaker_discovered", speaker: { id: "speaker-2", displayName: "Sato", color: "#78dfc2" } },
  {
    type: "transcript_finalized",
    segment: {
      id: "overlap-a",
      origin: "file",
      startMs: 0,
      endMs: 60_000,
      sourceText: "ちょっと待ってください、まだ説明が終わっていないので、今ここで次の話を始めると大事なところが聞こえなくなってしまいます。",
      translationText: "Please wait a moment—I haven't finished explaining yet, and if we start the next conversation now, the important part will be impossible to hear.",
      speakerId: "speaker-1",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "complete",
    },
  },
  {
    type: "transcript_finalized",
    segment: {
      id: "overlap-b",
      origin: "file",
      startMs: 0,
      endMs: 60_000,
      sourceText: "分かっていますけど、こちらも時間がないから、結論だけでも先に教えてもらえませんか。",
      translationText: "I understand, but we're running out of time too, so could you at least tell us the conclusion first?",
      speakerId: "speaker-2",
      isProvisional: false,
      transcriptionStatus: "complete",
      translationStatus: "complete",
    },
  },
  { type: "coverage_changed", readyThroughMs: 60_000 },
];

export const FIXTURE_LESSON: LessonCard = {
  schemaVersion: 2,
  selectedSegmentId: "seg-4",
  moments: [
    {
      title: "The sentence leaves a blank",
      speechBubble: "The speaker stops before the awkward part, but the listener can still hear the refusal hiding in the silence. Sneaky, but polite.",
      gesture: "point_self",
      sourceFocus: { color: "white", tailCue: "none" },
      boardSections: [{
        heading: "What is spoken",
        lines: [
          { text: "今日は — as for today", color: "white", mark: "none", tailCue: "none" },
          { text: "ちょっと — a little…", color: "baby_blue", mark: "none", tailCue: "point" },
        ],
      }],
      demonstration: {
        kind: "omitted_meaning",
        caption: "The uncomfortable ending is understood, not spoken.",
        items: [
          { label: "今日は", detail: "as for today", color: "white", mark: "none", tailCue: "none" },
          { label: "ちょっと……", detail: "a little…", color: "baby_blue", mark: "none", tailCue: "none" },
          { label: "[行けない]", detail: "[I can't go]", color: "pink", mark: "bracket", tailCue: "none" },
        ],
        result: { text: "Today doesn't work for me.", color: "yellow", mark: "none", tailCue: "none" },
      },
      ambiguityNote: { text: "The exact missing ending is uncertain; 行けない and 難しい are plausible readings.", color: "pink", mark: "bracket", tailCue: "none" },
    },
    {
      title: "Literal words, social meaning",
      speechBubble: "The dictionary gives you ‘a little,’ but the conversation gives you ‘no for today.’ Context wins this round.",
      gesture: "point_user",
      sourceFocus: { color: "baby_blue", tailCue: "point" },
      boardSections: [],
      demonstration: {
        kind: "literal_to_natural",
        caption: "A natural translation carries the intended refusal.",
        items: [
          { label: "Literal", detail: "As for today, a little…", color: "white", mark: "none", tailCue: "none" },
          { label: "Natural", detail: "Today doesn't work for me.", color: "yellow", mark: "none", tailCue: "none" },
        ],
        result: { text: "A soft, indirect no", color: "yellow", mark: "none", tailCue: "none" },
      },
    },
    {
      title: "Why the softness matters",
      speechBubble: "Leaving the refusal unfinished protects the mood and gives the other person room to understand without being blunt.",
      gesture: "heart_touch",
      sourceFocus: { color: "white", tailCue: "none" },
      boardSections: [{
        heading: "Politeness strategy",
        lines: [
          { text: "Avoid the blunt negative", color: "pink", mark: "bracket", tailCue: "none" },
          { text: "Let the listener infer the answer", color: "white", mark: "none", tailCue: "none" },
        ],
      }],
      demonstration: {
        kind: "tone_scale",
        caption: "The meaning stays similar while the delivery gets gentler.",
        items: [
          { label: "行きません", detail: "direct refusal", color: "pink", mark: "none", tailCue: "none" },
          { label: "今日は難しいです", detail: "soft explanation", color: "white", mark: "none", tailCue: "none" },
          { label: "今日はちょっと……", detail: "indirect and gentle", color: "baby_blue", mark: "none", tailCue: "none" },
        ],
        result: { text: "More room for the listener to save face", color: "yellow", mark: "none", tailCue: "underline" },
      },
    },
  ],
  suggestedFollowUps: ["Why use んですけど?", "Could this sound rude?", "How would I refuse more directly?"],
};

export const QUICK_PROMPTS = ["Break it down", "Translate this", "Cultural context", "Literal vs natural", "Tone & politeness", "What is omitted?"] as const;
