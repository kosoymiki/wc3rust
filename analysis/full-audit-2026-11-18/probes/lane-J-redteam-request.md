# Lane J -> lane I (ledger 528) red-team request

Lane J (game/HUD/render) finished its first pass at `d7af459` (worktree, hashes in
`probes/lane-J-hashes-start.txt`). Three of my conclusions depend on **your** crate, and I would
rather be corrected before the tech-lead merges. Please answer with `file:line` + one observation
each, in your own report under a `## Lane J challenges` section (and ping me).

## 1. Does `is_visible` failing closed hide entire armies?

`Sim::is_visible` (`crates/wc3-sim/src/lib.rs:6812-6822`) returns `true` for an owner with no seat
row ("neutral/omniscient sees everything") and `false` for any position outside the grid.

The scene builder drops enemy units when `!sim.is_visible(player, unit.pos)`
(`crates/wc3-game/src/game.rs:7210` and `:8076`), and the minimap does the same
(`crates/wc3-game/src/ui/minimap.rs:57,152`).

I claim: a *systematic* visibility failure (seat binding wrong, or unit positions expressed in a
different scale than `grid.cell_size`) cannot produce "a few dark cells" — it removes every enemy
from the scene while the player's own units still draw, which is exactly one of the shapes of the
operator's "black mission world". Is there a path in `wc3-sim` where `seat_row(owner)` returns
`None` for a real player (so the player silently becomes omniscient), or where the visibility grid
is never updated for a bound seat (e.g. `FogMaskEnable(false)`, `host.fog_mask_enabled`,
`FogModifyAll`)? File:line either way.

## 2. Can a real map produce a grid larger than 256x256 cells?

`fog_mask_from_sim` (`crates/wc3-game/src/render.rs:1309`) computes
`stride = max(w,h)/256`, and I measured that the cap is exceeded for 257..511 (`4000 -> 267`,
`511 -> 511`). My severity argument (P3, not P2) rests on: the grid is per **terrain tile**
(`Grid::new(w, h, CELL_SIZE=128.0)` at `wc3-sim/src/lib.rs:105,1092`, then `build_grid(...,
TERRAIN_TILE_SIZE)` at `wc3-game/src/game.rs:8139`), and WC3 maps are at most 256x256 tiles, so
`stride` is always 1 on real data.

Please confirm from `wc3-sim`'s own reading of `war3map.w3e`/`w3i` that nothing multiplies the grid
dimension by 32 (pathing resolution) anywhere before `Game::new` uses it — or name the place that
does, which would promote the finding.

## 3. Minimap lattice vs corner-sampled mask

`Game::minimap_fog_cells(32, 32)` (`game.rs:6645-6667`) samples visibility at cell **centres**
of a 32x32 lattice over `assets.world_size`. The 3D mask samples each stride block at its corner
(`render.rs:1316-1321`, `sx = x*stride`). With `stride > 1` the two lattices can disagree on a
vision boundary, i.e. the minimap can show fog where the world shows none (or the reverse). I
report this as an aliasing question (P4), not a defect, because `stride == 1` on real maps.

If you have evidence that a stride-reduced mask is ever produced in a shipping path (or that
`minimap_fog_cells` is called with a resolution that makes the mismatch reachable), say so and I
will raise it.

## What I am *not* challenging

Your determinism/state-hash work is out of my read range; I have no evidence against it. The only
`s`,im` behaviour my findings rely on is: (a) `is_visible` semantics as quoted, (b) grid units are
tiles at 128 world units, (c) visibility updates once per tick.
