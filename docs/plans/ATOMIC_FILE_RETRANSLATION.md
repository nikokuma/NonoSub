# Atomic file retranslation

Status: complete for audit repair R7.

## Goal

Changing the target language for an analyzed local file must preserve the complete current subtitle set until a complete replacement is ready. The replacement must apply in one canonical event and only if both the file session and the target-language request are still current.

## Design

1. Keep the current session target and every current translation visible while the new target is prepared.
2. Give file retranslation its own monotonically increasing request generation and cancellation token, separate from the file/live session generation.
3. Suppress duplicate requests for the same active session and language settings. This prevents unrelated style saves from restarting paid work.
4. If file analysis is still running, wait for its canonical `complete` phase before capturing the stable source transcript. Never retranscribe the media.
5. Translate the captured transcript in batches of six with up to 80 preceding lines of context. Each batch retains the existing one bounded retry and exact Structured Output validation.
6. Collect every batch outside canonical state. One terminal batch failure rejects the complete target switch and leaves the old target and subtitles untouched.
7. Before committing, revalidate the active session generation, retranslation generation, file mode, processing mode, and exact source revision.
8. Apply the complete language settings and complete replacement set through one `file_retranslation_applied` event.
9. Starting, replacing, stopping, or cancelling a session cancels its pending retranslation. Selecting the currently visible target also cancels a pending switch.

## Failure behavior

- Cancelled and superseded requests end silently.
- A current request failure produces one recoverable error explaining that the previous-language subtitles remain available.
- A failed identical request is not silently restarted by subsequent style/preference saves. Selecting a different target starts a new request.
- Coverage and playback phases do not regress while retranslation is staged.

## Non-goals

- No cross-window preference patching or desired-versus-applied preference UI; R8 owns that repair.
- No translation progress surface.
- No changes to live target-language restart behavior.
- No transcript persistence, retranscription, or new OpenAI request shape.

## Acceptance

- No partially translated target ever appears.
- Failed replacement leaves all old translations and the old canonical target intact.
- A rapid second target selection permanently invalidates the first.
- A new file session permanently invalidates the prior file's retranslation.
- One successful request changes every segment and the canonical language in one sequenced event.
- TypeScript reducer, Rust coordinator/commit tests, and the full `pnpm verify` suite pass.

The translation batches continue using strict Responses API Structured Outputs and local exact-set validation: [OpenAI Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs).

## Verification

- Focused TypeScript reducer coverage proves one-event language and segment replacement.
- Rust coverage proves duplicate suppression, failed-request suppression, supersession cancellation, stale-generation rejection, all-line atomic commit, and failure rollback.
- `pnpm verify` passes with zero Svelte errors or warnings, 81 frontend tests, a successful production build, 85 Rust tests, and warning-free clippy.
