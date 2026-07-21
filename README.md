# NonoSub

> NonoSub turns foreign-language media into an interactive language lesson while you watch.

NonoSub is a privacy-conscious, menu-bar-first Tauri desktop app for understanding media in another language. Open a local video for diarized, contextual subtitles or choose a visible macOS application, window, or display for live system-audio captions. Right-click any finalized line and Nono turns it into a structured language lesson on a floating chalkboard.

Built from scratch for OpenAI Build Week 2026 in the Education category using Codex, GPT‑5.6, and `gpt-4o-transcribe-diarize`.

## Build Week status

The repository currently contains:

- a menu-bar-first controller plus a temporary Clop-like file/live launcher, borderless viewer, compact live overlay, transparent chalkboard lesson, and optional Settings & Transcript workbench;
- local MP4/MOV playback through Tauri's range-capable scoped asset protocol;
- generalized source, subtitle, and explanation languages with an in-memory canonical session shared across every window;
- synchronized bilingual or fast original-only overlays, transcript history, Ask Nono pause/resume ownership, speaker rename/color, persistent placement, live Settings previews, and six focused subtitle presets: Clean, Classic Outline, Yellow Drop, Arcade, Momento, and Wired;
- Beginner, Intermediate, and Advanced progressive chalkboard lessons with one-to-three focused teaching moments, GPT-selected chalk colors/marks, deterministic diagrams, and a collapsed-by-default scroll-preserving follow-up drawer;
- a full-stage Three.js Nono presentation with bounded twin-tail point/underline cues, reduced-motion support, and a complete chalk-only fallback;
- a production character pipeline with a single canonical armature, export-only body masking, four-influence weight cleanup, restrained procedural hair follow-through, 130%-bounded tail reach, portable materials, a native `NonoToon` shader, and a development-only partial NonToon comparison;
- OS credential-vault storage for the OpenAI key, a non-sensitive local configured marker, and live model-access validation;
- pure-Rust MP4/MOV AAC decoding, mono 16 kHz WAV conversion, silence-aware chunking, and temporary-file cleanup;
- streamed diarized transcription parsing, contextual Structured Output translation, atomic generation-scoped target-only retranslation, coverage events, cancellation, and retry-once behavior;
- macOS 14 ScreenCaptureKit system-audio capture, 48→24 kHz PCM16 conversion, a default Realtime — Fast engine, an experimental transcript-locked Whisper→Luna engine, low-latency transcription-only sessions, one automatic reconnect, and graceful drain/close;
- readable timed splitting for paragraph-sized transcription turns, before contextual translation;
- original Japanese indirect-refusal and English reverse-direction fixtures plus fixture/unit tests that make no paid API calls.

Japanese→English and English→Japanese file analysis, structured tutoring, HEVC compatibility playback, bidirectional ScreenCaptureKit live translation, and three-chunk speaker continuity on the 10:30 fixture have passed on the development Mac. Remaining manual gates are timing/preset review, failure-path hardening, and final release review. See [Build Week log](docs/BUILD_WEEK_LOG.md).

## Supported media

Build Week scope is deliberately narrow:

- local `.mp4` or `.mov` files;
- the default decodable AAC audio track (automatic track selection only);
- speech and target languages supported by the selected OpenAI models, with Auto source detection by default;
- Apple Silicon macOS 14+ as the verified release target.

The repository includes three original, reproducible fixtures: a roughly 34-second two-voice technical clip, a 24-second indirect-refusal teaching clip, and a roughly 58-second English reverse-direction clip. See [`demo/README.md`](demo/README.md) for provenance and FFmpeg build commands.

Live Captions require macOS 14+ and use ScreenCaptureKit. NonoSub lists Apple's shareable applications, windows, and displays inside its visible launcher, then constructs a native content filter for the user's explicit selection. URL downloading, embedded browsing, accounts, saved transcripts, vocabulary decks, cloud sync, mobile, live diarization, and overlapping-speech separation research are outside Build Week scope.

Build Week media limitations are explicit: NonoSub does not ingest embedded subtitle tracks; it automatically uses the container's default decodable audio track, averages multichannel audio to mono, and uses a stateful linear file resampler. Very long diarized turns use estimated timing when split for readability. HEVC playback uses a macOS proxy and is rejected if its declared duration differs from the source by more than 750 ms. Live captions use adaptive Coordinated delay by default, and the last readable caption intentionally remains visible until its replacement or an explicit Stop.

For the most reliable file-mode speaker continuity, each recurring speaker should appear in the first roughly 30-second chunk used to build internal reference samples. Build Week does not attempt to discover a brand-new speaker late in a file.

## Run from source

Requirements:

- macOS on Apple Silicon for the verified path;
- Node.js 20 or newer;
- pnpm 10 or newer;
- stable Rust;
- Xcode command-line tools;
- an OpenAI API project with access to `gpt-5.6-sol` and `gpt-4o-transcribe-diarize`; Live Captions additionally use `gpt-realtime-translate`, while experimental Accurate mode uses `gpt-realtime-whisper` plus `gpt-5.6-luna`.

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

