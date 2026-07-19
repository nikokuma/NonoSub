# Build Week log

## July 13, 2026 — foundation and vertical spine

- Created `/Users/nico/Projects/NonoSub` from the Tauri 2 Svelte/TypeScript template.
- Confirmed the old NonoSub source was not imported.
- Copied only Nico-owned `Nono.glb` and recorded its matching SHA-256.
- Added strict checks, focused tests, CI, asset-rights documentation, privacy docs, and submission checklist.
- Built the canonical TypeScript/Rust contracts and deterministic Japanese indirect-refusal fixture.
- Implemented the player workspace, bilingual overlay, transcript rail, speaker controls, six focused presets, movable placement, tutor dock, and 3D fallback.
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
- Cloned the pushed GitHub checkpoint into a fresh directory and proved `pnpm install --frozen-lockfile`, Svelte diagnostics, 10 frontend tests, the production web build, 25 Rust tests, and warning-free Clippy from that clean checkout.

### Remaining July 14 manual proof

- [x] Select a real application in Apple's ScreenCaptureKit picker and confirm first realtime subtitles.
- [ ] Confirm English→Japanese live translation latency/quality; Japanese→English live is proven.
- [ ] Approve the new minimal viewer, compact overlay, chalkboard lesson, and Wired workbench in the native app.
- [x] Re-enter the API key in the stable checkpoint build for the remaining paid reverse-direction smoke.

## July 15, 2026 — bidirectional file acceptance

- Rebuilt the exact checkpoint as a native debug bundle and completed paid acceptance through the visible app with Computer Use.
- English→Japanese completed across two audio chunks from the original 58-second English fixture. The pipeline produced 16 short, independently clickable lines instead of paragraph-sized captions; the Japanese was natural and preserved the speaker's indirect “maybe another time” refusal.
- Japanese→English completed from the original 24-second indirect-refusal fixture with 12 short lines and stable Speaker 1/Speaker 2 labels. `今日はちょっと。` became “but today is a bit...”, and the next turn correctly inferred “So you're not going today?” rather than inventing a literal standalone meaning.
- The chalkboard lesson for `今日はちょっと。` explained the omitted negative ending, soft-refusal pragmatics, and uncertainty about the exact missing words. The reverse-direction English lesson also correctly taught the tone of “maybe another time.”
- Confirmed target-only language changes reuse the existing transcript and retranslate without retranscription.
- Confirmed Keychain is not read during launch. macOS requested access only when the first explicit paid model operation needed the saved key; access was approved in the system prompt.
- Made debug bundles always show the workbench while release bundles remain menu-bar-first, giving acceptance and development runs a reliable visible control surface.
- Fixed cross-line lesson isolation: each selected subtitle now restores only its own in-memory thread and board, and an in-flight answer cannot land on a newly selected line.
- Fixed coverage recovery in the viewer so a video paused on an empty translation buffer resumes reactively when coverage arrives or analysis completes, even though a paused video no longer emits time-update events.

## July 15, 2026 — reverse live and long-speaker acceptance

- Proved English→Japanese Live Captions from QuickTime through Apple's ScreenCaptureKit picker. The compact overlay showed bounded source/target rows, finalized lines remained clickable in the transcript, and Stop Current Session drained and closed the live session cleanly. Very long uninterrupted speech can still make the rolling source and translated tails feel slightly offset; the canonical finalized transcript remains complete.
- Generated a reproducible 10:30 H.264/AAC fixture by looping the original two-voice 33-second clip. NonoSub decoded it into seven silence-aware chunks and completed a full paid analysis through 10:28.
- The long run exposed three concrete diarization-contract defects. Known-speaker arrays were sent as JSON strings instead of repeated multipart `known_speaker_names[]` and `known_speaker_references[]` fields; explicit source-language mode also sent the unsupported `prompt` field; punctuation-only tails could create a false identity. All three now have focused regression coverage.
- After the multipart fix, the two core voice identities survived chunk boundaries. The full diagnostic run still showed OpenAI occasionally assigning a new remote label to a short trailing fragment of the immediately preceding voice; NonoSub now freezes discovery after the first chunk and attaches later unmatched fragments to the adjacent known voice instead of inventing Speaker 5.
- Rebuilt the native app and reran the 10:30 fixture through three consecutive chunk boundaries. The corrected run produced 90 lines through 4:17 with only Speaker 1 and Speaker 2, including the formerly broken 0:28 boundary, with no swaps or fabricated third identity.

