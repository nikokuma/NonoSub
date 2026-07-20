# R11 — Bounded long-session resources

R11 keeps canonical session history complete while bounding transient processing and UI work.

## Invariants

- Local media decoding never holds full-file float mono and resampled buffers at the same time.
- Files longer than four hours fail with a clear recoverable preparation error.
- The canonical Rust transcript remains complete and memory-only for the active session.
- The workbench initially renders the newest 200 transcript lines and loads older lines in 200-line batches.
- Recoverable errors retain only the newest 50 entries.
- The structured lesson cache is a 128-entry LRU and lesson requests accept at most 32 prior messages.
- Realtime event identity and clause pairing retain only bounded recent history.
- File retranslation polls lightweight completion state and clones the canonical snapshot only once.
- Session replacement, cancellation, and live restart release completed or obsolete task handles.

## Verification

- Focused Rust tests cover streaming resampling, duration rejection, bounded caches, errors, and realtime retention.
- Frontend tests cover ordered segment upserts, bounded errors, and 200-line transcript paging.
- `pnpm verify` passes before the R11 checkpoint is committed and pushed.
