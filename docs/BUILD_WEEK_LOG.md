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
- Nico confirmed the core tutor controls, speaker customization, and overlay movement work in the native app. Broader display-mode/style polish is intentionally parked.
- Fixed the tutor dock to retain the full conversation, scroll independently, and follow streamed answers. Rebalanced the rail to provide a useful reading viewport at 1280×720.
- Built an original 33-second technical fixture from Nico's two synthetic-voice recordings. A live diarization probe returned six clean turns with stable `A/B/A/B/A/B` labels.

## July 14, 2026 — invisible viewer integration

- Replaced the monolithic UI with a menu-bar controller and four synchronized surfaces: optional Wired workbench, minimal internal viewer, compact transparent overlay, and floating Nono lesson.
- Generalized Japanese/English contracts to source/target/explanation languages and file/live origins. Rust now owns a sequenced canonical snapshot; windows refresh if an event is missed.
- Added file target-only retranslation, viewer coverage buffering, pause/resume ownership across the lesson window, persistent overlay placement, and macOS activation-policy switching.
- Replaced streamed tutor prose with validated GPT‑5.6 `LessonCard` Structured Output, memory-only caching, quick prompts, and follow-up history that preserves manual scroll position.
- Implemented macOS 14 ScreenCaptureKit picker/capture with self-audio exclusion, 48→24 kHz PCM16 conversion, 100 ms realtime batches, source/translation delta reconciliation, graceful close draining, and one reconnect.
- Corrected the realtime session configuration against OpenAI's dedicated translation guide (`session.audio.output.language`).
- Fixed a native Swift-concurrency loader failure by adding `/usr/lib/swift` to NonoSub's macOS runtime search path; the menu-bar-first binary now launches cleanly.
- Browser visual QA passed at 1280×720, workbench minimum 960×680, lesson 780×620, and overlay 900×220. QA found and fixed a lesson-history jump-to-bottom regression.
- All 20 Rust tests, 8 TypeScript tests, strict Svelte checks, production build, and warning-free clippy pass. Nico's `JpTestFemale.mov` also passes the actual Symphonia AAC decoder test.
- Nico completed the first real ScreenCaptureKit picker smoke: selected application audio reached `gpt-realtime-translate`, and translated target captions accumulated correctly in the canonical transcript.
- The first native smoke exposed a secondary-window `/index.html?surface=…` development-route 404. Dynamic windows now use the root app route, with a regression test covering it.
- Enabled `gpt-realtime-whisper` input transcription so live captions can show source and translation together. Translated deltas now also move the session from Buffering to Ready if source transcription is delayed.
- Produced the first unsigned Apple Silicon release artifacts: an 8.9 MB `.dmg`, an 8.7 MB `.app.zip`, and an 18 MB arm64 app bundle. The bundle is ad-hoc signed with no Team ID, as expected for the Build Week source-first release.
- Nico proved Japanese→English live capture with paired source/translation history. The first overlay exposed long-utterance coordination and clipping problems even though the canonical transcript was healthy.
- Split live presentation from transcript storage: the overlay now shows only the current utterance, reserves stable source/translation rows, uses deliberate listening/translating placeholders, rolls long captions to their readable tails, clamps both rows to two lines, and scales from 30px at 900×220 to 18px at the minimum width. A long-live deterministic fixture protects this path without paid calls.
- Fixed completed-file transcription streams that use CRLF SSE delimiters. The parser now accepts both `\n\n` and `\r\n\r\n`, preserves UTF-8 code points split across network chunks, and has focused regression coverage for both cases.
- Re-ran `JpTestFemale.mov` through the native Open Video flow after the parser fix. The real pipeline completed without the malformed-JSON error and produced three diarized Japanese segments, three contextual English translations, and a validated Beginner chalkboard lesson from the first transcript line.
- The native proof also isolated HEVC playback: WebKit advanced the iPhone `.mov` timeline but rendered black, while an H.264 control rendered correctly. NonoSub now detects HEVC with Symphonia and creates a temporary 720p H.264/AAC playback proxy using macOS `avconvert`; the original file remains the transcription source, metadata is filtered, scoped access is limited to the proxy, and the proxy is deleted with the session.
- Re-ran the untouched `JpTestFemale.mov` after the compatibility change. Its frames, translated overlay, and correct `0:17` duration all rendered in the internal viewer.
- Checkpointed the implementation in the public GitHub repository on `codex/invisible-viewer-checkpoint` and opened draft PR #1.
- Built the original 24-second six-turn indirect-refusal fixture from Nico's supplied synthetic Japanese clips, plus a 58-second English-source fixture for reverse-direction acceptance. Both use original NonoSub visuals and have reproducible scripts.
- Added punctuation-aware splitting for paragraph-sized finalized file turns. Splitting happens before GPT translation, creates contiguous proportional timestamps, and retains every line as an independent lesson target. Focused Japanese regression coverage brings the Rust suite to 25 tests.
- Removed credential-vault reads from app launch and API-status checks. A non-sensitive local configured marker controls onboarding, while the key remains in Keychain and is fetched only for an explicit model operation. Debug builds also support a process-only `OPENAI_API_KEY` fallback so changing ad-hoc signatures do not block automation; release builds compile it out.
- Rebuilt and used Computer Use to confirm the fresh native app reaches onboarding without presenting a macOS Keychain password sheet.
- Rebuilt the current unsigned Apple Silicon `.dmg` and `.app.zip`, confirmed the app is ad-hoc signed with no Team ID, recorded SHA-256 hashes locally, and added exact judge, Gatekeeper, fixture, and source-build instructions.

### Remaining July 14 manual proof

- [x] Select a real application in Apple's ScreenCaptureKit picker and confirm first realtime subtitles.
- [ ] Confirm Japanese→English and English→Japanese live translation latency/quality.
- [ ] Approve the new minimal viewer, compact overlay, chalkboard lesson, and Wired workbench in the native app.
- [ ] Re-enter the API key once in a stable build, or provide it as a debug-process environment variable, for the remaining paid reverse-direction smoke.
