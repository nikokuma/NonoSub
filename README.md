# NonoSub

> NonoSub turns foreign-language media into an interactive language lesson while you watch.

NonoSub is a privacy-conscious Tauri desktop app for watching Japanese video with synchronized Japanese and contextual English subtitles. Click any past line to pause, inspect the surrounding dialogue, and ask Nono why the speaker said it that way.

Built from scratch for OpenAI Build Week 2026 in the Education category using Codex, GPT‑5.6, and `gpt-4o-transcribe-diarize`.

## Build Week status

The repository currently contains:

- a runnable Tauri 2 / Svelte 5 desktop shell;
- local MP4/MOV playback through Tauri's range-capable scoped asset protocol;
- the canonical session contract and a deterministic Japanese fixture;
- synchronized bilingual overlays, transcript history, click-to-pause selection, speaker rename/color, drag placement, and six subtitle presets;
- Beginner, Intermediate, and Advanced tutoring controls;
- a Three.js Nono tutor presentation with a text-first fallback;
- OS credential-vault storage for the OpenAI key and live model-access validation;
- pure-Rust MP4/MOV AAC decoding, mono 16 kHz WAV conversion, silence-aware chunking, and temporary-file cleanup;
- streamed diarized transcription parsing, contextual Structured Output translation, coverage events, cancellation, retry-once behavior, and streamed GPT‑5.6 tutoring;
- fixture/unit tests that make no paid API calls.

Still awaiting live acceptance proof: the final indirect-refusal demo video, one-minute transcript review, ten-minute speaker-continuity review, signed-off translation quality, and release packaging. Model access and the first live tutoring review have passed. See [Build Week log](docs/BUILD_WEEK_LOG.md).

## Supported media

Build Week scope is deliberately narrow:

- local `.mp4` or `.mov` files;
- AAC audio;
- Japanese speech translated to English;
- Apple Silicon macOS is the only verified release target.

The repository includes [`demo/NonoSubTwoSpeakerFixture.mp4`](demo/NonoSubTwoSpeakerFixture.mp4), an original roughly 34-second, two-voice Japanese test clip. See [`demo/README.md`](demo/README.md) for its provenance, purpose, and reproducible FFmpeg build command.

Livestreams, browser/system audio, YouTube/Twitch URLs, accounts, saved transcripts, vocabulary decks, cloud sync, mobile, and overlapping-speech separation research are post-hackathon work.

## Run from source

Requirements:

- macOS on Apple Silicon for the verified path;
- Node.js 20 or newer;
- pnpm 10 or newer;
- stable Rust;
- Xcode command-line tools;
- an OpenAI API project with access to `gpt-5.6` and `gpt-4o-transcribe-diarize` for live analysis.

```bash
pnpm install
pnpm verify
pnpm tauri dev
```

The browser-only `pnpm dev` preview demonstrates the deterministic fixture. Native file access, keychain storage, media decoding, and OpenAI calls require `pnpm tauri dev`.

Build the unsigned macOS artifact with:

```bash
pnpm tauri build --target aarch64-apple-darwin
```

For an unsigned Build Week download, macOS may quarantine the app. Prefer building from source. If you trust the repository and downloaded artifact, Gatekeeper can be cleared explicitly:

```bash
xattr -dr com.apple.quarantine /Applications/NonoSub.app
```

## Privacy

- The video file remains local and is made readable only through a temporary, user-selected Tauri asset scope.
- AAC is decoded locally. Temporary WAV chunks are sent to OpenAI's transcription API.
- Transcript context and tutor questions are sent to GPT‑5.6.
- The API key is stored in the operating-system credential vault and is never returned to the UI.
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
  → Svelte player, transcript, and tutor
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

- `gpt-4o-transcribe-diarize`: streamed finalized Japanese segments with timestamps and speaker labels.
- `gpt-5.6-sol`: the flagship GPT‑5.6 model, used for batched context-aware English subtitles through Structured Outputs and streamed grammar/tone/culture tutoring.
- Responses requests use `store:false`; requested batches contain up to six target lines and at most 80 preceding lines.

References: [speech to text](https://developers.openai.com/api/docs/guides/speech-to-text), [GPT‑5.6 guidance](https://developers.openai.com/api/docs/guides/latest-model), and [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs).

## Codex and provenance

This implementation was created in the primary Codex task used for the Build Week `/feedback` submission. It was scaffolded into a new repository and does not import source from the older experimental NonoSub. The only reused product asset is Nico-owned `static/assets/Nono.glb`; its SHA-256 is recorded in [Asset rights](ASSET_LICENSE.md).

## License

Source code is MIT licensed. Nono's model, character, name, likeness, artwork, logos, and brand assets are excluded from the MIT license and remain all rights reserved. See [LICENSE](LICENSE) and [ASSET_LICENSE.md](ASSET_LICENSE.md).
