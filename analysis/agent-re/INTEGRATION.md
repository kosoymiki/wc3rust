# INTEGRATION — `wc3-re` contracts for the ten layer owners

`crates/wc3-re` is now a member of the root workspace. Read-only consumers may
depend on it. Do not reimplement MPQ/BLP/MDX/SLK parsing in another crate; use
the typed APIs below so there is one canonical access layer.

```toml
# consumer Cargo.toml
wc3-re = { path = "../wc3-re" }
```

Build/test:

```bash
cd /data/data/com.termux/files/home/src/herofolends-rs
ulimit -v 4000000
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo check -p wc3-re --offline
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo test  -p wc3-re --offline
```

## 1. HUD / touch

- `wc3_re::icons::audit(&ObjectDb, &StormFs) -> IconAudit` — 1,182 verified
  command-button references; use `audit.sample` for the first card and
  `ObjectDb::icon_paths()` for the full list. `IconRef` carries
  `kind/object_id/field/path`.
- `wc3_re::blp::decode_first_mip(bytes) -> (BlpProbe, Vec<u8>)` — RGBA8 at
  64×64 for command buttons; scale in the HUD, do not resample the source.
- `wc3_re::skin::SkinTable::resolve(race, key)` for race UI textures/icons;
  `decorate()` implements FDF `DecorateFileNames`.
- `wc3_re::trigger::TriggerData` supplies all 676 GUI functions, argument
  types, defaults, limits and display layouts for a touch action editor.
- `wc3_re::jass::Runtime` exposes `world.context` (`GetTriggerPlayer/Unit`,
  chat string, enum unit) for contextual menus.

## 2. World / terrain

- `wc3_re::mapfiles::MapBundle::open(path)` → `terrain`, `info`, `pathing`,
  `objects`, `script`, `wts`, `entries`. Use `discover_maps(root)` for the 44
  campaign maps.
- `wc3_re::terrain::TerrainArt::load_dir` → tile records, `texture_path()`,
  `variant_uv(variation)` (2×2 atlas), cliff/water/weather rows.
- `wc3_re::blp::decode` for terrain atlases (512×512 BLP1).
- `wc3_formats::w3e::Terrain` is embedded in `MapBundle`; do not parse `w3e`
  twice.

## 3. Animation

- `wc3_re::mdx::summarize(bytes)` → counts (sequences, geosets, materials,
  textures, geoset animations, bones, attachments) and chunk list.
- `wc3_re::mdx::parse` returns `wc3_formats::mdx::MdxModel`; use
  `sequences` (`start..end` frames), `bones` (`Node` tracks), `geoset_animations`
  (alpha/colour), `materials[].layers[].texture_id`.
- `wc3_re::mdx::referenced_textures(model)` resolves layer texture ids to
  `TEXS` path + `replaceable_id`; `blp::decode` supplies RGBA.
- Replaceable ids 1 = team colour, 2 = team glow; higher ids need the renderer
  table (explicit gap — coordinate with `wc3-render`).

## 4. Data

- `wc3_re::objectdata::ObjectDb::load_dir(root)` → 1,665 objects (475 units,
  572 abilities, 133 items, 128 destructables, 278 doodads, 58 upgrades).
  `record(id)`, `field(id, name)`, `slk_field(table, column)`,
  `field_by_id(id, 4cc)` (UnitMetaData mapping), `icon_paths()`.
- `wc3_re::mapobjects::MapObjectData::parse` for `war3map.w3*`; two-table v1,
  NUL strings, sanity field. `ObjectDb::apply_map_objects` merges overrides.
- `wc3_re::slk::Sheet` for any additional SLK; `cell/record/row_index`.
- `wc3_re::txt::TextTable` for `*Func.txt`/`*Strings.txt`/`MiscData.txt`.
- `wc3_re::wts::Wts` for `TRIGSTR_n` resolution.

## 5. Simulation

- `wc3_re::jass::parse(source)` and `Runtime::new(program, seed)`:
  `run_function(name)`, `run_main()`. `Runtime::world` holds deterministic
  players/units/items/triggers/timers/groups/locations; `unimplemented` is the
  native census (never silent).
- `wc3_re::engine::Engine::boot(EngineConfig)` mirrors Game.dll boot; `tick(dt)`
  advances time/timers and fires zero-time initialization triggers; `enter_map`
  runs common.j + Blizzard.j + `war3map.j`.
- `Runtime::execute_trigger` and `world.context` are the integration point for
  a future event loop; trigger events are stored as `TriggerEvent`.
- RNG is deterministic but not the retail sequence (documented limitation).

## 6. UI framework

- `wc3_re::skin::SkinTable` (race → Default → literal) and
  `audit_fdf_decorate` for 71 FDF files.
- `wc3_formats::fdf::FdfDocument::parse_named` (access layer wrapper in
  `skin.rs`); props are lossless, `SetPoint` repeats preserved.
- `TriggerData` display layouts drive generated action text; `GuiFunction`
  carries `args/defaults/limits/category`.

## 7. Performance / assets

- `StormFs::open_wc3_install(root)` adds patch/expansion/base in priority order
  and a loose extracted tree; `read`/`info`/`listfile` are the only path APIs.
- `wc3_mpq::MpqArchive` is the canonical MPQ reader (header/hash/block,
  encryption, sector table, per-sector tags, PKWARE/Huffman/ADPCM). `StormFs`
  and `MapBundle` call it directly; `mpqex::MpqEx` is a compatibility facade
  only (`open/open_at/read/read_at/meta`), and
  `mpqex::{pkware,huffman,adpcm}` are type-identical re-exports of `wc3-mpq`.
- `blp::probe` is header-only; use it for atlases/manifests, `decode` for
  uploads. `mdx::summarize` avoids materializing tracks.
- Codec migration completed: no codec/reader copy remains in `wc3-re`; the
  parity gate is `tests/merge_parity.rs::mpqex_codecs_are_wc3_mpq_reexports`.

## 8. QA

- `wc3_re::scan::ScanReport::run(root, maps_root, seed)` → JSON/markdown with
  every count and failure list; `ScanReport::ok()` is the gate.
- Current gate: 3,935/3,935 BLP, 2,048/2,048 MDX, 33/33 SLK, 67/67 TXT,
  71/71 FDF, 44/44 map scripts, 91/91 AI, 44/44 maps, 676/676 TriggerData
  strings, 1,182/1,182 icons.
- `cargo test -p wc3-re` runs 33 tests, including whole-corpus parse and all
  44 maps' WTS/scripts; keep it green before merge.

## 9. Ownership / change protocol

- `crates/wc3-re/**` and `analysis/agent-re/**` are owned by the RE agent.
- Other crates stay read-only; propose API changes in
  `analysis/agent-re/INTEGRATION.md` or a follow-up task.
- Do not edit `wc3-formats`/`wc3-mpq` from this layer; route fixes through
  their owner. `wc3-re` consumes their public APIs and carries no fallback
  reader.
