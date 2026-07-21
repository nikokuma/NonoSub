# Nono animation and rebuild handoff

The submission asset is already promoted. This document is the safe path for future motion edits.

## Source of truth

- Nico's private checkpoint: `/Users/nico/Projects/Blendr/NonoSubCheckpointFinal.blend`
- Clip library: `/Users/nico/Projects/Blendr/NonoClipLibrary.blend`
- Prepared release scene: `/Users/nico/Projects/Blendr/NonoSubRelease.blend`
- Local candidate slot: `static/assets/NonoCandidate.glb`

Never overwrite the checkpoint. The old production-preparation route is retired because it damaged approved geometry and materials.

## Approved clips

The app recognizes these normalized clip names:

| Clip | Runtime use |
| --- | --- |
| `Idle` | Resting lesson state |
| `Neutral` | Board presentation |
| `Think` | GPT lesson preparation |
| `Thumbs_Up` | Lesson completion |
| `Point_User` | Explicit fixture/future cue |
| `Point_Self` | Explicit fixture/future cue |
| `Cheer` | Skip remainder |
| `Heart_Touch` | Explicit fixture/future cue |
| `Surprised` | Lesson failure |

Do not key procedural tail bones `spine.055–078`, the dynamic long-hair roots and descendants, or skirt-secondary bones. NonoSub applies those systems after the animation mixer, and the strip/audit pipeline rejects prohibited tracks.

## Rebuild

From `/Users/nico/Projects/NonoSub`:

```bash
B=/Applications/Blender.app/Contents/MacOS/Blender
CKPT=/Users/nico/Projects/Blendr/NonoSubCheckpointFinal.blend

$B -b $CKPT --python scripts/cut_nono_clips.py -- \
  --output /Users/nico/Projects/Blendr/NonoClipLibrary.blend
$B -b $CKPT --python scripts/prepare_nono_release.py -- \
  --output /Users/nico/Projects/Blendr/NonoSubRelease.blend \
  --actions-from /Users/nico/Projects/Blendr/NonoClipLibrary.blend
$B -b /Users/nico/Projects/Blendr/NonoSubRelease.blend \
  --python scripts/export_nono_final.py -- \
  --output /Users/nico/Projects/Blendr/NonoSubFinal2.glb
node scripts/strip_nono_glb.mjs /Users/nico/Projects/Blendr/NonoSubFinal2.glb
node scripts/audit_nono_glb.mjs /Users/nico/Projects/Blendr/NonoSubFinal2.glb
```

Install into `NonoCandidate.glb` first. Inspect all nine moods at 1× and 2× DPR, point and underline targets, outfit/hair/leg integrity, and console output before promoting it.

Clip ranges are defined in `CLIP_RANGES` in `scripts/cut_nono_clips.py`. The current cutter includes the quaternion-hemisphere and arm-safety repairs documented in [SESSION_REPORT_NONO_ARM_FIX.md](SESSION_REPORT_NONO_ARM_FIX.md).
