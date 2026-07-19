# Pinned lesson identity and placement

Status: complete for audit repair R9.

## Goal

Every Ask Nono request must remain attached to the exact finalized source utterance, session, learner settings, dialogue context, and local follow-up thread that produced it. Compact composer placement must remain temporary, while the full chalkboard restores and persists its own per-monitor position.

## Canonical lesson selection

1. Rust creates a monotonically increasing lesson selection ID when a finalized subtitle opens Ask Nono.
2. `LessonOpenContext` carries that selection ID, the canonical session ID, and a cloned canonical subtitle. The lesson surface renders this pinned subtitle instead of following the mutable global selection.
3. The request command accepts only the selection ID, question, learner level, and local thread. Rust resolves the selected subtitle, nearby dialogue, speakers, languages, and session from canonical state.
4. A request is rejected before the API call if its selection is no longer current, its session was replaced, or the selected source revision changed.
5. The same checks run again after the model returns. A late answer cannot enter the cache or appear on another selection.
6. Opening another subtitle or replacing/cancelling the session invalidates the previous open context. Frontend request generations additionally discard any already-resolving response.

## Cache identity

1. Build the in-memory cache key from the complete validated lesson input: schema version, session ID, exact source revision, learner level, language settings, canonical nearby dialogue, speaker map, exact trimmed question, and local thread.
2. Do not lowercase questions; capitalization can carry linguistic meaning.
3. Validate that every returned `LessonCard.selectedSegmentId` exactly matches the pinned segment. Retry a mismatched model response once as malformed output.
4. A source revision, language change, context change, learner change, or follow-up change produces a different key. Old entries are therefore unreachable even before R11 adds hard cache bounds.

## Frontend thread identity

1. Key the hidden local question thread by session ID plus the selected source revision, not segment ID alone.
2. Reopening the same unchanged line in the same session retains its thread.
3. Reused IDs in another session and revised source lines start clean threads.
4. A response is accepted only while the same Rust selection ID remains open.

## Placement semantics

1. The 720×210 composer remains cursor-relative and ephemeral.
2. Programmatic composer movement and resizing never writes `lessonPlacements`.
3. When the first validated lesson is ready, the 980×620 board restores the saved normalized position for the source monitor, or uses the safe upper-right default.
4. Only manual movement while the full lesson is visible persists a placement.
5. Programmatic restore/resize events are suppressed so they cannot race the movement listener.
6. A failed follow-up restores the existing board at its full-board position; an initial failure remains in the compact composer error state.

## Non-goals

- R10 owns file pause/resume and experimental external-media ownership.
- R11 owns hard bounds for lesson cache, message history, transcript rendering, errors, and coordinator history.
- No lesson schema, prompt presentation vocabulary, Nono animation, or visual redesign changes.
- No transcript or lesson persistence beyond the active process.

## Acceptance

- Reused segment IDs across sessions cannot share lessons or hidden threads.
- A stable ID whose source text changes cannot reuse or display its old lesson.
- Changed language, learner level, nearby dialogue, speaker map, question capitalization, or follow-up thread changes the cache identity.
- A late response from a replaced selection is discarded before cache insertion and rendering.
- Model output naming another segment is retried once and never displayed.
- Composer movement never overwrites saved board placement.
- The full board restores and saves independently on each monitor.
- Full `pnpm verify` passes.

## Verification

- Rust tests pin the canonical session/source revision, accept translation-only updates, reject revised source text and replacement sessions, bound canonical dialogue context, distinguish every cache input, and reject mismatched model-selected IDs.
- TypeScript tests isolate hidden threads by session and source revision while retaining them across a same-line reopen, and prevent composer/programmatic movement from persisting as board placement.
- The lesson surface no longer subscribes to mutable global selection state or submits transcript context supplied by the webview.
- `pnpm verify` passes with zero Svelte errors or warnings, 92 frontend tests, a successful production build, 92 Rust tests, and warning-free clippy.
