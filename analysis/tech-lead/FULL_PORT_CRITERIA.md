# FULL PORT — hard acceptance criteria (WC3 → Rust)

Nothing is "done" at an intermediate stage. This file defines what **10/10**
means for this port, derived from how established decompilation/source-port
projects define completeness, and enforced by `accept_full_port.sh` + corpus
gates. A partial stage is a failing state, not a milestone.

## Reference benchmarks (observed 2026-11-13)

| Project | What it defines as "complete" | Source |
|---|---|---|
| Ship of Harkinian (OoT) | Ships **no copyrighted assets**; the user supplies a supported copy of the game; the port runs that copy | `raw.githubusercontent.com/HarbourMasters/Shipwright/develop/README.md` (HTTP 200) |
| sm64 decomp | "**full decompilation** of Super Mario 64" for every listed release; assets are extracted from a prior copy of the game | `raw.githubusercontent.com/n64decomp/sm64/master/README.md` (HTTP 200) |
| OpenMW | Content criterion, not engine criterion: "The **main quests** in Morrowind, Tribunal and Bloodmoon are **all completable**" | `raw.githubusercontent.com/OpenMW/openmw/master/README.md` (HTTP 200) |

Method note: fetching was done with `curl --http1.1` as a bounded diagnostic
because the stealth-browser MCP server loses instances between calls on this
Termux build (`Instance not found` after spawn+navigate, reproduced 2026-11-13).
The same links remain the primary sources; no claim here depends on memory.

**Consequences for this project:** the port must (a) read the operator's own
MPQs at runtime and never redistribute assets, (b) cover the *whole* game — all
races, maps, abilities, items, heroes, cinematics — not a demo slice, and (c) be
judged by "can the content be played end to end", not by engine unit tests alone.

## Device-parity rule (added 2026-11-15)

A P0 row about what the operator **sees or hears** is PASS only when both hold:

1. its machine check exits 0, and
2. the `OPERATOR_BUGS.json` rows that define its acceptance are `verified`.

Unit tests alone are not acceptance for device behaviour: on 2026-11-14 the
old score marked UI/audio/render PASS while the device showed an empty army
panel, an invisible build ghost and a flat water band. The acceptance script
now enforces the mapping (`P0-10 -> #5,7,8,9,18`; `P0-11 -> #4,13`;
`P0-12 -> #10,11,12,14,15,16,17`) by reading the ledger, so a device regression
fails the P0 row and names the open row in the detail line.

Corollary: a real-map test that prints `skip:` proves nothing. The layer gate
(`bash analysis/tech-lead/run_layer_gates.sh`) marks a skipped real-map test as
SKIP, never PASS — `playability_real_map.rs` had been silently skipping because
its map path missed `Maps/Campaign/` until 2026-11-15.

## P0 criteria — all must PASS for 10/10

