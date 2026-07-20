# R10 — Explicit playback ownership

## Goal

Opening Ask Nono from the file viewer pauses only the active media instance and resumes it only when the same lesson-owned pause is still valid. Live external-media control remains best-effort and never sends a blind close toggle.

## Contract

- `PlaybackPauseLease` binds a pause to the session, media path, lesson selection, segment, source surface, and playback revision.
- `LessonClosedContext` identifies the exact selection being closed and whether it was explicitly closed or invalidated.
- Right-click is the only subtitle action that opens Ask Nono. Left pointer input is reserved for dragging.
- Successful experimental live pause exposes an explicit Resume External Media action. Closing Nono never toggles external playback.

## Verification

- Focused lease tests cover playing, already-paused, user override, coverage ownership, session replacement, stale close, and invalidation.
- Rust tests cover close-context serialization and removal of automatic external resume state.
- `pnpm verify` must pass before the R10 checkpoint.

## Non-goals

- No Nono scene, model, shader, tail, voice, live timing, or resource-bound changes.
