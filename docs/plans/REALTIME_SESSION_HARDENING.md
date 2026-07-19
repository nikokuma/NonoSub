# R6 — Realtime session hardening

Status: completed and verified.

## Goal

Make every live-caption connection prove that OpenAI accepted the intended session configuration before NonoSub starts the ScreenCaptureKit audio clock. Reconnects repeat the same handshake exactly once and cannot merge text or timing from the abandoned connection.

The implementation follows OpenAI's documented lifecycle: `session.created` is the first server event, `session.update` changes the session, and `session.updated` acknowledges the applied settings. Translation sessions accept a target output language and optional source transcription; transcription-only sessions additionally accept an optional source-language hint and the low-delay setting. Sources: [Realtime translation guide](https://developers.openai.com/cookbook/examples/voice_solutions/realtime_translation_guide) · [translation server events](https://developers.openai.com/api/reference/resources/realtime/translation-server-events) · [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)

## Protocol contract

- Open the mode-specific authenticated WebSocket.
- Wait up to eight seconds for `session.created` before sending configuration.
- Send one `session.update` with a unique generation/attempt `event_id`.
- Wait up to eight seconds for `session.updated` and validate the echoed configuration:
  - Translated mode must remain a translation session, use the requested target language, and enable `gpt-realtime-whisper` source transcription.
  - Original-only mode must be a transcription session using 24 kHz PCM, `gpt-realtime-whisper`, low delay, and the requested source hint when one was explicitly selected.
- Do not start ScreenCaptureKit or send PCM before acknowledgement succeeds.
- Correlate configuration errors through `error.event_id`; reject a closed, malformed, out-of-order, or timed-out handshake without streaming audio.
- Translation mode continues to auto-detect the source because the dedicated translation-session schema does not expose a source-language hint. Original-only mode sends the supported hint when Source is not Auto.

## Timing and reconnect behavior

- Advance the canonical live capture clock only after an audio append is successfully written to the active socket.
- Commit/control messages do not advance the clock.
- On an unexpected read/write failure, close active source/target clauses at the last successfully transmitted clock, show `reconnecting`, and spend the single reconnect allowance.
- A reconnect repeats `session.created → session.update → session.updated` with a new configuration event ID.
- Only after the new acknowledgement succeeds does the coordinator begin a new alignment epoch and capture resume.
- Audio already captured for a failed send is discarded rather than replayed into the new epoch.
- A requested stop or acknowledged `session.closed` never consumes the reconnect allowance.
- A failed reconnect produces one recoverable error and stops capture without affecting file mode.

## Error classification

- WebSocket authentication failures are non-retryable authentication errors.
- unavailable/forbidden endpoint or model responses are non-retryable model errors.
- rate limits and transport failures retain retryable classifications.
- rejected or semantically mismatched session configuration is non-retryable for that live start; retrying an unchanged invalid configuration would not help.
- Runtime server errors remain visible and recoverable, but a configuration-correlated error aborts the handshake.

## Non-goals

- R7 owns target-language retranslation for existing file sessions.
- R8 owns multi-window snapshot ordering.
- R11 owns long-session resource bounds.
- R12 owns broader media-time preservation when decode or capture fails.
- This repair does not change clause segmentation, subtitle rendering, external playback, speaker behavior, or lesson prompts.

## Acceptance

- No audio append can precede a validated `session.updated` acknowledgement.
- Both translated and original-only configurations validate their required echoed fields.
- Auto source omits the transcription language; an explicit source hint is sent only where the API supports it.
- Capture time advances by exactly the samples from successful append sends.
- One unexpected disconnect performs one fully acknowledged reconnect and creates one new alignment epoch.
- Normal stop/close does not reconnect.
- Configuration timeout, correlated error, wrong target, wrong session type, and malformed lifecycle ordering fail safely.
- Focused Rust regressions and the full `pnpm verify` suite pass.

## Verification

- Thirty focused live tests cover lifecycle parsing, configuration event IDs, translated/original-only acknowledgement validation, source-hint routing, correlated errors, malformed ordering, successful-send clocking, reconnect epochs, and the one-use reconnect allowance.
- `pnpm verify` passes with zero Svelte errors or warnings, 80 frontend tests, a successful production build, 80 Rust tests, and warning-free clippy.
- A paid/native reconnect fault injection remains part of the final R13 acceptance matrix; this checkpoint does not intentionally interrupt Nico's working live demo connection.
