# Submission checklist

Deadline: **Tuesday, July 21, 2026 at 5:00 PM PT**. Target implementation finish: July 20. July 21 is upload/recovery only.

## Product proof

- [x] Original 24-second synthetic Japanese indirect-refusal clip generated and disclosed as AI speech.
- [ ] First translated coverage within 20 seconds on the development Mac/network.
- [ ] Subtitle authored-marker timing within ±500 ms.
- [ ] Two speakers stable across three or more chunks in the ten-minute fixture.
- [x] Indirect refusal is translated naturally, not only word-for-word.
- [ ] Historical-line selection pauses playback and answers at all three levels.
- [ ] All six presets checked on light and dark frames.
- [ ] Invalid key, inaccessible model, network loss, rate limit, codec error, malformed output, cancellation, restart, and missing GLB checked.
- [ ] ScreenCaptureKit picker, permission denial, self-audio exclusion, one reconnect, and graceful live stop checked.
- [x] Japanese→English and English→Japanese pass in file mode.
- [ ] English→Japanese live mode passes; Japanese→English live mode is proven.
- [x] Workbench 960×680, lesson 780×620, and overlay 900×220 fit checks pass in fixture QA.
- [x] Lesson history stays scrollable and preserves manual scroll position when a new answer arrives.

## Release

- [x] Direct dependency/license notice generated and reviewed; locked transitive metadata remains reproducible.
- [ ] No secret or transcript appears in source, build output, or logs.
- [x] Exact pushed branch passed a frozen install, frontend checks/tests/build, 25 Rust tests, and warning-free Clippy from a fresh clone.
- [x] Unsigned Apple Silicon `.dmg` built.
- [x] Unsigned Apple Silicon `.app.zip` built.
- [ ] Gatekeeper instructions verified on a second Mac/user account.
- [x] Public GitHub repository and draft checkpoint PR created.

## Devpost

- [ ] Working project URL.
- [ ] Public sub-three-minute YouTube demo.
- [ ] Testable public source repository.
- [ ] Codex and GPT‑5.6 usage documented.
- [ ] Education category and pitch confirmed.
- [ ] Primary Codex task `/feedback` session ID included.
- [ ] Submit before 5:00 PM PT; retain confirmation screenshot.
