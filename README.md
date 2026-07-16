# NonoSub

> NonoSub turns foreign-language media into an interactive language lesson while you watch.

NonoSub is a privacy-conscious, menu-bar-first Tauri desktop app for understanding media in another language. Open a local video for diarized, contextual subtitles or listen to another macOS app through Apple's native system-audio picker. Click any finalized line and Nono turns it into a structured language lesson on a floating chalkboard.

Built from scratch for OpenAI Build Week 2026 in the Education category using Codex, GPT‑5.6, and `gpt-4o-transcribe-diarize`.

## Build Week status

The repository currently contains:

- a menu-bar controller plus separate Wired workbench, borderless viewer, compact live overlay, and floating lesson windows;
- local MP4/MOV playback through Tauri's range-capable scoped asset protocol;
- generalized source, subtitle, and explanation languages with an in-memory canonical session shared across every window;
- synchronized bilingual or fast original-only overlays, transcript history, click-to-pause/resume ownership, speaker rename/color, persistent placement, live Settings previews, and six focused subtitle presets: Clean, Classic Outline, Yellow Drop, Arcade, Momento Cutout, and Cyberia;
- Beginner, Intermediate, and Advanced progressive chalkboard lessons with one-to-three focused teaching moments, deterministic diagrams, and scroll-preserving follow-up history;
- a Three.js Nono presentation with a complete text fallback;
- OS credential-vault storage for the OpenAI key, a non-sensitive local configured marker, and live model-access validation;
- pure-Rust MP4/MOV AAC decoding, mono 16 kHz WAV conversion, silence-aware chunking, and temporary-file cleanup;
- streamed diarized transcription parsing, contextual Structured Output translation, target-only retranslation, coverage events, cancellation, and retry-once behavior;
- macOS 14 ScreenCaptureKit system-audio capture, 48→24 kHz PCM16 conversion, realtime translation or low-latency transcription-only sessions, one automatic reconnect, and graceful drain/close;
- readable timed splitting for paragraph-sized transcription turns, before contextual translation;
- original Japanese indirect-refusal and English reverse-direction fixtures plus fixture/unit tests that make no paid API calls.

Japanese→English and English→Japanese file analysis, structured tutoring, HEVC compatibility playback, bidirectional ScreenCaptureKit live translation, and three-chunk speaker continuity on the 10:30 fixture have passed on the development Mac. Remaining manual gates are timing/preset review, failure-path hardening, and final release review. See [Build Week log](docs/BUILD_WEEK_LOG.md).

## Supported media

Build Week scope is deliberately narrow:

- local `.mp4` or `.mov` files;
- AAC audio;
- any speech language supported by the selected models, with Auto source detection by default;
- any configured target/explanation language;
- Apple Silicon macOS is the only verified release target.

The repository includes three original, reproducible fixtures: a roughly 34-second two-voice technical clip, a 24-second indirect-refusal teaching clip, and a roughly 58-second English reverse-direction clip. See [`demo/README.md`](demo/README.md) for provenance and FFmpeg build commands.

Live Captions require macOS 14+ and use the Apple ScreenCaptureKit picker. URL downloading, embedded browsing, global media control, accounts, saved transcripts, vocabulary decks, cloud sync, mobile, live diarization, and overlapping-speech separation research are outside Build Week scope.

For the most reliable file-mode speaker continuity, each recurring speaker should appear in the first roughly 30-second chunk used to build internal reference samples. Build Week does not attempt to discover a brand-new speaker late in a file.

## Run from source

Requirements:

- macOS on Apple Silicon for the verified path;
- Node.js 20 or newer;
- pnpm 10 or newer;
- stable Rust;
- Xcode command-line tools;
- an OpenAI API project with access to `gpt-5.6-sol`, `gpt-4o-transcribe-diarize`, and optionally `gpt-realtime-translate`.

```bash
pnpm install
pnpm verify
pnpm tauri dev
```

The browser-only `pnpm dev` preview demonstrates the deterministic fixture. Native file access, keychain storage, media decoding, and OpenAI calls require `pnpm tauri dev`.

Repeated ad-hoc debug rebuilds have a changing macOS code identity, which can make Keychain ask for the Mac password again. NonoSub never reads Keychain during launch, so the workbench remains usable. For local development only, a key may instead be supplied to the Rust process without passing through the webview:

```bash
read -s OPENAI_API_KEY
export OPENAI_API_KEY
pnpm tauri dev
unset OPENAI_API_KEY
```