## July 15, 2026 — adaptive synchronization and progressive lessons

- Added measured Coordinated live timing using realtime `elapsed_ms` alignment, rolling p90 translation lag, a 600 ms margin, bounded delay adaptation, reconnect epochs, and complete clause release. Nico accepted Japanese and English arriving together at roughly 1.7–2.1 seconds behind.
- Kept Fast Source as the explicit lower-latency option and added session-only VLC-style file subtitle correction with bracket shortcuts, a reset shortcut, and a viewer HUD. Nico confirmed correction and new-file reset behavior.
- Replaced single crowded lesson cards with one-to-three focused teaching moments. The model chooses concepts and semantic demonstration data; NonoSub renders five deterministic chalkboard primitives without accepting model coordinates, HTML, or styling.
- Added visible lesson progress, named Next actions, Skip Rest, and erase/thinking/write transitions that run while follow-up API calls are in flight. The previous board remains recoverable on failure.
- Fixture browser QA at the 780×620 native lesson size exercised all three moments, Skip Rest, a complete follow-up rewrite, fixed navigation, and internally scrollable lesson content. Focused validation tests bring the frontend suite to 16 tests and the Rust suite to 40 tests.
- Rebuilt and targeted the exact native debug bundle for paid lesson acceptance. GPT‑5.6 correctly chose one focused moment for `今日はちょっと`, produced a deterministic omission diagram, and explained the indirect refusal without claiming one exact missing phrase.
- Asked the real follow-up “What exactly is omitted after ちょっと?” The replacement board offered plausible completions, marked the wording as intentionally unspecified, cited the following dialogue, and preserved the local question thread.
- Native acceptance exposed and fixed a fresh-window edge case where two absent card IDs could be mistaken for a skipped deck. A focused regression test protects the empty-to-first-lesson transition. Nono now leans with a code-rendered eraser/chalk-dust sweep while follow-up work runs.
- Replaced the placeholder Nono Pop subtitle preset with Momento Cutout: Foam/Ink bilingual cards, speaker-colored tabs and click/focus accents, stable unskewed text, and coordinated paper-wipe motion. A shared measured fitter now reduces font size against the real overlay height and scales only extreme overflow, preserving every rendered line inside the 184 px live-caption budget.
- Added Cyberia from the AllSongs/NowPlaying design studies: a compact deep-blue selected-track signal strip shared by file and live surfaces, bundled DotGothic16/JetBrains Mono typography, a semantic source/translation divider, speaker-linked tab/outline/interaction accents, six persistent palette controls, single-language layouts, and the same measured long-caption fitting path.
- Added three reference-driven broadcast/game presets shared by file and live surfaces: Classic Outline with a full black halo, Yellow Drop with a crisp black character backdrop and no directional shadow, and Arcade with a configurable amber/green terminal palette, smoky dialogue strip, scan texture, bilingual stacking, and measured long-caption fitting.
- Renamed the release-facing preset names to Momento, Wired, and Fallout, with automatic migration from the earlier Momento Cutout, Cyberia, and Arcade identifiers and palettes.
- Added an exact selected-style preview to Settings using the production subtitle renderers over a split light/dark scene. The six supported presets, display modes, font sizing, speaker visibility, custom palettes, and shared background-opacity control update immediately; the longer bilingual example visibly crosses both halves of the scene.
- Added persisted Translated versus Original only processing. File mode skips every GPT‑5.6 translation batch and buffers against finalized source coverage; live mode switches to a low-delay `gpt-realtime-whisper` transcription-only session with local speech commits. Finalized source captions remain clickable for Nono, with direct translation and cultural-context prompts available on demand.
- Replaced the lesson panel's CSS frame with Nico's transparent Blender chalkboard render. The export keeps a restrained handmade grain, removes the heavy dark backplate, preserves the warm frame and chalk props, and leaves lesson text and animations as responsive UI layers.
- Moved teaching-moment navigation below the physical chalkboard, removed board scrolling, composed text and demonstrations side by side, and tightened GPT lesson budgets so a crowded idea becomes another moment instead of overflowing. Stable teaching anchors now mark titles, source lines, sections, demonstration items, takeaways, and ambiguity notes for Nono's future tail-directed emphasis.
## July 16 — Chalk teaching direction

