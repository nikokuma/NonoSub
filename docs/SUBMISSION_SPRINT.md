# NonoSub submission sprint

> Historical planning record. The shipped teacher asset now uses the nine-mood pipeline documented in [NONO_ANIMATION_HANDOFF.md](NONO_ANIMATION_HANDOFF.md), and current acceptance is tracked in [SUBMISSION_CHECKLIST.md](SUBMISSION_CHECKLIST.md).

Last updated: July 18, 2026

This document is the execution source of truth for the final OpenAI Build Week sprint. It tracks what is already protected, what remains, the completion standard for each deliverable, and the scope that must not expand before submission.

Detailed implementation plans may refine a deliverable, but they must not silently broaden or contradict this document. After each deliverable, update its status, record the verification performed, and link the checkpoint commit.

## Schedule and freeze

| Date | Purpose | Exit condition |
|---|---|---|
| July 18 | Creation day 1 | Invisible product shell, lesson voice, and tail-pacing work substantially complete |
| July 19 | Creation day 2 | Approved Nono candidate integrated; creation and architecture freeze at end of day |
| July 20 | Testing and bug fixing | Acceptance matrix completed; only release-blocking fixes remain |
| July 21 | Write-up, packaging, and submission | Artifacts, demo, documentation, Devpost entry, and recovery buffer complete |

The app must remain demoable throughout the sprint. No unfinished experiment may replace a working release path.

## Status legend

- [x] Complete and protected
- [ ] Not complete
- **ACTIVE**: the single implementation deliverable currently in progress
- **BLOCKED**: cannot proceed without an explicit dependency or Nico decision
- **STRETCH**: may be attempted only after every submission requirement is green

## Non-negotiable safety rules

- Never modify `/Users/nico/Projects/Blendr/NonoSubCheckpoint.blend`.
- Every AI/model attempt starts from a separate copy of the checkpoint.
- Never replace `static/assets/Nono.glb` until Nico approves the candidate inside NonoSub.
- Preserve the complete body and both legs during the next production attempt. Do not remove covered geometry before submission.
- Keep every candidate GLB and `.blend` outside Git until it is explicitly approved for the release.
- Do not import legacy NonoSub source.
- Do not expose the OpenAI API key to a webview or log.
- Do not write transcript text, tutor prompts, captured audio, or request bodies to logs.
- Keep transcripts, live captions, lesson history, and generated lesson audio memory-only for the active session.
- NonoSub has no project license. Required third-party license and attribution notices remain allowed and required.
- NonToon is not on the critical path and cannot block submission.
- File mode remains the guaranteed demo path. Live mode must fail independently without breaking file playback.
- The July 19 end-of-day architecture freeze is firm.

## Protected working baseline

These capabilities already work and should not be redesigned unless testing reveals a release blocker.

- [x] Local MP4/MOV loading and playback.
- [x] AAC decoding, silence-aware chunking, diarized transcription, and speaker continuity.
- [x] Contextual file translation with generalized source, target, and explanation languages.
- [x] Japanese to English and English to Japanese file acceptance.
- [x] Target-only file retranslation without retranscription.
- [x] Original-only file processing that skips translation.
- [x] Live macOS system-audio capture through ScreenCaptureKit.
- [x] Coordinated and Fast Source live-caption modes.
- [x] Adaptive live delay and file subtitle offset controls.
- [x] Compact file/live subtitle surfaces and canonical transcript history.
- [x] Clickable current and historical subtitles.
- [x] Speaker labels, renaming, and colors for file sessions.
- [x] Six supported subtitle designs with settings previews.
- [x] GPT-5.6 structured multi-moment lessons.
- [x] Four-color chalk vocabulary, deterministic demonstrations, Next, and Skip.
- [x] Non-scrolling chalkboard composition and numbered visual hierarchy.
- [x] Menu-bar and multi-window foundation.
- [x] GitHub checkpoint `f898ef7` on `codex/invisible-viewer-checkpoint`.

## Creation deliverables

### C1 — Invisible product shell

Status: **ACTIVE — implementation complete; native acceptance pending**

