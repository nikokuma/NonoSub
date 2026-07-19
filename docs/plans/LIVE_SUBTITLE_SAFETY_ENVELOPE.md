# R2 — Live subtitle safety envelope

Status: implementation and automated/fixture verification complete; native acceptance and checkpoint pending.

## Goal

Keep every live subtitle preset compact and readable even if a realtime provider, reconnect, or future backend regression supplies an arbitrarily long source or translation string. Canonical transcript and lesson text remain complete.

## Safety contract

- Bound watching-overlay text before it reaches the DOM; never mutate the canonical segment.
- Prefer the newest readable tail for growing speech and prefix a visible ellipsis when older text is omitted.
- Use conservative width-, font-, script-, and display-mode-aware text budgets.
- Clamp each visible row in every preset as a final CSS safety barrier.
- In bilingual mode, source and translation receive two lines each.
- In a single-language mode, the visible row may use three lines.
- Metadata, speaker labels, preset frames, and interaction targets stay inside the envelope.
- Keep the live caption content region at or below 180 logical points and the native overlay window at or below 240 logical points, including transparent bleed for preset borders and shadows.
- The overlay may shrink for short captions but never expand into a large screen-covering panel.
- Continue using deterministic live font-size steps; do not reintroduce per-fragment DOM measurement or scaling.

## Non-goals

- Do not change live clause formation, source/translation pairing, or lag calculation from R1.
- Do not truncate transcript history, tutor context, or persisted preferences.
- Do not implement the deferred Sequential Learning presentation mode.
- Do not implement live diarization, multiple speaker lanes, or long-session pruning.
- File subtitles retain their existing exact DOM fitter and media-timestamp behavior.

## Acceptance

- Clean, Classic Outline, Yellow Drop, Fallout, Momento, and Wired remain fully contained at normal and minimum overlay widths.
- Pathological source and translation strings cannot grow the overlay beyond 240 logical points.
- Growing text shows its newest readable tail without layout flashing.
- Source-only and translation-only modes use the available height without clipping.
- Right-click/lesson selection still resolves the complete canonical segment by ID.
- Focused TypeScript regressions, Svelte checks, production build, Rust tests, and warning-free clippy pass.
- Browser fixtures are visually inspected over light and dark video-like backgrounds for all six presets.

## Verification

- Added a provider-independent grapheme envelope that retains the newest readable source and translation tails while leaving canonical session text untouched.
- Added two-line bilingual and three-line single-language CSS barriers to all six live renderers.
- Capped the caption host at 180 logical points and the transparent native window at 240 logical points with a 30-point top/bottom decorative bleed.
- Added a deliberately pathological fixture containing thousands of source and translation characters.
- At 900×240 and 520×240, every preset remained contained; the minimum-width pathological cards measured between 93 and 118 logical points high.
- Source-only and translation-only modes remained contained across every preset.
- Browser console inspection found no renderer warnings or errors.
- `pnpm verify` passed with zero Svelte warnings, 74 frontend tests, a successful production build, 57 Rust tests, and warning-free clippy.
