# Architecture

## Trust boundary

The Tauri webviews never receive a stored API key. A candidate onboarding key goes directly to Rust and is validated before it can replace the working credential in the operating-system vault. A separate local marker stores only non-secret capability results, validation time/schema, and configured state; startup reads that marker and never opens Keychain. Old markers are configured-but-unvalidated rather than ready. Rust constructs every later OpenAI request. Debug builds may read `OPENAI_API_KEY` from their process environment for local automation; release builds compile out that fallback.

Rust owns credentials, scoped media access, decoding, temporary audio, ScreenCaptureKit, OpenAI HTTP/WebSocket traffic, retries, cancellation, cleanup, and the canonical session. Svelte owns video playback, the four visual surfaces, transcript/lesson interactions, and non-sensitive preferences.

## Multi-window session

One Svelte build routes by query string:

- `?surface=workbench`: setup, language routing, progress, transcript, speakers, and styling;
- `?surface=viewer`: borderless internal video with hidden-on-idle controls;
- `?surface=overlay`: compact transparent live-caption window;
- `?surface=lesson`: always-on-top Nono chalkboard;
- `?surface=launcher`: temporary file-drop and live-start surface.

Rust's `SessionSnapshot` is authoritative. Each window installs its `SessionEvent` listener before requesting a snapshot, queues events during that request, then drains only contiguous envelopes. Duplicate events are ignored; a session-ID change or sequence gap triggers one serialized snapshot refresh so a delayed request cannot overwrite newer state. Starting a file or live session cancels the previous mode.

Preferences are non-sensitive and persisted locally, but Rust orders the running app's canonical value. Windows submit deep leaf patches with their last observed revision. Rust rebases stale patches under one lock, applies language-session side effects in that same order, increments the revision, and broadcasts a validated full preference envelope. This preserves unrelated changes from simultaneous surfaces without allowing style or placement saves to restart translation.

The tray calls Rust directly. Closing a window hides it; quitting is explicit. macOS activation policy is `Accessory` for overlay-only operation and `Regular` while the workbench or viewer is visible.

## Analyzed File flow

1. `prepare_media` canonicalizes an MP4/MOV path and dynamically grants only that file to Tauri's range-capable asset protocol.
2. `prepare_audio` decodes AAC through Symphonia, mixes to mono, resamples to 16 kHz, and writes signed 16-bit WAV chunks into a process-owned temporary directory.
3. The first target boundary is 30 seconds and later boundaries are 120 seconds. Each searches ±5 seconds for the quietest suitable 200 ms window. The fallback overlaps by 1.5 seconds.
4. `gpt-4o-transcribe-diarize` receives each chunk with streamed diarized JSON. Source `auto` omits a language hint; a selected source sends it.
5. Local timestamps receive the chunk's global offset. Boundary duplicates prefer the more complete text; distinct overlaps remain available for two-line stacking.
6. Paragraph-sized turns are split at punctuation or whitespace into display-sized units. Their timestamps are allocated proportionally before translation, preserving click targets and contextual translation while preventing wall-of-text overlays.
7. One clean 2–10 second first-chunk reference per speaker becomes an internal WAV data URL, up to four speakers. UI names remain separate from stable internal IDs. Build Week speaker discovery is then frozen; later unmatched trailing fragments inherit the adjacent known voice rather than creating a false identity.
8. Pending lines are translated in batches of at most six. `gpt-5.6-sol` receives the language pair, speaker context, and up to 80 preceding lines with low reasoning, `store:false`, and a strict schema constrained to the exact requested IDs and batch length. NonoSub additionally rejects missing, duplicate, unknown, blank, incomplete, and refused output before applying any result. Retryable failures receive one retry; other terminal batch failures become clickable source-only lines so coverage and later batches continue, with an explicit one-line retry in Settings & Transcript.
9. Playback starts at 15 seconds of translated coverage, pauses below two seconds of lead, and resumes at eight seconds.
10. Changing the file target reuses the finalized source transcript without redecoding or retranscribing. A separate monotonically increasing request generation stages every validated translation batch off-state; the old target remains visible until one exact, current-session `file_retranslation_applied` event atomically replaces every line. Failure, cancellation, a newer target request, or a changed source revision leaves the prior subtitles untouched.

## Live Captions flow

Live mode is compiled only on macOS and requires macOS 14 or later.

