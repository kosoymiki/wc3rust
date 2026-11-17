# FINAL ROUND — toolchain audit + 1v1 physics/pathing product work

Ledger **355** (`r2-sim`), 2026-11-13. Owner paths: `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`, `analysis/agent-sim/**`.

## 1. Toolchain audit (observed, `command -v` + `--version`)

| Tool | State | Version / evidence |
|---|---|---|
| assimp | **gap**: pkg `assimp 6.0.6 aarch64` installed, library only, no CLI in `$PREFIX/bin`; `pyassimp 1.16.x` installs but binds fail (`AssimpError: assimp library not found`; lib is `/data/data/com.termux/files/usr/lib/libassimp.so`) | `pkg list-installed`; pyassimp import rc 0 but load error |
| gltf-transform CLI | **gap**: present but crashes on Termux (`sharp` native module) | `gltf-transform --version` → sharp.mjs error |
| gltfpack | **installed** via npm; validated | 1.3, `gltfpack -i toolcheck.gltf -o toolcheck.glb -cc` rc 0, 1232 B |
| trimesh | present; glTF round-trip OK | 5.2.1, triangle vertices=3 |
| pygltflib | **installed** (pip, rc 0); wrote/read a real glTF | 1.16.5 |
| three | **installed** (npm prefix `~/.local/share/wc3-tools`) | 0.187.0 |
| ImageMagick | present (`convert` deprecated, use `magick`) | IM7 |
| optipng / pngquant | present | 7.9.1 / 3.0.3 |
| ffmpeg / sox | present | 9.0.2 / SoX_ng 14.8.1 |
| radare2 / rabin2 | present | 6.2.6 (abi 174, linux-arm_64) |
| stormlib | **gap**: no Termux package/CLI; MPQ capability is covered by the repo's Rust `wc3-mpq` (StormLib-compatible reader) | `command -v stormlib` → missing |

Tool-validation artifact: pygltflib wrote `toolcheck.gltf` (triangle), trimesh
loaded it (3 vertices), gltfpack compressed it to `toolcheck.glb` (1232 B, rc 0).

## 2. H2 base used (cited)

- `harness2_corpus_search("MDX to glTF conversion assimp gltf-transform 3D viewer pipeline")` → corpus id **34084** (`gist.b1.0028-blender-3d-3a-noob-to-pro-top`).
- `harness2_corpus_search("WPM pathing map cell size 32 movement collision stuck")` → corpus id **61461** (`dream.rts-building-footprint-vs-unit-arrival-deposit-d-4261`: building footprints block cells; arrival thresholds must include footprint half-extent; non-building spawns in blocked cells relocate).
- `harness2_experience(search: "WPM pathing native cell size 32 turn rate")` → experience ids **4364, 4439, 4435**.
- Skill loaded: `fullstack-primer` (`~/.config/opencode/skills/fullstack-primer/SKILL.md`).
- stealth-browser docs probe: `https://pypi.org/project/pygltflib/`, method `stealth-browser-mcp`, date 2026-11-13, completeness **partial** (169 chars rendered; local package version verified instead).

## 3. Final product work

### 3.1 Pathing from WPM at native resolution
- `crates/wc3-sim/src/lib.rs:74` `PATHING_CELL_SIZE = 32.0`; `:704` `Sim::set_pathing(width,height,cell_size,walkable)` replaces grid, resizes A*, clears flow cache, rebuilds spatial hash and fog buffers; `:741` `walkable_pos_near`.
- Fog radius now uses `grid.cell_size` (32), not the old 128 constant.
- `crates/wc3-game/src/sim_bridge.rs:179` `apply_map_pathing`, `:198` `apply_assets_pathing`; `spawn_map_scene` applies the WPM **before** spawning units. Human04 gate now asserts grid **(512×512, cell 32)**.
- Game hook patch `analysis/agent-sim/patches/game-melee-1v1.patch` replaces the downsampled `build_grid` with `apply_assets_pathing` (dry-run rc 0).

### 3.2 Movement/collision/turn physics
- `turnRate` from `UnitData.slk` ported into `UnitStats.turn_rate` and `Unit.turn_rate`; `rotate_towards` (`lib.rs:599`) caps facing change at `turn_rate × TAU / 30` per tick.
- Blocked-path guard: when a pure Move/MoveAttack has no path and the straight line is blocked, the unit stops with `Event::Error("path blocked")` instead of running in place (harvest/build/harvest-lumber orders exempt so they can approach blocked target cells).
- Collision: `collision/2` radius already from data; separation + `can_stand` 5-sample check; 32-unit cells now match unit radii.
- Constants from tables: `spd` 270 (footman/grunt), `turnRate` 0.6, `acquire` 500, `collision` 31/2, `def` armor, `dmgplus1`, cooldown×30; item/ability values unchanged.

### 3.3 1v1 Orc vs Alliance setup
- `crates/wc3-sim/src/lib.rs:2962` `Sim::setup_melee_1v1(player_race, enemy_race, bases)` — main hall, gold mine and **5 workers per side** on walkable cells, real `GOLD_MINE_GOLD=12500`, enemy AI enabled with race composition; `crates/wc3-game/src/sim_bridge.rs:244` thin delegate.
- Patch `patches/game-melee-1v1.patch` makes the no-map scenario Orc player vs Human AI (hall + mine + 5 peons/peasants); dry-run clean. Owner applies it to `game.rs` (read-only for this layer).

## 4. Evidence (commands + rc/counts)

| Gate | rc | Counts |
|---|---:|---|
| `cargo test -p wc3-sim --test physics_1v1 --offline` | **0** | **4 passed** (WPM 512² path/no-stuck, turn/constants, collision, 1v1 setup) |
| `cargo test -p wc3-game --lib melee_1v1 --offline` | **0** | 1 passed |
| `cargo test -p wc3-game --lib map_units_spawn --offline` | **0** | 2 passed; native WPM grid asserted |
| `cargo check -p wc3-game --lib --offline` | **0** | `final-game-check.log` |
| QA parity harness (24 gates) | **0** | **24/24 PASS** (`final-parity.log`, `final-parity.json`) |
| `patch --dry-run` game-melee-1v1 | **0** | clean |

Human04 map census remains: raw 90 = 79 units + 3 gold mines + 6 `sloc` + 2
map-object-only (`nC02`, `nC03`), Y spread 10927.7, halls 0.

**Device:** `H2_DEVICE_SLOT` is unset → no device launch/install (constraint:
device only with `H2_DEVICE_SLOT=1`). No screenshots claimed.

## 5. Gaps / next (honest)

- **MDX→glTF/viewer**: no converter exists in repo/toolchain; assimp and Blender
  do not parse MDX. Next: export the existing `wc3-formats` MDX meshes through
  pygltflib (or a Rust glTF writer) and validate with trimesh/gltfpack.
- **assimp CLI** missing on Termux (lib-only package); pyassimp cannot bind the
  Termux lib. Use gltfpack/trimesh/pygltflib for glTF checks.
- **stormlib** absent as a CLI; `wc3-mpq` is the canonical MPQ reader.
- World-layer `model_render.rs` was mid-edit (duplicate `pivot`) during one test
  build; it recovered and the game tests above pass.
- 1v1 scenario requires the HUD/game owner to apply `game-melee-1v1.patch`.