Release builds ignore this environment fallback and use the operating-system credential vault.

Build the unsigned macOS artifact with:

```bash
pnpm tauri build --target aarch64-apple-darwin
```

For an unsigned Build Week download, macOS may quarantine the app. Prefer building from source. If you trust the repository and downloaded artifact, Gatekeeper can be cleared explicitly:

```bash
xattr -dr com.apple.quarantine /Applications/NonoSub.app
```

The complete review flow is in [Judge instructions](docs/JUDGE_INSTRUCTIONS.md).

## Privacy

- The video file remains local and is made readable only through a temporary, user-selected Tauri asset scope.
- AAC is decoded locally. Temporary WAV chunks are sent to OpenAI's transcription API.
- Live Captions streams only the selected system audio to OpenAI as short PCM16 batches and never writes it to disk.
- Transcript context and tutor questions are sent to GPT‑5.6.
- Original-only mode skips automatic GPT‑5.6 translation; finalized source captions remain clickable, and translation or cultural help is requested only when the user asks Nono.
- The API key is stored in the operating-system credential vault and is never returned to the UI.
- A local marker records only that a key was configured, allowing launch without a Keychain access prompt; it contains no key material.
- Transcript and tutor history are memory-only for the current app session.
- Temporary audio is deleted when the prepared session is replaced, cancelled, or the process exits.
- There are no NonoSub accounts, analytics, subscriptions, hosted proxy, or cloud database.

OpenAI API inputs and outputs are not used for training by default. Standard Responses requests may have up to 30 days of abuse-monitoring retention even with `store:false`. See [Privacy and data flow](docs/PRIVACY.md) and [OpenAI's data controls](https://developers.openai.com/api/docs/guides/your-data).

## Architecture

```text
local MP4/MOV
  → Rust AAC demux/decode
  → mono 16 kHz WAV chunks
  → gpt-4o-transcribe-diarize
  → stable speaker/timeline merge
  → GPT-5.6 structured contextual translation
  → canonical session events
  → canonical Rust session
  → Svelte viewer, overlay, transcript, and lesson windows

original-only local MP4/MOV
  → the same diarized transcription and timeline merge
  → no automatic GPT-5.6 translation
  → immediately usable source captions and on-demand Nono lessons

selected macOS system audio
  → ScreenCaptureKit (NonoSub audio excluded)
  → stateful 48→24 kHz PCM16
  → gpt-realtime-translate WebSocket
  → source/translation deltas
  → compact always-on-top overlay

selected macOS system audio (original only)
  → ScreenCaptureKit and stateful 24 kHz PCM16
  → gpt-realtime-whisper transcription-only WebSocket
  → immediate source captions with no translation output
```

Rust owns local media access, secrets, decoding, chunk scheduling, OpenAI requests, retries, cleanup, and canonical live analysis. Svelte owns playback, synchronized display, tutoring interaction, and non-sensitive preferences. Details: [Architecture](docs/ARCHITECTURE.md).

## Tests

```bash
pnpm check
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
```

Tests cover the TypeScript reducer, active/overlap selection, coverage hysteresis, Rust resampling/WAV output, silence-aware boundaries, global timestamps, overlap merging, streamed event parsing, contracts, and output extraction. Paid live acceptance tests are intentionally separate.

## Model usage

- `gpt-4o-transcribe-diarize`: streamed finalized source-language segments with timestamps and speaker labels.
- `gpt-5.6-sol`: contextual any-language subtitles and validated `LessonCard` Structured Outputs for grammar, tone, meaning, and culture.
- `gpt-realtime-translate`: streaming source and translated transcript deltas for Live Captions.
- Responses requests use `store:false`; requested batches contain up to six target lines and at most 80 preceding lines.

References: [speech to text](https://developers.openai.com/api/docs/guides/speech-to-text), [GPT‑5.6 guidance](https://developers.openai.com/api/docs/guides/latest-model), and [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs).

## Codex and provenance

This implementation was created in the primary Codex task used for the Build Week `/feedback` submission. It was scaffolded into a new repository and does not import source from the older experimental NonoSub. The only reused product asset is Nico-owned `static/assets/Nono.glb`; its SHA-256 is recorded in [Asset rights](ASSET_RIGHTS.md).

No license is granted for this repository. Nono's model, character, name, likeness, artwork, logos, and brand assets remain all rights reserved. Third-party components retain their respective terms; see [Third-party notices](THIRD_PARTY_NOTICES.md).
