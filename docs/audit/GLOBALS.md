# Global and process-wide state inventory (AUD-032)

Registry: `docs/audit/WC3RUST_CODE_AUDIT.md` (AUD-032, P4).
Audit revision: `b64b9b6`; scan and classification by the AUD-032 lane (LEDGER 493).

Scan (reproducible):

```
rg -n "^\s*(pub\(crate\) )?(pub )?static |OnceLock|LazyLock|AtomicPtr|thread_local!" \
   --type rust crates apps tools -g '!*/tests/*'
```

Every hit is classified as one of:

- **D — immutable process-wide data**: written once (or never), read-only after
  init. Safe to share across sessions; must not be "cleaned up".
- **C — process-wide cache/telemetry**: mutable, keyed by shipped data or
  monotonic counters, not per-match state. Safe if the key is stable and the
  growth is bounded.
- **S — session state**: describes a running match/mission/activity. Must have
  an explicit lifecycle (set on start, cleared on teardown).
- **T — test-only** (`#[cfg(test)]`): nothing to fix; listed so the scan result
  is complete.

## D — immutable process-wide data

| Location | Item | Why it is immutable |
| --- | --- | --- |
| `crates/wc3-sim/src/{roster,items,upgrades,shops,extras,creeps,destructables,combat,bounty,abilities,item_reqs}_gen.rs` | generated `pub static` tables (`ROSTER`, `ITEMS`, `UPGRADES`, `SPELL_DEFS`, `RECIPES`, …) | `&'static [..]` literals generated from retail data; never mutated |
| `crates/wc3-sim/src/roster_gen.rs:7494-7619` | `ROSTER_OVERLAY`, `ABILITY_OVERLAY`, `UPGRADE_OVERLAY` | balance patch rows, const |
| `crates/wc3-sim/src/data.rs:945` | `SHOP_UNITS` | const table |
| `crates/wc3-sim/src/buffs.rs:37` | `POOL: OnceLock<Mutex<HashSet<&'static str>>>` | name interner for static buff ids; inserts are idempotent and the key set is closed |
| `crates/wc3-sim/src/jass/native_status.rs:81,589` | `OVERRIDES` table + `INDEX: OnceLock<BTreeMap<_, &'static StatusEntry>>` | classification table, derived once |
| `crates/wc3-sim/src/jass/registry.rs:46,50` | `INDEX`, `DEFS` OnceLocks | native registry snapshot |
| `crates/wc3-sim/src/jass/natives.rs:32,672` | `HANDLERS` table + `INDEX` OnceLock | dispatch table |
| `crates/wc3-sim/src/save.rs:664` | `ID_POOL: OnceLock<BTreeMap<&'static str, ()>>` | name pool built from the static defs; used by `intern()` |
| `crates/wc3-sim/src/abilities.rs:158,541` | `GLOBAL: SpellNumberTable`, `SPELL_DEFS` | AUD-023 lane owns this file; table is const |
| `crates/wc3-bink/src/tables.rs` (21 statics) | VLC/DC/intra tables | const bitstream tables |
| `crates/wc3-render/src/daynight.rs:1348-2056` (12 statics) | `LORDAERON_*`, `ASHENVALE_*`, `FELWOOD_*`, `DALARAN_*`, `DUNGEON_*`, `UNDERGROUND_*` | `DncLight` presets, const |
| `crates/wc3-render/src/texture.rs:163,183` | sRGB LUTs (`OnceLock<[f32;256]>`, `OnceLock<Box<[u8;65536]>>`) | computed from formulas at first use, read-only |
| `crates/wc3-render/src/mesh.rs:335,368,373` | `MODEL: OnceLock<LightModel>`, two env `OnceLock<bool>` | derived from env/const, read-only after init |
| `crates/wc3-mpq/src/lib.rs:249` | `TABLE: OnceLock<[u32; 0x500]>` | MPQ crypt table, pure function of constants |
| `crates/wc3-data/src/lib.rs:728,733` | `ROC`, `TFT: OnceLock<GameDatabase>` | parsed shipped database; no interior mutation after load |
| `crates/wc3-turnip-vendor/src/lib.rs:43,45` | `GIPA`, `PHYS_PROC: OnceLock<Option<..>>` | resolved `dlsym` pointers |
| `crates/wc3-vulkan-loader/src/lib.rs:21` | `GIPA: OnceLock<Option<Gipa>>` | resolved loader entry points |
| `crates/wc3-adrenotools/src/linker.rs:101,102` | `SYMBOLS`, `DEFAULT_NAMESPACE: OnceLock<..>` | resolved once per process |
| `crates/wc3-adrenotools/src/hooks.rs:83,84,85,86,87` | real-`dlopen`/`fopen`/GSL pointers (`OnceLock`/`AtomicPtr`, write-once) | hook trampolines, installed once |
| `crates/wc3-render/src/perf.rs:18-20,60` | `ALLOC_CALLS/BYTES`, `FREE_CALLS`, `TEST_ALLOC` | global allocator + monotonic counters |
| `apps/bench/src/gpu_bench.rs:33` | `ALLOC: perf::CountingAlloc` | global allocator wiring |

