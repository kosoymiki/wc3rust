# DISPATCH STATUS — single base v0 (2026-11-13)

Dispatcher: this session. Root: `ses_e4095a1b9ffeRpzK387GOaL3Ly`.
Published: `analysis/tech-lead/BASE.md` (v0 execution base) and
`analysis/tech-lead/WC3RE_MERGE_MATRIX.md` (per-file/per-item absorption).
All 11 layers were reactivated in EXCLUSIVE execution scope; no device runs.

## Owner table (exclusive execution scope)

| layer | session | OWNER_PATHS | state | evidence |
|---|---|---|---|---|
| anim | `ses_e30595420ffdGjgXSvyWSGFoT0` | `wc3-formats/src/mdx.rs`, `wc3-anim/**` | **DONE** | weights test passes; 542-model sweep 528 PASS/14 WARN/0 FAIL; `merge-anim.md` |
| HUD | `ses_e30595424ffe9AqABZXayrv0Rb` | `wc3-game/src/{mobile_hud,game,hud,ui}.rs` | **DONE** | `cargo check -p wc3-game --lib` rc 0; 6 HUD + 4 routing + 6 ui + `ui_fdf` 10/10; `merge-hud.md` |
| world | `ses_e30595423ffdGkcohq9P8MIxiL` | `terrain_render,render,model_render,doodads.rs` + terrain parts of `assets.rs` | **DONE** | doodads 2,509/2,509 (86 models, 0 failures), seam ratio 1.301; `merge-world.md` |
| QA | `ses_e30595409ffe2wSkLaUsMd22EV` | `scripts/qa/**` | **PARTIAL (green scripts)** | py_compile rc 0; acceptance plan rc 3 (freeze); `merge-qa.md` |
| RE | `ses_e30424702ffe4bTUAPcwFK1lIp` | `wc3-re/**`, `wc3-jass/**` | **DONE (+PARITY #4/#6/#7)** | 44 lib + 5 + 1 + 2 + 3 + 6 rc 0; `scan-final.json` `ok=true`, 0 errors, `scripts.unimplemented={}`, 44/44 main; `qa-jass-map-runtime.json` 44/44, 713 triggers, 6 723 placements; `merge-re.md` §10.3/§10.4. **ROUND 7 re-verified on the current tree**: wc3-re lib **47 passed** (`round7-re-tests.log`), `scan-r7.json` `unimplemented={}`/0 errors/44 main/791 natives, creeps **3 passed**, parity+gate rc 0 (44/44, 713 triggers, 6 723 units/468 spawned/6 255 matched), `map_units_spawn` **2 passed** on current `game.rs` `e7db1c68…`; report `analysis/agent-re/PARITY_R7.md`. **ROUND 8 (abilities #11 / items #14 runtime)**: ability state levels/permanent/cooldown, cast context + `Engine::cast_ability`, item entities/inventory/charges/use/drop/pickup events/flags/shop stock/item pools, trigger unit-filter fix; wc3-re lib **61 passed** (14 new `ability_item_tests`), all-targets rc 0, scan `unimplemented={}`/0 errors/44 main/791 natives, parity+gate rc 0; report `analysis/agent-re/PARITY_R8.md`. **P0 parser gap close (ROUND 9)**: `wc3-jass` bare `return` no longer consumes the next line and `elseif` chains preserve the first-true branch + else (root cause `crates/wc3-jass/src/lib.rs`; removed unused `wc3-sim` dep); QA gates **triggers.jass 45/45, triggers.return 3/3, triggers.elseif 1/1 PASS** (`analysis/agent-qa/parity/report-r9.json`), full `PARITY PARTIAL gates=23 fail=0 gap=3`; wc3-re lib **63**, wc3-jass lib **11** + triggers gate; release `wc3re scan` 0 errors, `unimplemented={}`, 44/44 main, 791 natives; report `analysis/agent-re/PARITY_R9_PARSER.md` |
| assets | `ses_e3059540affeOQRBuuqdX6w6Vh` | `wc3-mpq/**`, `wc3-assetdb/**`, `wc3-audio/**` | **DONE** | 61 tests; mips 28,748/28,748, incomplete 0; `merge-assets.md` |
| GPU | `ses_e30595412ffdOJEvwiBqSyRYDi` | `wc3-gpu/**`, `wc3-render/**`, render integration after world | **DONE scope** | `wc3-render` 25 tests; app/smoke/bench check rc 0; `merge-gpu.md` |
| FDF | `ses_e3059541bffeFozxokptFv8d5b` | `wc3-formats/src/fdf.rs`, `text.rs`, fonts | **DONE scope** | fdf 11, ui 16, text 11, hud::cyrillic 1; `ui_fdf` now 10/10; `merge-fdf.md` |
| data | `ses_e3059541fffejS1YGmXDWBw12Q` | `wc3-data/**`, `gamedata.rs` | **DONE scope + bridge** | 34+2+8+20+1 tests rc 0; typed bridge test passes; `merge-data.md` |
| sim | `ses_e3059541fffdUO0AGBqY6t1juG` | `wc3-sim/**` + new `sim_bridge.rs` | **DONE** | sim 99 tests; bridge 4 tests; `wc3-re` ScriptOrder patches applied, wc3-game lib rc 0; `merge-sim.md` |
| touch | `ses_e30595423ffe7cc9wy7YBbXx9q` | `wc3-input/**`, `camera.rs`, game input | **DONE** | input 19/19; game input 17/17; `merge-touch.md` |

## First integration rc

| gate | rc | detail |
|---|---:|---|
| `cargo metadata --no-deps` | 0 | 28 packages, 0 duplicate names |
| `cargo check` core 10 crates | 0 | mpq/formats/re/jass/data/sim/input/anim/gpu/render |
| `cargo check -p wc3-assetdb -p wc3-audio` | 0 | assets scope |
| `cargo check -p wc3-game --lib` | 0 | HUD/sim/data/RE bridge compiled |
| `cargo test` core 8 crates | 0 | 316 tests / 35 suites |
| `cargo test -p wc3-render --lib` | 0 | 25 passed |
| `cargo test -p wc3-re -p wc3-jass -p wc3-sim` | 0 | 60 passed |
| `cargo test -p wc3-game --test ui_fdf` | 0 | 10/10 |
| `cargo test -p wc3-game --lib` | 0 | **63 passed, 5 ignored**; `cargo test -p wc3-game` all suites 0 |
| `wc3re scan` | 0 | **overall true**: 3,935 BLP, 2,048 MDX, 33 SLK, 67 TXT, 71 FDF, 44/44 maps, 91/91 AI, 1,182 icons, 0 errors; machine counts identical to `analysis/agent-re/scan.json` |
| release APK `cargo build --release -p herofolends-android` | 0 | final rebuild 2m43s; libmain.so 7.3 MB; log `apk_build_final.log` |

## Central merges applied by the dispatcher

- `0006-assets-consolidated.patch` — **superseded**: current `assets.rs` already uses `MapBundle` (line 14/504), `StormSource` and the canonical `wc3_anim` vertex weight; the patch dry-run fails (9/9 hunks, stale base). No action.
- MDX chunk walker moved to `wc3-formats::mdx` (patch 0002) and `wc3-re::mdx` delegates.
- BLP1 all-mip fix applied to `wc3-formats/src/blp.rs` (syntax fixed; 28,748 mips complete).
- `wc3-formats::map::parse_doodads` + `MapBundle.doodads` added (one doodad reader).
- `wc3-data` joined the root workspace (nested `[workspace]` removed).
- Data P3 applied: `ReLayer.typed`, `object_info`, `typed_from_objects`; fixed `ObjectDb::load_storm` `\` path stems (now `unitdata`, not `units\unitdata`; `metas` 0→1).
- Sim `ScriptOrder` patches applied to `wc3-re/jass` + `sim_bridge` module.
- `wc3-audio` graph corrected (no MPQ dependency).

## Remaining

1. World: **done** (both tests green).
2. Assets consolidation: **done/superseded** — `assets.rs` uses `MapBundle`/`StormSource`/`wc3_anim`; patch 0006 is stale and is NOT applied.
3. RE map-runtime: PARITY #4/#6/#7 delivered. The verified patch
   `analysis/tech-lead/patch-game-parity46.diff` was **applied by HUD on 2026-11-13
   12:02** (post-apply SHA-256 `fad4edd2…`) and re-verified by RE:
   `cargo check -p wc3-game --lib` rc 0, `map_units_spawn` 2 passed (90/90 +
   frame loop), full game lib 95 passed / 18 ignored / 0 failed.
4. QA layer remains **PARTIAL (green scripts)** — acceptance plan returns rc 3 on the freeze marker; QA-owned, not a merge blocker.
5. **Base v0 is green**; no device runs performed. Device acceptance waits for the tech-lead/operator.

## Latest verification (2026-11-13, dispatcher resume)

| gate | rc | artifact |
|---|---:|---|
| `cargo test -p wc3-re -p wc3-jass` | 0 | `analysis/tech-lead/base-v0-re.log` — 44 lib + 5 + 1 + 2 + 3 + 6 |
| `cargo test -p wc3-game --lib` | 0 | `analysis/tech-lead/base-v0-game.log` — **91 passed, 18 ignored** |
| `wc3re scan` | 0 | `analysis/agent-re/scan-final.json` (no `wc3-formats` change since scan; 08:41Z) |
| `cargo check -p wc3-game --lib` | 0 | `analysis/agent-re/check-game-r3.log` |
| `game.rs` patch check/test | 0/0 | `check-game-patch.log`, `game-map-units-spawn.log` |
