import { describe, expect, it } from "vitest";
import { DEFAULT_LANGUAGES, DEFAULT_STYLE, DEFAULT_SYNC, type SpeakerProfile } from "./contracts";
import { applyPreferenceAction, decodePreferenceEnvelope, effectiveStyle, mergePreferencePatch, parsePreferences, preferencePatchBetween, renameSpeaker, serializePreferences } from "./preferences";

describe("local preferences and tutor context", () => {
  it("round-trips styles and clamps persisted overlay position", () => {
    const serialized = serializePreferences({ level: "advanced", style: { ...DEFAULT_STYLE, position: { x: 4, y: -2 } }, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true, lessonPlacements: {}, experimentalExternalPause: false });
    const parsed = parsePreferences(serialized);
    expect(parsed?.level).toBe("advanced");
    expect(parsed?.style.position).toEqual({ x: 0.92, y: 0.12 });
  });

  it("migrates older preferences to coordinated live timing", () => {
    const parsed = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }));
    expect(parsed?.sync.liveMode).toBe("coordinated");
    expect(parsed?.processingMode).toBe("translated");
  });

  it("migrates the placeholder Nono Pop preset to Momento", () => {
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

  it("migrates older styles to the complete Wired palette", () => {
    const legacyStyle = { ...DEFAULT_STYLE } as Partial<typeof DEFAULT_STYLE> & { cyberiaColors?: typeof DEFAULT_STYLE.wiredColors };
    delete legacyStyle.wiredColors;
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: legacyStyle,
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.wiredColors).toEqual(DEFAULT_STYLE.wiredColors);
  });

  it("migrates Cyberia to Wired while preserving customized colors", () => {
    const style = {
      ...DEFAULT_STYLE,
      preset: "cyberia",
      cyberiaColors: { ...DEFAULT_STYLE.wiredColors, panel: "#123456", sourceText: "#fedcba" },
    } as unknown as Partial<typeof DEFAULT_STYLE> & { preset: string; cyberiaColors: typeof DEFAULT_STYLE.wiredColors };
    delete style.wiredColors;
    const parsed = parsePreferences(JSON.stringify({ level: "advanced", style, languages: DEFAULT_LANGUAGES }));
    expect(parsed?.style.preset).toBe("wired");
    expect(parsed?.style.wiredColors.panel).toBe("#123456");
    expect(parsed?.style.wiredColors.sourceText).toBe("#fedcba");
  });

  it.each(["classic-outline", "yellow-drop", "fallout"] as const)("round-trips the %s preset", (preset) => {
    const style = { ...DEFAULT_STYLE, preset };
    const parsed = parsePreferences(serializePreferences({ level: "beginner", style, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "translated", onboardingComplete: true, lessonPlacements: {}, experimentalExternalPause: false }));
    expect(parsed?.style.preset).toBe(preset);
  });

  it("migrates older styles to the default Fallout palette", () => {
    const legacyStyle = { ...DEFAULT_STYLE } as Partial<typeof DEFAULT_STYLE>;
    delete legacyStyle.falloutColors;
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: legacyStyle,
      languages: DEFAULT_LANGUAGES,
    }));
    expect(parsed?.style.falloutColors).toEqual(DEFAULT_STYLE.falloutColors);
  });

  it("migrates Arcade to Fallout while preserving a green terminal palette", () => {
    const style = {
      ...DEFAULT_STYLE,
      preset: "arcade",
      arcadeColors: { text: "#53ff9b", panel: "#071109" },
    } as unknown as Partial<typeof DEFAULT_STYLE> & { preset: string; arcadeColors: typeof DEFAULT_STYLE.falloutColors };
    delete style.falloutColors;
    const parsed = parsePreferences(JSON.stringify({ level: "advanced", style, languages: DEFAULT_LANGUAGES }));
    expect(parsed?.style.preset).toBe("fallout");
    expect(parsed?.style.falloutColors).toEqual(style.arcadeColors);
  });

  it("round-trips original-only processing", () => {
    const parsed = parsePreferences(serializePreferences({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES, sync: DEFAULT_SYNC, processingMode: "original_only", onboardingComplete: true, lessonPlacements: {}, experimentalExternalPause: false }));
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

  it("migrates and clamps normalized lesson placement", () => {
    const parsed = parsePreferences(JSON.stringify({
      level: "beginner",
      style: DEFAULT_STYLE,
      languages: DEFAULT_LANGUAGES,
      lessonPlacements: { display: { monitorKey: "display", x: 2, y: -1 } },
    }));
    expect(parsed?.lessonPlacements.display).toEqual({ monitorKey: "display", x: 1, y: 0 });
  });

  it("applies native menu preference actions without mutating input", () => {
    const base = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }))!;
    const updated = applyPreferenceAction(base, "display_source");
    expect(updated?.style.displayMode).toBe("source");
    expect(base.style.displayMode).toBe("both");
    expect(applyPreferenceAction(base, "live_mode_fast_source")?.sync.liveMode).toBe("fast_source");
    expect(base.experimentalExternalPause).toBe(false);
    expect(applyPreferenceAction(base, "external_pause_on")?.experimentalExternalPause).toBe(true);
  });

  it("defaults experimental external pause off for v3 preferences", () => {
    const parsed = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }));
    expect(parsed?.experimentalExternalPause).toBe(false);
  });

  it("accepts only newer valid canonical preference envelopes", () => {
    const preferences = {
      level: "beginner" as const,
      style: DEFAULT_STYLE,
      languages: DEFAULT_LANGUAGES,
      sync: DEFAULT_SYNC,
      processingMode: "translated" as const,
      onboardingComplete: true,
      lessonPlacements: {},
      experimentalExternalPause: false,
    };
    expect(decodePreferenceEnvelope({ revision: 4, preferences, rebased: false }, 3)?.revision).toBe(4);
    expect(decodePreferenceEnvelope({ revision: 3, preferences, rebased: false }, 3)).toBeUndefined();
    expect(decodePreferenceEnvelope({ revision: 5, preferences: { broken: true }, rebased: false }, 4)).toBeUndefined();
  });

  it("merges narrow preference patches without overwriting unrelated settings", () => {
    const base = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }))!;
    const moved = mergePreferencePatch(base, { style: { position: { x: 0.2, y: 0.3 } } });
    const translated = mergePreferencePatch(moved, { languages: { target: "ja", explanation: "ja" } });

    expect(translated.style.position).toEqual({ x: 0.2, y: 0.3 });
    expect(translated.languages).toEqual({ source: "auto", target: "ja", explanation: "ja" });
    expect(base.style.position).toEqual(DEFAULT_STYLE.position);
  });

  it("derives leaf-only patches for native menu actions", () => {
    const base = parsePreferences(JSON.stringify({ level: "beginner", style: DEFAULT_STYLE, languages: DEFAULT_LANGUAGES }))!;
    const updated = applyPreferenceAction(base, "display_source")!;
    expect(preferencePatchBetween(base, updated)).toEqual({ style: { displayMode: "source" } });
  });
});
