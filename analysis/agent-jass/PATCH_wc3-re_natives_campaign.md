# Cross-owner patch: the 13 unimplemented campaign natives (2026-11-21)

Measured by the campaign-jass lane (`analysis/agent-jass/map_boot_matrix_20261121.json`,
`wc3re map-boot` rc=0): 44/44 maps boot, `main_ok=44`, 325 distinct natives called,
**13 unimplemented / 1360 calls**. `crates/wc3-re/src/mapboot.rs:383`
(`maps_jass_natives_cover_called_set`) is red on the same set.

Owner of the file: `crates/wc3-re/src/jass/natives.rs` (agent L lane). The
campaign-jass OWNER_PATHS are `crates/wc3-jass/**` and
`crates/wc3-game/src/campaign.rs`, so this lane cannot apply the patch; the exact
arms are below so it is mechanical.

## 1. Three arms that never match (typo, no space in the native name)

`crates/wc3-re/src/jass/natives.rs:2696-2698` matches on the **lowercased** name,
but three literals carry a space and/or CamelCase, so they can never fire:

```rust
| "setsky model" | ...            // never: the native is "setskymodel"
| "setunit CreepGuard"            // never: "setunitcreepguard"
| "define startlocation" | "defineStartLocation" => Value::Null,  // never: "definestartlocation"
```

Fix (drop the wrong spellings):

```rust
| "setskymodel" | "setwaterbasecolor" | "setwaterdeforms"
| "setterrainfog" | "setterrainfogex" | "setunitcreepguard"
| "definestartlocation" => Value::Null,
```

Effect: `definestartlocation` 257 calls disappear from the ledger (it is a
melee-lobby write; `Value::Null` is the correct no-op for a campaign map).

## 2. `IsQuestDiscovered` (6 boot calls, all in `Orc03`; static sites also Human03/Undead05)

`Quest.discovered` already exists (`vm.rs:456`) and `questsetdiscovered` already
writes it (`natives.rs:2264-2273`), so the getter is the missing half:

```rust
"isquestdiscovered" => {
    let discovered = a_handle(args, 0)
        .and_then(|handle| self.world.quests.get(&handle))
        .is_some_and(|quest| quest.discovered);
    Value::Bool(discovered)
}
```

## 3. `GetUnitRace` (16 boot calls, all in `Undead08`; static sites in Undead07 `Shield01/02` region filters)

Retail reads the race from unit data; the VM's `Unit` carries only its rawcode
(`Unit.id`), and Blizzard's rawcode convention encodes the race in the first
letter. `common.j:152-157` fixes the values: `RACE_HUMAN=1`, `RACE_ORC=2`,
`RACE_UNDEAD=3`, `RACE_NIGHTELF=4`, `RACE_DEMON=5`, `RACE_OTHER=7`
(`ConvertRace` is the identity in this VM, `natives.rs:125`).

```rust
"getunitrace" => {
    const RACE_HUMAN: i64 = 1;
    const RACE_ORC: i64 = 2;
    const RACE_UNDEAD: i64 = 3;
    const RACE_NIGHTELF: i64 = 4;
    const RACE_OTHER: i64 = 7;
    let race = a_handle(args, 0)
        .and_then(|handle| self.world.units.get(&handle))
        .map(|unit| match unit.id.chars().next().map(|c| c.to_ascii_lowercase()) {
            Some('h') => RACE_HUMAN,
            Some('o') => RACE_ORC,
            Some('u') => RACE_UNDEAD,
            Some('e') => RACE_NIGHTELF,
            Some('n') => RACE_OTHER,
            _ => RACE_OTHER,
        })
        .unwrap_or(0);
    Value::Int(race)
}
```

Why it changes the mission: `Undead07.j:3881` and `:4079` are
`return ( RACE_UNDEAD == GetUnitRace(GetEnteringUnit()) )` shield-region
filters, and `:3920`/`:4118` the leaving-unit counterpart. With the native
falling through, the default is a handle/0, so every shield trigger is inert.

## 4. Remaining 10 (lobby / selection / AI / buffs) — decide, do not guess

| native | calls | why it is not outcome-critical *today* |
|---|---|---|
| `setplayerracepreference` | 257 | melee lobby preference; campaign scripts set it for players who are already placed |
| `setplayerraceselectable` | 257 | same (lobby UI state) |
| `forceplayerstartlocation` | 250 | lobby start-location lock |
| `setstartlocprio` | 197 | lobby start-location priority |
| `setstartlocpriocount` | 94 | lobby start-location count |
| `pausecompai` | 15 | pauses a computer AI; this VM runs no AI, so there is nothing to pause (revisit when the AI lane lands) |
| `unitremovebuffs` | 6 | the VM has no buff subsystem; no state to remove |
| `groupenumunitsselected` | 2 | player-selection enumeration (no interactive selection in a headless boot) |
| `syncselections` | 2 | multiplayer selection sync |
| `getgametypeselected` | 1 | lobby game-type query (`Blizzard.j:6414`, `GetGameTypeSelected`) |

Either implement them as the no-op/typed-default arms they semantically are
(they are already no-ops through the fallthrough, only uncounted), or add them to
a dated allowlist in `mapboot::maps_jass_natives_cover_called_set`. Do not leave
the test red: an unimplemented native that is *not counted* is the exact failure
mode the campaign-jass gate exists to prevent.

## Verification after applying

```bash
scripts/h2-cargo run -q -p wc3-re --bin wc3re --offline -- map-boot \
  --json analysis/agent-jass/map_boot_matrix_20261121.json \
  --native-json analysis/agent-jass/native_coverage_20261121.json
scripts/h2-cargo test -q -p wc3-re --offline mapboot
scripts/h2-cargo test -q -p wc3-jass --offline campaign_boot
```

Expected: `unimplemented=0` once §1–§3 and the §4 decision are in
(`definestartlocation` 257 + `getunitrace` 16 + `isquestdiscovered` 6 = 279 of
1360 calls close by §1–§3 alone; the other 1081 are the lobby/AI/selection
family).