## C — process-wide caches and telemetry

| Location | Item | Key / bound | Verdict |
| --- | --- | --- | --- |
| `crates/wc3-game/src/hud/inventory.rs:547` | `TABLE: OnceLock<Mutex<IconTable>>` | keyed by item `def_index`; re-install overwrites the row and interns one copy per distinct icon name | Safe: install is per-install-of-the-game, not per match; `install_icons` is idempotent (`inventory.rs:572-602`) |
| `crates/wc3-game/src/hud.rs:2117` | `FONTS: OnceLock<Option<HudFonts>>` | one font atlas per process | Safe: fonts do not depend on the match |
| `crates/wc3-game/src/cinematic.rs:405` | `LIBRARY: OnceLock<MovieLibrary>` | movie index of the install | Safe: keyed by install root; a missing library stays `None` |
| `crates/wc3-game/src/lib.rs:41` | `LOG_SINK: OnceLock<fn(&str)>` | one sink per process | Safe: the app sets it at startup; a later caller cannot replace it (documented) |
| `crates/wc3-game/src/mem.rs:42` | `PAGE_KB: OnceLock<u64>` | `sysconf` value | Safe |
| `crates/wc3-game/src/render.rs:103,119,131` | `LAST_PASS`, `ENABLED`, `COUNT` | per-frame diagnostics | Safe: telemetry; `COUNT` bounds the error log (`render.rs:135`) |
| `crates/wc3-game/src/doodads.rs:100` | `ONCE: OnceLock<()>` | one-time diagnostic sink install | Safe |
| `crates/wc3-game/src/campaign.rs:1620` | `CAMPAIGN_RULE_WARNED: AtomicBool` | warn-once | Safe |
| `crates/wc3-sim/src/ai.rs:13` | `FLAG: OnceLock<bool>` | `H2_AI_DEBUG` env, read once | Safe: env is process configuration |
| `crates/wc3-formats/src/diag.rs:28-30` | `SINK`, `COUNTS`, `TOTAL` | diagnostic channel; `MAX_LINES_PER_REASON = 8` | Safe: bounded per reason (`diag.rs:23`) |
| `crates/wc3-adrenotools/src/hooks.rs:82,88` | `PARAMS: AtomicPtr<HookParams>`, `TRACE: OnceLock<bool>` | hook configuration, installed once before the hooked call | Safe: write-once before use; lifetime documented at the declaration |
| `crates/wc3-adrenotools/src/linker.rs:103` | `TARGET_ID: AtomicU16` | target app id | Safe: one target process |
| `crates/wc3-game/src/cinematic.rs:88-93` | `thread_local! ACTIVE_MOVIE` | per-thread playback handle | Safe: playback is owned by the thread that started it; dropped with the thread |
| `apps/android-native/src/lib.rs:523-525` | `STATE`, `VIBRATOR`, `VIBRATE` atomics | platform service handles cached per process | Safe: handles are not match state |

## S — session state (and its lifecycle)