- Extended GPT-5.6 lesson Structured Outputs with a versioned four-color chalk presentation score, fixed chalk marks, and bounded point/underline tail cues.
- Replaced filled lesson cards with multilingual Klee One chalk writing, hand-drawn separators, brackets, corrections, tone scales, and persistent tail-drawn underlines.
- Added a full-stage Three.js Nono layer, runtime tail-rig validation, deterministic DOM-to-world cue targeting, constrained twin-tail CCD motion, reduced-motion behavior, and a complete chalk-only fallback for the current unskinned GLB.
- Added a repeatable GLB audit for skin attributes, both 12-bone tail chains, and the required `Idle`, `Think`, and `Present` clips. The current placeholder fails exactly this asset gate without blocking tutoring.
- Browser QA exercised every teaching moment at 1040×720 and 900×640. The board remains non-scrolling with no cue escaping its frame; the compact composition was adjusted so the takeaway and ambiguity treatment cannot overlap.

## July 17 — Production Nono integration

- Preserved `NonoSubCheckpoint.blend` unchanged, captured the latest live Blender scene as `NonoSubProductionSource.blend`, and generated the consolidated `NonoSubProduction.blend` working asset.
- Retargeted all teacher-jacket detail meshes from the duplicate 286-bone armature to `Nono_Rig`, joined and chest-bound the four visible plane details, isolated backups/construction geometry in `SOURCE_ONLY`, and made `NONO_EXPORT` the only export surface.
- Created an export-only covered-body mask while preserving the full body in source, normalized and capped weights to four influences, repaired a 386-vertex unweighted jacket detail through nearest-deformation-bone binding, and centered the canonical rig at the origin.
- Replaced nonportable Blender Shader-to-RGB graphs with semantic glTF materials, reduced production color textures to 1024 px, and added four restrained, hair-conforming anime highlight strokes.
- Exported and audited an 8.0 MB interim GLB with one skin, 54 skinned meshes, 95,728 triangles, 54 draws, both complete 12-bone tail chains, and all five procedural hair roots. Only the user-authored `Idle`, `Think`, and `Present` actions remain pending.
- Added 130%-bounded distal tail extension with stable plug-side joints, four-iteration constrained CCD, sanitized Blender/glTF bone-name resolution, rest-pose restoration, and no frame-to-frame accumulation.
- Added critically damped long-hair follow-through with reduced-motion reset; body clips are forbidden from keying the procedural hair, tail, and skirt-secondary bones by both Blender final-export validation and GLB audit.
- Added portable PBR fallback, submission-safe `NonoToon`, and a development-only altered NonToon-inspired comparison for ramp, rim, hair-specular, and basic-specular behavior. All three render the real skinned candidate without WebGL errors; NonoToon and the experimental variant each held 60 fps in the lesson fixture.
- Kept `NonoToon` as the release default. The experimental comparison remains development-only until Nico visually prefers it; its zlib attribution is included without granting a license to NonoSub or Nico-owned assets.

## July 18 — Live clause lifecycle repair

- Replaced historical nearest-caption translation matching with independent append-only source and target clause tracks.
- Added immutable closure at punctuation/quiet, idle, aligned-time, capture-age, and grapheme boundaries; incoming deltas honor an already-reached quiet boundary even if the scheduler has not ticked yet.
- Paired utterance groups monotonically by realtime epoch and aligned-time compatibility. Hard-split continuations remain attached to their original source and cannot shift the next translation.
- Kept one lag observation per epoch/alignment frame, updating it with the latest fragment receipt rather than weighting token fragmentation.
- Added reconnect isolation, one-time late translation fill, coordinated partial-translation hiding, replacement-ready caption holding, and immediate clearing on stop.
- Added focused adversarial regressions for shared timestamps, missing timing, elapsed resets, target-first output, source/target split-count mismatches, 441-grapheme streams, late translation, reconnects, and stop behavior.
- Added `unicode-segmentation` for real grapheme boundaries and recorded its required third-party attribution.
- Nico accepted the repaired sustained-live behavior after native playback testing; R2 now owns independent visual containment for every subtitle style.

