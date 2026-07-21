# Submission checklist

Deadline: **Tuesday, July 21, 2026 at 5:00 PM PT**. Target implementation finish: July 20. July 21 is upload/recovery only.

## Product proof

- [x] Original 24-second synthetic Japanese indirect-refusal clip generated and disclosed as AI speech.
- [ ] First translated coverage within 20 seconds on the development Mac/network.
- [ ] Subtitle authored-marker timing within ±500 ms.
- [x] Two speakers stable across three or more chunks in the ten-minute fixture.
- [x] Indirect refusal is translated naturally, not only word-for-word.
- [ ] Historical-line selection pauses playback and answers at all three levels.
- [ ] All six presets checked on light and dark frames.
- [ ] Invalid key, inaccessible model, network loss, rate limit, codec error, malformed output, cancellation, restart, and missing GLB checked.
- [ ] ScreenCaptureKit source chooser, permission denial, self-audio exclusion, one reconnect, and graceful live stop checked.
- [x] Japanese→English and English→Japanese pass in file mode.
- [x] Japanese→English and English→Japanese live modes pass on the development Mac.
- [x] Workbench 960×680, lesson 780×620, and overlay 900×240 fit checks pass in fixture QA.
- [x] Lesson history stays scrollable and preserves manual scroll position when a new answer arrives.

## Release

- [ ] Final release checked on Apple Silicon macOS 14+ with the default decodable AAC track.
- [x] Adaptive Coordinated live delay and intentional last-caption retention documented in the demo/judge notes.
- [x] Deferred limitations documented: automatic track selection, multichannel averaging, linear file resampling, estimated long-turn split timing, and HEVC proxy alignment.
- [x] Rust 1.97.1 pinned locally and in CI; Rust 1.97 Clippy regressions repaired without suppressions.

- [x] Direct dependency/license notice generated and reviewed; locked transitive metadata remains reproducible.
- [x] Secret and sensitive-log source scan passed; the only `sk-` match is the explicit fake unit-test token `sk-test-generation-isolation`.
- [x] Exact pushed R13 commit `e011e37` passed a fresh-clone `pnpm install --frozen-lockfile` and complete `pnpm verify`.
- [x] Unsigned Apple Silicon `.dmg` built.
- [x] Unsigned Apple Silicon `.app.zip` built.
- [x] Arm64 executable verified; app and DMG copies pass strict ad-hoc signature verification.
- [ ] Gatekeeper instructions verified on a second Mac/user account.
- [x] Public GitHub repository and draft checkpoint PR created.

## R13 artifacts

- Version: `0.1.0`
- DMG SHA-256: `9c91f905293df92f5629cc22da4e4cde1f8187dca321eb0f931444b326c2ff5b`
- App ZIP SHA-256: `a397e0817fc4c467f6dd17cf5e930bfa57f59d8d9d3b4d5bec0da400c149dafc`
- Automated: 135 frontend tests, 110 Rust tests, Svelte check, production build, warning-free Clippy, exact-commit fresh-clone verification, five-surface route smoke test, arm64 bundle/signature verification.
- Still manual: paid model calls and injected network/rate-limit faults; ScreenCaptureKit denial/revocation/reconnect; physical monitor removal; missing-WebGL device behavior; downloaded-copy Gatekeeper on another Mac/user.

## Final reliability artifacts

- Version: `0.1.0`
- Submission DMG SHA-256: `ec65f62f928a5c890552cac05f51a8914331cc3c8b8562eafa6d4a3c08c3cbab`
- Submission App ZIP SHA-256: `e2390c01d46e27b810ef600c1a1fb1b6f7c390c7999ee15938e093f4d0a06532`
- Automated: 155 frontend tests, 120 Rust tests, zero Svelte diagnostics, production build, Rust 1.97.1 warning-free Clippy, Apple Silicon Tauri bundle, arm64 executable check, and strict ad-hoc signature verification for the app inside the submission DMG.
- Dependency note: Rust 1.97.1 emits a non-fatal duplicate Swift symbol linker diagnostic from the current `apple-cf`/`screencapturekit` bridge combination. Test and release binaries link successfully; it is not suppressed.
- Still manual: complete paid/native acceptance matrix, clean-clone verification of the final pushed SHA, GitHub Actions result, and Gatekeeper launch from another Mac/user account.

## Devpost

- [ ] Working project URL.
- [ ] Public sub-three-minute YouTube demo.
- [ ] Testable public source repository.
- [ ] Codex and GPT‑5.6 usage documented.
- [ ] Education category and pitch confirmed.
- [ ] Primary Codex task `/feedback` session ID included.
- [ ] Submit before 5:00 PM PT; retain confirmation screenshot.
