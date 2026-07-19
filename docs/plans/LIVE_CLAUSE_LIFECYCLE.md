# R1 — Live clause lifecycle repair

Status: accepted; checkpoint pending.

## Goal

Keep continuous realtime source and translation output bounded, correctly paired, and immutable after finalization. This repair addresses the failure where one old live caption accumulated later translation output until it covered the screen.

## Contract and invariants

- Parse realtime translation deltas only from the documented append-only `delta` field and require `event_id`.
- Deduplicate text by `event_id`; multiple events may share one `elapsed_ms` value.
- Keep source and translation clause tracks independent.
- Close either track after terminal punctuation plus 350 ms quiet, 1.2 seconds idle, eight seconds of aligned audio, ten seconds of capture-clock age, or 220 grapheme clusters.
- Pair source and target groups monotonically within one realtime epoch using aligned-time compatibility.
- Keep hard-split continuation pieces attached to their original utterance; they cannot consume the next utterance's translation slot.
- A source-only fallback reserves one target group. Late translation may fill it once, while a finalized target never reopens.
- Close both active tracks at reconnect and align the new epoch from the current transmitted-audio clock.
- Keep one rolling lag sample per `(epoch, elapsed_ms)` and update it with the latest fragment receipt.
- Clear the watching caption after four seconds of silence and immediately when capture finishes.
- Keep partial target text available to canonical transcript state, but hide it from a Coordinated watching overlay until the target clause is complete.

## Non-goals

- R2 owns the independent frontend height/line safety envelope for all subtitle themes.
- R11 owns long-session pruning, transcript virtualization, and other resource bounds.
- This repair does not change file transcription, file translation, Nono lessons, model assets, shaders, or tail animation.

## Acceptance

- Sustained Japanese → English and English → Japanese speech produces several bounded clauses rather than one growing block.
- Source and target appear together in Coordinated mode.
- Long target or source hard splits do not shift the next utterance.
- A reconnect never merges text from different epochs or moves the display cursor backward.
- Stopping live capture removes the final watching caption.
- Focused Rust/TypeScript regressions and the full `pnpm verify` suite pass.

## Native acceptance

- Nico confirmed sustained live output was substantially improved.
- The measured live delay remained bounded and source/translation clauses no longer grew into a full-screen historical block.