| # | Criterion | Machine check | 2026-11-13 | 2026-11-15 (cached) | Why it changed / what is missing |
|---:|---|---|---|---|---|
| P0-1 | Asset coverage 100%: every listfile file parsed, 0 read failures | `wc3re manifest` → `files=7451, read_fail=0` | **PASS** | **PASS** (`files=7451 read_failed=0`) | manifest artifact |
| P0-2 | Object data consumed: units/weapons/upgrades/buffs 100% typed and used by the sim | `mechanics_gap_map.json` rows for unit/weapon/upgrade tables | **PARTIAL** | **PASS** (all object rows PORTED) | gap map all-PORTED |
| P0-3 | Ability execution: every ability row is either executable or explicitly classified data-only | count executable vs 763 rows | **FAIL** (13/763) | **PASS** (343 executable, 420 data-only classified) | classification artifact |
| P0-4 | Map execution: **44/44** shipped maps boot (main + triggers) without crash | boot log per map | **FAIL** (2/44 verified) | **PASS** (44 booted, failed=[]) | `map_boot_matrix.json` |
| P0-5 | Melee completeness: 4 races playable, 1v1–4v4 vs AI, expansion mines, upkeep tax | gap map `economy.upkeep`, melee matrix test | **FAIL** | **PASS** (4 races, sizes 1–4, upkeep PORTED) | `melee_matrix.json` |
| P0-6 | Hero completeness: levels, skill points, attributes, items, drops, altar revive | gap map `heroes.*` all PORTED | **FAIL** | **PASS** (all PORTED) | gap map |
| P0-7 | Items/shops/drops: all 273 items usable, drop tables, restock | gap map `items.*` all PORTED | **FAIL** | **PASS** (all PORTED) | gap map |
| P0-8 | Save/load: full state round-trip, identical state hash | round-trip test | **FAIL** (absent) | **PASS** (cached `persistence.save` rc=0) | cached rc, not re-run today |
| P0-9 | Multiplayer lockstep: 2 clients, deterministic, no desync | lockstep test | **FAIL** (absent) | **PASS** (cached `net.multiplayer` rc=0) | cached rc, not re-run today |
| P0-10 | UI/input parity: A/M/S/H/P, control groups 1–9, card, tooltips, minimap, **icons/labels** | `hotkeys` + `hud::` + `OPERATOR_BUGS` rows 5,7,8,9,18 | **PASS** | **FAIL** | unit tests pass, device rows open: #7 army icons, #8 hero abilities, #9 labels, #18 truncation |
| P0-11 | Audio parity: positional SFX/music, `underruns=0`, **each unit its own voice set** | `GAME_AUDIO_STATS` + `OPERATOR_BUGS` rows 4,13 | **PASS** | **FAIL** | 63 samples underruns=0, but per-def voice capture missing (#4/#13) |
| P0-12 | Rendering parity: MDX/BLP/W3E/water/cliffs rendered; art labelled honestly | asset/terrain tests + `OPERATOR_BUGS` rows 10,11,12,14,15,16,17 | **PASS** | **FAIL** | ghost still invisible (#10), map-edge water flat (#14); 13 screenshots magenta=0 (#15 verified) |
| P0-13 | Determinism/replay: identical state hash across runs | `tests/determinism.rs` | **PASS** | **PASS** (cached `determinism` rc=0) | layer gate runs wc3-sim |
| P0-14 | Per-mechanic tests: every PORTED row has a named passing test | `run_mechanic_tests.sh --check` | **PASS** for ported rows | **PASS** (95 rows, 79 ported, 0 failing, 841 unmapped legacy reported) | audit artifact |

P0-14 is measured on gap-map rows only: every PORTED row must appear in
`mechanic_tests.json` with rc=0 and in `mechanic_targets.tsv`. Legacy tests that
predate the mechanic ids are listed by the TEST_POLICY audit as
`unmapped_legacy_tests` for transparency; they neither prove nor block a row.

Score 2026-11-15 (cached acceptance, `bash analysis/tech-lead/accept_full_port.sh`):
**11 / 14 PASS**. The project is therefore **not** accepted; the remaining 3 P0
items (P0-10 UI, P0-11 audio voices, P0-12 render) are device-parity rows whose
fixes are in the tree but whose operator evidence is still open in
`OPERATOR_BUGS.json`.`--run` re-executes the live test targets; the cached mode
reads `mechanic_tests.json` rc values and is labelled as such.

## Gates (must, corpus)

- `gate.port.full-parity-acceptance` — `accept_full_port.sh` must report 14/14;
  reporting an intermediate stage as done is a violation.
- `gate.port.content-coverage` — every map/race/ability/item has executable
  coverage or an explicit, counted data-only classification.
- `gate.port.mechanic-test-coverage` — every PORTED mechanic row resolves to a
  named test that passes in `mechanic_tests.json`.
- `gate.h2.agent-revival` — every idle/stopped delegated agent is revived with a
  THICK packet or closed with a report on every supervision sweep; silence is a
  violation.

## Acceptance run

```bash
bash analysis/tech-lead/accept_full_port.sh          # prints P0 table + score, rc=1 unless 14/14
bash analysis/tech-lead/run_mechanic_tests.sh --check
```

The script reads real artifacts only (`mechanics_gap_map.json`,
`mechanic_tests.json`, the manifest output, the map-boot log). No self-reported
status is accepted: a criterion flips to PASS only when its command exits 0.
