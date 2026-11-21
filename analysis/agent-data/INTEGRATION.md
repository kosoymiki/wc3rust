# INTEGRATION — `wc3-data` ⇄ canonical resource access (`crates/wc3-re`)

Owner of this document: WC3 DATA layer (`analysis/agent-data`).
Consumer of: the canonical MPQ/BLP/MDX/script/table access layer owned by the RE
agent (`crates/wc3-re`, `analysis/agent-re/RESOURCE_ACCESS.md`).

`wc3-data` intentionally contains **no archive reader**. It parses bytes and
validates them; every byte comes from a `ResourceSource`. This file states the
exact contract the RE layer must satisfy so the two layers join without a second
raw reader.

## 1. The trait `wc3-data` needs

`Source` is the canonical name in `analysis/tech-lead/BASE.md`;
`ResourceSource` stays as a re-export alias for older call sites.

```rust
// crates/wc3-data/src/source.rs
pub trait Source {
    /// Read a resource addressed by in-game path (`Units\UnitData.slk`).
    fn read(&self, path: &str) -> Result<Vec<u8>>;      // Err(DataError::Missing) when absent
    fn exists(&self, path: &str) -> bool { self.read(path).is_ok() }
    /// Short description used in reports, e.g. `mpq:war3.mpq`.
    fn describe(&self) -> String;
}
```

Required semantics:

1. **Path form** — WC3 paths use `\`. `wc3-data` normalises `\`/`/` and never
   assumes a case (Android storage is case sensitive), so the implementation must
   do a case-insensitive lookup: `unitUI.slk`, `UNITUI.SLK` and `unitUI.SLK` must
   all resolve to the same entry.
2. **Layering order** — when several archives are open, later layers win:
   `war3.mpq` → `war3x.mpq` → `war3xlocal.mpq` (localised strings/art) → map.
   Patch archives (1.26/1.27) sit on top of the base archive. `exists` and `read`
   must agree on the same resolved winner.
3. **Errors** — an absent resource must be `DataError::Missing`
   (`wc3_data::DataError::Missing { path, root }`), never a panic and never an
   empty buffer. Corrupt archives may return any other `DataError`.
4. **Threading** — `GameData` is meant to be shared (`&GameData` from many game
   systems). The source itself is only used during load, so it does not have to be
   `Sync`; if it is, `GameData` stays `Sync` and can live in a `OnceLock`.
5. **No caching requirement** — `wc3-data` reads each file once per load.

Adapter sketch (RE layer side):

```rust
pub struct MpqSource { /* archives, listfile, hash table */ }

