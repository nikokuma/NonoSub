import { describe, expect, it } from "vitest";
import { DEFAULT_LANGUAGES, DEFAULT_STYLE, type SpeakerProfile } from "./contracts";
import { FIXTURE_SEGMENTS } from "./fixtures";
import { buildTutorContext, parsePreferences, renameSpeaker, serializePreferences } from "./preferences";

describe("local preferences and tutor context", () => {
  it("round-trips styles and clamps persisted overlay position", () => {
    const serialized = serializePreferences({ level: "advanced", style: { ...DEFAULT_STYLE, position: { x: 4, y: -2 } }, languages: DEFAULT_LANGUAGES, onboardingComplete: true });
    const parsed = parsePreferences(serialized);
    expect(parsed?.level).toBe("advanced");
    expect(parsed?.style.position).toEqual({ x: 0.92, y: 0.12 });
  });

  it("renames only the selected stable speaker", () => {
    const speakers: Record<string, SpeakerProfile> = { a: { id: "a", displayName: "Speaker 1", color: "#fff" } };
    expect(renameSpeaker(speakers, "a", "  Haru ").a.displayName).toBe("Haru");
    expect(renameSpeaker(speakers, "missing", "Haru")).toBe(speakers);
  });

  it("includes selected, preceding, and available following dialogue", () => {
    const context = buildTutorContext(FIXTURE_SEGMENTS, "seg-4", 2, 1);
    expect(context.map((segment) => segment.id)).toEqual(["seg-2", "seg-3", "seg-4", "seg-5"]);
  });
});
