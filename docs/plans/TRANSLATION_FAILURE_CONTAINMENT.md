# R5 — Exact translation validation and source fallback

Status: completed and verified.

## Goal

Make file translation batches all-or-nothing and prevent one malformed or terminally failed GPT response from freezing playback. Every requested segment must receive exactly one valid translation, or the complete batch becomes a visible, clickable source-only fallback with an explicit retry action.

OpenAI Structured Outputs guarantees schema adherence for ordinary completed output, but refusals and incomplete responses are explicit exceptions that applications must handle separately. NonoSub therefore validates both the Responses envelope and the semantic relationship between requested and returned segment IDs. [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)

## Response contract

- The JSON schema uses the current batch size as both `minItems` and `maxItems`.
- Returned `segment_id` values are constrained to the requested IDs in the schema and revalidated locally.
- The local validator requires an exact set match:
  - No missing requested ID.
  - No unknown ID.
  - No duplicate ID.
  - Exactly one output per requested segment.
- Translation text must contain non-whitespace content.
- Translation and ambiguity strings are trimmed; an empty ambiguity note becomes `None`.
- Valid outputs are reordered into request order before the pipeline mutates state.
- `status: incomplete` is a retryable malformed response.
- A structured refusal is classified explicitly and is not automatically retried.
- Missing output text, malformed JSON, semantic validation failure, transient HTTP failure, and network failure retain their existing safe error classifications.

## Retry and terminal behavior

- Retry a retryable batch failure exactly once.
- Authentication and unavailable-model failures remain session-fatal because the user must fix configuration.
- Cancellation remains silent and generation-scoped under R3.
- Any other failure after the allowed retry is terminal only for that batch:
  - Clear translation and ambiguity data.
  - Mark every batch segment `failed`.
  - Upsert each failed segment into canonical state.
  - Emit one segment-addressable recoverable error per line.
  - Continue processing later chunks.
- `pending` is the only translation status that blocks translated coverage.
- `complete`, `failed`, and `skipped` are display-ready. A failed line advances coverage using its source timestamps, so startup and catch-up hysteresis cannot deadlock behind it.
- Failed lines are not automatically retried on every later chunk.

## Presentation and explicit retry

- While status is `pending`, the selected style may show its existing translating/catching-up treatment.
- When status is `failed` and no translation exists, every subtitle style renders the source row only—even if the user selected translation-only mode.
- Failed captions remain clickable for Nono lessons.
- Settings & Transcript labels the translation as unavailable and provides `Retry translation` on that line.
- Retry captures the active R3 generation, marks only that segment pending, sends it with up to 80 preceding lines, retries one transient/malformed failure once, and then either finalizes the translation or restores the failed source fallback.
- Retry never retranscribes media and never rewinds or blocks already-ready coverage.

## Non-goals

- R6 owns realtime session configuration and reconnect hardening.
- R7 owns atomic whole-session target-language changes and competing retranslation generations.
- R8 owns multi-window snapshot ordering.
- R9 owns lesson-cache revision invalidation.
- R11 owns long-session error-list compaction.
- This repair does not alter prompts for translation quality, subtitle timing, diarization, live clause coordination, or Nono lesson schemas.

## Acceptance

- Missing, duplicate, unknown, blank, refused, and incomplete outputs are rejected deterministically.
- A malformed response is retried once and never partially applied.
- A terminal batch failure marks every requested line failed and advances source-backed coverage past the batch.
- Later translation batches continue after a recoverable terminal failure.
- Failed lines do not display an endless “translating” placeholder.
- Source fallback works for source, bilingual, and translation-only display settings in all six presets.
- Retry targets one stable segment ID and returns to source fallback if it fails again.
- Focused Rust and TypeScript regressions and the full `pnpm verify` suite pass.

## Verification

- OpenAI response tests cover exact-set ordering, whitespace normalization, missing, duplicate, unknown, blank, incomplete, and refusal output.
- Pipeline tests prove failed segments advance source-backed coverage while pending segments still block it, and terminal batch failures emit a canonical failed upsert plus a segment-addressable error for every line.
- Frontend tests prove failed source fallback in source, bilingual, and translation-only display modes while pending translations retain their waiting row.
- `pnpm verify` passes with zero Svelte errors or warnings, 80 frontend tests, a successful production build, 73 Rust tests, and warning-free clippy.
