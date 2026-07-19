# Architecture

## Trust boundary

The Tauri webviews never receive a stored API key. The onboarding value goes directly to a Rust command, is written to the operating-system credential vault, is cleared from UI state, and is never returned. A separate local marker stores only that setup completed; startup and status checks use that marker and never open Keychain. This prevents changing ad-hoc development builds from blocking launch behind a macOS password sheet. Rust constructs every later OpenAI request. Debug builds may read `OPENAI_API_KEY` from their process environment for local automation; release builds compile out that fallback.

Rust owns credentials, scoped media access, decoding, temporary audio, ScreenCaptureKit, OpenAI HTTP/WebSocket traffic, retries, cancellation, cleanup, and the canonical session. Svelte owns video playback, the four visual surfaces, transcript/lesson interactions, and non-sensitive preferences.

## Multi-window session

One Svelte build routes by query string:

- `?surface=workbench`: setup, language routing, progress, transcript, speakers, and styling;
- `?surface=viewer`: borderless internal video with hidden-on-idle controls;
- `?surface=overlay`: compact transparent live-caption window;
- `?surface=lesson`: always-on-top Nono chalkboard.

Rust's `SessionSnapshot` is authoritative. Each window requests a snapshot, then applies sequenced `SessionEvent` envelopes. A session-ID change or event gap forces a fresh snapshot. Starting a file or live session cancels the previous mode. Preferences are local, non-sensitive, and broadcast to every open surface.

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
10. Changing the file target reuses source segments and retranslates them without redecoding or retranscribing.

## Live Captions flow

Live mode is compiled only on macOS and requires macOS 14 or later.

1. `AsyncSCContentSharingPicker` presents Apple's native display/application/window picker.
2. ScreenCaptureKit captures audio only at 48 kHz mono and excludes NonoSub's own process audio.
3. A stateful converter averages adjacent float samples into 24 kHz PCM16. Approximately 100 ms is base64 encoded per message; PCM is never written to disk.
4. Rust authenticates a WebSocket to `/v1/realtime/translations?model=gpt-realtime-translate`, configures `session.audio.output.language`, and appends audio continuously.
5. Source and translated transcript deltas upsert one provisional `Live Audio` segment. Output completion or a quiet delta interval finalizes it.
6. One unexpected disconnect triggers a single reconnect and a visible `reconnecting` phase. A second failure becomes recoverable rather than affecting file mode.
7. Stop sends `session.close`, stops appending audio, drains until `session.closed` or a bounded timeout, then closes capture.

Realtime translation currently auto-detects its source language; the source override is honored by file transcription. Changing the live target stops the active stream and asks the user to restart Live Captions with the new language.

## Structured lessons

Clicking any finalized line selects the source utterance, opens the lesson surface, and pauses file playback through a cross-window event. Closing the lesson resumes only when the viewer had been playing. Live capture never pauses.

`gpt-5.6-sol` returns a strict `LessonCard` deck with one to three ordered teaching moments. Nono presents exactly one moment at a time; each has a short speech bubble, at most two compact text sections, an optional ambiguity note, and one bounded demonstration chosen from sentence breakdown, omitted meaning, literal-to-natural, tone scale, mini dialogue, or none. GPT supplies semantic labels and teaching content, while Svelte owns every coordinate, style, arrow, animation, Next/Skip control, and progress marker.

Follow-up questions begin the API request while the current board erases, then hold a thinking state until the complete validated deck is ready and animate the next moment into place. Invalid output is retried once and never partially rendered; a failed request restores the previous board. Decks are cached in memory by segment, learner level, and question. Follow-ups include the selected source, nearby dialogue, up to 80 preceding lines, available following context, and the local lesson thread.

## Cleanup and failure isolation

`tempfile::TempDir` owns file chunks and speaker references. Replacing/cancelling a session or exiting drops the directory. Live PCM is not persisted. Transcripts, speaker names, lesson cards, and chats remain memory-only.

Live capture is a separate module behind `cfg(target_os = "macos")`; permission, picker, WebSocket, and capture failures surface in the workbench without changing file playback. Non-macOS builds return a clear unavailable error for live mode.

## Build Week proof status

Automated decoder, resampler, chunking, readable-turn splitting, merge, event parsing, PCM conversion, canonical sequencing, reducer, and preference tests pass. The native app launches without touching Keychain, and Nico's AAC-in-MOV file passes the real decoder and Japanese→English file pipeline. ScreenCaptureKit translation is proven in both directions, and the 10:30 two-voice fixture retained only Speaker 1/Speaker 2 across three consecutive chunks in the corrected native rerun.
