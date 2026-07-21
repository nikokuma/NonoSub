# Session Report — Nono Face Keys: Blink, Smiles, >.< Squint, and the Rest-Position Bug

**Date:** 2026-07-21 · **Branch:** `main` · **Orchestrator:** Claude Fable 5 (Claude Code) · **Implementers:** two Codex `gpt-5.6-sol` agents (high reasoning, write-enabled)

## What shipped

New face layer for the Nono mascot, authored in Blender by Nico this session and wired end-to-end into the app:

- **Shape keys (exported as glTF morph targets):** `Nono_Head` = Blink / SmallSmile / BigSmile (sculpted by Nico); `Nono_BrowsNLashes` = Blink. A driver linked lash-Blink to head-Blink in Blender; the release pipeline strips it (runtime drives both).
- **`Nono_Squint`** — the >.< marks as a separate 16-vert mesh (built procedurally to Nico's reference), skinned to the head bone, hidden by the app at load.
- **`Nono_Mouth` / `Nono_Lips`** materials — mouth-cavity rose + subtle lip tint with gloss (roughness 0.25).
- **Runtime behavior:** auto-blink every 8–12 s (150 ms ramp); SmallSmile on `thumbs_up`; BigSmile on `cheer`; **instant** >.< snap (Blink=1 + lash-hide + squint-show, same frame) on `cheer` and `surprised`, reverting when the one-shot ends; no blinking while squinting; `?nonoExpression=squint` dev override; per-lesson-moment `gesture` cues (fixture demos `point_self` → `point_user` → `heart_touch`, activating the three previously-dead clips).

## Codex usage tracking (for hackathon submission)

| Agent | Chat name | Thread/Chat ID | Scope |
|---|---|---|---|
| **Codex-A** | `Codex Companion Task: Codex-A: shape-key export pipeline Repo: /Users/nico/…` | `019f8618-c3de-78b0-888b-e710bcd94b32` | `scripts/prepare_nono_release.py`, `scripts/export_nono_final.py`, `scripts/strip_nono_glb.mjs`, `scripts/audit_nono_glb.mjs` |
| **Codex-B** | `Codex Companion Task: <task> Repo: /Users/nico/Projects/NonoSub (SvelteKit …` | `019f8619-79ac-7b23-848d-c6d9a84ad3dc` | `src/lib/nonoToon.ts`, `src/lib/NonoScene.svelte`, `src/lib/LessonSurface.svelte`, `src/lib/contracts.ts`, `src/lib/fixtures.ts`, tests |

- Model `gpt-5.6-sol`, reasoning effort **high**, sandbox workspace-write, both invoked via the Codex CLI plugin. Resume with `codex resume <thread-id>`.
- **Codex-A ran two turns**: initial implementation, then a follow-up on the same thread after the orchestrator's Blender verification found the eye-decal materials are opaque-by-design (the initial audit spec was wrong) and supplied node-graph ground truth (color ramp → lit-end bake; texture-behind-mix → rewire).
- Footnote: stray thread `019f8618-de3c-7c81-93c3-3de62993ccbd` ("Codex Companion Task: --help") is a plugin CLI probe, no work in it.
- Orchestrator-authored code (kept out of Codex for speed, disclosed for accuracy): the one-line audit exemption for legitimately-white `Nono_Eye_Shine`, and the `pose_position = "POSE"` guard in `prepare_nono_release.py` (see bug story below).

## The two bugs found and fixed along the way

1. **"Her eyes are missing / shoes are white" (reported in the promoted build):** the color-bake pass in `prepare_nono_release.py` only resolved *unlinked* toon sockets. `EyeShine` (Base Color ← color ramp) and `Material.002`/moon (texture hidden behind a Mix node) silently baked nothing → colorless opaque planes over the irises. Codex-A's fix follows links (RGB nodes, ramps → lit-end color, Mix chains → rewire the hidden texture directly into Base Color). App-side, Codex-B stopped the shoe-accent material from being tinted pink by role inference and made eye materials respect authored colors/alpha.
2. **Every clip exported as a frozen T-pose:** the checkpoint had been saved while the rig was displaying **Rest Position** (left over from the morning's shape-key sculpting session). A rest-position armature ignores all pose animation during export sampling — all 555 channels of every clip baked flat. Diagnosed by bisection (old exporter + new blend still flat; actions evaluate fine when assigned manually), fixed in the live session and permanently guarded in `prepare_nono_release.py`.

## Verification evidence

- `audit_nono_glb.mjs`: **passed** — 103,120 tris, 58 draws, 10.5 MB; morph targets `[Blink, SmallSmile, BigSmile]` on `Nono_Head`, `[Blink]` on `Nono_BrowsNLashes`; `Nono_Squint` present + skinned; 0 morph-weight channels in clips; all 9 clips present.
- Clip motion check: `Think` upper-arm rotation = 135 keys, full range (matches the known-good previous build exactly).
- `pnpm test`: **170/170**; `pnpm check`: 0 errors.
- In-browser (`?nonoAsset=candidate`): idle animates with correct materials (navy shoes, socks, irises/shines, gray+gold tail plugs); `cheer` and `surprised` show the **>.< snap** with lashes hidden and revert cleanly; `thumbs_up` keeps normal eyes; lesson moment 1 fires `point_self`.
- Master file `NonoSubCheckpointFinal.blend` never modified by scripts (mtime verified each run). Backups made this session: `backup-0721-prefacekeys`, `backup-0721-presmilelips`.

## State at hand-off

- `static/assets/NonoCandidate.glb` = verified new build. **`static/assets/Nono.glb` (release slot) not yet overwritten** — promotion is one command away and awaits Nico:
  ```bash
  cp static/assets/NonoCandidate.glb static/assets/Nono.glb && node scripts/audit_nono_glb.mjs static/assets/Nono.glb
  ```
  (Bear-hoodie form already preserved as `static/assets/NonoHoodie.glb`.)
- Rebuild-from-source chain unchanged from `docs/NONO_CANDIDATE_PROMOTION.md`, with `--actions-from ~/Projects/Blendr/NonoClipLibrary.blend`.

---

# Addendum — Anime Toon Shader (same day, follow-up session)

Nico asked for a real anime toon look (referencing lilxyzw's NonToon Unity shader). Port assessment: NonToon's `.scshader` is Unity pipeline scaffolding whose lighting math lives in the external `jp.lilxyzw.shadercore` HLSL library — not portable in the timeframe. Instead the standard three.js anime stack was built natively on the existing `MeshToonMaterial` base:

- **Inverted-hull outlines** — `createNonoOutlines()` in `src/lib/nonoToon.ts`: per outlined mesh, a companion mesh sharing the same geometry, `MeshBasicMaterial` `BackSide`, vertices extruded along normals in `begin_vertex` (before morph/skinning chunks, so bones and blink/smile shape keys deform the hull). SkinnedMesh sources re-bind the shared skeleton; morph sources share `morphTargetInfluences` by reference; outlines are parented to their source mesh so visibility (e.g. lash-hide during >.<) inherits for free. Per-role dark-tint colors + widths live in exported `NONO_OUTLINE_CONFIG`; eye decals, squint geometry, and transparent materials are skipped, while the mouth receives its own thin outline. Dev kill-switch `?nonoOutline=0`.
- **Tinted cel shade** — `gradientmap_pars_fragment` chunk override: shadow band is now `mix(nonoShadeTint, white, step)` (per-role colored shade — rosy skin shadow, blue-violet hair shadow) instead of grayscale darkening. Cache key bumped to `-v3`.
- **Polish** — hair sheen band enabled for the default `toon` variant (0.18), hair/tail rim strength 0.22 → 0.28.

## Codex usage tracking (addendum)

| Agent | Chat name | Thread/Chat ID | Scope |
|---|---|---|---|
| **Codex-C** | `Codex Companion Task: Codex-C: anime toon shader Repo: /Users/nico/Projects/NonoSub…` | `019f8693-b6e1-7632-a0f7-eb60554a3436` | `src/lib/nonoToon.ts`, `src/lib/NonoScene.svelte`, `src/lib/nonoToon.test.ts` |

Model `gpt-5.6-sol`, reasoning effort high, workspace-write, zero spec deviations either turn. Resume with `codex resume 019f8693-b6e1-7632-a0f7-eb60554a3436`.

**Codex-C ran three turns** (orchestrator feedback loop, like Codex-A before it). Turn 2 came from Nico's visual review: (1) mouth + lips now get a thin deep-maroon outline (width 0.002, `0x4a1420`); (2) the face's toon ramp was flattened (`[240,244,248,255]`, shade tint near-white) so the nose disappears under cel shading, anime-style — body skin shading untouched; (3) the hairclips (green + red, previously inheriting the hair outline that blended into them) now get dedicated per-mesh outlines colored as their own base color darkened to 22% — dark-green rim on the green clip, dark-red on the red.

Turn 3 (second visual review): (1) the rosy lips were undone — the lips primitive kept UVs into the face texture, so a post-pass in `applyNonoMaterials` hands the lips material the face's own texture map + white color and zeroes the gloss: lips now render identical to surrounding skin, with only the thin mouth outline remaining; (2) outline hulls no longer draw over nearby character geometry (the hair hulls were covering the ahoge `Nono_Hair_Fwip` and side sweeps) — all outline materials now apply a small clip-space depth bias (`gl_Position.z += 0.0002 * gl_Position.w`), letting real surfaces beat the hull while silhouette lines survive; (3) green clip outline strengthened (width 0.010, color multiplier 0.16). Note: turn 3 was sent via `codex exec resume <thread-id>` directly (the plugin forwarder lost its resume pointer) — same thread, same worker.

## Verification

- `pnpm test` 178/178 (was 170; +8 outline/ramp/lips tests across the three turns) · `pnpm check` 0 errors 0 warnings.
- Browser (`?nonoAsset=candidate`): idle/point_self outlined with correct materials; `cheer` and `surprised` show >.< with outlines intact; `thumbs_up` normal eyes; `?nonoOutline=0` cleanly removes outlines (A/B confirmed); console error-free.
