# ART PUSH — Agent C: Assets / formats (ledger 587)

Task: `art-layer-C-assets` · Date: 2026-11-21 · Repo: `herofolends-rs` → symlink to shared `wc3rust` tree.

## Goal

Supply real, retail-verified BLP icon keys for ≥8 common unit defs and verify the
building-ghost alpha pipeline; expose a def→icon-key table that Agent B's
`assets.rs` can consume without any edits to `wc3-game/**` or `wc3-render/**`.

## Actions (OWNER paths only)

* `crates/wc3-assetdb/src/unit_icons.rs` (new): public `UnitIcon`,
  `UNIT_ICON_KEYS` (10 entries), `icon_key_for_def`, `blp_path_for_def`,
  `icon_key_from_path`, plus the ghost-alpha contract (`GHOST_ALPHA`,
  `MAGENTA_IS_TRANSPARENCY_KEY`, `AUTHORITATIVE_ALPHA_BITS`,
  `GhostTexture`, `GHOST_BUILDING_TEXTURES`, `alpha_is_authoritative`,
  `ghost_mesh_alpha`). Pure data + helpers; no game/UI/render imports.
* `crates/wc3-assetdb/src/lib.rs`: added `pub mod unit_icons;` (one line).
* `crates/wc3-assetdb/tests/art_c_icons.rs` (new): named tests
  `artC_icon_keys` and `artC_ghost_alpha`, with the decoded-retail catalog
  included at compile time as the oracle.
* `analysis/agent-assets/art-push-20261120/icon_keys_verified.tsv` (new):
  per-key provenance (object-data `Art=` field, sha256_16, alpha_bits, source).

No edits to `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-audio/**`,
`Cargo.toml` or `scripts/**`. Those files have other agents' uncommitted work
and were preserved.

## Evidence — exact def → icon key table (Agent B consume this)

Source of truth: extracted retail object data `src/wc3rust/analysis/war3_data`
(`[def] Art=…`); file existence in `analysis/war3_mpq_listing.txt`; full decode
in `analysis/agent-assets/texture-catalog.tsv` (3,935/3,935 BLP OK, prior
ledger 299 audit). All paths: `ReplaceableTextures\CommandButtons\<key>.blp`.

| def id | icon key (cache key) | BLP basename | alpha_bits | sha256_16 | object-data source |
|---|---|---|---:|---|---|
| `hfoo` | `BTNFootman` | `BTNFootman.blp` | 0 | `dd28007cb84bd5d1` | `[hfoo] Art=…` |
| `hpea` | `BTNPeasant` | `BTNPeasant.blp` | 0 | `912f02061d340a72` | `[hpea] Art=…` |
| `htow` | `BTNTownHall` | `BTNTownHall.blp` | 0 | `5db1d553c5d1bd41` | `[htow] Art=…` |
| `hkee` | `BTNKeep` | `BTNKeep.blp` | 0 | `e31cb4c019692f26` | `[hkee] Art=…` |
| `hcas` | `BTNCastle` | `BTNCastle.blp` | 0 | `b58c8b052d215335` | `[hcas] Art=…` |
| `opeo` | `BTNPeon` | `BTNPeon.blp` | 0 | `7e5977b7b910a31f` | `[opeo] Art=…` |
| `ogru` | `BTNGrunt` | `BTNGrunt.blp` | 0 | `549da6e31bc8605d` | `[ogru] Art=…` |
| `ogre` | `BTNGreatHall` | `BTNGreatHall.blp` | 0 | `342f472204c0d239` | `wc3-data coverage.rs:943` fixture; MPQ listing |
| `etol` | `BTNTreeOfLife` | `BTNTreeOfLife.blp` | 0 | `e131e7994f88b5b0` | `[etol] Art=…` |
| `unpl` | `BTNNecropolis` | `BTNNecropolis.blp` | 0 | `27fde3e8e6eceb21` | `[unpl] Art=…` |

**Agent B integration:** `use wc3_assetdb::unit_icons::{icon_key_for_def, blp_path_for_def, UNIT_ICON_KEYS};`
`icon_key_for_def("opeo") == Some("BTNPeon")`. These are the same basenames
`icon_name_for_def`/`unit_icon_name` produce from object data; the failure was
downstream (command-icon BLPs were never loaded into the texture cache), so
this table is the verified key set to seed, not a new naming scheme.

### Ghost alpha pipeline

`CONSOLE_UI_SPEC.md` §10.2 (citing `GITHUB_UI_RESEARCH.md` §3.6): a paletted BLP1
stores an alpha plane after the index plane; transparent texels may keep
magenta `#ff00ff` in RGB — **alpha is authoritative, never the colour key**.

| building BLP | alpha_bits | alpha_mean | decode |
|---|---:|---:|---|
| `Buildings\Human\TownHall\TownHallCastleKeep.blp` | 8 | 232 | ok |
| `Buildings\Orc\GreatHall\GreatHall.blp` | 8 | 229 | ok |

Both own a real 8-bit alpha plane with partial transparency (not a magenta
cutout). The ghost draw (`crates/wc3-game/src/game.rs:7285`, READ_ONLY) scales
real per-mesh/layer alpha by `ghost_alpha = 0.5`; `ghost_mesh_alpha(255) == 127`.
Command icons are `alpha_bits=0` opaque palettes — their magenta surround is a
colour key in a different (icon-atlas) path, not the building-ghost path.

### Verification (exact)

```
$ scripts/h2-cargo test -p wc3-assetdb -p wc3-formats --offline   # rc=0
test artC_ghost_alpha ... ok
test artC_icon_keys ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
Full log: `/data/data/com.termux/files/usr/tmp/opencode/artC-test-final.log`.
An earlier run hit rc=101 from a concurrent mid-edit `wc3-audio` E0308
(`game_audio.rs:543`, Agent E's OWNER path); it was corrected in-flight to
`Option<u32>` and the rerun is clean. Captured as evidence 1207.

## Result

Done. 10 verified def→icon keys exposed through a public assetdb API, ghost
alpha contract verified against the decoded-retail catalog, both named tests
rc=0. This is a **catalog / verification artifact, not hand-drawn art**; it
does not claim any Blizzard-level asset was authored here (HONEST ART).

## Risks / Next

* Agent B must still seed the command-icon BLPs into the texture cache; the key
  table only fixes the naming/resolver contract.
* `ogre` uses the `wc3-data` coverage fixture for its `Art=` field (the raw
  extracted block has no `Art=` line); BLP existence and decode are independently
  proven via MPQ listing + catalog.
* No retail MPQ needed at test runtime — the catalog is compile-time data.

## H2_PERSISTED

* capture **1207** (transient concurrent dependency compile error, exact rc/symptom),
  capture **1211** (delivered catalog + tests, rc=0 evidence).
* experience **4886** (retail MPQ as oracle; icon alpha_bits=0 vs ghost alpha_bits=8;
  compile-time catalog oracle for named tests; bounded retry on concurrent mid-edit).
* corpus preflight evidence `ev-078089152bc5f1bf` (`harness2_corpus_pack`).
* monitor ledger **587**: registered → 3 checkpoints → closed `state=completed`,
  `last_status=done`, `h2_ids_json={captures:[1207,1211],experiences:[4886]}`.
