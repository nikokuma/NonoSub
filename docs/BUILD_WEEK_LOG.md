# Build Week log

## July 13, 2026 — foundation and vertical spine

- Created `/Users/nico/Projects/NonoSub` from the Tauri 2 Svelte/TypeScript template.
- Confirmed the old NonoSub source was not imported.
- Copied only Nico-owned `Nono.glb` and recorded its matching SHA-256.
- Added strict checks, focused tests, CI, MIT/source versus reserved-brand licensing, privacy docs, and submission checklist.
- Built the canonical TypeScript/Rust contracts and deterministic Japanese indirect-refusal fixture.
- Implemented the player workspace, bilingual overlay, transcript rail, speaker controls, six presets, movable placement, tutor dock, and 3D fallback.
- Visually checked the 1280×720 layout and fixed a grid-row collapse found during browser QA.
- Launched the native macOS Tauri binary successfully after the final icon/config pass.
- Implemented keychain storage, range-scoped media loading, decoder/chunker, OpenAI protocol layer, pipeline events, coverage buffering, retry/cancellation, and streamed tutoring.
- Automated checks pass; live model-access and short Japanese transcription probes are now proven, while original demo and long-content acceptance remain pending.

## Human gates still required

- [x] Nico requested optional hackathon credits before July 17 at noon PT.
- [x] Nico entered an API key; live GPT‑5.6 Sol and diarized-transcription probes passed.
- [ ] Nico approves the player/overlay interaction in the native app.
- [ ] Nico approves one-minute Japanese transcript quality.
- [ ] Nico approves ten-minute two-speaker stability.
- [ ] Nico approves natural English and the indirect refusal.
- [ ] Nico compares Beginner and Advanced explanations.
- [ ] Nico approves the public repository and unsigned release wording.

## July 14, 2026 — live access checkpoint

- Hackathon credits requested and API key stored in the OS credential vault.
- Confirmed a `store:false` Responses call through `gpt-5.6-sol` returned the exact requested probe text.
- Confirmed `gpt-4o-transcribe-diarize` transcribed a locally generated Japanese AAC/WAV probe into two diarized segments.
- Corrected the implementation from the unavailable lookup slug `gpt-5.6` to the concrete flagship model ID `gpt-5.6-sol`.
