# Agent Handoff — Promote the Fixed Nono Model into the App

**Audience:** any agent (or human) tasked with making the corrected mocap Nono the one the real app shows.
**State as of 2026-07-21:** everything lives on branch **`codex/final-reliability-repair`** in the main checkout at `/Users/nico/Projects/NonoSub` — **there is no separate git worktree** — and the changes are **uncommitted** in the working tree.

## Where "this version" of Nono actually is

| Thing | Location | Status |
|---|---|---|
| The fixed, verified GLB (model + 9 mocap clips) | `static/assets/NonoCandidate.glb` | ✅ built, audit-passed, in-app verified |
| The release slot the production app loads | `static/assets/Nono.glb` | ⚠️ still the OLD bear-hoodie model — deliberately untouched |
| Source blend (Nico's master — never modify) | `~/Projects/Blendr/NonoSubCheckpointFinal.blend` | backup: `NonoSubCheckpointFinal.backup-0721.blend` |
| Fixed clip library / export intermediate | `~/Projects/Blendr/NonoClipLibrary.blend`, `NonoSubRelease.blend` | regenerable from the checkpoint |

**Why the app doesn't show it yet:** `nonoAssetFromLocation()` in [`src/lib/nonoToon.ts`](../src/lib/nonoToon.ts) returns `/assets/NonoCandidate.glb` **only in dev builds with `?nonoAsset=candidate`** in the URL; every production build and every default dev load uses `/assets/Nono.glb`. Promotion = copying the candidate over the release slot. No code changes are needed.

## Preconditions — do not skip

1. **Nico has explicitly approved the candidate.** They review at `http://localhost:1420/?surface=lesson&nonoAsset=candidate&nonoMood=<idle|neutral|think|thumbs_up|point_user|point_self|cheer|heart_touch|surprised>` (dev server: `pnpm dev`, port **1420**; in a plain browser click "Break it down" to mount the 3D stage). Do not promote without their sign-off — the previous "approved" builds turned out broken twice.
2. The current `Nono.glb` is Nono's **bear-hoodie form**, a distinct asset Nico wants kept. Preserve it before overwriting.

## Promotion steps

```bash
cd /Users/nico/Projects/NonoSub

# 1. Preserve the hoodie form (also lives in git history, but keep it addressable)
cp static/assets/Nono.glb static/assets/NonoHoodie.glb

# 2. Promote the candidate into the release slot
cp static/assets/NonoCandidate.glb static/assets/Nono.glb

# 3. Re-audit the release slot (must pass: ~103k tris, 55 draws, 10.4 MB, 9 clips)
node scripts/audit_nono_glb.mjs static/assets/Nono.glb

# 4. Tests
pnpm test
```

Then verify visually **without** the candidate param: `http://localhost:1420/?surface=lesson` → click "Break it down" → confirm the teacher-form Nono (camel blazer, navy skirt, cyan hair) idles at the chalkboard with tails pointing, and `?nonoMood=think` shows a clean hand-to-chin pose (no arm contortion). Check the browser console for errors. Note: copying a GLB into `static/` triggers a Vite full reload that resets in-page lesson state — reload and re-click.

Finally, commit everything on the branch (scripts, `src/lib` changes, both GLBs, docs) — Nico has not asked for any commits so far, so **ask before committing/merging**.

## If the GLB ever needs to be rebuilt from source

Full chain (each step is idempotent; Blender binary at `/Applications/Blender.app/Contents/MacOS/Blender`; nothing ever saves the checkpoint):

```bash
B=/Applications/Blender.app/Contents/MacOS/Blender
CKPT=~/Projects/Blendr/NonoSubCheckpointFinal.blend

$B -b $CKPT --python scripts/cut_nono_clips.py -- --output ~/Projects/Blendr/NonoClipLibrary.blend
$B -b $CKPT --python scripts/prepare_nono_release.py -- \
    --output ~/Projects/Blendr/NonoSubRelease.blend --actions-from ~/Projects/Blendr/NonoClipLibrary.blend
$B -b ~/Projects/Blendr/NonoSubRelease.blend --python scripts/export_nono_final.py -- \
    --output ~/Projects/Blendr/NonoSubFinal2.glb
node scripts/strip_nono_glb.mjs ~/Projects/Blendr/NonoSubFinal2.glb   # removes procedural tail/hair/skirt channels
node scripts/audit_nono_glb.mjs ~/Projects/Blendr/NonoSubFinal2.glb   # must pass before installing
cp ~/Projects/Blendr/NonoSubFinal2.glb static/assets/NonoCandidate.glb
```

If Nico re-records or re-times clips: frame ranges live in `CLIP_RANGES` inside `scripts/cut_nono_clips.py` (60 fps raw-take frames). If they edit the checkpoint in a live Blender session, **the session must be saved first** — the headless chain reads the file on disk (this exact gap caused a day of confusion).

## Known traps (learned the hard way)

- **Never route the export through `NonoSubProduction2.blend` / `prepare_nono_production.py`** — that legacy path deletes the legs, replaces materials with pink-fallback palette stand-ins, and adds an unwanted hair shine.
- The cut clips carry amplification with a hemisphere-canonicalized delta and a 60–130° gain taper; the cutter aborts if any amplified arm bone exceeds 165° from neutral. If it aborts, something upstream regressed — investigate, don't raise the limit.
- Blender 5.x: after assigning an action in scripts, also set `animation_data.action_slot = action.slots[0]`, or the rig silently evaluates the previous action.
- `Nono.glb` = bear hoodie (until promotion); teacher form is the candidate. Don't "clean up" `NonoHoodie.glb` after promotion — Nico wants the hoodie form kept for future use.
