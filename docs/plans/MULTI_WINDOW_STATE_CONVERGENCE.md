# Multi-window state convergence

Status: complete for audit repair R8.

## Goal

Workbench, viewer, overlay, lesson, and launcher-adjacent flows must converge on the same canonical session and preferences even when windows open mid-session, events arrive during startup, or two surfaces save unrelated preferences close together.

## Session bootstrap

1. Install the `session-event` listener before requesting the initial Rust snapshot.
2. Queue envelopes received while the snapshot request is in flight.
3. Publish the snapshot, then drain queued envelopes in sequence order.
4. Ignore duplicate or older envelopes from the same session.
5. On a sequence gap or session-ID change, request a fresh snapshot and reconsider the queued event against it.
6. Serialize asynchronous gap recovery so concurrent callbacks cannot apply refreshes out of order.
7. Replace the separate `initialSession` plus `subscribeSession` calls on all four session-rendering surfaces with one listener-first subscription.

## Preference patch broker

1. Add an in-memory Rust preference broker with a monotonically increasing revision.
2. The first window seeds it from the existing validated local preference record; later windows receive that canonical value.
3. Writers submit deep partial patches, not complete preference objects.
4. Rust merges each patch under one lock, increments the revision, and broadcasts a full canonical preference envelope in the same order.
5. A stale base revision does not reject an unrelated patch; it is rebased onto the newest canonical value. Same-field conflicts remain last-writer-wins by Rust receipt order.
6. Every receiving window accepts only newer revisions, validates the resulting preference shape, and persists the canonical result locally.
7. Rust applies session-language side effects inside the same serialized broker operation, and only when the language field itself was patched. Style, position, lesson placement, and other saves cannot restart or cancel retranslation.
8. Overlay position, viewer subtitle position, lesson placement, menu actions, onboarding, and every workbench control emit the narrowest practical leaf patch.
9. File/live launch consumes the already-current preferences passed to it and no longer rewrites the complete stored object.

## Persistence and migration

- Bump browser storage to `nonosub-preferences-v5` and retain v4/v3/v2/legacy fallback reads.
- Preference revisions remain process-local and contain no transcript, lesson, media, or secret data.
- Existing preference schema and visual defaults remain unchanged.

## Non-goals

- R9 owns lesson selection/cache identity and lesson placement semantics beyond safe preference merging.
- R10 owns playback pause/resume ownership.
- No cloud sync, cross-device persistence, transcript persistence, or API-key handling changes.
- No new visual surface or setting.

## Acceptance

- An event emitted during initial snapshot loading is never lost or double-applied.
- Event gaps and session replacements converge to the latest Rust snapshot.
- A delayed old snapshot cannot overwrite newer event state.
- Concurrent style and language patches preserve both changes.
- Concurrent lesson placements on different monitors preserve both entries.
- Stale same-field patches resolve deterministically by broker receipt order.
- All surfaces use the new listener-first/bootstrap APIs.
- Full `pnpm verify` passes.

## Verification

- Listener-before-snapshot bootstrap is covered for an event that arrives during snapshot loading, an already-reflected event, a sequence gap, a replacement session, and concurrent out-of-order recovery.
- Preference tests cover stale independent patches, deterministic same-field ordering, monitor-specific placement merging, legacy preset action diffs, invalid envelopes, and first-seed ownership.
- Every session-rendering surface uses the shared subscription coordinator, and every preference writer emits a narrow patch.
- `pnpm verify` passes with zero Svelte errors or warnings, 89 frontend tests, a successful production build, 88 Rust tests, and warning-free clippy.
