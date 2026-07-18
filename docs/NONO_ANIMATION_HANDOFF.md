# Nono animation handoff

Use `/Users/nico/Projects/Blendr/NonoSubProduction.blend` for suit capture. Do not animate `NonoSubCheckpoint.blend`, `NonoSubProductionSource.blend`, or either source-only armature.

## Canonical character

- Armature: `Nono_Rig`
- Export collection: `NONO_EXPORT`
- Reference/backups: `SOURCE_ONLY` (keep excluded)
- Scene rate: 30 fps
- Keep the rig at the world origin with object rotation `0, 0, 0` and scale `1, 1, 1`.

The jacket details, outfit, hair, plugs, and tails already target the canonical rig. Do not merge the source armatures back into the export collection.

## Required actions

Create exactly these actions on `Nono_Rig`:

| Action | Timing | Behavior |
| --- | --- | --- |
| `Idle` | 60–120 frames | Seamless breathing, blink, and extremely small body sway. |
| `Think` | 60–120 frames | Seamless thoughtful head tilt and facial change. |
| `Present` | 30–60 frames | One cheerful teaching beat; avoid a wide arm sweep across the chalkboard. |

Body and facial bone keys are welcome. Remove object-level translation/scale and root `spine` translation/scale. Store the actions with fake users or NLA tracks so Blender does not discard them.

Do not key:

- tail bones `spine.055–078`;
- long-hair roots `spine.021`, `.031`, `.039`, `.085`, `.093`, or any descendants;
- `skirt_root*` secondary bones.

NonoSub applies those motions procedurally after the animation mixer. `scripts/export_nono_final.py` rejects release actions that key them.

## Weight and clipping checks

Before final export, inspect the complete outfit at representative poses:

1. T-pose.
2. Arms relaxed down.
3. Arms forward.
4. Shoulder lift.
5. Deep elbow bend and cuff compression.
6. Torso lean.
7. Head tilt in both directions.
8. The final `Present` pose.

Check shoulders/armpits, elbows/cuffs, blazer opening/buttons, waist/skirt top, neck bow/collar, long-hair shoulder clearance, hair bow, tail plugs, skirt/thigh intersections, and exposed-body seams. Correct weights in `NonoSubProduction.blend`; never edit the checkpoint as the fix.

## Final export

From `/Users/nico/Projects/NonoSub`:

```bash
/Applications/Blender.app/Contents/MacOS/Blender \
  -b /Users/nico/Projects/Blendr/NonoSubProduction.blend \
  --python-exit-code 1 \
  --python scripts/export_nono_final.py -- \
  --output /Users/nico/Projects/Blendr/NonoSubFinal.glb

node scripts/audit_nono_glb.mjs /Users/nico/Projects/Blendr/NonoSubFinal.glb
```

The final-export script samples at 30 fps, exports only the three required actions, and does not save over the production Blender file. Replace `static/assets/Nono.glb` only after the strict audit passes and the exact lesson-window comparison is approved.
