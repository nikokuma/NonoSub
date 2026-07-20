# R13 — State Cleanup and Release Acceptance

## Scope

R13 closes the reliability sprint without changing NonoSub's product design. It hardens production startup, session ordering, preference validation, keychain behavior, temporary-file ownership, and the Nono model loader. It then records the release verification that can be automated on the development Mac and leaves genuinely manual native checks explicit.

## Implementation checkpoints

1. Remove fixture state from Tauri startup and make event subscription/recovery monotonic and teardown-safe.
2. Require generation and session identity for canonical mutations and keep mutation/event ordering centralized.
3. Sanitize preferences and user-controlled speaker/language/style values before persistence or broadcast.
4. Keep startup keychain-free, remove unused opener permissions, and sweep only stale NonoSub-owned temporary directories.
5. After the Fable model checkpoint, make GLTF loading and Three.js disposal safe across unmount/replacement.
6. Run the full source, route, security, packaging, and artifact verification; record manual acceptance items separately.

## Completion evidence

The sprint log and submission checklist must record commands, results, artifact hashes, and unresolved manual/native risks. Passing unit tests cannot be reported as proof of ScreenCaptureKit permissions, paid live model access, Gatekeeper behavior, or multi-display interaction; those remain named manual checks until observed.
