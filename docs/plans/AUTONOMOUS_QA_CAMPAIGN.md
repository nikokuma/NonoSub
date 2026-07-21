# NonoSub Autonomous QA Campaign

## Purpose

Run a repeatable, evidence-backed acceptance campaign while Nico is away. The campaign evaluates lesson-model quality, subtitle behavior, layout, native macOS behavior, and failure recovery. It does not score Nono's authored animation, shader, body placement, or cable-tail motion.

The tester may operate the development Mac and the already-open Safari stream. It must not edit product code while testing. Findings are reported first; fixes happen in separate, scoped tasks.

## Test environment

- Test the exact commit recorded at the start of the run.
- Use an Apple Silicon macOS 14+ debug or release build.
- Before Nico leaves, run `scripts/save_qa_api_key.sh` and approve the one Keychain read. It copies the existing value directly into a permission-`600` file outside the repository without printing it.
- Launch debug runs through `scripts/run_unattended_qa.sh`. The wrapper provides `OPENAI_API_KEY` only to that process tree. Debug builds read it before Keychain, so the key never enters a webview and a rebuilt unsigned binary does not repeatedly trigger a password prompt.
- Remove `~/Library/Application Support/com.nono.nonosub/qa-openai-api-key` immediately after the unattended campaign.
- Use the NonoSub source chooser to select Safari, a specific Safari window, and a display in separate trials.
- Never record API keys, request bodies, captured PCM, personal transcripts, or lesson prompts in logs.
- Store only synthetic fixture outputs and sanitized measurements in QA artifacts. Do not commit screenshots or transcripts from a third-party livestream.
- Stop and report if macOS requires a password, a new security permission cannot be accepted safely, the API account reports billing trouble, or the selected stream contains private information.

## A. Lesson-model bakeoff

### Candidates

Use the production lesson prompt, strict schema, learner presets, output limits, and `store:false` unchanged. Change only the model and reasoning effort:

| Candidate | Model | Reasoning effort | Intended question |
|---|---|---:|---|
| A | Current production `gpt-5.6-sol` deployment | low | Submission-quality baseline |
| B | `gpt-5.5` | none | Fast/low-cost "instant" candidate |
| C | `gpt-5.5` | low | Balanced 5.5 candidate |
| D | `gpt-5.5` | medium | Higher-deliberation 5.5 candidate |

Probe model access before the paid matrix. An unavailable candidate is recorded as unavailable, not silently replaced.

### Dataset

Use at least 18 authored cases, with exact expected teaching facts and ambiguity notes:

- Japanese indirect refusal: `今日はちょっと……`.
- Japanese particle breakdown: `何ですか？`.
- Japanese honorifics, omitted subjects, counters, casual contraction, sentence-final particles, and a genuinely ambiguous reference.
- English idiom, sarcasm, phrasal verb, politeness softener, and culture-dependent expression translated/explained in Japanese.
- Spanish, French, Korean, Mandarin, and Arabic examples covering gender/register, pronoun omission, particles, honorifics, or right-to-left layout as appropriate.
- One incorrect learner premise that Nono must correct gently.
- One line whose literal reading is valid but pragmatically misleading.
- One line that cannot be resolved from context; certainty must not be invented.
- Beginner and Advanced requests for the same source line.

Every case defines:

- Exact source quotation.
- Required facts.
- Forbidden claims.
- Acceptable ambiguity.
- Expected learner level.
- Expected board demonstration type where one is genuinely useful.
- Maximum board density.

### Execution

- Run each case twice per candidate with candidate identity hidden from the reviewer.
- Preserve raw structured outputs only in a local, ignored QA directory containing synthetic fixture text.
- Validate schema before scoring presentation.
- Record first-token/complete latency, input/output tokens, retry count, schema validity, and estimated API cost.
- Randomize candidate order per case to reduce order bias.

### Scoring

Score each response from 0–4 on:

1. Linguistic accuracy.
2. Pragmatic and cultural accuracy.
3. Literal-versus-natural distinction.
4. Calibration of ambiguity and uncertainty.
5. Learner-level fit.
6. Concision and teaching sequence.
7. Meaningful board structure and demonstration choice.
8. Meaningful chalk color/mark/tail cue choice.
9. Exact preservation of quoted source text.

Hard failures:

- Invented certainty or cultural misinformation.
- Changed source quotation.
- Invalid strict JSON after retry.
- More content than the non-scrolling board can display.
- A decorative demonstration that conflicts with the explanation.
- Missing primary teaching cue.

Model selection rule:

- Accuracy and calibration are gating metrics.
- A faster/cheaper candidate may win only if it has no material accuracy regression and its hard-failure rate is no worse than the production baseline.
- Nico makes the final choice from blinded side-by-side examples of the closest candidates.
- Do not change the production model identifier during the test run.

## B. Lesson presentation and interaction

Use deterministic fixture cards first, then successful live model outputs.

- Right-click a finalized file subtitle and a finalized live subtitle.
- Confirm the compact Ask Nono composer opens near the selected line.
- Confirm the user's submitted question disappears during thinking and is never rendered as a chat bubble.
- Confirm the composer expands into Nono plus the complete board without clipping at 1× and 2× display scale.
- Confirm source text, translation, speech bubble, section numbers, chalk marks, progress, Next, Skip, Ask Another, and Close remain inside the usable window.
- Confirm the board never scrolls.
- Confirm Next and Skip advance exactly once.
- Confirm Ask Another erases/replaces the board and retains hidden context without rendering chat history.
- Confirm long Japanese, long English, mixed-script, and right-to-left lessons fit.
- Confirm missing GLB and WebGL failure preserve the full text lesson.
- Confirm closing a file lesson respects playback ownership; closing a live lesson does not stop capture.

