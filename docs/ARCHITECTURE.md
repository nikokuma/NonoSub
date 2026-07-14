# Architecture

## Trust boundary

The Tauri webview never receives a stored API key. The one-time onboarding value is sent directly to a Rust command, written to the OS credential vault, cleared from UI state, and never returned. All later OpenAI authorization is constructed in Rust.

Rust owns:

- credential-vault operations;
- scoped local media access;
- AAC demux/decode and resampling;
- temporary WAV chunks and cleanup;
- OpenAI HTTP/SSE requests;
- retry/error classification and cancellation;
- timeline normalization, boundary merge, stable internal speaker IDs, and live session events.

Svelte owns:

- video playback and synchronization;
- overlay/transcript/tutor rendering;
- session-event reduction;
- speaker display names/colors;
- non-sensitive style and learner preferences.

## Canonical flow

1. `prepare_media` canonicalizes an MP4/MOV path and dynamically grants only that file to Tauri's range-capable asset protocol.
2. `prepare_audio` decodes the default AAC track through Symphonia, averages channels to mono, linearly resamples to 16 kHz, and writes signed 16-bit WAV chunks into a process-owned temporary directory.
3. The first target boundary is 30 seconds. Later targets are 120 seconds. Each searches ±5 seconds for the lowest 200 ms RMS window. If no suitable boundary can be selected, the next chunk overlaps by 1.5 seconds.
4. `start_analysis` streams each upload through `gpt-4o-transcribe-diarize` using `diarized_json`, Japanese guidance, and automatic server chunking.
5. Finalized local timestamps receive the chunk's global offset. Boundary duplicates prefer the more complete text; distinct overlaps remain available for two-line stacking.
6. A clean 2–10 second first-chunk segment per speaker becomes an internal WAV data-URL reference, up to four speakers. UI names remain separate from internal IDs.
7. Pending lines are translated in batches of at most six. GPT‑5.6 receives speaker IDs and at most 80 preceding lines, uses low reasoning, `store:false`, and a strict JSON schema.
8. The UI starts after 15 seconds of translated coverage. It pauses below 2 seconds of lead and resumes at 8 seconds.
9. Tutor calls include the selected line, learner level, up to 80 preceding lines, available following context, and the local question thread. Plain-text deltas stream back to the dock.

## Cleanup

`tempfile::TempDir` owns every chunk and speaker reference. Replacing/cancelling a session drops the directory. Process exit also removes it. The app never persists transcripts or tutor messages.

## Known Build Week risks

- Symphonia decoding is implemented and fixture-tested at the sample/WAV layer, but original AAC sample media still needs live acceptance.
- The known-speaker multipart representation must be confirmed against a real account response.
- Exact long-video speaker stability and retry behavior require the planned ten-minute fixture.
- Linear resampling is intentionally simple; it is adequate for speech transcription but a dedicated band-limited resampler remains an optional quality improvement.