Goal: NonoSub should disappear into the watching experience. Subtitles float by themselves; Nono and the chalkboard appear on demand without an application backdrop.

Required work:

- [x] Make the lesson window transparent, frameless, and always on top.
- [x] Render only Nono, her speech bubble, the chalkboard, and lesson controls.
- [x] Remove the large white/background application surface around the lesson.
- [x] Allow the complete lesson assembly to be dragged and persist/clamp its position per display.
- [ ] Ensure transparent empty regions do not block unrelated applications.
- [x] Preserve interactive board controls and text selection where appropriate.
- [x] File lessons pause and resume only when NonoSub owns the pause.
- [x] Live capture continues while the lesson is open.
- [x] Closing a live lesson returns the overlay to the newest caption.
- [x] Text-only tutoring remains complete if 3D rendering fails.

Acceptance:

- [ ] The lesson appears beside Safari, Firefox, and QuickTime with no visible app rectangle.
- [ ] It remains usable at the minimum supported window size.
- [ ] Dragging, closing, Next, Skip, and follow-up input all work.
- [ ] File and live ownership behavior passes focused tests.

Focused implementation plan: `docs/plans/INVISIBLE_PRODUCT_SHELL.md`.

Implementation verification: 980×620 collapsed and 980×720 expanded lesson fixtures, 420×190 launcher, 1180×720 viewer, and 900×240 overlay were visually inspected. The complete TypeScript, Svelte, Rust, build, and clippy suites must remain green before acceptance. Native cross-application placement and right-click-menu acceptance remain for the final Mac pass.

### C2 — Menu-bar, context-menu, and file-entry workflow

Status: [ ]

Goal: normal watching should not require the workbench.

Required menu-bar actions:

- [x] Open Video…
- [x] Start/Stop Live Captions.
- [x] Show/Hide Subtitles.
- [x] Show Nono Lesson when a lesson target exists.
- [x] Arrange Overlay.
- [x] Languages.
- [x] Subtitle preset.
- [x] Learner level.
- [ ] Voice mute/volume shortcut.
- [x] Show Settings/Workbench.
- [x] Quit.

Required contextual workflow:

- [x] Right-click subtitles for preset, display-mode, speaker-name, arrangement, lesson, and Settings/Language access.
- [x] Ordinary subtitle clicks continue to open lessons.
- [x] Escape closes the lesson drawer first, then the lesson.
- [x] Add a small summonable Clop-like MP4/MOV drop target.
- [x] Accept native file-drop and open the invisible viewer immediately.
- [x] Show a compact, recoverable unsupported-file error.
- [x] Preserve Open Video and workbench diagnostic paths as alternatives.

Acceptance:

- [ ] A new user can start file or live captions without navigating the workbench after onboarding.
- [ ] The workbench is needed only for onboarding, settings, transcript inspection, errors, and diagnostics.

### C3 — Nono voice and meaningful speech bubble

Status: [ ]

Goal: Nono should teach audibly and the speech bubble should carry the concise spoken explanation rather than decorative filler.

Required work:

- [ ] Extend the lesson presentation contract with a concise spoken version for each teaching moment.
- [ ] Keep detailed grammar and demonstrations on the chalkboard.
- [ ] Display the exact spoken wording in the bubble.
- [ ] Generate lesson audio only after structured lesson validation.
- [ ] Begin `Present` when speech begins, then return naturally to `Idle`.
- [ ] Stop current speech on Next, Skip, close, subtitle replacement, or a new question.
- [ ] Generate spoken follow-up answers.
- [ ] Add mute, volume, and voice-reply preferences.
- [ ] Keep generated audio in memory only.
- [ ] Never block the written lesson when speech generation fails.
- [ ] Avoid mispronouncing source quotations when explanation and source languages differ.
- [ ] Never speak step numbers or interface labels.

Acceptance:

- [ ] One Beginner and one Advanced lesson have useful, non-repetitive spoken explanations.
- [ ] Bubble text and audio match.
- [ ] Muting, interruption, failure fallback, and lesson transitions work.

### C4 — Natural twin-tail teaching motion