For a short unattended QA run, Nico can copy the existing Keychain value once into a private local file outside the repository. The setup command never prints the key:

```bash
scripts/save_qa_api_key.sh
```

After the one macOS approval, an agent can launch debug NonoSub without another password prompt:

```bash
scripts/run_unattended_qa.sh
```

The wrapper requires file permissions of `600` or `400`, exports the key only to the launched debug-process tree, and relies on the same Rust-only `OPENAI_API_KEY` path. Delete `~/Library/Application Support/com.nono.nonosub/qa-openai-api-key` when unattended testing ends. This is intentionally a short-lived QA convenience, not release credential storage.

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

selected macOS system audio (Transcript-Locked — Accurate)
  → ScreenCaptureKit and stateful 24 kHz PCM16
  → gpt-realtime-whisper finalized source clauses
  → bounded ordered gpt-5.6-luna Responses worker at low reasoning
  → terminal-validated target paired with the immutable source
  → source-only fallback on translation failure

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

Before replacing Nono's GLB, run the dedicated asset gate:

```bash
pnpm audit:nono-rig
```

It requires one canonical skin, normalized four-influence weights, skinned `Nono_Tails`, both supported 12-bone chains, the five procedural hair roots, required material roles and hair UVs, no source-only objects, the 120,000-triangle/60-draw/15 MB budgets, and all nine approved mood clips without procedural tail, hair, or skirt tracks.

The production Blender source is prepared outside the public repository so Nico-owned construction assets remain private. The shipped teacher-form GLB contains 103,072 triangles, 55 draw calls, 54 skinned meshes, both tail chains, and nine clips. The repeatable rebuild path is:

```bash
/Applications/Blender.app/Contents/MacOS/Blender \
  -b /path/to/NonoSubCheckpointFinal.blend \
  --python-exit-code 1 \
  --python scripts/cut_nono_clips.py -- \
  --output /path/to/NonoClipLibrary.blend

/Applications/Blender.app/Contents/MacOS/Blender \
  -b /path/to/NonoSubCheckpointFinal.blend \
  --python-exit-code 1 \
  --python scripts/prepare_nono_release.py -- \
  --output /path/to/NonoSubRelease.blend \
  --actions-from /path/to/NonoClipLibrary.blend

/Applications/Blender.app/Contents/MacOS/Blender \
  -b /path/to/NonoSubRelease.blend \
  --python-exit-code 1 \
  --python scripts/export_nono_final.py -- \
  --output /path/to/NonoFinal.glb

node scripts/strip_nono_glb.mjs /path/to/NonoFinal.glb
node scripts/audit_nono_glb.mjs /path/to/NonoFinal.glb
```

`static/assets/Nono.glb` is the promoted teacher form. The former bear-hoodie form is preserved as `static/assets/NonoHoodie.glb`; `static/assets/NonoCandidate.glb` remains an ignored development rebuild slot. Development builds may compare `nonoShader=toon`, `nontoon`, or `portable`; production always uses the audited release asset with `NonoToon`.

The exact suit-capture rules, forbidden procedural bones, pose checks, and final command are in [Nono animation handoff](docs/NONO_ANIMATION_HANDOFF.md).

## Model usage

- `gpt-4o-transcribe-diarize`: streamed finalized source-language segments with timestamps and speaker labels.
- `gpt-5.6-sol`: contextual any-language subtitles and validated `LessonCard` Structured Outputs for grammar, tone, meaning, and culture.
- `gpt-realtime-translate`: default Realtime — Fast source and translated transcript deltas for Live Captions.
- `gpt-realtime-whisper`: source transcription for Original-only and Transcript-Locked live sessions.
- `gpt-5.6-luna`: experimental Transcript-Locked text translation at fixed low reasoning, with streamed transport, terminal validation, exact Arabic-digit locking, and one retry. See the [Luna benchmark](docs/LUNA_LIVE_TRANSLATION_BENCHMARK.md).
- Responses requests use `store:false`; requested batches contain up to six target lines and at most 80 preceding lines.

References: [speech to text](https://developers.openai.com/api/docs/guides/speech-to-text), [GPT‑5.6 guidance](https://developers.openai.com/api/docs/guides/latest-model), and [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs).

## Codex and provenance

This implementation was created in the primary Codex task used for the Build Week `/feedback` submission. It was scaffolded into a new repository and does not import source from the older experimental NonoSub. Nico owns the Nono assets and recorded motion; exact technical contributions from Codex, Claude Fable 5, and read-only Kimi/Codex investigations are recorded in the [AI contribution ledger](docs/AI_CONTRIBUTIONS.md). Production asset hashes are recorded in [Asset rights](ASSET_RIGHTS.md).

No license is granted for this repository. Nono's model, character, name, likeness, artwork, logos, and brand assets remain all rights reserved. Third-party components retain their respective terms; see [Third-party notices](THIRD_PARTY_NOTICES.md).
