# AI contribution ledger

NonoSub was built from a fresh repository during OpenAI Build Week 2026. This ledger records contributions by deliverable and commit instead of assigning an unverifiable percentage to any participant.

## Nico

- Defined the product, education pitch, scope, learner experience, and release priorities.
- Owns the Nono character, name, likeness, source models, outfits, artwork, and recorded motion.
- Created and approved the visual references, subtitle directions, chalkboard asset, teacher-form model, and animation performances.
- Performed native acceptance testing and made the final product and submission decisions.

## Primary Codex task

The primary Codex task built the application from scratch and remains the source of the majority of the implementation. Its work includes:

- the Tauri 2, Rust, Svelte 5, Vite, and strict TypeScript foundation;
- local-media decoding, chunking, diarized transcription, contextual translation, cancellation, retry, and cleanup;
- realtime ScreenCaptureKit capture, OpenAI transport, adaptive synchronization, and live caption lifecycle;
- the menu-bar product shell, viewer, overlay, launcher, settings/transcript, and floating lesson surfaces;
- all subtitle renderers and preference migration;
- structured GPT-5.6 lesson contracts, chalkboard presentation, interaction, and accessibility fallbacks;
- reliability repairs, regression suites, CI, release verification, and most project documentation;
- the model promotion and enlarged lesson-stage integration in commits `536d56c` and `e6e848c`.

The Build Week submission uses this primary task's `/feedback` session. The final session ID is recorded in the submission form rather than hard-coded into the application.

## Codex sessions orchestrated through Claude Fable 5

Claude Fable 5 orchestrated and reviewed five bounded Codex CLI implementation tasks for Nono's procedural teaching tails. Codex generated the implementation in these commits:

| Commit | Deliverable | Codex session ID |
| --- | --- | --- |
| `248cf8a` | Spring dynamics and tip-drawn underline | `019f7c66-fc92-7f42-84e3-30e7c29cd04b` |
| `d3fd107` | Curve-based cable solver | `019f7c6e-72fc-7250-88ef-3d89860db193` |
| `7dff836` | Anticipation and retract motion | `019f7c7a-6780-7d33-9606-a564d2bc1955` |
| `776b178` | Moment-synced cues, tip aim, and chalk prop | `019f7d45-a175-7070-b5bb-0bb63fcc8f18` |
| `6d7f740` | One-cue-per-moment lesson validation | `019f7d4c-5326-7703-97d9-93beb026fda2` |

The failed sandbox-only precursor `019f7c1d-fb78-7962-be6c-7dc55ed0c110` changed no files and is not counted as implementation.

## Claude Fable 5 direct implementation

Claude Fable 5 directly implemented the animation/export and model-repair lane checkpointed in `d0fdc3c` and `3b71c3b`:

- the repaired clip cutter and quaternion-hemisphere handling;
- fidelity-preserving release preparation and export/audit updates;
- the current nine-clip candidate/production rebuild path;
- Nono mood and clip-name integration changes;
- the model-repair report, archival diff, and promotion handoff.

Fable wrote the technical processing code; the character artwork, source model, clothing, and recorded motion remain Nico's work and property.

## Read-only investigations

- Codex thread `019f84ee-068b-78b2-89cc-9b8d571d3976` diagnosed the quaternion-hemisphere animation problem in a read-only sandbox and changed no code.
- Kimi session `session_04a02ab1-f3be-4876-ac19-1c278c1d0909` independently verified the animation failure and identified wrist/finger despiking concerns. It changed no code.

Read-only analysis is deliberately distinguished from implementation.

## Runtime OpenAI models

Runtime model behavior is separate from development-agent work:

- `gpt-4o-transcribe-diarize` produces timestamped, diarized file transcripts.
- `gpt-5.6-sol` provides contextual translations and validated structured teaching lessons.
- `gpt-realtime-translate` produces live source/translation transcript events.

Responses requests use `store:false`. Runtime inputs, privacy behavior, and retention limits are documented in [PRIVACY.md](PRIVACY.md).