| Location | Item | Lifecycle | Verdict |
| --- | --- | --- | --- |
| `crates/wc3-re/src/jass/vm.rs:521,523` | `DEFAULT_LOCAL_PLAYER`, `DEFAULT_LOCAL_PLAYER_ANY` | set by the campaign before boot; **reset added** for non-campaign maps | **Fixed in this lane** |
| `crates/wc3-game/src/campaign.rs:1623,1624` | `REGISTRY`, `ACTIVE: OnceLock<Mutex<Option<..>>>` | `store_active` on mission start; `clear_active_campaign()` exists but no production caller | Documented limitation (see below) |
| `apps/android-native/src/lib.rs:149` | `LIFECYCLE_QUEUE` | drained by the render loop every frame, capacity 64 (`lib.rs:183`) | Safe: bounded, drained |
| `apps/android-native/src/lib.rs:164,166,169,173` | `GAME_FOR_CHECKPOINT`, `WATCHER_SAVED`, `PENDING_CHECKPOINT`, `LAST_SAVED_TICK` | registered at `lib.rs:1425`; pointer cleared + tick reset at teardown (`lib.rs:1902-1905`) | Safe by contract (AUD-026 documents the SDL lock/main-thread argument); no cross-session leak observed: teardown resets, an unclean exit ends the process |
| `apps/android-native/src/lib.rs:315,849` | `LOG`, `STAMP: OnceLock<..>` | one logcat function pointer; map stamp cache | Safe: `LOG` is a function pointer; `STAMP` is keyed by the loaded map path |

### Fix applied: JASS seat default leaked between sessions

`Runtime::set_default_local_player` / `set_local_player_any_default` are
process-wide because a map script's `main` runs during boot. A campaign mission
sets the human seat (Human01 is seat **2**, not 0) and the wildcard flag; nothing
reset them. A melee map booted afterwards produced a `World` with that seat, so
`GetLocalPlayer() == Player(0)` was false and every unguarded script block was
silently dropped — the exact "indistinguishable default" failure mode AUD-031
also targets.

Changes:

- `crates/wc3-re/src/jass/vm.rs:540-544` — `reset_local_player_default()` restores
  seat 0 and clears the wildcard, with the session rationale on the function.
- `crates/wc3-game/src/game.rs:1610-1628` — `attach_map_runtime` resets the
  defaults before booting a map that is **not** under `Maps\Campaign`
  (`map_file_is_campaign`, the path form of `map_path_is_campaign`). Campaign
  missions keep the seats their layer installed.
- Tests: `crates/wc3-re/src/jass/vm.rs::tests::local_player_default_is_session_state_and_resets`.

Edge: a campaign map forced into the legacy melee arena
(`MELEE_MAP_STEMS`) is still a `Maps\Campaign` path, so it keeps whatever seat
was installed. That arena deliberately does not run the mission script
(`game.rs:1341-1368`), so the inherited seat only affects the legacy script boot
that the arena already treats as non-canonical; making the seat explicit in
`EngineConfig` is the clean upgrade if the arena ever adopts the script.

### Documented limitation: campaign session globals

`REGISTRY` is a chapter-list cache with an explicit invalidation entry point
(`reload_campaign_registry`) and is cloned out on every read, so it cannot leak
into gameplay state. `ACTIVE` holds the running mission; `clear_active_campaign()`
exists and is the intended teardown, but the current menu/mission flow in this
tree has no production caller of either `store_active` or
`clear_active_campaign` — the campaign module is exercised by its own tests only.
That is recorded here instead of being "fixed" by guessing a call site: when the
campaign flow is wired to the UI, mission start must call `store_active` (already
does) and returning to the menu must call `clear_active_campaign()`.

## T — test-only globals (listed for completeness)

- `crates/wc3-game/src/assets.rs:4238` (`LINES`), `hudskin/pack.rs:983,1080`,
  `terrain_render.rs:3028` (`GPU`), `crates/wc3-re/src/mapboot.rs:358`
  (`MATRIX`), `crates/wc3-adrenotools/src/hooks.rs:521` (`TEST_LOCK`),
  `crates/wc3-audio/tests/no_alloc.rs`, `crates/wc3-data/tests/*`,
  `crates/wc3-mpq/tests/*`, `crates/wc3-game/tests/world_maps.rs`,
  `crates/wc3-assetdb/src/bin/audio_bench.rs`, `crates/wc3-mpq/examples/locale_audit.rs`.
  They exist only in test/bench binaries; a test that mutates a shared
  process-wide value (fonts, log sink) is serialised by the test harness or by a
  local mutex.

## Verification of the session fix

```
scripts/h2-cargo test -p wc3-re --lib local_player      # 1 passed
scripts/h2-cargo check -p wc3-game                      # rc=0
```

The seat-default test exercises the process-wide atomics directly and asserts
that a `World::default()` created after the reset answers seat 0 / no wildcard.
The call site is covered by the existing map-runtime tests, which boot real maps
through `attach_map_runtime`.
