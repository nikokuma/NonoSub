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
  { type: "session_reset", mode: "file", languages: { source: "ja", target: "en", explanation: "en" } },
  { type: "phase_changed", phase: "ready" },
  { type: "speaker_discovered", speaker: { id: "speaker-1", displayName: "Aiko", color: "#ff83bd" } },
  { type: "speaker_discovered", speaker: { id: "speaker-2", displayName: "Sato", color: "#78dfc2" } },
  ...FIXTURE_SEGMENTS.map((segment): SessionEvent => ({ type: "transcript_finalized", segment })),
  { type: "coverage_changed", translatedThroughMs: 60_000 },
];

export const LONG_LIVE_FIXTURE_EVENTS: SessionEvent[] = [
  { type: "session_reset", mode: "live", languages: { source: "auto", target: "en", explanation: "en" } },
  { type: "phase_changed", phase: "ready" },
  {
    type: "caption_upserted",
    segment: {
      id: "live-long",
      origin: "live",
      startMs: 84_000,
      endMs: 108_000,
      sourceText: "配信とかはしたいと思ってるし、なんならあの動画がまだ編集中なんですけど、字幕だけ、あと字幕だけちょっと待って、あとちょっと、ちょっとあの音が聞き取りづらいところがあったからそこだけちょっと",
      translationText: "Um, I do want to do streams and things, and if anything, the video is still being edited, but only the captions—just wait a bit, and just a little longer—there were a few spots where the audio was hard to hear, just that part.",
      speakerId: "live-audio",
      isProvisional: true,
      transcriptionStatus: "pending",
      translationStatus: "pending",
    },
  },
];

export const FIXTURE_LESSON: LessonCard = {
  selectedSegmentId: "seg-4",
  title: "A refusal hiding in ちょっと",
  speechBubble: "The speaker never says ‘no’ outright—the unfinished ちょっと makes the listener do the social homework.",
  boardSections: [
    { heading: "Literal pieces", lines: ["今日は — as for today", "ちょっと — a little…", "The ending is deliberately omitted"] },
    { heading: "Natural meaning", lines: ["Today does not work for me.", "In this context: a soft refusal"] },
    { heading: "Why it sounds polite", lines: ["A blunt negative is avoided", "The pause invites the listener to infer it"] },
  ],
  ambiguityNote: "ちょっと does not always mean no; the setup and trailing pause establish that reading here.",
  suggestedFollowUps: ["Why use んですけど?", "Could this sound rude?", "How would I refuse more directly?"],
};

export const QUICK_PROMPTS = ["Break it down", "Literal vs natural", "Tone & politeness", "What is omitted?"] as const;