Status: [ ]

Goal: retain GPT-directed point/underline behavior while making the cable tails slow, deliberate, and physically believable.

Required work:

- [ ] Add a small anticipation before reaching.
- [ ] Slow point reach to approximately 900–1,200 ms.
- [ ] Hold the point for approximately 900 ms.
- [ ] Slow underline reach to approximately 700 ms.
- [ ] Slow underline sweep to approximately 900–1,200 ms.
- [ ] Hold the completed emphasis for approximately 500 ms.
- [ ] Slow retraction to approximately 900 ms.
- [ ] Replace abrupt interpolation with natural eased motion.
- [ ] Reduce routine extension and protect the plug-side joints more strongly.
- [ ] Freeze tail assignment through the complete teaching moment.
- [ ] Reduce constant idle sway.
- [ ] Run at most one deliberate gesture sequence per moment.
- [ ] Keep the chalk underline after retraction.
- [ ] Preserve reduced-motion and no-rig fallbacks.

Acceptance:

- [ ] Point and underline are readable without feeling frantic.
- [ ] Tails do not snap, swap, tear at the plugs, or distort Nono.
- [ ] Targets around all board edges are handled safely.

### C5 — Controlled Nono reconstruction bake-off

Status: [ ]

Goal: produce an approved production character without risking the checkpoint.

Candidate tracks:

- [ ] Conservative Codex candidate: preserve all geometry/materials and make the minimum export changes.
- [ ] Fable/Claude candidate: focus on armature consolidation, outfit retargeting, weighting, and export.
- [ ] Kimi candidate: focus on faithful material/texture handling, hair appearance, and GLB compatibility.
- [ ] NonoSub integration audit: compare every candidate in identical app camera, lighting, and lesson conditions.

Every candidate must provide:

- [ ] Its own `.blend` derived from a copy of the checkpoint.
- [ ] A candidate GLB.
- [ ] Front, three-quarter, side, and back renders.
- [ ] A report covering armatures, objects, materials, animations, and known issues.
- [ ] No source deletion or destructive replacement.

Selection rules:

- [ ] Nico approves the visual result before integration.
- [ ] Mix-and-match work only from candidates whose source changes are understood and reproducible.
- [ ] A candidate that looks wrong is rejected even when it passes a technical audit.

### C6 — Nono production geometry and weights

Status: [ ]

- [ ] Full body and both legs preserved.
- [ ] One canonical armature with the checkpoint rest pose preserved.
- [ ] Duplicate-rig outfit details retargeted without moving them.
- [ ] Shirt, blazer, skirt, accessories, hair, plugs, and tails weighted correctly.
- [ ] Both 12-bone cable-tail chains remain skinned and operational.
- [ ] Hair bones remain available to authored or restrained procedural motion.
- [ ] No unweighted vertices or more than four exported influences.
- [ ] No exposed body holes, tail-plug tearing, or obvious clipping in lesson poses.
- [ ] Candidate stays within the current desktop performance budget.

Acceptance poses:

- [ ] Rest/T-pose inspection.
- [ ] Arms down.
- [ ] Arms forward.
- [ ] Shoulder lift.
- [ ] Elbow bend.
- [ ] Torso lean.
- [ ] Head tilt.
- [ ] Final Present pose.

### C7 — Nono materials and anime hair shine

Status: [ ]

Material strategy:

- [ ] Preserve or bake Nico-approved Blender colors, face textures, eye textures, and clothing detail.
- [ ] Let textures define Nono's identity.
- [ ] Let Three.js provide restrained toon bands, rim lighting, eye readability, and scene lighting.
- [ ] Map final material roles explicitly instead of relying on fuzzy production guesses.
- [ ] Preserve exported appearance for unknown roles.
- [ ] Compare portable materials, NonoToon, and the NonToon-inspired experiment under identical conditions.
- [ ] Keep NonoToon as the safe release fallback.

Hair shine:

- [ ] Remove the generated specks/strokes from the rejected production attempt.
- [ ] Create one connected, intentional anime highlight shape following the front hair mass.
- [ ] Prefer a painted hair-texture mask or one carefully fitted overlay.
- [ ] Verify the highlight at normal and minimum lesson sizes.