## July 18 — Live subtitle safety envelope

- Added a frontend-only grapheme envelope so pathological live payloads render the newest readable tail while transcript history and Nono lesson context retain the complete canonical segment.
- Added independent two-line bilingual and three-line single-language clamps to Clean, Classic Outline, Yellow Drop, Fallout, Momento, and Wired.
- Capped visible caption content at 180 logical points and the transparent native overlay at 240 logical points, including 30-point top/bottom bleed for preset borders and shadows.
- Added deterministic light/dark, display-mode, long-caption, waiting-state, and pathological browser fixtures. All six presets stay contained at 900×240 and the 520×240 minimum width with no renderer warnings.
- Full verification passes with zero Svelte warnings, 74 frontend tests, a successful production build, 57 Rust tests, and warning-free clippy. Native sustained-live acceptance remains before the R2 checkpoint.

## July 19 — Session generation isolation

- Replaced the resettable process-wide cancellation flag with monotonically increasing file/live run generations and a private cancellation token for each run. Starting or cancelling a session permanently invalidates the prior token.
- Tagged selected media and prepared audio with their originating generation. The frontend carries the opaque generation from media preparation through audio preparation and file analysis, preventing mismatched artifacts from being analyzed.
- Added one generation gate shared by file pipeline, live capture, reconnect, error, completion, and old-session retranslation events. Validation and canonical-state mutation occur under the same ordering guard, so replacement cannot interleave between them.
- Classified ordinary replacement/cancellation separately from service failure. Cancelled pipelines finish silently instead of emitting a fatal error or reopening recovery UI.
- Added adversarial tests for permanent old-token cancellation, stale fatal-event rejection without sequence mutation, cancellation without fatal output, and exact frontend generation handoff.
- Full verification passes with zero Svelte warnings, 75 frontend tests, a successful production build, 60 Rust tests, and warning-free clippy.

## July 19 — Stable file boundary identities

- Made the first accepted file subtitle ID canonical across overlapping transcription chunks. A more-complete boundary result now revises the existing line in place instead of replacing it with the later chunk's local ID.
- Added normalized text matching, deterministic overlap scoring, and one-to-one reconciliation against only pre-existing segments. Distinct or simultaneous lines from the same incoming chunk can no longer collapse into one identity.
- Exact duplicates retain their completed translation and timing. A real source-text revision clears its old translation and ambiguity note, then returns only that stable subtitle to the translation queue.
- Stable source revisions emit once per text version, so the canonical transcript updates without duplicate IDs or repeated event churn. A selected subtitle continues resolving through the revision.
- Added adversarial coverage for longer boundary replacements, shorter-before-fuller arrival order, exact duplicates, repeated simultaneous phrases, cross-speaker overlap, same-chunk overlaps, split long-sentence parts, translation invalidation, and frontend selection retention.
- Full verification passes with zero Svelte warnings, 76 frontend tests, a successful production build, 68 Rust tests, and warning-free clippy.

## July 19 — Translation failure containment

- Tightened GPT‑5.6 subtitle Structured Outputs to the exact requested batch length and segment-ID enum, then added local all-or-nothing validation for missing, duplicate, unknown, blank, incomplete, refused, and out-of-order output.
- Preserved one bounded retry for malformed and transient responses. Authentication and inaccessible-model errors remain session-fatal; every other terminal batch failure marks only that batch failed and continues translating later chunks.
- Separated pending from failed translation state. Pending is now the only status that blocks coverage; failed lines advance with their original timestamps, display source text in every display mode and preset, and remain clickable for Nono lessons.
- Added an explicit per-line Retry translation action in Settings & Transcript. It captures the active session generation, reuses up to 80 preceding lines without retranscription, and safely restores source fallback if the retry fails again.
- Full verification passes with zero Svelte errors or warnings, 80 frontend tests, a successful production build, 73 Rust tests, and warning-free clippy.