1. `AsyncSCContentSharingPicker` presents Apple's native display/application/window picker.
2. ScreenCaptureKit captures audio only at 48 kHz mono and excludes NonoSub's own process audio.
3. A stateful converter averages adjacent float samples into 24 kHz PCM16. Approximately 100 ms is base64 encoded per message; PCM is never written to disk.
4. Rust authenticates the mode-specific WebSocket and waits for the documented first `session.created` event.
5. Rust sends one generation/attempt-scoped `session.update`, waits for `session.updated`, and validates the echoed session type, target language, source transcription, input format, and explicit source hint where supported. ScreenCaptureKit does not start and PCM cannot be sent before this acknowledgement.
6. Approximately 100 ms of audio is appended at a time. The capture clock follows every ScreenCaptureKit sample; a separate transmission timeline maps successfully sent spans back to capture time, so silence, failed writes, and reconnect gaps cannot compress later speech.
7. **Realtime — Fast** sends audio to `gpt-realtime-translate`; source and translated transcript deltas enter independent append-only clause tracks and pair monotonically within one connection epoch.
8. **Transcript-Locked — Accurate** sends the same audio to `gpt-realtime-whisper`. Each completed Whisper item becomes an immutable, capture-timed source segment and enters one eight-slot ordered `gpt-5.6-luna` Responses worker. Luna receives at most 12 prior successful pairs/6,000 characters, streams internally, and becomes visible only after a valid terminal event, identity check, output validation, and exact Arabic-digit check. Queue or translation failure retains a clickable source-only line and an explicit retry; it never silently changes engines.
9. One unexpected read, write, or remote close spends a single reconnect allowance. The replacement connection repeats the complete acknowledged handshake before a new timing epoch begins; normal stop/close never reconnects.
10. Stop sends `session.close`, stops appending audio, aborts queued/in-flight Luna work where applicable, drains until `session.closed` or a bounded timeout, then closes capture.

Realtime — Fast currently auto-detects its source language because the dedicated translation-session contract does not expose a source hint. Original-only, Transcript-Locked, and file transcription honor an explicit source override. Changing the live engine or target affects only the next session; the active session keeps its canonical engine identity.

## Structured lessons

Opening Ask Nono on a finalized line creates a monotonic Rust selection ID containing the canonical session ID and exact source revision. The lesson surface renders that pinned subtitle rather than following mutable global selection state. Rust resolves nearby dialogue, speakers, and languages from canonical state when the question is submitted, then validates the selection again after GPT returns. A replacement session, another selection, or a revised source line prevents the late answer from entering the cache or board. File and external-media pause ownership are handled separately from lesson identity.

`gpt-5.6-sol` returns a strict `LessonCard` deck with one to three ordered teaching moments. Nono presents exactly one moment at a time; each has a short speech bubble, at most two compact text sections, an optional ambiguity note, and one bounded demonstration chosen from sentence breakdown, omitted meaning, literal-to-natural, tone scale, mini dialogue, or none. GPT supplies semantic labels and teaching content, while Svelte owns every coordinate, style, arrow, animation, Next/Skip control, and progress marker.

Follow-up questions begin the API request while the current board erases, then hold a thinking state until the complete validated deck is ready and animate the next moment into place. Invalid output—including a card naming another segment—is retried once and never partially rendered; a failed follow-up restores the previous board. Exact in-memory cache identity includes schema, session, source revision, learner level, languages, canonical context, speakers, capitalization-preserving question, and local thread. Hidden frontend threads use session plus source revision, so reused IDs and revised lines cannot inherit another lesson.

The compact composer is positioned temporarily near the clicked subtitle. Its programmatic movement is never persisted. When the complete board appears it restores the normalized full-board position for that monitor, and only manual movement of the visible lesson board updates that preference.

## Cleanup and failure isolation

`tempfile::TempDir` owns file chunks and speaker references. Replacing/cancelling a session or exiting drops the directory. Live PCM is not persisted. Transcripts, speaker names, lesson cards, and chats remain memory-only.

Live capture is a separate module behind `cfg(target_os = "macos")`; permission, picker, WebSocket, and capture failures surface in the workbench without changing file playback. Non-macOS builds return a clear unavailable error for live mode.

## Build Week proof status

Automated decoder, resampler, chunking, readable-turn splitting, merge, event parsing, PCM conversion, canonical sequencing, reducer, and preference tests pass. The native app launches without touching Keychain, and Nico's AAC-in-MOV file passes the real decoder and Japanese→English file pipeline. ScreenCaptureKit translation is proven in both directions, and the 10:30 two-voice fixture retained only Speaker 1/Speaker 2 across three consecutive chunks in the corrected native rerun.
