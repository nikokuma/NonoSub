# R12 — Transport, cancellation, and media-time correctness

R12 makes network completion explicit and preserves the source timeline through silence, decode faults, failed sends, and reconnects.

## HTTP and SSE

- OpenAI connections use a 15-second connect timeout and 45-second read-idle timeout.
- Responses requests are limited to 120 seconds; transcription uploads/streams to 300 seconds.
- Diarized transcription succeeds only after `transcript.text.done`; explicit stream errors and truncated EOF are retryable failures.
- Error handling exposes only sanitized status/code/request-ID metadata.
- One retry uses bounded jittered backoff and respects a bounded `Retry-After` delay.
- File request waits and retry delays observe the session cancellation token.

## Live lifecycle and clocks

- Live startup is serialized by a lease installed before the Apple picker and OpenAI connection.
- Replaced starts cancel and drain prior startup/session tasks.
- WebSocket sends time out after five seconds; shutdown drains for up to three seconds before aborting.
- Ping receives Pong throughout configuration and active capture.
- Capture and transmission clocks are separate. Capture time advances for every sample; sent spans map to capture intervals.
- Silence, failed sends, reconnects, and skipped/corrupt file packets create timeline gaps rather than compressing later speech.
- The 48→24 kHz converter preserves an odd input sample between calls.

## Verification

- Fault-focused tests cover terminal SSE, explicit stream errors, cancellation/backoff, duplicate starts, send failure, clock gaps, reconnect epochs, corrupt packet gaps, and odd-sample continuity.
- `pnpm verify` passes before the R12 checkpoint is committed and pushed.