### C8 — Nono animation integration

Status: [ ]

Required clips:

- [ ] `Idle`: breathing, blinking, and small sway.
- [ ] `Think`: thoughtful expression and head tilt.
- [ ] `Present`: short cheerful teaching gesture.

Rules:

- [ ] Bake body/facial motion at 30 fps.
- [ ] Remove unintended root translation and scale curves.
- [ ] Do not key procedural tail bones, selected dynamic-hair roots, or skirt-secondary bones.
- [ ] Store named actions so glTF includes them.
- [ ] Crossfade without leaving Nono in a rest/T-pose.
- [ ] If all three cannot be authored, prioritize Idle and a short Present; use a restrained procedural Think fallback.

Acceptance:

- [ ] No clothing collapse, hair-face intersection, skirt/thigh intersection, or plug tearing across every clip.
- [ ] Missing animation degrades gracefully rather than showing a permanent T-pose.

### C9 — NonToon investigation

Status: **STRETCH**

- [ ] Inspect NonToon in Unity if Unity is installed for other work.
- [ ] Compare its ramp, rim, hair-specular, and basic-specular concepts with NonoToon.
- [ ] Do not assume Unity/HLSL materials transfer directly to Three.js/glTF.
- [ ] Cap the experiment tightly.
- [ ] Ship NonoToon if the port is unstable, slower, visually worse, or unfinished.

Promotion requires:

- [ ] No WebGL warnings.
- [ ] Correct skinned rendering.
- [ ] Stable 60 fps on the development Mac.
- [ ] No transparency or animation artifacts.
- [ ] Nico prefers it in a controlled visual comparison.

## Creation freeze checklist

Complete before ending July 19:

- [ ] C1 invisible product shell accepted.
- [ ] C2 menu-bar/drop workflow accepted.
- [ ] C3 Nono voice accepted or a documented written-only fallback approved.
- [ ] C4 tail timing accepted.
- [ ] An approved Nono candidate replaces the shipped GLB only after passing C5–C8.
- [ ] File and live vertical spines still work after integration.
- [ ] No open architecture changes remain.
- [ ] GitHub checkpoint pushed.

## Testing and bug-fixing day — July 20

### Automated verification

- [ ] `pnpm check`.
- [ ] `pnpm test`.
- [ ] `pnpm build`.
- [ ] `pnpm check:rust`.
- [ ] Tauri release build.
- [ ] Final GLB audit.
- [ ] Secret and log-content scan.
- [ ] Clean-clone source build.

### File-mode acceptance

- [ ] Japanese to English with the gathered Japanese fixture.
- [ ] English to Japanese with the gathered English fixture.
- [ ] Original-only mode skips translation latency.
- [ ] Bilingual mode starts, pauses, and catches up correctly.
- [ ] Manual subtitle timing earlier/later/reset works and resets for a new file.
- [ ] Speaker identities remain stable.
- [ ] Current and historical lines open the correct lesson.
- [ ] Lesson pause/resume ownership is correct.
- [ ] Unsupported codecs and playback-proxy failures are recoverable.

### Live-mode acceptance

- [ ] Japanese to English.
- [ ] English to Japanese.
- [ ] Coordinated mode.
- [ ] Fast Source mode.
- [ ] Source and target clauses pair correctly.
- [ ] Reconnect does not merge unrelated epochs or rewind captions.
- [ ] Permission denial opens useful guidance.
- [ ] Stop/restart drains and closes cleanly.
- [ ] NonoSub does not capture its own audio.
- [ ] Clicking a live caption does not stop capture or lose later transcript lines.

### Visual acceptance

- [ ] All six subtitle presets over light and dark footage.
- [ ] Long text at narrow and standard overlay widths.
- [ ] Source, translation, and bilingual display modes.
- [ ] Multi-display placement and monitor removal.
- [ ] Floating lesson over Safari, Firefox, and QuickTime.
- [ ] Minimum lesson size.
- [ ] Nono targets around all board corners and long text.
- [ ] Simultaneous point and underline.
- [ ] Reduced motion.
- [ ] Missing model and 3D failure fallback.
- [ ] Voice enabled, muted, interrupted, and failed.

