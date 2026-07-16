import { describe, expect, it } from "vitest";
import { DEFAULT_LANGUAGES, DEFAULT_STYLE, DEFAULT_SYNC, type SpeakerProfile } from "./contracts";
import { FIXTURE_SEGMENTS } from "./fixtures";
import { buildTutorContext, effectiveStyle, parsePreferences, renameSpeaker, serializePreferences } from "./preferences";

describe("local preferences and tutor context", () => {
  it("round-trips styles and clamps persisted overlay position", () => {
    const serialized = serializePreferences({ level: "advanced", style: { ...DEFAULT_STYLE, position: { x: 4, y: -2 } }, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true });
    const parsed = parsePreferences(serialized);
    expect(parsed?.level).toBe("advanced");
    expect(parsed?.style.position).toEqual({ x: 0.92, y: 0.12 });
  });

  it("migrates older preferences to coordinated live timing", () => {
    const parsed = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }));
    expect(parsed?.sync.liveMode).toBe("coordinated");
    expect(parsed?.processingMode).toBe("translated");
  });

  it("migrates the placeholder Nono Pop preset to Momento Cutout", () => {
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: { ...DEFAULT_STYLE, preset: "nono-pop" },
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.preset).toBe("momento");
  });

  it.each(["cinema", "contrast", "manga", "retro"])("falls back from removed preset %s", (preset) => {
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: { ...DEFAULT_STYLE, preset },
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.preset).toBe(DEFAULT_STYLE.preset);
  });

  it("migrates older styles to the complete Cyberia palette", () => {
    const legacyStyle = { ...DEFAULT_STYLE } as Partial<typeof DEFAULT_STYLE>;
    delete legacyStyle.cyberiaColors;
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: legacyStyle,
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.cyberiaColors).toEqual(DEFAULT_STYLE.cyberiaColors);
  });

  it("round-trips customized Cyberia colors", () => {
    const style = {
      ...DEFAULT_STYLE,
      preset: "cyberia" as const,
      cyberiaColors: { ...DEFAULT_STYLE.cyberiaColors, panel: "#123456", sourceText: "#fedcba" },
    };
    const parsed = parsePreferences(serializePreferences({ level: "advanced", style, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true }));
    expect(parsed?.style.cyberiaColors.panel).toBe("#123456");
    expect(parsed?.style.cyberiaColors.sourceText).toBe("#fedcba");
  });

  it.each(["classic-outline", "yellow-drop", "arcade"] as const)("round-trips the %s preset", (preset) => {
    const style = { ...DEFAULT_STYLE, preset };
    const parsed = parsePreferences(serializePreferences({ level: "beginner", style, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true }));
    expect(parsed?.style.preset).toBe(preset);
  });

  it("migrates older styles to the default Arcade palette", () => {
    const legacyStyle = { ...DEFAULT_STYLE } as Partial<typeof DEFAULT_STYLE>;
    delete legacyStyle.arcadeColors;
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: legacyStyle,
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.arcadeColors).toEqual(DEFAULT_STYLE.arcadeColors);
  });

  it("round-trips a green Arcade terminal palette", () => {
    const style = {
      ...DEFAULT_STYLE,
      preset: "arcade" as const,
      arcadeColors: { text: "#53ff9b", panel: "#071109" },
    };
    const parsed = parsePreferences(serializePreferences({ level: "advanced", style, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true }));
    expect(parsed?.style.arcadeColors).toEqual(style.arcadeColors);
  });

  it("round-trips original-only processing", () => {
    const parsed = parsePreferences(serializePreferences({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "original_only", onboardingComplete: true }));
    expect(parsed?.processingMode).toBe("original_only");
  });

  it("forces source display only while preserving the translated preference", () => {
    const translatedStyle = { ...DEFAULT_STYLE, displayMode: "both" as const };
    expect(effectiveStyle(translatedStyle, "original_only").displayMode).toBe("source");
    expect(effectiveStyle(translatedStyle, "translated").displayMode).toBe("both");
    expect(translatedStyle.displayMode).toBe("both");
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
