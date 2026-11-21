# Orc01 (`Human04` too) runs as the melee arena — one-line patch for `game.rs`

Lane: 503 (campaign + JASS) · date 2026-11-18 · owner of `game.rs`: HUD / rename lane (497)

## Symptom (observed)

```
CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Orc01.w3m bytes=311106 units=42 quests=2 script=false
```

* the chapter loads from `War3.mpq` (correct), the quest panel and camera are
  empty, and the mission clock never advances: `war3map.j` was never booted;
* `Game::campaign_view().script_booted == false` → `Game::engine.is_none()`;
* the same happens for `Human04` (both are campaign chapters *and* arena stems).

## Mechanism (`file:line`)

| step | evidence |
| --- | --- |
| arena stems are matched by file name only | `crates/wc3-game/src/game.rs:1331` `const MELEE_MAP_STEMS: &[&str] = &["Human04", "Orc01", "Melee", "(2)Ponds_v1.4"];` |
| the stem rule is the first arm of `melee`, so it wins even for `\Maps\Campaign\` paths | `crates/wc3-game/src/game.rs:850-866` (`melee = …stem… \|\| (start_locations >= 2 && !map_path_is_campaign)`) |
| a melee game never attaches the script | `crates/wc3-game/src/game.rs:1003-1005` (`if !game.melee { game.attach_map_runtime(); }`) |
| `reset_scenario` then takes the arena branch, so no bridge and no script world | `crates/wc3-game/src/game.rs:1170-1176` |

## Patch (2 lines, `game.rs`)

```rust
        let melee = assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("map="))
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                Self::MELEE_MAP_STEMS
                    .iter()
                    .any(|candidate| stem.eq_ignore_ascii_case(candidate))
            })
+           // A chapter is never the arena: `Maps\Campaign\*` owns its script
+           // even when its stem is also in the arena list (Human04, Orc01).
+           && !map_path_is_campaign(&assets)
            || (assets.start_locations >= 2 && !map_path_is_campaign(&assets));
```

(`map_path_is_campaign(&assets)` is already defined in the same file, line 201.)

## Status: worked around in the campaign lane, patch still preferred

`Game::start_campaign_mission` now mounts the chapter's engine itself
(`campaign_engine`) and calls the public `Game::restart()`, so a chapter boots
even while this rule stands (Orc01: `CAMPAIGN_LOAD … script=true`,
`crates/wc3-game/tests/campaign.rs::campaign_orc01_from_mpq`). The patch is still
the right cleanup: it deletes the workaround, and it fixes `Human04` — the other
chapter whose stem is in the arena list — for every caller, not only for the
campaign loader.

## Verification after the patch

```
scripts/h2-cargo test -p wc3-game --test campaign campaign_orc01_from_mpq -- --nocapture
```

* `CAMPAIGN_LOAD … script=true`
* no `ORC01_SCRIPT_BLOCKED` line
* `ORC01_SCRIPT`, `ORC01_QUESTS`, `ORC01_CAMERA`, `ORC01_CINE`, `ORC01_NEXT`
  stay green (they already pass from the VM-side boot)
* `campaign_mission_load` (Human01) must stay green — Human01 is not an arena
  stem, so its path is unchanged.

## What already works without the patch (evidence)

`crates/wc3-re/tests/campaign_boot.rs` boots the same map straight from the
archive and drives it for 900 ticks:

```
ORC01_BOOT … functions=1216 triggers=118 actions=118 executed=4 sleeps=1 threads=1
ORC01_THREADS time=30.00 sleeps=17 wakes=16 parked=1 beats=48
ORC01_SPREAD first=Some(0.0) last=Some(26.13) spread=26.13
```

and `crates/wc3-game/tests/campaign.rs::campaign_orc01_from_mpq` judges that
world with the campaign layer's own types:

```
ORC01_OBJECTIVES n=2 required=2 titles=["Сбор Орды", "Защищайте Кэрна"]
ORC01_QUESTS n=2 titles=["Сбор Орды", "Защищайте Кэрна"]
ORC01_CAMERA keyframes=6 end=20.35 max_step=400.0
ORC01_CINE subtitles=6 speaker="Бугай" fades=11
ORC01_NEXT id=Orc02 index=25 cleared=true
```

So the only missing piece on the game path is the arena-stem gate above.