### Privacy, lifecycle, and failure acceptance

- [ ] Invalid API key and inaccessible model.
- [ ] Network loss and rate limit.
- [ ] Malformed transcription or structured lesson output.
- [ ] Cancellation and session replacement.
- [ ] Keychain status does not block app launch unnecessarily.
- [ ] No API key, transcript, prompt, captured audio, or request body in logs.
- [ ] Temporary playback/audio files are deleted.
- [ ] App restart restores preferences but no private transcript or lesson history.
- [ ] Closing windows does not quit active captions unexpectedly.
- [ ] Explicit Quit stops capture and cleans the session.

### Release dry run

- [ ] Build the unsigned Apple Silicon `.dmg` and `.app.zip`.
- [ ] Test both artifacts from a clean location.
- [ ] Verify Gatekeeper instructions.
- [ ] Record hashes and version information.
- [ ] Confirm repository and demo links from a signed-out browser.

## Write-up, packaging, and submission day — July 21

### Repository and documentation

- [ ] Final README.
- [ ] Installation and clean source-build instructions.
- [ ] Supported media and macOS requirements.
- [ ] Privacy disclosure and retention wording.
- [ ] Model usage and data-flow explanation.
- [ ] Architecture diagram.
- [ ] Known limitations and excluded scope.
- [ ] Codex collaboration explanation.
- [ ] Build Week provenance/log.
- [ ] Judge instructions.
- [ ] Asset-rights statement.
- [ ] Third-party notices.
- [ ] Confirm no NonoSub project license exists.

### Demo package

- [ ] Final Japanese indirect-refusal demo clip.
- [ ] Optional English-to-Japanese proof clip.
- [ ] Subtitle-style screenshots.
- [ ] Architecture graphic.
- [ ] Public YouTube demo under three minutes.

Demo sequence:

1. Introduce the indirect-refusal problem.
2. Open the clip.
3. Show synchronized bilingual subtitles.
4. Switch one subtitle style.
5. Click the refusal line.
6. Show Nono speaking and teaching with the chalkboard and tails.
7. Ask one follow-up.
8. Briefly prove Live Captions.
9. Close with the Education pitch.

### Release artifacts

- [ ] Final unsigned Apple Silicon `.dmg`.
- [ ] Final `.app.zip`.
- [ ] Gatekeeper instructions.
- [ ] Exact source-build instructions.
- [ ] Artifact hashes and version recorded.
- [ ] Downloaded artifacts retested.

### Devpost submission

- [ ] Education category.
- [ ] Final project description and pitch.
- [ ] Public repository.
- [ ] Public demo video.
- [ ] GPT-5.6, transcription, and realtime model explanation.
- [ ] Codex-building explanation.
- [ ] `/feedback` session ID.
- [ ] Optional credits acknowledgment where required.
- [ ] Final links and uploads verified.
- [ ] Submission confirmation captured.

## Deferred concept — Sequential source/translation presentation

After the audit repairs, explore an optional live-caption mode that presents listening and meaning as two deliberate beats instead of always showing both rows together:

1. Accumulate provisional source speech off-screen until a complete source clause is ready.
2. Show that complete source clause and keep it visible while its translation is generated.
3. Continue transcribing the next source clause into a hidden ordered queue; new provisional speech must not replace the visible clause.
4. When the visible clause's translation is finalized, clear the source row and replace it with the translation only.
5. Hold the translation for a readable interval, approximately 2.5–4 seconds and adaptive to text length, then fade it out.
6. Promote the next completed queued clause without skipping, merging, or reordering speech.
7. If the queue grows faster than it can be displayed, shorten dwell time or fall back to coordinated bilingual presentation rather than allowing latency to grow without bound.

This could become a third mode alongside **Coordinated** and **Fast Source**, tentatively named **Listen → Meaning** or **Sequential Learning**. It is not part of R1 or R2 and must not be implemented until the release audit repairs are complete.