Do not score body animation, facial animation, shader quality, Nono placement, tail naturalness, point accuracy, or underline animation in this campaign.

## C. Live subtitle behavior

### Required language trials

- Japanese speech → English subtitles.
- English speech → Japanese subtitles.
- Original-only mode.
- Coordinated mode.
- Fast Source mode.

### Clause integrity invariants

- Append-only deltas never gain invented spaces.
- A finalized source clause is not replaced by a shorter suffix.
- Once a translation for a complete source clause is displayed, an unpaired final 5–10% source tail must not flash by itself afterward.
- Coordinated mode does not reveal source until the matching translation is ready, except for the documented six-second source-only fallback.
- When the next clause is not ready, the previous complete caption remains visible; a separate generic coordinating card must not replace it.
- A late translation updates the same source-only fallback segment rather than creating a duplicate.
- Captions leave at readable timings and do not rewind after reconnect.
- Multiple deltas with the same `elapsed_ms` remain distinct by event ID.
- Missing `elapsed_ms` does not discard text or corrupt later timing.

### Stress speech

- One speaker, slow short sentences.
- One speaker, a 45-second run-on explanation.
- Rapid alternating speakers.
- Two people talking over each other briefly.
- Whisper/quiet phrase followed by loud speech.
- Long pause mid-sentence.
- Music-only gap.
- Punctuation-free speech.
- Code-switching inside one sentence.

For the current single-identity live pipeline, overlap is evaluated for readable segmentation and stability, not diarization accuracy.

### Visual safety

Test Clean, Classic Outline, Yellow Drop, Arcade, Momento, and Wired at minimum/default/maximum font size over light, dark, and busy footage.

- No flash, warp, or whole-card reflow on each incoming delta.
- Source and translation remain inside their style geometry.
- Long text does not cross the top or bottom of the compact overlay.
- Safe-area logic works at bottom, middle, and top placement.
- Overflow is bounded by fitting/wrapping and clause rotation rather than clipping.
- Right-click still finds the intended finalized caption.
- Dragging moves the overlay without selecting text or invoking macOS Look Up.

## D. Capture-source chooser

- After onboarding, Start Live Captions opens the NonoSub chooser rather than Apple's protected picker.
- Applications, Windows, and Displays are visible and searchable.
- NonoSub excludes itself and hides tiny, untitled, off-screen, and non-content windows.
- Selecting Safari captures Safari audio while excluding NonoSub's own audio.
- Selecting one Safari window starts successfully if that window still exists.
- Selecting a display starts successfully.
- Closing the selected app/window produces an actionable recoverable error.
- A source that disappears between list and Start reports Refresh/Choose Another rather than hanging.
- Refresh preserves no stale selection.
- Cancel opens no paid realtime connection.
- Duplicate Start actions leave only one capture stream and one realtime connection.
- Permission denial and revoked permission leave file mode usable.

## E. File-mode acceptance

- Japanese MOV/MP4 → English, including both provided Nico fixtures.
- English MOV/MP4 → Japanese.
- Original-only skips translation but subtitles remain clickable for lessons.
- The first short chunk starts useful coverage promptly.
- Short files complete without waiting forever for 15 seconds of coverage.
- 15-second startup and 2/8-second catch-up hysteresis remain correct for longer files.
- Manual subtitle offset earlier/later/reset works and resets for the next file.
- Translation retry does not duplicate or reorder captions.
- Changing target language reuses the transcript and replaces translations atomically.
- Unsupported codec, corrupt packet, cancellation, replacement, and app Quit clean up temporary media.

## F. Fault and endurance matrix

- Invalid API key.
- Unavailable lesson, transcription, and realtime models.
- Network disconnect during file transcription, translation, lesson generation, and live capture.
- 429 with `Retry-After`.
- SSE EOF without a terminal event.
- Malformed structured translation and lesson output.
- WebSocket send stall and unexpected source end.
- Reconnect after silence and reconnect during active speech.
- Duplicate Start Live.
- Cancel during Apple's permission flow, websocket configuration, file decode, and retry delay.
- Four-hour synthetic transcript and 30-minute simulated live event stream.
- Monitor removal and saved placement clamping.
- App restart, explicit Quit, and temporary-directory sweep.

Verify bounded state while running endurance tests:

- 50 recoverable errors.
- 128 lesson cache entries.
- 32 hidden thread messages.
- 2,048 recent realtime event IDs per track.
- 256 live units or 120 seconds of pairing history.
- 200 transcript DOM rows initially, with older rows loaded in 200-line batches.

## Evidence and reporting

For each failure, record:

- Severity: blocker, high, medium, or low.
- Exact build commit and configuration.
- Reproduction steps.
- Expected and actual result.
- Whether it reproduces twice.
- A sanitized screenshot or short screen recording when visual.
- Relevant event IDs/timings with text removed.
- Suspected subsystem, without changing code.

The final report contains:

- Pass/fail table by section.
- Model bakeoff table and blinded examples.
- Top blockers with deterministic repro steps.
- Performance/latency summary.
- Known limitations that are acceptable for submission.
- A separate proposed repair order. Testing and fixing remain distinct checkpoints.

## Exit criteria

- No blocker in the guaranteed local-file demo.
- No high-severity clause-integrity, clipping, privacy, cleanup, or stale-session defect.
- Safari app selection works without relying on Apple's protected content picker.
- Japanese→English and English→Japanese pass in file and live smoke tests.
- Lesson model choice is backed by scored evidence rather than one anecdotal answer.
- All six subtitle styles remain readable under long-caption stress.
- The release verification suite and native artifact checks remain green.