impl wc3_data::source::Source for MpqSource {
    fn read(&self, path: &str) -> wc3_data::Result<Vec<u8>> {
        let resolved = self.resolve(path)          // case-insensitive, layered
            .ok_or_else(|| wc3_data::DataError::Missing {
                path: path.to_string(),
                root: self.root_label(),           // e.g. "<mpq:war3.mpq>"
            })?;
        Ok(self.extract(&resolved)?)
    }
    fn describe(&self) -> String { format!("mpq:{}", self.layer_names().join("+")) }
}
```

## 2. Files `wc3-data` asks for

Exactly these names (all optional except the ten tables, which are reported in
`GameData::missing()` when absent):

| Kind | Paths |
|---|---|
| Tables | `Units\UnitData.slk`, `Units\UnitBalance.slk`, `Units\UnitWeapons.slk`, `Units\UnitAbilities.slk`, `Units\unitUI.slk`, `Units\AbilityData.slk`, `Units\ItemData.slk`, `Units\UpgradeData.slk`, `Units\DestructableData.slk`, `Doodads\Doodads.slk` |
| Metadata | `Units\UnitMetaData.slk`, `Units\AbilityMetaData.slk`, `Units\UpgradeMetaData.slk`, `Units\DestructableMetaData.slk`, `Units\DoodadMetaData.slk`, `Units\MiscMetaData.slk`, `Units\SkinMetaData.slk`, `Units\UpgradeEffectMetaData.slk`, `Units\AbilityBuffMetaData.slk` |
| Profile text | `Units\{UnitGlobal,Command,CommonAbility,CampaignUnit,CampaignAbility,CampaignUpgrade,HumanUnit,HumanAbility,HumanUpgrade,OrcUnit,OrcAbility,OrcUpgrade,UndeadUnit,UndeadAbility,UndeadUpgrade,NightElfUnit,NightElfAbility,NightElfUpgrade,NeutralUnit,NeutralAbility,NeutralUpgrade,Item,ItemAbility}{Strings,Func}.txt` |
| Misc/UI | `UI\MiscData.txt`, `UI\MiscUI.txt`, `UI\WorldEditData.txt`, `Units\MiscData.txt`, `UI\UnitEditorData.txt`, `UI\WorldEditStrings.txt` |
| Buffs | `Units\AbilityBuffData.slk` (not shipped by RoC/TFT retail — absence is normal and reported) |
| Map-local | `war3map.w3u/.w3a/.w3t/.w3q/.w3b/.w3d/.w3h`, `war3map.wts` (caller-supplied paths, not archive lookups) |

## 3. What the RE layer must expose beyond `read`

### 3.1 Listfile catalogue (path validation)

```rust
// crates/wc3-data/src/source.rs
impl Listfile {
    pub fn from_paths<I, S>(origin: impl Into<String>, paths: I) -> Self;  // preferred
    pub fn parse(origin: impl Into<String>, text: &str) -> Self;           // for `(listfile)` text
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self>;
    pub fn extend(&mut self, other: &Listfile);
}
```

Needs: the union of `(listfile)` entries from every opened archive, one path per
line, with the original case (the catalogue folds it itself). `wc3-data` uses it
to resolve model/icon paths (`GameData::catalog()`, `validate::PathStat`). If the
RE layer can enumerate archive entries directly, `Listfile::from_paths` accepts
that iterator — no text round-trip needed.

Measured baselines for the extracted RoC listfile (`analysis/list-War3-clean.txt`,
10 523 entries): `Units\*.mdx` = 411, `Buildings\*.mdx` = 131,
`ReplaceableTextures\CommandButtons\*.blp` = 691. These are the numbers the path
validator is expected to report when backed by `war3.mpq`.

### 3.2 Layered string resolution

`war3xlocal.mpq` carries the localised TFT `*Strings.txt` files. When the RE layer
exposes them as ordinary resources (`Units\HumanUnitStrings.txt` resolving through
the localised layer), `wc3-data` picks the names up automatically — the loader
iterates the profile list and takes the newest layer, no API change needed.

### 3.3 Map object data

Map files are read by the caller (the map loader already knows the map path) and
handed to:

```rust
data.apply_object_data("war3map.w3u", ObjectKind::Unit, &bytes)?;
data.set_wts(&wts_bytes)?;
```

No RE-layer change is required for this; it is listed so the map loader knows the
entry point.

## 4. Non-goals (kept in their own layers)

| Concern | Owner |
|---|---|
| MPQ/CASC container parsing, hash tables, decompression | `wc3-mpq` / RE layer |
| BLP decoding, MDX model parsing, JASS scripts, terrain (`w3e`/`wpm`) | `wc3-formats` + RE layer |
| Gameplay simulation, pathing, combat math beyond the table formulas | `wc3-sim` |
| Rendering, audio | `wc3-render`, `wc3-audio` |

## 5. Verification hook

When the RE adapter lands, this check must pass (it is the acceptance test for
both layers):

```rust
let source = MpqSource::open("War3.mpq")?;          // RE layer
let mut data = GameDatabase::load(&source)?;
data.set_catalog(Listfile::from_paths("war3.mpq", source.listfile_paths()));
let report = data.validate();
assert_eq!(report.error_count(), 0);
assert_eq!(report.catalog.unwrap().building_models, 131);
assert_eq!(report.path("unit-model").unwrap().missing, 0);
// One lookup path for the game:
let footman = data.object("hfoo").expect("hfoo");
assert!(footman.name.is_some() && footman.icon.is_some() && footman.stats.hp == Some(420));
```

## 5b. Composed path (no second read)

When the install database already parsed everything (`wc3-re::objectdata::ObjectDb`),
the game builds the typed view from those values:

```rust
let mut parts = wc3_data::DatabaseParts::default();
for (name, sheet) in &db.sheets {
    if let Some(kind) = wc3_data::table::TableKind::from_sheet_name(name) {
        parts.tables.push((kind, sheet.clone()));
    }
}
for text in db.texts() { parts.profiles.push((text.path.clone(), sections_of(text))); }
for (kind, table) in db.metas() { parts.meta.push((*kind, table.clone())); }
parts.profile_overlay = db.profile_overlay.clone();
parts.wts_entries = map.wts.as_ref().map(|wts| wts.entries.clone());
let typed = wc3_data::GameDatabase::from_parts(parts)?;
```

This is the path `wc3-game::re::ReLayer` uses (`analysis/tech-lead/merge-data.md`,
patch P3): one parse, one merge, one lookup path.

Everything `wc3-data` needs is already expressed by `ResourceSource`, `Listfile`
and `GameData::load`; no further coupling is planned.