Multiple simultaneous stream speakers require separate speaker-aware lanes and ordered queues. The current realtime path intentionally exposes one `Live Audio` identity because it does not provide reliable diarization. Stable live speaker identity, stacked speaker lanes, and overlapping-speaker separation remain post-hackathon research rather than submission scope.

## Hard scope cut

Do not add before submission:

- Delayed external video or YouTube playback.
- Embedded browser.
- Twitch rewind.
- Full NonToon shader port.
- Accounts or cloud sync.
- Vocabulary decks.
- Transcript persistence or search.
- Mobile support.
- Overlapping live-speaker separation.
- Native Windows or Linux release verification.
- New subtitle preset families.
- Any architecture that jeopardizes the working file demo.

## Deliverable completion protocol

For every creation deliverable:

1. Select exactly one deliverable as **ACTIVE**.
2. Create or update its focused implementation plan under `docs/plans/`.
3. Confirm dependencies, non-goals, and acceptance checks before implementation.
4. Implement without broadening this sprint.
5. Run automated checks proportional to risk.
6. Complete visible acceptance with Nico when the result is presentation-sensitive.
7. Update this document with status, verification, decisions, and known risks.
8. Add a Build Week log entry.
9. Create and push a GitHub checkpoint.
10. Begin the next deliverable only after the previous one is accepted or explicitly deferred.

## Current next actions

1. Checkpoint the current invisible-shell implementation before repairs.
2. Repair the live clause lifecycle and add an independent overlay safety envelope.
3. Isolate session producers and repair file boundary/translation failure behavior.
4. Harden realtime startup, retranslation, multi-window state, lessons, and playback ownership.
5. Bound long-session resource use and preserve media timing across failures.
6. Complete the remaining audit cleanup and full native acceptance matrix.

## Audit repair checkpoints

Status: **R13 IMPLEMENTATION COMPLETE — final manual native matrix remains**

The July 18 independent review found release-blocking correctness and containment issues. Repairs are performed one at a time; each receives focused tests, the full verification suite, visible acceptance when applicable, and its own GitHub checkpoint before the next repair begins.

- [x] R0 — Preserve and push the current invisible-shell baseline.
- [x] R1 — Rebuild live source/translation clause lifecycle so finalized captions cannot reopen. Accepted and checkpointed in `dc37d09`.
- [x] R2 — Bound every live subtitle style independently of backend text size. Accepted and checkpointed in `7e18141`.
- [x] R3 — Add session generations, per-run cancellation, and stale-event rejection. Checkpointed in `780cb8b`.
- [x] R4 — Preserve stable IDs during file chunk-boundary reconciliation. Checkpointed in `3225d88`. Focused plan: `docs/plans/FILE_BOUNDARY_ID_STABILITY.md`.
- [x] R5 — Validate exact structured translation output and continue source-only after terminal failure. Focused plan: `docs/plans/TRANSLATION_FAILURE_CONTAINMENT.md`.
- [x] R6 — Harden realtime configuration acknowledgement, timing, source hints, and reconnects. Focused plan: `docs/plans/REALTIME_SESSION_HARDENING.md`.
- [x] R7 — Make file target-language retranslation atomic and generation-scoped. Focused plan: `docs/plans/ATOMIC_FILE_RETRANSLATION.md`.
- [x] R8 — Repair multi-window snapshot ordering and preference patching. Focused plan: `docs/plans/MULTI_WINDOW_STATE_CONVERGENCE.md`.
- [x] R9 — Pin lesson identity and correct cache/placement behavior. Focused plan: `docs/plans/PINNED_LESSON_IDENTITY.md`.
- [x] R10 — Introduce explicit file and external-media playback ownership.
- [x] R11 — Bound long-session coordinator, event, error, and transcript-rendering costs.
- [x] R12 — Preserve file/live media timing across decode and send failures.
- [x] R13 — Complete audit cleanup and bundle/source verification. Manual permission, paid-network, multi-display, and Gatekeeper acceptance remain explicitly tracked.

Approved repair defaults:

