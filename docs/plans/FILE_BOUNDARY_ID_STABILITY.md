# R4 — Stable file subtitle identities at chunk boundaries

Status: completed and verified.

## Goal

Keep a finalized file subtitle's public ID stable when a later overlapping transcription chunk supplies a more complete version of the same utterance. Reconciliation must update the existing subtitle in place instead of creating a second canonical line and orphaning its translation, selection, or lesson target.

## Identity contract

- The first accepted subtitle at a timeline location owns the canonical public ID for that utterance.
- Chunk-local OpenAI IDs are provenance only. They may seed a new public ID, but they never replace an already-published canonical ID.
- A boundary duplicate requires both substantial timestamp overlap and normalized textual containment, preserving the existing distinction between repeated/overlapping speech with different text.
- Matching is one-to-one within a reconciliation pass. Two incoming fragments cannot inherit the same existing ID.
- When a more complete duplicate replaces source text or timing, it inherits the existing ID and becomes a new content revision of that same subtitle.
- A source-text revision clears any old translation and ambiguity note and returns translation to pending state so GPT translates the revised source.
- An exact-text duplicate retains the existing canonical segment, including its completed translation and timing, instead of causing needless retranscription or retranslation events.
- A shorter or equal duplicate cannot downgrade a more complete canonical line.
- Distinct overlapping utterances remain separate so the existing two-line overlap presentation continues to work.
- Reconciliation output remains deterministically ordered by start time, then end time, then stable ID.

## Pipeline behavior

1. Freeze the number of pre-existing canonical segments at the start of a merge.
2. Process incoming variants from most to least complete so a short fragment cannot consume the identity before its fuller version.
3. Score an incoming segment against unmatched pre-existing segments; never deduplicate two segments when there was no pre-existing boundary identity.
4. Select the strongest compatible match by overlap coverage, temporal distance, speaker compatibility, and deterministic index order.
5. Keep the existing segment unchanged for exact or less-complete duplicates.
6. For a more-complete duplicate, copy its revised content into the existing slot while preserving the existing public ID.
7. Discard a redundant incoming variant if it matches an identity already consumed by a better variant; append genuinely unmatched incoming segments with their newly generated IDs.
8. Emit `TranscriptFinalized` again only when the stable ID's source text actually changes. Existing `emitted_sources` behavior already supplies this revision event.

## Non-goals

- R5 owns exact GPT translation-output validation and terminal source-only fallback.
- R7 owns competing target-language retranslation requests within one active file session.
- R8 owns snapshot/listener ordering across windows.
- R9 owns lesson-cache invalidation and pinning when a selected subtitle's content revision changes.
- This repair does not redesign chunking, timestamps, diarization, subtitle presentation, or the OpenAI transcription request.

## Acceptance

- A more-complete overlap keeps the original ID and updates its text once.
- Canonical state contains one subtitle rather than old/new ID duplicates.
- The selected subtitle ID still resolves after a boundary revision.
- A completed translation is retained for an exact duplicate and invalidated for changed source text.
- Two repeated or simultaneous incoming utterances cannot inherit one ID.
- Distinct overlapping speakers remain separate.
- Long-sentence split parts keep stable identities where one-to-one matches exist.
- Focused Rust and TypeScript regressions and the full `pnpm verify` suite pass.
