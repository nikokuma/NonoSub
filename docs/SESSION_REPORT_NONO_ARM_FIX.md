# Session Report — Nono Model Rebuild & Arm-Contortion Root-Cause Fix

**Date:** 2026-07-21 · **Branch:** `codex/final-reliability-repair` (main checkout, no worktree) · **Primary agent:** Claude Fable 5 (Claude Code)
**Companion diff:** [`docs/nono-arm-fix-session.diff`](nono-arm-fix-session.diff) — every working-tree change from this session.

---

## 1. Starting point

Nico reported that the in-app Nono GLB (`static/assets/NonoCandidate.glb`, built the previous day from the mocap sprint) was badly broken despite the source file `~/Projects/Blendr/NonoSubCheckpointFinal.blend` looking correct in Blender:

- Left arm (viewer's right) grotesquely contorted in most gesture clips; both arms in Cheer/Surprised
- Legs missing, shoe laces gone, shoe tongue floating over the shoe
- Pink eyebrows / lashes / hairclips, flat unsmoothed shading, unwanted hair shine

## 2. Phase 1 — Model rebuild (visual defects)

**Root cause:** the GLB had been exported through `NonoSubProduction2.blend`, built by the legacy `scripts/prepare_nono_production.py`, which (a) deleted every body face below z=0.76 that wasn't hand-weighted (missing legs), (b) discarded all original materials for flat palette stand-ins whose fallback color is pink (pink brows/lashes/clips, flat look, alpha-BLEND shoes causing the tongue/laces sorting bugs), and (c) fabricated the hair-shine strokes.

**Fix:** new fidelity-preserving `scripts/prepare_nono_release.py` that builds `~/Projects/Blendr/NonoSubRelease.blend` directly from the checkpoint (never saving the checkpoint): keeps original geometry/modifiers/materials, normalizes skin weights to the 4-influence GPU budget (deform bones only), forces opaque alpha where blending isn't needed, applies the audit's semantic object/material names, and **bakes each toon node-chain's flat color into the Principled Base Color** — the checkpoint's ShaderToRGB toon ramps are invisible to the glTF exporter, which was exporting those materials as white (white blazer/skirt in the first rebuild attempt).

**Result:** audit-passing GLB (103,072 tris, 55 draws, 10.4 MB) with correct legs, laces, shoe layering, colors, dark lashes, hairclips, smooth shading, no fake shine. Verified in-app.

## 3. Phase 2 — Stale-source elimination

Animations still looked wrong. Verified Nico's live Blender session had unsaved changes (`bpy.data.is_dirty == True`); backed up the on-disk checkpoint (`NonoSubCheckpointFinal.backup-0721.blend`), saved the session, and switched the pipeline to take clips from the checkpoint itself (with automatic hip mean-centering in the copy) instead of the deprecated Production2 file. Proved GLB ↔ release-blend ↔ checkpoint equivalence by reimporting the GLB into Blender and rendering identical frames. Arms were **still** contorted → the defect had to be inside the clip data itself.

## 4. Phase 3 — Independent multi-agent investigation (Nico's request)

Nico asked for three independent investigators, **read-only, no changes**, to avoid primary-agent bias. All three received the same evidence brief (`arm_investigation_brief.md`, 7 candidate hypotheses) and worked concurrently:

| Agent | How invoked | Verdict |
|---|---|---|
| **Fable subagent** | Claude Code `Agent` tool, model `claude-fable-5`, general-purpose | H1 confirmed with decisive numbers (below); H2–H7 refuted with measurements |
| **Codex — model `gpt-5.6-sol`** | Codex CLI via the `openai-codex` Claude Code plugin (codex-rescue forwarder) | Same mechanism found independently: reproduced the baked bad quaternion to within 0.016° from the long-way delta × 1.6 gain |
| **Kimi (kimi-code CLI v0.27.0, default model)** | `kimi --add-dir … -p "<brief prompt>"`, resumed with `kimi -S <session>` | H1 confirmed (GLB matches blend to <0.5°/frame; elbow bends up to 176.5° = anatomically impossible); plus a distinct second defect: one-frame ~180° euler-wrap wrist/finger spins |

### Codex usage tracking (for hackathon submission)

- **Chat/thread ID:** `019f84ee-068b-78b2-89cc-9b8d571d3976`
- **Chat name:** `Codex Companion Task: <task> Investigate a 3D animation export bug in the r...`
- **Model:** `gpt-5.6-sol` · **Sandbox:** read-only · **Background job id (plugin runtime):** `bidjt8ffi`
- **Full transcript:** `~/.codex/sessions/2026/07/21/rollout-2026-07-21T08-46-59-019f84ee-068b-78b2-89cc-9b8d571d3976.jsonl` (resume in the CLI with `codex resume 019f84ee-068b-78b2-89cc-9b8d571d3976`)
- **Scope note:** Codex performed **investigation only — it made no code changes** (read-only sandbox; it could not even launch Blender). All code in the companion diff was written by Claude Fable 5. Earlier commits on this branch (`codex/final-reliability-repair`, e.g. `d6419b2`, `05bd107`, `f88277b`) predate this session and reflect prior Codex-assisted work already in git history.

### Kimi usage tracking

- **Session ID:** `session_04a02ab1-f3be-4876-ac19-1c278c1d0909` (resumable with `kimi -S session_04a02ab1-f3be-4876-ac19-1c278c1d0909`)
- **Logs:** scratchpad `kimi_stream.jsonl` (tool-call transcript), `kimi_stderr.txt`, `kimi_final_err.txt` (consolidated analysis)
- Investigation only; no code changes.

## 5. The root cause (consensus, with numbers)

The 9 clips were cut from the raw 13-minute take with per-bone amplification `amplified = neutral · (neutral⁻¹ · raw)^gain`. The raw take stores `upper_arm.L` X euler as **−347°…−360°** (a wrapped representation of ~0–13°). Euler→quaternion conversion of a wrapped euler yields the **negative-hemisphere quaternion**, so the neutral-relative delta was represented as **317.184° instead of the equivalent 42.816°** — and the 1.6 gain amplified the long-way rotation. Measured effective gain on `upper_arm.L`: **3.3–7.5×**; every gesture clip drove it to ~179° from neutral (raw max ≤44°). `upper_arm.R` sat in the positive hemisphere and amplified correctly — hence one broken arm (both in Cheer, where the right side wrapped too). The original cutting code ran interactively in a Blender MCP session and was never persisted, which is why the bug left no trace in the repo. Export, strip, skinning, and the three.js runtime were all formally exonerated (GLB tracks match the blend to <0.5°/frame).

## 6. Phase 4 — The fix (implemented by Claude Fable 5)

`scripts/cut_nono_clips.py` is now a real, persisted, headless cutting script (previously a design-doc stub):

1. **Hemisphere canonicalization** — `if delta.w < 0: delta.negate()` before exponentiation (the one-line core fix)
2. **Gain taper** — full gain below 60° of raw rotation, fading to 1.0× at 130°, so full-range performances (Cheer's 138° elbow) play as performed instead of folding the limb flat
3. **All rotations routed through quaternion space** with frame-continuity euler decomposition (kills the raw take's alternating euler-representation flips)
4. **Quaternion-space despike** of finger/hand channels (repairs genuine Rokoko solve glitches, e.g. a one-frame 180° whole-right-hand flip at Surprised f12–13)
5. **Acceptance gate** — the cut aborts if any amplified arm bone exceeds 165° from neutral

Chain: `cut_nono_clips.py` (reads checkpoint, never saves it) → `~/Projects/Blendr/NonoClipLibrary.blend` → `prepare_nono_release.py --actions-from` that library → `export_nono_final.py` → `strip_nono_glb.mjs` → `audit_nono_glb.mjs` → `static/assets/NonoCandidate.glb`.

**Verification:** max arm deltas now 51–142° (vs 179° before, matching the raw performance); worst adjacent-key rotation step in the final GLB dropped from 179.7° to 52.7° (a plausible thumb-tip curl); Workbench renders of all 9 clips show natural poses; audit passed; confirmed live in the app with zero console errors.

## 7. Files changed this session

| File | Change |
|---|---|
| `scripts/cut_nono_clips.py` | **New** (replaced stub): headless clip cutter with the hemisphere fix, taper, despike, acceptance gate |
| `scripts/prepare_nono_release.py` | **New**: fidelity-preserving release-file builder (toon color bake, semantic renames, weight normalization, hip centering, action import) |
| `scripts/export_nono_final.py` | Modified (this branch): optional-gesture validation, heart_touch rename, hip-amplitude rules |
| `scripts/audit_nono_glb.mjs` | Modified (this branch): heart_touch rename, optional-clip handling |
| `src/lib/nonoToon.ts`, `src/lib/NonoScene.svelte` | Modified (this branch): mood union + dev overrides, one-shot/fallback clip logic |
| `static/assets/NonoCandidate.glb` | Rebuilt binary (10.4 MB) |
| `.claude/launch.json` | **New**: dev-server launch config |
| `docs/` (this report, the diff, `NONO_CANDIDATE_PROMOTION.md`) | **New** |

Out-of-repo artifacts: `~/Projects/Blendr/NonoSubRelease.blend` (export intermediate), `NonoClipLibrary.blend` (fixed clips), `NonoSubFinal2.glb` (pre-install GLB), `NonoSubCheckpointFinal.backup-0721.blend` (safety backup). `NonoSubCheckpointFinal.blend` was saved once at Nico's request and never modified by scripts. `NonoSubProduction2.blend` is deprecated.

## 8. Open items

- Nico's per-clip review of all 9 gestures, then promotion to the release slot (see `docs/NONO_CANDIDATE_PROMOTION.md`)
- Kimi side-flag (unverified): residual tail `spine.0xx` channels may remain in the shipped GLB and semantic tail node naming may differ from the app's tail-rig expectations — recount only if tails misbehave
- Scripted blink/facial layer (bone-based; no shape keys) — planned next
- Minor thumb-tip jitter (≤53°/frame) left as-is; invisible at app scale
