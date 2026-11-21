# MDX gallery (runtime-path renders)

Every image here is produced by `analysis/agent-anim/mdxaudit` from the extracted
`War3.mpq` tree through the **same** animation path the game uses:

```
Animator::from_model(&model) -> play(sequence) -> update(dt) -> world_matrices()
    -> GeosetSkin::from_geoset + skin_positions
```

Visibility mirrors `crates/wc3-game/src/assets.rs::mesh_alphas_for`: the
per-sequence geoset extent gate (`Geoset::present_in_sequence`), the `hide`
helper-plane rule, and the "any in-range KGAO key" alpha rule. Textures are decoded
from `analysis/war3_full/**/*.blp` with `wc3_formats::blp::decode` and sampled per
triangle; team-colour layers use a flat blue tint.

Camera: three-quarter view (25° yaw, 10° pitch) so ground-plane props are visible,
orthographic, depth buffered, fitted per model.

| file | content |
| --- | --- |
| `gallery-stand.png` | 538 contact-sheet thumbnails, one per model with a Stand (or Portrait, for `*_portrait.mdx`) sequence |
| `gallery-walk.png` | 219 thumbnails, one per model with a Walk sequence |
| `gallery-effect.png` | 4 one-shot models without Stand/Walk (`InfernalBirth`, `WispExplode`, two Abomination explosions) |
| `gltf/*.glb` | curated MDX→glTF exports (HeroArchMage Spell, HeroBladeMaster Bladestorm, FireBall/Arrow flight+impact) |
| `gltf-batch/*.glb` + `gltf-verify.csv` | 76 hero/missile GLBs, Khronos validator 0 errors/0 warnings, structural PASS |
| `gltf-verify/*.png` | independent glTF skinning renders (t=0 vs mid-sequence) with frame-diff evidence |
| `hero-abilities.png` | 19 hero cast tiles rendered through `AnimatorRegistry::ability_animator(Cast)` (round 8) |
| `projectile-flight.png` | 67 missile flight tiles (`ProjectileSequence::Flight`) |
| `projectile-impact.png` | 67 missile impact tiles (`ProjectileSequence::Impact`) |
| `hero-abilities.csv` / `projectiles.csv` | sequence, matched alias, fallback flag, moving nodes, visibility, coverage, verdict, note |
| `gallery-index.csv` | `sheet,row,column,model,stand_sequence,walk_sequence,verdict` — maps every tile |
| `hero-Footman.png` … `hero-TownHall.png` | 192 px renders, one row per sequence (Stand/Walk/Attack/Death), three frames per row (start / representative / end) |
| `hero-diff.txt` | two-frame changed-pixel percentage per hero sequence (animation proof) |

Thumbnail frame selection: the frame with the most visible geosets, closest to the
middle of the sequence (effect models often show nothing at 50 % of their length).

Known blank tiles: `PlagueCloud.mdx` and `PlagueCloudTarget.mdx` — particle-only
models with zero geosets (verified: `geosets=0`).

Reproduce:

```bash
analysis/agent-anim/mdxaudit/target/debug/mdxaudit gallery \
  analysis/war3_full/Units analysis/war3_full/Buildings \
  --gallery analysis/agent-anim/gallery --thumb 96 --columns 24
analysis/agent-anim/mdxaudit/target/debug/mdxaudit heroes <4 hero mdx> \
  --gallery analysis/agent-anim/gallery --thumb 192

# Round 8: hero abilities + projectile/missile sequences (registry-driven)
analysis/agent-anim/mdxaudit/target/debug/mdxaudit abilities \
  --assets analysis/war3_full --gallery analysis/agent-anim/gallery \
  --thumb 128 --columns 10
```
