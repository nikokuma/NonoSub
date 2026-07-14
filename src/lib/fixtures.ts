import type { SessionEvent, SubtitleSegment } from "./contracts";

export const FIXTURE_SEGMENTS: SubtitleSegment[] = [
  { id: "seg-1", startMs: 1_000, endMs: 5_600, sourceText: "佐藤さん、昨日のメッセージ、見ました？", naturalEnglish: "Sato, did you see my message yesterday?", speakerId: "speaker-1", transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-2", startMs: 6_200, endMs: 8_800, sourceText: "え、何ですか？", naturalEnglish: "Huh? What message?", speakerId: "speaker-2", transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-3", startMs: 9_400, endMs: 13_600, sourceText: "駅前の店、今日までなんです。", naturalEnglish: "That place by the station closes today.", speakerId: "speaker-1", transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-4", startMs: 14_500, endMs: 20_600, sourceText: "行きたくないわけじゃないんですけど、今日はちょっと……。", naturalEnglish: "It's not that I don't want to go, but today is… a little difficult.", ambiguityNote: "The unfinished phrase is a conventional, indirect refusal.", speakerId: "speaker-2", transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-5", startMs: 21_100, endMs: 25_700, sourceText: "「ちょっと」って、今日は行かないってこと？", naturalEnglish: "By “a little,” do you mean you're not going today?", speakerId: "speaker-1", transcriptionStatus: "complete", translationStatus: "complete" },
  { id: "seg-6", startMs: 26_400, endMs: 30_600, sourceText: "まあ、そういう感じです。すみません。", naturalEnglish: "Well… that's basically it. Sorry.", speakerId: "speaker-2", transcriptionStatus: "complete", translationStatus: "complete" },
];

export const FIXTURE_EVENTS: SessionEvent[] = [
  { type: "phase_changed", phase: "ready" },
  { type: "speaker_discovered", speaker: { id: "speaker-1", displayName: "Aiko", color: "#ff83bd" } },
  { type: "speaker_discovered", speaker: { id: "speaker-2", displayName: "Sato", color: "#78dfc2" } },
  ...FIXTURE_SEGMENTS.map((segment): SessionEvent => ({ type: "transcript_finalized", segment })),
  { type: "coverage_changed", translatedThroughMs: 60_000 },
];

export const QUICK_PROMPTS = ["Break it down", "Literal vs natural", "Tone & politeness", "What is omitted?"] as const;
