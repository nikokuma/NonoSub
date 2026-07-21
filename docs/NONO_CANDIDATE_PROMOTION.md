# Nono teacher-form promotion — completed

The repaired teacher-form Nono was promoted on July 21, 2026 in commit `536d56c`.

## Current asset slots

| Asset | Purpose | Status |
| --- | --- | --- |
| `static/assets/Nono.glb` | Production teacher form | Promoted and audit-passed |
| `static/assets/NonoHoodie.glb` | Preserved original bear-hoodie form | Tracked and addressable |
| `static/assets/NonoCandidate.glb` | Local rebuild/comparison slot | Ignored development asset |

The default lesson URL loads the teacher form without `nonoAsset=candidate`. Development-only asset and mood overrides remain available for comparison.

## Verified production asset

- 103,072 triangles;
- 55 draw calls;
- 54 skinned meshes;
- one canonical skin;
- both supported 12-bone tail chains;
- nine clips: `Idle`, `Neutral`, `Think`, `Thumbs_Up`, `Point_User`, `Point_Self`, `Cheer`, `Heart_Touch`, and `Surprised`;
- SHA-256 `06ae6d953f0568c4153ef7cda139b9b786789130749f357c41c94568cea1946e`.

The preserved hoodie hash and ownership terms are recorded in [ASSET_RIGHTS.md](../ASSET_RIGHTS.md).

## Rebuild from Nico's private Blender source

The pipeline never overwrites the checkpoint:

```bash
B=/Applications/Blender.app/Contents/MacOS/Blender
CKPT=~/Projects/Blendr/NonoSubCheckpointFinal.blend

$B -b $CKPT --python scripts/cut_nono_clips.py -- \
  --output ~/Projects/Blendr/NonoClipLibrary.blend
$B -b $CKPT --python scripts/prepare_nono_release.py -- \
  --output ~/Projects/Blendr/NonoSubRelease.blend \
  --actions-from ~/Projects/Blendr/NonoClipLibrary.blend
$B -b ~/Projects/Blendr/NonoSubRelease.blend --python scripts/export_nono_final.py -- \
  --output ~/Projects/Blendr/NonoSubFinal2.glb
node scripts/strip_nono_glb.mjs ~/Projects/Blendr/NonoSubFinal2.glb
node scripts/audit_nono_glb.mjs ~/Projects/Blendr/NonoSubFinal2.glb
cp ~/Projects/Blendr/NonoSubFinal2.glb static/assets/NonoCandidate.glb
```

After visual approval, preserve the current production asset before copying a newly audited candidate into `Nono.glb`.

## Known traps

- Do not route this asset through legacy `NonoSubProduction2.blend` or `prepare_nono_production.py`; that path previously removed legs and replaced approved materials.
- The clip cutter uses a hemisphere-canonicalized delta and bounded amplification. If its 165° arm safety check fails, investigate rather than raising the limit.
- Blender 5.x action assignment also requires `animation_data.action_slot = action.slots[0]`.
- Save the live Blender checkpoint before a headless rebuild; the command reads the file on disk.

Implementation and investigation attribution is recorded in [AI_CONTRIBUTIONS.md](AI_CONTRIBUTIONS.md).
