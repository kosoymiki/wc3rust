# donorintake — original game extraction (ledger 386)

Analysis tool for the donor corpus and the operator's install. Detached cargo
package: it depends on the project crates by path and does **not** modify them
(`crates/` is READ-ONLY for this task's owner).

```text
donorintake maps index  [--root DIR]... [--json OUT] [--md OUT]
donorintake maps verify <map.w3m|w3x> [--json OUT] [--md OUT]
donorintake assets report [--json OUT] [--md OUT] [--archive MPQ]... [--loose DIR]...
```

## What each command proves

| command | evidence |
|---|---|
| `maps index` | every donor map opened through `wc3-re::mapfiles::MapBundle`: players from a full `war3map.w3i` reader (War3Net layout), melee flag, starts, mines, units, doodads/trees (shipped `DestructableData.slk` truth), parse errors, per-map sha256 |
| `maps verify` | maps one map against the **real runtime spawn path** (`wc3_game::sim_bridge::spawn_map_scene`): runtime start/mine/destructable counts, walkable cells from the applied WPM grid, creep camps, shops, and the exact ids the sim data tables drop |
| `assets report` | every used-set reference from the shipped tables (`wc3-data`, `wc3-audio`, `wc3-re`) resolved by archive hash lookup in `War3.mpq` + `War3x.mpq` + `War3xlocal.mpq`; have/total per family and the missing list |

Layout helpers worth knowing:

- `src/w3i.rs` — full map header (version, flags, players, forces). The
  `wc3-formats` view intentionally stops at the four localized strings.
- `src/gd.rs` — "does the shipped data define this id?" from
  `analysis/war3_data` + `analysis/war3x_data` SLK tables.
- `src/doodads.rs`, `src/units.rs` — labelled fallbacks for formats whose
  canonical parser is behind (v6 doodads, v7/sub10 units); the index reports
  which reader was used per map.
- `src/sources.rs` — archive/loose resource set with the completion rules the
  engine itself uses: missing `.mdx`, `.mdl`→`.mdx`, `numVar` `<base>N.mdx`
  variants, and path separator normalisation.

## Tests

```bash
CARGO_TARGET_DIR=~/src/herofolends-rs/target cargo test --offline
```

- `tests/map_corpus.rs` — 99/99 maps parse, player prefix agreement, Justice is
  the canonical 1v1, every melee map places a start and a mine per player.
- `tests/assets.rs` — the used-set asset gate. Needs root for the app's
  `War3.mpq` (`su -c '<target>/debug/deps/assets-*'`); prints `skip` otherwise.

## Artifacts

`analysis/agent-re/{map_index.json,MAP_INDEX.md,melee_verify_justice.json,MELEE_VERIFY_JUSTICE.md,original_assets.json,ORIGINAL_ASSETS.md,MAP_CORPUS.md}`
and `analysis/tech-lead/DONOR_INTAKE_FINDINGS.md`.