- Watching overlays show a bounded latest clause while complete text remains available to transcript history and Nono lessons.
- Failed file translations become source-only display-ready lines with an explicit retry path instead of freezing playback.
- Experimental external-media pause never sends an automatic close toggle; resuming is an explicit user action.
- No Nono model, shader, tail-animation, licensing, or new product-surface work is mixed into repair checkpoints.

R0 verification: `pnpm verify` passed with zero Svelte errors or warnings, 69 frontend tests, a successful production build, 48 Rust tests, and warning-free clippy.

R7 verification: `pnpm verify` passed with zero Svelte errors or warnings, 81 frontend tests, a successful production build, 85 Rust tests, and warning-free clippy.

R8 verification: `pnpm verify` passed with zero Svelte errors or warnings, 89 frontend tests, a successful production build, 88 Rust tests, and warning-free clippy.

R9 verification: `pnpm verify` passed with zero Svelte errors or warnings, 92 frontend tests, a successful production build, 92 Rust tests, and warning-free clippy.

R10 verification: `pnpm verify` passed with zero Svelte errors or warnings, 123 frontend tests, a successful production build, 95 Rust tests, and warning-free clippy. File playback now uses a session/media/selection/revision-bound pause lease; subtitle lesson activation is right-click-only; stale or invalidated closes cannot resume playback; experimental external pause never posts a blind close toggle and instead offers an explicit resume action.

R11 verification: `pnpm verify` passed with zero Svelte errors or warnings, 128 frontend tests, a successful production build, 101 Rust tests, and warning-free clippy. Local audio conversion is incremental and rejects files over four hours; transcript rendering pages the newest 200 lines; segment upserts avoid full-array sorting; errors, lesson cache/thread input, realtime event identities, and live pairing history are bounded; completed original-only drafts are released; and file retranslation clones canonical state only after completion.

R12 verification: `pnpm verify` passed with zero Svelte errors or warnings, 128 frontend tests, a successful production build, 107 Rust tests, and warning-free clippy. HTTP endpoints now use bounded connect/read/request timeouts; transcription requires a terminal done event and rejects explicit SSE errors; retries back off with jitter and observe cancellation; live starts are lease-serialized; WebSocket sends and graceful shutdown are bounded; Ping/Pong is accepted throughout; capture and transmission clocks are separate; silent suppression and send failures retain source-time gaps; decode errors insert packet-duration silence; and odd 48 kHz samples survive converter call boundaries.

R13 verification: `pnpm verify` passed with zero Svelte errors or warnings, 135 frontend tests, a successful production build, 110 Rust tests, and warning-free clippy. Tauri surfaces start empty instead of rendering fixtures; subscription failures retry with teardown-safe cleanup; snapshot recovery is monotonic; generation events also require the active session identity; preference/speaker inputs are sanitized at both boundaries; startup remains Keychain-free; the unused opener capability was removed; stale cleanup is restricted to NonoSub-owned temp prefixes; explicit Quit releases active media resources; and late GLTF completion plus Three.js resources are safely disposed. All five production surface routes returned HTTP 200. The arm64 app and DMG built, were ad-hoc signed, and passed strict bundle verification. Paid API, ScreenCaptureKit permission/revocation, multi-display removal, and second-Mac Gatekeeper checks remain manual acceptance rather than being inferred from unit tests.

## Checkpoints and decisions

| Date | Commit or artifact | Decision |
|---|---|---|
| July 17 | `f898ef7` | Chalk lessons, tail runtime, shader experiments, tests, and production scripts checkpointed. Broken candidate GLB and Blender production files excluded. |
| July 18 | This sprint document | Use one durable master checklist and a focused implementation plan per deliverable. |
| July 18 | Independent Codex, Claude/Fable, and Kimi review | Repair the confirmed live-caption lifecycle, containment, session-isolation, file-pipeline, window-state, lesson, playback, resource, and timing defects as R0–R13. |

## Shared-system impact

NonoSub is a standalone repository. This sprint requires no changes to the Nono VTuber monorepo, shared orchestrator, live services, or VMs.
