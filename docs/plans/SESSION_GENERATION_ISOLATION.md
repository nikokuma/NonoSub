# R3 — Session generation isolation

Status: completed and verified.

## Goal

Prevent preparation, file analysis, live capture, and language-update work from an obsolete run from mutating the canonical state of its replacement. Normal replacement and cancellation must not produce a fatal application error.

## Contract and invariants

- Every file selection or live-capture start creates one monotonically increasing internal generation.
- Replacing or cancelling a run permanently cancels that run's private token. A later run never resets an older token.
- File preparation returns its opaque generation to the trusted frontend orchestration layer; audio preparation and analysis must present the same generation.
- Selected media and prepared audio are tagged with their generation. Mismatched artifacts cannot be analyzed.
- Every asynchronous producer captures its generation before its first await.
- Producer events are accepted only while that generation is current and its token is not cancelled.
- Generation validation and canonical mutation occur under one ordering guard, so replacement cannot interleave between the check and the write.
- Obsolete producer events are silently rejected as cancellation, never converted into `FatalError`.
- File pipeline cancellation resolves normally. Genuine active-run failures retain existing recoverable/fatal behavior.
- Session IDs remain public synchronization identities; generations remain an internal producer-isolation mechanism.

## Producers covered

- macOS playback-proxy preparation and local audio decoding/chunking.
- File transcription and contextual translation pipeline events.
- Live transcription/translation events, reconnect status, recoverable errors, and completion.
- File target-language retranslation events are scoped to their originating session generation.

## Non-goals

- R4 owns stable IDs during chunk-boundary reconciliation.
- R5 owns exact translation-output validation and source-only terminal fallback.
- R7 owns atomic target-language retranslation and a separate per-language request generation.
- R8 owns frontend snapshot/listener ordering.
- R11 owns task-handle draining and long-session memory bounds.
- This repair does not change subtitle presentation, clause timing, Nono lessons, model assets, shaders, or playback ownership.

## Acceptance

- Starting run B permanently cancels run A without sharing or resetting its token.
- A late segment, phase, coverage, error, or completion event from run A cannot change run B's snapshot or sequence.
- A stale media-preparation result cannot replace run B's selected/prepared media.
- Cancelling analysis produces no fatal error and does not reopen Settings.
- Live events emitted after replacement are rejected by the same generation gate.
- A retranslation from an older session cannot write into the current session.
- File and live launch orchestration still works with the opaque generation handoff.
- Focused Rust/TypeScript tests and the full `pnpm verify` suite pass.
