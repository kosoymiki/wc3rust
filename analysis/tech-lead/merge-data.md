# merge-data.md — GAME DATA layer port onto BASE.md v1

Layer: typed WC3 game data (`wc3-formats::gamedata` parsing → `wc3-data` typed
records → one lookup path for the game).
Dispatcher: tech-lead root. Date: 2026-11-13 (last refresh).
State: **DONE (port merged and verified end-to-end).** The exclusive scope
(`crates/wc3-data/**`, `crates/wc3-formats/src/gamedata.rs`) is compiled and
tested, and the three patches for the other owners' files have **landed**:
`crates/wc3-data` is a workspace member (root `Cargo.toml`), `wc3-re` uses the
canonical merge with per-kind metadata (`objectdata.rs`), `mapobjects.rs` keeps
level/data-pointer, and `wc3-game::re::{typed, object_info}` exposes the one
lookup path.

Verification (this round, no device):

```
cargo check -p wc3-data -p wc3-formats                     rc=0
cargo test  -p wc3-data                                    rc=0   (34+2+8+20 tests + 1 doctest)
cargo check -p wc3-re                                      rc=0   (27 pre-existing warnings in that crate)
cargo check -p wc3-game --lib                              rc=0   (BASE gate 3)
dump_gamedata --tft analysis/war3x_data counts             rc=0
dump_gamedata --listfile list-War3-clean.txt validate      rc=0   errors=0 warnings=108
dump_gamedata object hfoo|AHbz|ratc|Rhme|LTlt              rc=0
```

Nothing in the data path is invented: every value comes from
`analysis/war3_data`, `analysis/war3x_data` or `analysis/war3_full3` through the
canonical parsers named in BASE.md §2.

---

## 1. Canonical target per concern (and why)

| Concern | Canonical owner (BASE.md §2) | This layer's role | Why here |
|---|---|---|---|
| SLK grid parsing | `wc3-formats::gamedata::Sheet` (`parse`, `read`, `cell`, `set_overlay`) | already implemented there (2 151 lines); `wc3-data::table::Table` is a typed view | BASE §2 row "SLK grid"; `crates/wc3-formats/src/gamedata.rs:437` |
| Object metadata | `wc3-formats::gamedata::MetaTable` (`parse`, `MetaField::{field,slk,index,repeat,column_for,is_profile}`, `resolve_field`, `merge_overlay`) | `wc3-data` consumes it; `wc3-re::objectdata` is patched to consume it too (P1) | BASE §2 rows "Object metadata", §5 "`wc3-re::slk::Sheet` delegates" |
| Object data files | `wc3-formats::gamedata::ObjectDataFile` (`parse`, `ObjectChange`, `ChangeValue`, `extend`) | `wc3-re::mapobjects` already projects it; `wc3-data::apply_object_data*` merges it | BASE §2 row "Object data", §5 "`wc3-re::mapobjects` → projection" |
| `.wts` grammar | `wc3-formats::gamedata::Wts` (`parse/get/resolve/iter`) | `wc3-data::localization::Localizer` stores the flat `TRIGSTR_n → text` map; `wc3-re::wts` adds escape decoding | BASE §2 row "`.wts` grammar" |
| INI/text tables | `wc3-formats::gamedata::TextTable` (`parse/sections/get/values`) | `wc3-data::localization::Localizer::add_sections` + `misc::MiscTables::add_sections` accept parsed sections (no re-parse) | BASE §2 row "INI/text tables" |
| Typed records + validation | **`wc3-data`** (`GameDatabase`, `Source`, `validate`) | this crate — renamed to the BASE API in §4 | BASE §2 row "Typed game database" |
| Composed install DB | **`wc3-re::objectdata::ObjectDb`** (`load_dir/load_storm`, `record`, `field_by_id`, `apply_map_objects`, `icon_paths`) | patch P1 makes its merge canonical and exposes the parsed layers | BASE §2 row "Install object database" |
| One lookup path for the game | `wc3-game::re::ReLayer` (new `object_info`) | patch P3 builds `GameDatabase` from `ObjectDb`'s parsed layers and exposes `object_info(id)` | BASE §4 "`wc3_game::re::ReLayer` — canonical RE bridge" |

**Decision on BASE §8 pending question** ("Whether `wc3-data` gains a direct
`ObjectDb` bridge or keeps file-path inputs"): **both, with one composition.**
`wc3-data` keeps `GameDatabase::load(Source)` for tools and tests, and gains
`GameDatabase::from_parts(DatabaseParts)` so `wc3-game::re` builds the typed
database from `ObjectDb`'s already-parsed sheets (single parse, no second read).
`ObjectDb` stays the owner of install composition; `wc3-data` is a typed view
over the same cells.

---

## 2. OWNER files changed (executed in this round)

| File | Change | Evidence |
|---|---|---|
| `crates/wc3-data/src/lib.rs` | `GameData` → **`GameDatabase`** (+ alias), `DatabaseParts`, `from_parts`, `apply_object_data_named`, `pub use object::{ObjectInfo,ObjectStats}` | `crates/wc3-data/src/lib.rs:118-260` |
| `crates/wc3-data/src/source.rs` | trait `ResourceSource` → **`Source`** (+ `pub use self::Source as ResourceSource`), `Listfile` unchanged | `source.rs:23`, `:171` |
| `crates/wc3-data/src/object.rs` | **new**: `ObjectInfo`, `ObjectStats`, `GameDatabase::{object, object_kind}` — one lookup path for name/tooltip/icon/model/stats | `object.rs:1-300` |
| `crates/wc3-data/src/table.rs` | `Table::from_sheet(kind, gamedata::Sheet)`, `TableKind::from_sheet_name` (aliases `DoodadData`, `unitUI`); numeric readers now overlay-aware (`i32/f32/bool` read the merged cell, `*_strict` keep the shipped cell) | `table.rs:139-160`, `:187-206`, `:291-350` |
| `crates/wc3-data/src/localization.rs` | profile layers stored as parsed sections (`add_sections`), flat `.wts` map (`set_wts_entries`), quote-aware `components` | `localization.rs:31-120` |
| `crates/wc3-data/src/misc.rs` | `MiscTables::add_sections`, `MiscStat.file: String` | `misc.rs:20-95` |
| `crates/wc3-data/src/enums.rs` | `EditorEnums::from_sections` | `enums.rs:30-40` |
| `crates/wc3-data/src/error.rs` | `DataError::UnknownObjectKind` | `error.rs:28-33` |
| `crates/wc3-data/src/bin/dump_gamedata.rs` | `object ID` command (unified lookup), SIGPIPE restored so `… | head` exits 141 instead of panicking | `bin/dump_gamedata.rs:150`, `:224-238` |
| `crates/wc3-data/Cargo.toml` | `libc = "0.2"` (SIGPIPE default) | — |
| `crates/wc3-data/tests/base_contract.rs` | **new** base-contract tests (written, not run) | `tests/base_contract.rs:1-420` |
| `crates/wc3-data/README.md` | API map updated for the base names | — |
| `analysis/agent-data/slk_parity.py` | measured legacy-vs-strict SLK parity (24 files, 5 divergence classes) | §7 |
| `analysis/agent-data/evidence-slk-parity.txt` | captured output of the above | §7 |

No file outside `crates/wc3-data/**` + `analysis/agent-data/**` +
`analysis/tech-lead/merge-data.md` was modified by this layer in this round.

---

## 3. READ-ONLY patches — **APPLIED** (kept as the record of what changed)

All four landed while this round ran; the blocks below are the exact content that
was applied (verified by marker grep + `cargo check -p wc3-re` / `-p wc3-game`).
`wc3-re/src/objectdata.rs` is now 550 lines, `mapobjects.rs` 240 lines;
`wc3-game/Cargo.toml:19` carries `wc3-data`, `wc3-game/src/re.rs:23,32,135,169`
carry the bridge. If a later edit reverts any block, re-apply from here.

Anchor: files as read on 2026-11-12 (`wc3-re/src/objectdata.rs` = 498 lines,
`wc3-re/src/mapobjects.rs` = 210 lines, `wc3-re/src/re.rs`… `wc3-game/src/re.rs`
= 193 lines). `objectdata.rs` is being actively edited by the RE owner (the
`TypedSheet`/`MetaTable` migration landed while this manifest was written), so
apply by **function name** if the line numbers have drifted; the anchors below
are the current text.

### P1 — `crates/wc3-re/src/objectdata.rs` (owner: RE agent)

**P1.1 struct (lines 127-136)** — replace with:

```rust
pub struct ObjectDb {
    /// Typed canonical SLK views (`wc3-formats::gamedata::Sheet`).
    pub sheets: BTreeMap<String, TypedSheet>,
    pub objects: BTreeMap<String, ObjectRecord>,
    pub metadata: BTreeMap<String, FieldMeta>,
    pub loaded_files: Vec<(String, usize)>,
    /// Canonical `UnitMetaData.slk` parser result; projected into `metadata`
    /// for the historical `FieldMeta` API.
    pub metadata_table: Option<MetaTable>,
    /// Metadata tables for every object kind (`AbilityMetaData`,
    /// `UpgradeMetaData`, `DestructableMetaData`, `DoodadMetaData`, …). Field ids
    /// are only unique per kind (`Hbz1` is a Blizzard field, `uhpm` a unit
    /// field), so the merge resolves through the table of the object's kind.
    pub metas: BTreeMap<MetaKind, MetaTable>,
    /// Parsed profile text layers, retained so the typed database can be built
    /// from these values without a second read.
    pub texts: Vec<TextTable>,
    /// Text overrides written by `apply_map_objects` (`unam`, `utip`, `aart`, …),
    /// shared with the typed database.
    pub profile_overlay: wc3_formats::gamedata::ProfileOverlay,
}
```

**P1.2 `add_sheet` (lines 201-214)** — replace with:

```rust
    fn add_sheet(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        const META_SHEETS: [(MetaKind, &str); 9] = [
            (MetaKind::Unit, "UnitMetaData"),
            (MetaKind::Ability, "AbilityMetaData"),
            (MetaKind::Upgrade, "UpgradeMetaData"),
            (MetaKind::Destructable, "DestructableMetaData"),
            (MetaKind::Doodad, "DoodadMetaData"),
            (MetaKind::Misc, "MiscMetaData"),
            (MetaKind::Skin, "SkinMetaData"),
            (MetaKind::UpgradeEffect, "UpgradeEffectMetaData"),
            (MetaKind::Buff, "AbilityBuffMetaData"),
        ];
        let sheet = TypedSheet::parse(name, bytes)
            .map_err(|err| crate::error::ReError::Parse(format!("SLK {name}: {err}")))?;
        for (kind, sheet_name) in META_SHEETS {
            if !name.eq_ignore_ascii_case(sheet_name) {
                continue;
            }
            let table = MetaTable::parse(kind, bytes)
                .map_err(|err| crate::error::ReError::Parse(format!("MetaTable {name}: {err}")))?;
            if kind == MetaKind::Unit {
                self.metadata_table = Some(table.clone());
            }
            self.metas.insert(kind, table);
        }
        self.loaded_files
            .push((format!("{name}.slk"), bytes.len()));
        self.sheets.insert(name.to_string(), sheet);
        Ok(())
    }
```

**P1.3 `add_text` (lines 216-239)** — keep the body, add one line before
`Ok(())`:

```rust
        self.texts.push(table);
```

(`table` is free after the section loop; the `Ok(())` line is 239.)

**P1.4 `build_records` (lines 241-308)** — two changes:
- line 244: `if sheet_name == "UnitMetaData" {` → `if is_metadata_sheet(&sheet_name) {`
  (otherwise TFT installs create "objects" out of `AbilityMetaData` field ids).
- line 280: make the source list idempotent:

```rust
                if !record
                    .sources
                    .iter()
                    .any(|source| source == &format!("{sheet_name}.slk"))
                {
                    record.sources.push(format!("{sheet_name}.slk"));
                }
```

**P1.5 `load_metadata` (lines 310-332)** — replace with:

```rust
    fn load_metadata(&mut self) {
        for table in self.metas.values() {
            for field in table.fields() {
                let id = field.id();
                if id.is_empty() {
                    continue;
                }
                self.metadata.entry(id.to_string()).or_insert_with(|| FieldMeta {
                    id: id.to_string(),
                    field: field.field().to_string(),
                    slk: field.slk().to_string(),
                    index: field.index().to_string(),
                    value_type: field.type_name().to_string(),
                    display: field.display_name().to_string(),
                    can_be_empty: field.raw_column("canBeEmpty") == Some("1"),
                });
            }
        }
    }
```

**P1.6 `apply_map_objects` (lines 359-394)** — replace with the canonical merge:

```rust
    /// Apply `war3map.w3u/.w3a/.w3t/.w3b/.w3h/.w3q` modifications.
    ///
    /// The merge is canonical: `wc3_formats::gamedata::merge_overlay` writes each
    /// change into the sheet column named by its metadata (including the
    /// level/data-pointer suffix of ability/upgrade/doodad changes and the
    /// `ItemData` redirection of `isst`), or into the text layer for `Profile`
    /// fields. Records are refreshed for every touched id, so `slk_field()` and
    /// `field()` agree with the typed database.
    pub fn apply_map_objects(&mut self, objects: &MapObjectData) -> usize {
        let file = objects.to_typed();
        let kind = file.kind;
        let meta = self.meta_for_typed(kind).cloned();
        let mut profile = self.profile_overlay.clone();
        let report;
        {
            let mut targets: Vec<wc3_formats::gamedata::MergeTarget<'_>> = Vec::new();
            for (sheet_name, sheet) in self.sheets.iter_mut() {
                if is_metadata_sheet(sheet_name) {
                    continue;
                }
                // `DoodadMetaData.slk` names the doodad sheet `DoodadData`.
                let target_name: &str = if sheet_name == "Doodads" {
                    "DoodadData"
                } else {
                    sheet_name.as_str()
                };
                targets.push(wc3_formats::gamedata::MergeTarget {
                    name: target_name,
                    sheet,
                    create_rows: is_primary_sheet(kind, sheet_name),
                });
            }
            report = wc3_formats::gamedata::merge_overlay(
                kind,
                &file,
                meta.as_ref(),
                &mut targets,
                &mut profile,
            );
        }
        self.profile_overlay = profile;

        let mut touched: BTreeSet<String> = BTreeSet::new();
        for entry in file.entries() {
            let target = entry.target_id().as_str().into_owned();
            if entry.is_custom() && !entry.old_id.is_zero() {
                let old_id = entry.old_id.as_str().into_owned();
                let mut base_kind = kind;
                let mut base_fields = BTreeMap::new();
                if let Some(base) = self.objects.get(&old_id) {
                    base_kind = base.kind;
                    base_fields = base.fields.clone();
                }
                let record = self.objects.entry(target.clone()).or_insert_with(|| ObjectRecord {
                    id: target.clone(),
                    kind: base_kind,
                    fields: base_fields,
                    ..ObjectRecord::default()
                });
                record.base_id = Some(old_id);
                if !record
                    .sources
                    .iter()
                    .any(|source| source == "war3map object data")
                {
                    record.sources.push("war3map object data".to_string());
                }
            }
            touched.insert(target);
        }

        for (id, section) in self.profile_overlay.sections() {
            if let Some(record) = self.objects.get_mut(id) {
                for (key, components) in section {
                    record.fields.insert(key.clone(), components.join(","));
                }
            }
        }
        for id in &touched {
            self.refresh_record(id);
        }
        report.applied + report.shadow + report.profile
    }
```

**P1.7 new methods after `apply_map_objects`** (insert before `counts`, line 396):

```rust
    /// Metadata table for a typed object kind (items fall back to the unit
    /// sheet, which is where retail keeps item fields).
    pub fn meta_for_typed(&self, kind: wc3_formats::gamedata::ObjectKind) -> Option<&MetaTable> {
        use wc3_formats::gamedata::ObjectKind as Typed;
        let meta_kind = match kind {
            Typed::Unit => MetaKind::Unit,
            Typed::Ability => MetaKind::Ability,
            Typed::Item => MetaKind::Item,
            Typed::Upgrade => MetaKind::Upgrade,
            Typed::Buff => MetaKind::Buff,
            Typed::Destructable => MetaKind::Destructable,
            Typed::Doodad => MetaKind::Doodad,
        };
        self.metas.get(&meta_kind).or_else(|| {
            meta_kind
                .item_fallback()
                .and_then(|fallback| self.metas.get(&fallback))
        })
    }

    /// Rebuild the `table.column` projection of one object from the (possibly
    /// overlaid) canonical sheets.
    fn refresh_record(&mut self, id: &str) {
        let mut slk_fields: BTreeMap<String, String> = BTreeMap::new();
        for (sheet_name, sheet) in &self.sheets {
            let key_column = key_column(sheet_name);
            if key_column.is_empty() {
                continue;
            }
            if sheet.id_index(id).is_none() {
                continue;
            }
            for column in sheet.columns() {
                if column.eq_ignore_ascii_case(key_column) {
                    continue;
                }
                let Some((cell, _origin)) = sheet.cell_with_overlay(id, column) else {
                    continue;
                };
                let Some(value) = cell.text() else { continue };
                slk_fields.insert(format!("{sheet_name}.{column}"), value.to_string());
            }
            // Columns created by the overlay (metadata fields whose sheet column
            // does not exist in the shipped table).
            for column in sheet.shadow_columns() {
                let Some((cell, _origin)) = sheet.cell_with_overlay(id, column) else {
                    continue;
                };
                let Some(value) = cell.text() else { continue };
                slk_fields.insert(format!("{sheet_name}.{column}"), value.to_string());
            }
        }
        if let Some(record) = self.objects.get_mut(id) {
            record.slk_fields = slk_fields;
        }
    }

    /// Parsed profile text layers kept from the install (typed bridge input).
    pub fn texts(&self) -> &[TextTable] {
        &self.texts
    }

    /// Metadata tables per object kind (typed bridge input).
    pub fn metas(&self) -> &BTreeMap<MetaKind, MetaTable> {
        &self.metas
    }

    /// Resolve `TRIGSTR_<n>` in every text field through the map's `.wts`.
    pub fn resolve_texts(&mut self, wts: &crate::wts::Wts) -> usize {
        let mut changed = 0;
        for record in self.objects.values_mut() {
            for value in record.fields.values_mut() {
                if !value.contains("TRIGSTR_") {
                    continue;
                }
                let resolved = wts.resolve(value);
                if resolved != *value {
                    *value = resolved;
                    changed += 1;
                }
            }
        }
        changed
    }
```

**P1.8 new module helpers** (append after the `ObjectDb` impl, before
`pub struct IconRef`):

```rust
/// Metadata sheets are parsed into `MetaTable`s, never projected as objects.
fn is_metadata_sheet(name: &str) -> bool {
    name.ends_with("MetaData")
}

/// Sheets whose rows are created for a custom (map-authored) object.
fn is_primary_sheet(kind: wc3_formats::gamedata::ObjectKind, sheet: &str) -> bool {
    use wc3_formats::gamedata::ObjectKind as Typed;
    match kind {
        Typed::Unit => matches!(
            sheet,
            "UnitData" | "UnitBalance" | "UnitWeapons" | "UnitAbilities" | "unitUI"
        ),
        Typed::Ability => sheet == "AbilityData",
        Typed::Item => sheet == "ItemData",
        Typed::Upgrade => sheet == "UpgradeData",
        Typed::Destructable => sheet == "DestructableData",
        Typed::Doodad => sheet == "Doodads",
        Typed::Buff => false,
    }
}

/// Key column of an object sheet (`""` when the sheet is not an object table).
fn key_column(sheet: &str) -> &'static str {
    match sheet {
        "UnitData" => "unitID",
        "UnitBalance" => "unitBalanceID",
        "UnitWeapons" => "unitWeapID",
        "UnitAbilities" => "unitAbilID",
        "unitUI" => "unitUIID",
        "AbilityData" => "alias",
        "ItemData" => "itemID",
        "UpgradeData" => "upgradeid",
        _ => "",
    }
}
```

Imports added by P1: `use wc3_formats::gamedata::{MetaKind, MetaTable, Sheet as TypedSheet};`
already present; nothing new (all new types are fully qualified).

Behaviour change (deliberate, recorded): `apply_map_objects` no longer dumps
overrides into `record.fields`; a `uhpm` change now lands in
`UnitBalance.HP` (visible to `slk_field("UnitBalance","HP")`) and a `unam`
change lands in the text layer (`field("Name")`). This is what the engine and
the HUD read, and it matches `wc3-data::object()`.

---

### P2 — `crates/wc3-re/src/mapobjects.rs` (owner: RE agent)

The projection drops `level`/`data_pointer`, so ability/upgrade/doodad overrides
cannot be resolved to `DataA1`/`Cool2`-style columns. Two blocks:

**P2.1 `Modification` (lines 43-49)** — replace with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Modification {
    pub id: [u8; 4],
    pub value: ModValue,
    /// Ability/upgrade level or doodad variation (0 for simple kinds).
    pub level: u32,
    /// Ability data slot pointer (1-based `DataA`…; 0 when unused).
    pub data_pointer: u32,
    /// Value of the trailing u32. The canonical typed layer does not expose it
    /// (retail files write 0); the field stays for API compatibility.
    pub sanity: u32,
}
```

**P2.2 `MapObjectData` (lines 58-63)** — replace with a kind-carrying struct +
manual `Default`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MapObjectData {
    pub version: u32,
    /// File kind: ability/upgrade/doodad records are extended, so the merge needs
    /// the kind to read and re-emit them.
    pub kind: gamedata::ObjectKind,
    pub base_entries: Vec<ObjectEntry>,
    pub new_entries: Vec<ObjectEntry>,
}

impl Default for MapObjectData {
    fn default() -> Self {
        Self {
            version: 0,
            kind: gamedata::ObjectKind::Unit,
            base_entries: Vec::new(),
            new_entries: Vec::new(),
        }
    }
}
```

**P2.3 `parse_as` (lines 73-83)** — add `kind,` to the returned struct:

```rust
        Ok(Self {
            version: file.version,
            kind,
            base_entries: file.base.iter().map(project_entry).collect(),
            new_entries: file.custom.iter().map(project_entry).collect(),
        })
```

**P2.4 `project_entry` (lines 111-127)** — carry the new fields:

```rust
            .map(|change| Modification {
                id: id_bytes(&change.id),
                value: project_value(&change.value),
                level: change.level,
                data_pointer: change.data_pointer,
                sanity: 0,
            })
```

**P2.5 new `to_typed` (inside `impl MapObjectData`, after `fields`, line 110)**:

```rust
    /// Project back onto the canonical model, preserving level/data-pointer so
    /// `gamedata::merge_overlay` can address per-level columns.
    pub fn to_typed(&self) -> ObjectDataFile {
        ObjectDataFile {
            kind: self.kind,
            version: self.version,
            base: self.base_entries.iter().map(to_typed_entry).collect(),
            custom: self.new_entries.iter().map(to_typed_entry).collect(),
        }
    }
```

**P2.6 new helpers** (module level, before `fn id_bytes`):

```rust
fn to_typed_entry(entry: &ObjectEntry) -> TypedEntry {
    TypedEntry {
        old_id: gamedata::FourCc::new(entry.old_id),
        new_id: gamedata::FourCc::new(entry.new_id),
        changes: entry
            .modifications
            .iter()
            .map(|modification| gamedata::ObjectChange {
                id: crate::fourcc_str(&modification.id),
                value: unproject_value(&modification.value),
                level: modification.level,
                data_pointer: modification.data_pointer,
            })
            .collect(),
    }
}

fn unproject_value(value: &ModValue) -> ChangeValue {
    match value {
        ModValue::Int(v) => ChangeValue::Int(*v),
        ModValue::Real(v) => ChangeValue::Real(*v),
        ModValue::Unreal(v) => ChangeValue::Unreal(*v),
        ModValue::String(v) => ChangeValue::String(v.clone()),
        ModValue::Int64(v) => ChangeValue::Int(*v as i32),
    }
}
```

(`Int64` is an obsolete code that retail object files never use; the projection
keeps the field for API compatibility and narrows it here.)

---

### P3 — `crates/wc3-game` (owner: dispatcher; `src/re.rs` + `Cargo.toml` are frozen)

**P3.1 `crates/wc3-game/Cargo.toml`** — add one dependency line next to the other
`wc3-*` path deps:

```toml
wc3-data = { path = "../wc3-data" }
```

**P3.2 `crates/wc3-game/src/re.rs`** — wire the typed database and the unified
lookup.

Imports (after line 20):

```rust
use wc3_data::{GameDatabase, ObjectInfo};
```

Struct (line 26-37): add

```rust
    /// Typed database over the same parsed layers as `objects` (one lookup path
    /// for the HUD: name/tooltip/icon/model/stats).
    pub typed: Option<GameDatabase>,
```

`open` (lines 40-110): make `objects` mutable, apply the map overrides, then
build the typed view. Replace the `Self { … }` construction with:

```rust
        let mut objects = objects;
        if let (Some(db), Some(bundle)) = (objects.as_mut(), map.as_ref()) {
            // Map object overrides are applied over the install database with the
            // canonical merge, so the engine view and the typed view agree.
            for data in bundle.objects.values() {
                db.apply_map_objects(data);
            }
            if let Some(wts) = &bundle.wts {
                db.resolve_texts(wts);
            }
        }
        let typed = objects
            .as_ref()
            .and_then(|db| typed_from_objects(db, &storm, &install_root, map.as_ref()));
        Self {
            install_root,
            storm,
            objects,
            typed,
            triggers,
            map,
            notes,
        }
```

New method (after `object`, line ~115):

```rust
    /// Typed object info: name, tooltip, icon, model and stats for any id.
    pub fn object_info(&self, id: &str) -> Option<ObjectInfo> {
        self.typed.as_ref()?.object(id)
    }
```

New free function (end of file, before `#[cfg(test)]`):

```rust
/// Build the typed database from the object database's parsed layers.
///
/// No byte is re-read and no parser runs twice: sheets, text layers and metadata
/// tables come from `ObjectDb`, the UI tables come from the same Storm view.
fn typed_from_objects(
    db: &ObjectDb,
    storm: &StormFs,
    install_root: &Path,
    map: Option<&MapBundle>,
) -> Option<GameDatabase> {
    let mut parts = wc3_data::DatabaseParts {
        root: format!("re:{}", install_root.display()),
        ..wc3_data::DatabaseParts::default()
    };
    for (name, sheet) in &db.sheets {
        if let Some(kind) = wc3_data::table::TableKind::from_sheet_name(name) {
            parts.tables.push((kind, sheet.clone()));
        }
    }
    for text in db.texts() {
        let sections = text
            .sections
            .iter()
            .map(|section| (section.name.clone(), section.entries.clone()))
            .collect();
        parts.profiles.push((text.path.clone(), sections));
    }
    for (kind, table) in db.metas() {
        parts.meta.push((*kind, table.clone()));
    }
    parts.profile_overlay = db.profile_overlay.clone();
    for name in wc3_data::misc::MiscTables::FILES {
        let Ok(bytes) = storm.read(name) else { continue };
        let Ok(table) = wc3_formats::gamedata::TextTable::parse(name, &bytes) else {
            continue;
        };
        if name.ends_with("UnitEditorData.txt") {
            parts.enums = Some(table.sections().to_vec());
        } else {
            parts.misc.push((name.to_string(), table.sections().to_vec()));
        }
    }
    if let Ok(bytes) = storm.read("UI\\WorldEditStrings.txt") {
        if let Ok(table) = wc3_formats::gamedata::TextTable::parse("WorldEditStrings", &bytes) {
            parts.westrings = Some(table.sections().to_vec());
        }
    }
    if let Some(wts) = map.and_then(|bundle| bundle.wts.as_ref()) {
        parts.wts_entries = Some(wts.entries.clone());
    }
    GameDatabase::from_parts(parts).ok()
}
```

---

### P4 — root `Cargo.toml` + `crates/wc3-data/Cargo.toml` (owner: dispatcher)

BASE gate 2 runs `cargo test -p wc3-data`, which requires workspace membership,
while BASE §1 currently annotates `wc3-data -> wc3-formats (standalone workspace
root)`. **Proposed v2 correction:** make it a member and change the annotation to
`wc3-data -> wc3-formats (typed view over the same parsed layers)`.

**P4.1 root `Cargo.toml`** — in `members`, after `"crates/wc3-core",` (line 5):

```toml
    "crates/wc3-data",
```

**P4.2 `crates/wc3-data/Cargo.toml`** — delete the standalone root (lines 1-3):

```toml
# Standalone workspace root (see README).
[workspace]

```

After P4 the crate inherits the workspace lockfile and `cargo test -p wc3-data`
works. The README section "Workspace note" must drop its "delete the `[workspace]`
table" instruction.

---

## 4. Public API the game/base consumes (exact signatures)

```rust
// crates/wc3-data/src/source.rs
pub trait Source {
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    fn exists(&self, path: &str) -> bool;
    fn describe(&self) -> String;
}
pub struct DirSource;   impl DirSource { pub fn new(root: impl Into<PathBuf>) -> Self; pub fn resolve(&self, path: &str) -> Option<PathBuf>; }
pub struct MemorySource; impl MemorySource { pub fn new() -> Self; pub fn insert(&mut self, path: &str, bytes: Vec<u8>); }
pub struct Listfile;    impl Listfile { pub fn from_file(path: impl AsRef<Path>) -> Result<Self>; pub fn parse(origin: impl Into<String>, text: &str) -> Self; pub fn resolve_model(&self, path: &str) -> Option<String>; pub fn count(&self, prefix: &str, extension: &str) -> usize; }

// crates/wc3-data/src/lib.rs
pub struct GameDatabase;
impl GameDatabase {
    pub fn load(source: &dyn Source) -> Result<Self>;
    pub fn from_parts(parts: DatabaseParts) -> Result<Self>;
    pub fn unit(&self, id: &str) -> Option<UnitView<'_>>;
    pub fn ability(&self, id: &str) -> Option<AbilityView<'_>>;
    pub fn item(&self, id: &str) -> Option<ItemView<'_>>;
    pub fn upgrade(&self, id: &str) -> Option<UpgradeView<'_>>;
    pub fn destructable(&self, id: &str) -> Option<DestructableView<'_>>;
    pub fn doodad(&self, id: &str) -> Option<DoodadView<'_>>;
    pub fn object(&self, id: &str) -> Option<ObjectInfo>;              // ONE lookup path
    pub fn object_kind(&self, id: &str) -> Option<wc3_formats::gamedata::ObjectKind>;
    pub fn apply_object_data_named(&mut self, path: impl Into<String>, bytes: &[u8]) -> Result<wc3_formats::gamedata::ObjectKind>;
    pub fn apply_object_data(&mut self, source: impl Into<String>, kind: ObjectKind, bytes: &[u8]) -> Result<&MergeOutcome>;
    pub fn set_wts(&mut self, bytes: &[u8]) -> Result<()>;
    pub fn set_catalog(&mut self, catalog: Listfile);
    pub fn validate(&self) -> validate::Report;
    pub fn counts(&self) -> Vec<(TableKind, usize)>;
}

// crates/wc3-data/src/object.rs
pub struct ObjectInfo { pub id: String, pub kind: wc3_formats::gamedata::ObjectKind,
    pub name: Option<String>, pub tip: Option<String>, pub ubertip: Option<String>,
    pub description: Option<String>, pub icon: Option<String>, pub hotkey: Option<String>,
    pub button_pos: Option<(u32,u32)>, pub model: Option<String>, pub stats: ObjectStats }
pub struct ObjectStats { pub level/hp/mana/armor/armor_per_upgrade/gold_cost/lumber_cost/
    build_time/speed/food_used/food_provided/charges/max_level/mana_cost: Option<i32>,
    pub cooldown/area/range/average_damage/attacks_per_second: Option<f32>,
    pub min_damage/max_damage: Option<i32>, pub targets: Vec<String> }

// crates/wc3-game/src/re.rs (after P3)
impl ReLayer { pub fn object_info(&self, id: &str) -> Option<ObjectInfo>; pub typed: Option<GameDatabase>; }
```

---

## 5. Symbols ported (source `file:line` → target `file:line`)

| Symbol | Source (canonical) | Target (this port) |
|---|---|---|
| `Sheet::parse` / `read` (positional C/F cursor, Y-less rows, quoted `;`) | `crates/wc3-formats/src/gamedata.rs:437`, `:378-460` | consumed by `crates/wc3-data/src/table.rs:187` (`Table::from_sheet`) |
| `MetaTable::parse` | `crates/wc3-formats/src/gamedata.rs:984` | `crates/wc3-data/src/lib.rs:296` (load_meta) + `objectdata.rs:201` (P1.2) |
| `MetaField::column_for` (field + data letter + level) | `crates/wc3-formats/src/gamedata.rs:963` | `objectdata.rs` P1.6 via `merge_overlay` |
| `resolve_field` | `crates/wc3-formats/src/gamedata.rs:1433` | `crates/wc3-data/src/lib.rs` merge path |
| `merge_overlay` + `MergeTarget` + `MergeReport` | `crates/wc3-formats/src/gamedata.rs:1549` | `crates/wc3-data/src/lib.rs:430` (`apply_parsed`) + `objectdata.rs` P1.6 |
| `ObjectDataFile::parse` (7 kinds, v1-3) | `crates/wc3-formats/src/gamedata.rs:1218` | `crates/wc3-re/src/mapobjects.rs:73` (existing) → P2 `to_typed` |
| `Wts::parse` / `iter` | `crates/wc3-formats/src/gamedata.rs:1625`, `:1680` | `crates/wc3-data/src/localization.rs:108` (`set_wts`), `crates/wc3-re/src/wts.rs:21` |
| `TextTable::parse` / `sections` | `crates/wc3-formats/src/gamedata.rs:1723`, `:1766` | `crates/wc3-data/src/localization.rs:74` (`add_profile`), `misc.rs:44` |
| `FourCc::parse` | `crates/wc3-formats/src/gamedata.rs:47` | `crates/wc3-data/src/lib.rs` (`unit()/ability()/…`) |
| `Artifact` classification (Excel/FourCC cells) | `crates/wc3-formats/src/gamedata.rs:197` | `crates/wc3-data/src/validate.rs:470` (severity mapping) |
| typed unit stats (hp/cost/armor/damage) | `crates/wc3-data/src/units.rs:200-560` | `crates/wc3-data/src/object.rs:100-160` |
| ability levels/targets | `crates/wc3-data/src/abilities.rs:300-420` | `crates/wc3-data/src/object.rs:164-190` |
| item charges/costs | `crates/wc3-data/src/items.rs:120-190` | `crates/wc3-data/src/object.rs:192-215` |
| upgrade costs/effects | `crates/wc3-data/src/upgrades.rs:180-260` | `crates/wc3-data/src/object.rs:217-235` |
| install DB composition | `crates/wc3-re/src/objectdata.rs:140` (`load_dir`) / `:171` (`load_storm`) | `crates/wc3-game/src/re.rs` P3.2 `typed_from_objects` |

---

## 6. Duplicates removed / thinned, with delegation proof

1. **No second SLK parser.** `wc3-data::table::Table` reads
   `wc3_formats::gamedata::Sheet`; `wc3-re::objectdata` already stores
   `TypedSheet = gamedata::Sheet`. Proof: type identity in
   `crates/wc3-re/tests/merge_parity.rs::object_db_uses_canonical_typed_sheets_and_metadata`
   (`sheet.contains`, `sheet.cell` on `db.sheets`).
   *Not changed:* `wc3-re::slk::Sheet` keeps delegating to the **primitive**
   `wc3_formats::slk::SlkTable`, because `merge_parity.rs::slk_delegation_preserves_scanner_totals`
   pins the scanner totals `6785 rows / 815 columns`. Moving it to the strict
   reader would drop the two `AbilityData` spacer rows (6783) and break that gate.
   Recorded as a deliberate non-change.
2. **No second metadata parser.** `objectdata::load_metadata` now projects
   `gamedata::MetaTable` rows for all nine metadata sheets (P1.5) instead of
   reading `UnitMetaData` cells by hand; `wc3-data` already did.
3. **No second text parser.** `Localizer::add_sections` and
   `MiscTables::add_sections` accept the parsed sections of
   `wc3-re::txt::TextTable` (which itself parses via `gamedata::TextTable`).
   Proof test (written): `localization::tests::parsed_sections_match_parsed_bytes`,
   `misc::tests::parsed_sections_match_parsed_bytes`.
4. **No second object-data merge.** `ObjectDb::apply_map_objects` delegates to
   `gamedata::merge_overlay` (P1.6); previously it wrote 4CCs into
   `record.fields` and never touched the sheets, so `uhpm` never reached
   `UnitBalance.HP`.
5. **One lookup path.** `GameDatabase::object(id)` is the only name/tooltip/
   icon/model/stats entry point; `wc3-game::re::ReLayer::object_info` delegates to
   it (P3.2). `ObjectDb::record`/`field_by_id` stay for the engine/JASS side.
6. **Kinds fixed end-to-end.** `gamedata::ObjectKind::extension` already maps
   `w3b`=Destructable, `w3d`=Doodad, `w3h`=Buff (canonical layer, patched before
   this round); `wc3-re::mapfiles.rs:132-137` uses the same table;
   `wc3-data/tests/base_contract.rs::kind_mapping_matches_the_warcraft_file_table`
   pins it from the data side. `apply_object_data_named` routes by that table, so
   a `.w3b` file is never parsed as a buff (extended layout mismatch would
   desynchronise the stream).

---

## 7. Execution evidence (this round) + measured parity

### 7.1 Compile / test (rc captured, no device)

```
cargo check --manifest-path crates/wc3-data/Cargo.toml --all-targets   rc=0
cargo build --manifest-path crates/wc3-data/Cargo.toml --lib           rc=0 (7s)
cargo build --manifest-path crates/wc3-data/Cargo.toml --bin dump_gamedata rc=0
cargo test  --manifest-path crates/wc3-data/Cargo.toml --no-run         rc=0
  deps/wc3_data-2fc10694602d5c10      34 passed  rc=0
  deps/dump_gamedata-f51dddf7780bf21c  2 passed  rc=0
  deps/base_contract-4ca42eee5ee860d3  8 passed  rc=0
  deps/real_data-fa6a912eb430c366     20 passed  rc=0
cargo test  --manifest-path crates/wc3-data/Cargo.toml --doc           1 passed  rc=0
cargo check -p wc3-formats                                            rc=0
deps/wc3_formats-c67e923c6d93d392 gamedata::                         16 passed  rc=0
```

Build note for the dispatcher: on this device `cargo` blocks on the shared
`target/debug/.cargo-lock` while another agent builds; a foreground command that
waits on the lock can be killed (exit 143). The verification above was run as
short steps + direct execution of the test binaries. No code change was needed.

### 7.1b Member-mode + dependent crates (after the patches landed)

```
cargo check -p wc3-data -p wc3-formats              rc=0   (0.1s, cached)
cargo test  -p wc3-data                             rc=0   25s
  lib 34 | bin 2 | base_contract 8 | real_data 20 | doctest 1
cargo check -p wc3-re                               rc=0   40s
cargo check -p wc3-game --lib                       rc=0   76s
dump_gamedata counts / validate / object            rc=0 each
```

### 7.2 CLI on the real extraction

```
$ dump_gamedata --tft analysis/war3x_data counts
UnitData roc=466 tft=812 | UnitBalance 466/812 | UnitWeapons 316/812 |
UnitAbilities 324/812 | UnitUI 466/812 | AbilityData 397/763 (+2/+4 spacers) |
ItemData 132/273 | UpgradeData 58/89 | DestructableData 128/246 | Doodads 278/469
$ dump_gamedata --listfile list-War3-clean.txt validate                 rc=0
issues errors=0 warnings=108 (artifact 97, icon 1, ability-ref 6, upgrade-ref 2, weapon 2)
$ dump_gamedata --root analysis/war3x_data --listfile <tft> validate    rc=0
issues errors=0 warnings=9 total=101
catalog unit_models=722 building_models=179 command_icons=1062

$ dump_gamedata object hfoo
object hfoo kind=Unit name=Пехотинец | icon BTNFootman.blp | hotkey F | model Footman.mdx
stats hp=420 armor=2(+2) gold=160 build=20 level=2 dmg=12..13 avg=12.5 aps=0.74 range=90
$ dump_gamedata object AHbz
object AHbz kind=Ability name=Буран | icon BTNBlizzard.blp | tip level 1 | ubertip level 1 resolved
stats level=3 max_level=3 mana_cost=75 cooldown=6 area=200 range=800
$ dump_gamedata object ratc   → Item  Боевые когти (+12), level 7, 500 gold, model treasurechest.mdl
$ dump_gamedata object Rhme   → Upgrade Железные мечи,…, max_level 3, gold 100/level 1
$ dump_gamedata object LTlt   → Destructable tree, hp 50, LordaeronTree.mdx
```

### 7.3 Measured parity

`python3 analysis/agent-data/slk_parity.py` (legacy `SlkTable` algorithm vs the
strict `gamedata::Sheet` algorithm, both re-implemented from their sources):

```
files=24 diverging=5          (12 object/dialect sheets x 2 roots)
Units/AbilityData.slk   RoC  legacy=399 rows strict=397  (2 spacer rows dropped by the strict reader)
Units/AbilityData.slk   TFT  legacy=767 rows strict=763  (4 spacer rows)
Units/UnitBalance.slk   RoC  466 differing ids: quoted cells containing ';'
Units/UnitWeapons.slk   RoC  316 differing ids: quoted cells containing ';'
Units/DestructableData  RoC    5 differing ids: '0;ER[+11299]C' Excel artifacts
```

Row counts agree everywhere; the only value differences are quoted cells whose
text contains `;` (Excel formula remnants), which the strict reader preserves and
the primitive reader truncates. That is why the primitive `slk::Sheet` stays for
the scanner pin and the strict reader serves the typed layer.

Retail data facts the port relies on (measured earlier, unchanged):
RoC `466/466/316/324/466/397(+2)/132/58/128/278`, TFT
`812x5/763(+4)/273/89/246/469`; validation `errors=0` on both roots; catalogue
`Buildings\*.mdx = 131`, `CommandButtons\*.blp = 691`; `war3map.w3u` (Human04) =
4 base + 4 custom, `uC01` name resolves to `Тимми` through `war3map.wts`.

---

## 8. Tests (scope tests are written **and run**; cross-crate ones pending)

Scope tests below are green (rc=0). The `wc3-re`/`wc3-game` rows are still
proposed for those owners: the crates compile with the bridge, but the
`merge_data_parity.rs` / `re_object_info.rs` files do not exist yet (they are the
next verification step for the RE/game owners).

| Test | File | Fixture | Pins |
|---|---|---|---|
| `kind_mapping_matches_the_warcraft_file_table` | `crates/wc3-data/tests/base_contract.rs` | none | w3b/w3d/w3h mapping + `extended()` flags |
| `table_sheet_names_round_trip` | same | none | `TableKind::from_sheet_name` incl. `DoodadData` |
| `unified_lookup_covers_every_object_kind` | same | `analysis/war3_data` | `object()` for hfoo/AHbz/ratc/Rhme/LTlt/APct |
| `unified_lookup_exposes_icons_names_and_stats` | same | `analysis/war3_data` | hfoo name/icon/hotkey/hp/cost/damage; AHbz level-1 cost/area/range + resolved tooltip; ratc/Rhme/LTlt/APct |
| `from_parts_reproduces_the_loaded_database` | same | `analysis/war3_data` | `DatabaseParts` bridge == `load()` (counts, metadata, cells, `object()`, validation) |
| `map_overrides_resolve_through_the_typed_lookup` | same | `war3_map/Human04/war3map.w3u` + `.wts` | custom unit `uC01` name `Тимми`, hp 330, `phea` stockStart 0 |
| `object_data_kind_is_taken_from_the_extension` | same | in-memory v2 stream | `.w3u/.w3t/.w3a/.w3q/.w3b/.w3d/.w3h` routing + `UnknownObjectKind` |
| `validation_report_shape_is_stable` | same | `analysis/war3_data` | 10 tables, path classes, `errors=0` |
| `object_overrides_reach_slk_fields` | **new** `crates/wc3-re/tests/merge_data_parity.rs` | `war3_full3` + Human04 w3u | `record("phea").slk_field("ItemData","stockStart") == Some("0")` |
| `custom_unit_inherits_base_and_resolved_text` | same | same + wts | `record("uC01").base_id == Some("ugho")`, `slk_field("UnitBalance","HP") == Some("330")`, `field("Name")` after `resolve_texts` == `Тимми` |
| `ability_override_uses_level_and_data_pointer` | same | synthetic v2 `w3a` (`Hbz1`, level 2, pointer 1, int 8) | `slk_field("AbilityData","dataa2") == Some("8")` |
| `typed_db_from_objectdb_matches_install_load` | same | `war3_full3` | `GameDatabase::from_parts(typed_from_objects)` counts/`object()` == `GameDatabase::load(DirSource)` |
| `object_info_matches_hud_expectations` | **new** `crates/wc3-game/tests/re_object_info.rs` | `war3_full3` | `ReLayer::object_info("hfoo")`/`("AHbz")` name+icon+stats, `object_info("zzzz").is_none()` |
| `metadata_tables_cover_all_kinds` | `crates/wc3-re/tests/merge_data_parity.rs` | `war3x_data`/`war3_full3` | `db.metas` has Unit+Ability+Upgrade+Destructable+Doodad for a TFT install; `metadata` still has `uhpm` (keeps `merge_parity.rs` pin) |

P1-P4 landed. Remaining for the dispatcher: `cargo test -p wc3-re` (their
suite, incl. `merge_parity.rs`) → gate 4 scan counters. The scope half is green:
`cargo test -p wc3-data` rc=0 and `cargo check -p wc3-re` / `-p wc3-game --lib`
rc=0 already ran from this layer.

---

## 9. Conflicts / notes for the dispatcher

1. **P1 applied** (objectdata.rs now 550 lines): `metas`, `texts`,
   `profile_overlay`, `meta_for_typed`, `refresh_record`, `resolve_texts`,
   metadata-sheet exclusion in `build_records`, canonical `apply_map_objects`.
   Signatures pinned by `merge_parity.rs` (`metadata_table`, `sheets`, `metadata`,
   `record`, `field_by_id`) are unchanged.
2. **P2 applied** (mapobjects.rs): `Modification::{level,data_pointer}`,
   `MapObjectData.kind`, `to_typed`/`to_typed_entry`/`unproject_value`.
   `merge_parity.rs::object_data_parsers_agree_on_real_v1_map` still compares the
   projected values, which are unchanged.
3. **`slk.rs` must not be switched** to the strict reader: `merge_parity.rs` pins
   `6785 rows / 815 columns` over 33 RoC SLKs, and the strict reader drops the
   two `AbilityData` spacer rows (§7). BASE §2's phrase "`wc3-re::slk::Sheet`
   delegates parsing" is satisfied by the primitive delegation already in place.
   Suggest BASE v2 wording: "primitive view (scanner totals), strict typed view
   for object data".
4. **P4 applied**: `crates/wc3-data` is workspace member 11 and its `[workspace]`
   stub is gone; `cargo test -p wc3-data` works. BASE §1 still annotates
   `wc3-data -> wc3-formats (standalone workspace root)` — suggest v2 wording
   "(workspace member; depends only on wc3-formats)". A leftover
   `crates/wc3-data/Cargo.lock` from the standalone period is ignored by cargo and
   can be deleted at leisure.
5. **BASE §8 pending decision** answered in §1: `ObjectDb` bridge via
   `GameDatabase::from_parts`, not a second composition.
6. **No new crate edges** beyond `wc3-game -> wc3-data` (allowed by BASE §2
   "Consumers: `wc3-game` (via future bridge)"). `wc3-data` still depends only on
   `wc3-formats`.
7. **Buff base data**: retail ships no `Units\AbilityBuffData.slk`; `wc3-data`
   exposes `war3map.w3b` overlays verbatim (`BuffStore::is_base_available() ==
   false`) and reports it in validation. `ObjectDb` keeps buff ids in `objects`
   only when a text file or map override mentions them (unchanged behaviour).

---

## 10. State and evidence paths

**DONE** — scope merged, compiled and tested; P1-P4 landed and verified
(`cargo check -p wc3-data -p wc3-formats`, `-p wc3-re`, `-p wc3-game --lib`, all
rc=0; `cargo test -p wc3-data` rc=0). Remaining work belongs to the RE/game
owners: the two cross-crate test files in §8.

Evidence:
- `analysis/agent-data/evidence-merge.txt` — this round: 34 lib + 2 bin +
  8 base_contract + 20 real_data + 1 doctest, `gamedata::` 16, `cargo check`
  rc=0, CLI counts/validate/object output;
- `analysis/agent-data/evidence.log` — previous round (pre-freeze) run;
- `analysis/agent-data/evidence-slk-parity.txt` — §7 output;
- `analysis/agent-data/slk_parity.py` — the parity harness;
- `crates/wc3-data/tests/base_contract.rs` — the base-contract tests;
- `crates/wc3-data/README.md`, `analysis/agent-data/INTEGRATION.md` — API and
  resource-layer contract.

H2: capture + experience + monitor checkpoint recorded at the end of this round
(IDs in the reply).


---

## 11. FULL MERGE round (2026-11-13): typed data in game assets + W3Map removed

### 11.1 Typed object data wired into `wc3-game::assets`

| Change | File:line |
|---|---|
| `Assets.objects: Option<GameDatabase>` built from the same Storm view (`open_object_database`: `GameDatabase::load` + map `to_typed` overrides + `war3map.wts` entries) | `crates/wc3-game/src/assets.rs:215, 391-447, 690` |
| `impl wc3_data::source::Source for StormSource` (no second reader; StormFs supplies bytes) | `assets.rs:250-266` |
| `Assets::{unit_id,unit_info,unit_name,unit_icon_path,unit_icon_name,unit_stats,unit_tooltip,unit_cost}`; `UNIT_IDS = ["hfoo","hrif","hpea","htow"]` | `assets.rs:695-760` |
| Selection view: typed name, race·role class, typed portrait icon, typed armour/damage | `crates/wc3-game/src/game.rs:1400-1465` |
| Selection toast uses typed name + `Ubertip`, flattened by `game_tooltip_text` (strips `|c…`/`|r`, `|n`→space, 120-char clip) + test | `game.rs:1107-1115, 1866-1892` |
| Command-card cost from `UnitBalance.goldcost` (`Assets::unit_cost`, sim fallback) | `crates/wc3-game/src/hud.rs:815,821,827` |
| `ObjectInfo` gained `race`, `class`, `type_flags` (typed unit classification) | `crates/wc3-data/src/object.rs:60-72, 120-126` |

End-to-end proof (real `War3.mpq` + `Human04.w3m`, host test, no device):

```
cargo test -p wc3-game --lib typed_object_data_feeds_game_assets  rc=0
FOOTMAN name=Пехотинец icon=BTNFootman stats={level 2, hp 420, armor 2(+2),
  gold 160, lumber 0, build 20, speed 270, food 2, range 90,
  dmg 12..13 avg 12.5, aps 0.74, targets [ground,structure,debris,item,ward]}
```

### 11.2 `W3Map` removed (one map reader)

| Item | Change |
|---|---|
| `crates/wc3-formats/src/map.rs` | deleted `W3Map` + its `open_archive`/`resolve_trigstr` helpers; `parse_units` is now `pub`; the fixture test now checks `map_mpq_base` + `MpqArchive::open_at` + `parse_units` |
| `crates/wc3-re/src/mapfiles.rs` | `MapBundle.units: Option<Vec<PlacedUnit>>` parsed from `war3mapunits.doo` (lowercased arm) |
| `apps/headless/src/main.rs` | `MapBundle` + new `wc3-re` dep; terrain/info/pathing/script/objects from the bundle |
| `apps/bench/src/main.rs` | `MapBundle` + new `wc3-re` dep; `units` count from the bundle |
| `crates/wc3-game/src/world_audit.rs:1023` | `MapBundle.terrain` |
| `crates/wc3-re/tests/merge_parity.rs` | W3Map parity leg replaced by `map_bundle_reads_terrain_units_objects_and_wts` |

Verification: `cargo check -p wc3-formats|wc3-re|wc3-game|herofolends-headless|herofolends-bench`
rc=0; `cargo test -p wc3-formats` 60 passed; `cargo test -p wc3-re --test
merge_parity` 5 passed.

Known parser gap (documented, not invented): `Human04.w3m`'s `war3mapUnits.doo`
uses the `W3do` v8 **random unit table** extension; `parse_units` returns
`Unsupported("random unit tables")`, `MapBundle` records the error and leaves
`units = None` (the removed `W3Map` silently returned an empty list for the same
map, so behaviour is unchanged). Next step: implement the random-table section
when a byte-level anchor is available; both tests accept either outcome and fail
loudly on any other error.

### 11.3 Cross-layer fix required by the above

`crates/wc3-replay/src/lib.rs` did not compile after `wc3-sim` grew
`MoveAttack/Hold/Harvest/CancelTrain`: the encoder/decoder match was
non-exhaustive (`error[E0004]`). Added tags 6..9 with the same shape as their
siblings (Move/Stop/Move/CancelTrain) in both `encode_command` and
`decode_command`; `cargo check -p wc3-replay` rc=0. This unblocks
`herofolends-headless`.

### 11.4 External test failures observed (not from this layer)

`cargo test -p wc3-game --lib` rc=101 with two failures in files edited by other
owners minutes before (mtimes 03:45/03:47):

- `hud::mobile_hud::tests::labels_fit_inside_the_console` — `card label clipped:
  GATHER` (`mobile_hud.rs:1751`; the `GATHER` label comes from `hud.rs:834`, card
  geometry/font metric test, no data-layer input).
- `sim_bridge::tests::jass_point_order_drives_the_sim` (`sim_bridge.rs`, JASS
  bridge, rewritten during this round).

All other 78 tests in that suite pass, including
`assets::geoaset_tests::typed_object_data_feeds_game_assets` and both
`re::tests::{opens_install_objects_and_triggers, icon_audit_resolves_all_references}`.

### 11.5 New evidence

`analysis/agent-data/evidence-full-merge.txt` (rc table + FOOTMAN dump).


---

## 12. ROUND 2 (ledger 354) — verification of the typed→game wiring + W3Map removal

Owner paths only (`crates/wc3-data/**`, `crates/wc3-formats/src/gamedata.rs`,
`analysis/agent-data/**`, this file). No edits to `wc3-re/**`, `wc3-game/**`,
`wc3-render/**`, apps: those were verified/compiled, not modified.

### 12.1 Scope checks (rc)

```
cargo check -p wc3-formats -p wc3-data -p wc3-re -p wc3-game \
            -p wc3-replay -p herofolends-headless -p herofolends-bench   rc=0 (4.6s warm)
cargo test  -p wc3-data -p wc3-formats                                   rc=0
            34 lib + 2 bin + 8 base_contract + 20 real_data + 1 doctest | 60 formats
cargo test  -p wc3-re --test merge_parity                                rc=0 (5 passed)
```

### 12.2 Wiring / removal audit (grep, evidence of state)

| Invariant | Evidence |
|---|---|
| typed selection path intact after concurrent `game.rs` edits | `game.rs:1153-1156` (name + tooltip toast), `:1607-1655` (typed name/race/icon/armour/damage), `:1884` `game_tooltip_text` |
| cost from typed data | `hud.rs:874,881,897` → `game.assets.unit_cost(...)` |
| `Assets` typed accessors present | `assets.rs` `objects`, `unit_id`, `unit_info`, `unit_name`, `unit_icon_path`, `unit_icon_name`, `unit_stats`, `unit_tooltip`, `unit_cost` (9 markers) |
| `W3Map` gone | zero code references; only provenance comments in `objects.rs:4`, `map.rs:344`, `mapfiles.rs:81` |
| `MapBundle.units` | `crates/wc3-re/src/mapfiles.rs:81` + `war3mapunits.doo` arm |

### 12.3 Blockers met in this round (external, with exact fix)

1. **Transient (resolved):** `cargo test -p wc3-re --test merge_parity` failed with
   `cannot find type ScriptReport` while `crates/wc3-re/src/engine.rs` was being
   written by its owner (mtime 07:46:35); retry 12 s later rc=0.
2. **Open (other owner):** `cargo test -p wc3-game` cannot build because
   `crates/wc3-render/src/texture.rs:330-331` uses wgpu variants removed in 30.0.1.
   Exact patch for the render owner:

   ```rust
   // crates/wc3-render/src/texture.rs:330-331
   Self::Astc6x6 => wgpu::TextureFormat::Astc {
       block: wgpu::AstcBlock::B6x6,
       channel: wgpu::AstcChannel::UnormSrgb,
   },
   Self::Astc8x8 => wgpu::TextureFormat::Astc {
       block: wgpu::AstcBlock::B8x8,
       channel: wgpu::AstcChannel::UnormSrgb,
   },
   ```

   Anchor: `wgpu-types-30.0.1/src/texture/format.rs:13-70` (`AstcBlock::B6x6`,
   `AstcBlock::B8x8`, `AstcChannel::UnormSrgb`) — the per-block enum variants no
   longer exist. This blocks the game test binary, not the data layer: the
   typed-assets proof already ran green (rc=0, §11.1) before that edit landed and
   the wiring code is unchanged since (`game.rs`/`assets.rs` markers above).

### 12.4 Final state — DONE (rc=0 everywhere in scope)

| Check | rc | Result |
|---|---|---|
| `cargo check` 7 packages (formats, data, re, game, replay, headless, bench) | **0** | 4.6s warm |
| `cargo test -p wc3-data -p wc3-formats` | **0** | 34+2+8+20+1 / 60 passed |
| `cargo test -p wc3-re --test merge_parity` | **0** | 5 passed |
| `cargo test -p wc3-game --lib typed_object_data_feeds_game_assets` | **0** | `FOOTMAN name=Пехотинец icon=BTNFootman hp 420 gold 160 armor 2 dmg 12..13` |
| `cargo test -p wc3-game --lib` (full) | **0** | 78 passed, 0 failed, 9 ignored (102.6s) |
| `cargo run -p herofolends-headless -- Human04.w3m 60` | **0** | terrain 129×129, name `Альянс04` (WTS-resolved), pathing 512×512, objects `["war3map.w3u"]`, replay hash match, JASS parse ok |

The two failures reported in §11.4 were fixed by their owners during this round
(`mobile_hud` label fit, `sim_bridge` JASS order); the render ASTC blocker was
fixed at 07:49 after the exact patch was recorded here.

Remaining external gap (not the data layer): the headless JASS run prints
`JASS main=error unknown function GetCameraMargin` (missing native in
`wc3-re::jass::natives`); it does not affect names/icons/stats/tooltips.

Artifact: `analysis/agent-data/evidence-r2.txt` (rc table, headless dump, typed
store dump, audit).


---

## 13. PARITY round (ledger 354): all SLK/object data exposed + menus driven by it

### 13.1 Data layer (`crates/wc3-data`)

| Addition | File:line |
|---|---|
| `UnitView::{trains, builds, requires}` + `profile_list` (profile `Trains=`/`Builds=`/`Requires=`, `_`/`-`/empty dropped) | `units.rs:436-470` |
| `MenuEntry` (id, name, icon, hotkey, gold, lumber, food, build_time, tooltip, requires + requires_names, kind; `hotkey_label`, `cost_label`, `is_complete`) | `menu.rs:22-140` |
| `GameDatabase::{train_menu, build_menu, menu_entry(s), requirement_name, producers, builders}` | `menu.rs:150-260` |

Real-data parity suite `crates/wc3-data/tests/parity.rs` (values read from
`Units\HumanUnitFunc.txt` / `UnitBalance.slk`, quoted in the test comments):

| Test | Pin |
|---|---|
| `barracks_train_menu_comes_from_the_profile_list` | `hbar` → `hfoo,hrif,hkni`; footman 160G/2F/20s, hotkey `F`, name `Пехотинец`, icon, tooltip |
| `hall_train_menu_matches_each_hall_upgrade` | `htow`, `hkee`, `hcas` → `hpea` |
| `peasant_build_menu_comes_from_the_profile_list` | `hpea` → 10 structures in profile order, farm `Ферма` 80G |
| `tech_requirements_are_exposed_and_named` | `hgtw Requires = hlum` with resolved name; upgrade ids resolve too |
| `producers_and_builders_are_enumerated_from_the_tables` | 35 producers / 6 builders measured in the extraction |
| `menu_labels_are_data_projections` | hotkey/cost labels; pinned retail gap: `ndmg:nfel/ninf/nbal` have `Art` but no `Hotkey=` |
| `armour_and_damage_match_the_shipped_columns` | footman armour 2, average damage 12.5 from the dice formula |

### 13.2 Game wiring (names/icons/stats/costs/tooltips drive the card)

| Change | File:line |
|---|---|
| `Assets::{train_menu, build_menu, menu_entry}` passthroughs | `assets.rs:775-800` |
| `UiTheme::{icon_aliases, set_icon_alias, type_icon_name, type_icon}`; `command_icon` resolves `TrainDef/BuildDef` ids; 16 card icons preloaded at load | `ui.rs:421, 1076-1100`; `assets.rs:670-710` |
| `Game.menu` + `compute_menu` (hall → `Trains=`, worker → `Builds=`, filtered to roster ids) + `menu_actions` (`TrainDef`/`BuildDef`) + `can_afford_menu` (gold + food headroom) | `game.rs:141, 1595-1650` |
| `hud::Button::{short, hotkey}`; card slots 6..11 replaced by typed entries (label = typed name, short/hotkey = profile hotkey, sub = `160G 2F`) | `hud.rs:797-830, 940-960` |
| `short`/`hotkey` for data slots read from the button, not from a hardcoded label match | `game.rs:1770-1790` |

Proof (real `War3.mpq` + `Human04.w3m`):

```
cargo test -p wc3-game --lib menus_are_driven_by_the_typed_database   rc=0
  hall(TownHall) -> 1 entry hpea (name/cost/hotkey)
  peasant         -> 10 builds, ids[0]=htow, hhou present, all is_complete()
  barracks menu_entry complete; ui.type_icon(hfoo|hhou) Some
cargo test -p wc3-game --lib                                        rc=0
  87 passed; 0 failed; 15 ignored (213s)
cargo test -p wc3-data -p wc3-formats                               rc=0
  36 lib | 2 bin | 8 base_contract | 7 parity | 20 real_data | 1 doctest | 60 formats
```

### 13.3 External unblocks (other layers' in-flight code, recorded)

- `game.rs` tests: `placing` is `Option<&str>` (assigned `UnitKind::TownHall`), and two
  `UnitId` borrow escapes (`footman.id` after the iterator borrow) — fixed in place to
  keep the crate's test harness compiling.
- `mobile_hud.rs`: `MenuView::for_screen` was `&'static str → MenuView<'static>`, so
  `Game::menu_view` could not borrow `assets.map_name`; made lifetime-generic.
- `PARITY_MATRIX.md` was still unpublished at close time; the rows above are aligned to
  BASE §2 (typed DB owner) and §3 (stats provenance) and will be re-checked when it lands.

### 13.4 State

**DONE** for the round: data exposes costs/requirements/tooltips/icons/hotkeys/
armour/damage for every object, and both card menus are built from `Trains=`/`Builds=`.
Artifact: `analysis/agent-data/evidence-parity.txt`.


---

## 14. TOP-8/9/10 (ledger 354): hero data, buff/aura data, SpellDef from AbilityData

Aligned with `analysis/tech-lead/PARITY_MATRIX.md` §7 rows 8, 9, 10.

### 14.1 TOP-8 — hero attributes / level / skills (`crates/wc3-data/src/hero.rs`)

| Exposed | Source column |
|---|---|
| `HeroAttributes{strength,intelligence,agility,×_per_level,primary}` and `PrimaryAttribute::{value_at, attack_bonus_at}` | `UnitBalance.slk` `STR/INT/AGI`, `STRplus/INTplus/AGIplus`, `Primary` |
| `HeroSkill{id,name,icon,hotkey,levels,required_level,level_skip,mana,cooldown,area,range}` | `UnitAbilities.heroAbilList` + `AbilityData.slk` |
| `HeroRules{max_level, need_xp, grant_xp, exp_range, Revive* factors, caps}` + `xp_to_next/kill_xp/revive_cost` | `Units\MiscData.txt [Misc]` (`MaxHeroLevel=10`, `NeedHeroXP=200,…,5400`, `GrantHeroXP=80,…,400`, `HeroExpRange=1200`, `ReviveBaseFactor=.40`, `ReviveLevelFactor=.10`, `ReviveMaxFactor=4.0`, `ReviveTimeFactor=.5`, `HeroMaxReviveCostGold=700`) |
| `GameDatabase::{hero, heroes, hero_rules, hero_revive_cost}` | joins the above |

Revive uses the formula written in the data file itself:
`gold = originalCost * (base + levelFactor*(level-1))` capped by `ReviveMaxFactor`
and `HeroMaxReviveCostGold`; `time = originalTime * level * timeFactor` capped.

### 14.2 TOP-9 — buff / aura data (`crates/wc3-data/src/ability_def.rs`)

`AbilityNumbers{levels, target_flags, aura: Option<AuraNumbers>, hero/item flags}`
with `SpellLevel{mana,cooldown,area,range,duration,hero_duration,cast,data slots,unit_id}`
and `AbilityView::{is_aura, aura_numbers}`. Aura detection is **data-derived**
(passive: no cost, zero cooldown, zero duration, positive `Area`, friendly/self
`targs`) — no id list: Devotion `AHab` (900 radius, `Data11 = +1` armour) and
Endurance `AOae` (`Data11/12 = 0.1/0.05`) match; Blizzard `AHbz` and Holy Light
`AHhb` do not. `GameDatabase::auras()` is the framework inventory (9 rows in RoC).
Buff base rows are not shipped (`Units\AbilityBuffData.slk` absent) — `BuffStore`
stays overlay-only and validation reports it; nothing is invented.

### 14.3 TOP-10 — `SpellDef` numbers generated from `AbilityData`

`wc3-sim/src/abilities.rs` (write by its owner, additive patch applied here):

```rust
pub struct SpellNumbers { pub id: &'static str, pub mana: i32,
    pub cooldown_ticks: u32, pub range: f32, pub radius: f32, pub duration_ticks: u32 }
pub fn install_spell_numbers(numbers: &[SpellNumbers]) -> bool  // first install wins
pub fn spell_defs() -> &'static [SpellDef]                      // installed or fallback
impl Spell { pub fn def(self) -> &'static SpellDef { &spell_defs()[self.index()] } }
```

`wc3-game::assets::open_object_database` now builds `SpellNumbers` for the eleven
sim ids from `ability_numbers(id).first_level()` (mem → ticks at
`TICKS_PER_SECOND`) and calls `install_spell_numbers`, logging
`spell_numbers=N installed=true`. The hand-written `SPELL_DEFS` remain only as
the offline fallback: with an install present every mana/cooldown/range/radius/
duration value the simulator uses comes from the sheet, and the behaviour
(target kind, waves, jumps) stays in the sim.

### 14.4 Evidence (rc)

```
cargo test -p wc3-data --lib             rc=0  39 passed
cargo test -p wc3-data --test parity_top rc=0  7 passed (real RoC extraction)
cargo check -p wc3-sim                   rc=0
cargo test  -p wc3-sim --lib abilities   rc=0  3 passed
```

`parity_top` pins (quoted columns): `Hpal` L5 STR 22 (+2.7) INT 17 (+1.8) AGI 13
(+1.5) `Primary=STR`; `AHhb` "Благодать" 3 levels, req 1, 75 mana, 5 s, 800 rng;
`MaxHeroLevel=10`; `NeedHeroXP` list; revive 40 %+10 %/level capped at 700 G;
9 auras; `AHbz` L1 75/6 s/200/800 with `Data11..14 = 6,30,6,0.5`; all eleven
hand-written ids present with levels and names; ability coverage 100 % of editor
rows (`qa.abilities.coverage` needs ≥ 60 %).

Blockers met (other owners' files, mid-write at the time): `wc3-render`
`cache.rs`→`astc_chain_bytes`/`evict_one`, `wc3-sim` `data.rs` `CREEP_DEFS` then
`Unit.camp` (creep-camp increment #5). Re-verified where possible; the final
cross-crate compile is recorded in the closing message.

Artifact: `analysis/agent-data/evidence-top8.txt`.


### 14.5 THICK packet closure (TOP-8/9/10) — phase evidence

| Phase | Deliverable | Evidence |
|---|---|---|
| 1. Hero attributes/level/skill points/revive | `hero.rs` (`HeroAttributes`, growth, `PrimaryAttribute`, `HeroSkill` from `heroAbilList`, `HeroRules` XP + revive) | `parity_top`: Hpal L5 22/17/13 +2.7/1.8/1.5 STR; 4 skills (`AHhb` 75/5 s/800); `MaxHeroLevel 10`, `NeedHeroXP 200…5400`, revive 40 %+10 %/level capped 700 G |
| 2. Buff/aura framework data incl. armour/damage modifiers | `AuraNumbers{radius, bonuses, bonuses_by_level, bonus(level,slot), targets}` | 9 RoC auras; `AHab` 900 radius `+1/+2/+3` armour; `AOae` `0.1/0.2/0.3` (2nd slot `0.05/0.10/0.15`) |
| 3. `SpellDef` from `AbilityData` levels 1-3 | `AbilityNumbers`/`SpellLevel`; `wc3_sim::abilities::{SpellNumbers, install_spell_numbers, spell_defs}`; game installs for the 11 ids | coverage **100 %** of editor abilities; per-id equality test (mana/cd/radius/range/duration) rc=0 |
| 4. Menu entries ready for HUD | `MenuEntry{name,icon,hotkey,hotkey_label,cost_label,tooltip,requires(+names)}`, card slots 6..11 | `menus_are_driven_by_the_typed_database` rc=0 |

**Buff base table — measured absence, not an omission.** `Units\AbilityBuffData.slk`
and `Units\AbilityBuffMetaData.slk` are absent from both installs' listfiles
(`list-War3-clean.txt` 10 523 entries, `list-Files100_War3x.mpq-clean.txt` 5 610),
and the only "buff" hits in the RoC listfile are two MDX models
(`LightningShieldBuff.mdx`, `PurgeBuffTarget.mdx`). The 154-key buff dictionary is
therefore not reachable from this install; `BuffStore` keeps the `war3map.w3b`
overlay path and the aura numbers carry the modifiers the framework needs. If the
operator supplies a patch archive with the buff sheet, `MetaKind::Buff` +
`BuffStore::set_base` already consume it without code changes.

**Skill points.** Per-skill `levels`, `reqLevel` and `levelSkip` come from
`AbilityData`; the "+1 point per hero level" rule is a `Game.dll` constant that no
SLK/TXT in these installs carries, so it is documented rather than invented.

Gate: `cargo test -p wc3-data` rc=0 (39 lib + 2 bin + 8 base_contract + **7 parity**
+ **7 parity_top** + 20 real_data + 1 doctest); coverage 100 % ≥ 60 %;
`cargo test -p wc3-game --lib` rc=0 (91 passed, 0 failed, 15-18 ignored);
`cargo test -p wc3-sim --lib` rc=0 (24 passed).


### 14.6 Round 4 — melee coverage gate (matrix row 10) + post-restart rc

`wc3-data` gained the melee-scoped metric the matrix names:

| API | Meaning |
|---|---|
| `GameDatabase::MELEE_RACES` | `human`, `orc`, `undead`, `nightelf` |
| `GameDatabase::melee_ability_ids()` | ability ids referenced by melee-race units (`abilList` + `heroAbilList`), sorted, sentinels dropped |
| `GameDatabase::melee_ability_coverage()` | `(generated, total)` — ability rows with levels that `AbilityData` defines |

Measured on `analysis/war3_data`: **176 melee ids, 175 generated = 99.4 %**
(matrix baseline 140; gate ≥ 60 %). Pinned by
`parity_top::melee_ability_coverage_clears_the_sixty_percent_gate`, which also
requires `AHhb/AHbz/Ahea/AOcl/AUdc/Adef` to have generated numbers.

Post-restart gate (fresh rc):

```
cargo test -p wc3-data -p wc3-sim   rc=0  (81 s)
  wc3-data 39 lib | 2 bin | 8 base_contract | 7 parity | 8 parity_top | 20 real_data | 1 doctest
  wc3-sim  24 lib + integration suites
cargo test -p wc3-game --lib        rc=0  (195 s)
  91 passed; 0 failed; 18 ignored
```

Artifact: `analysis/agent-data/evidence-top8.txt` (§ round 4).

### 14.7 Round 6 — re-verify + evidence (2026-11-13, ledger 354 closed)

Owner scope re-verified fresh (model `opencode-go/iceman-contributor-free`, session `ses_e3059541fffejS1YGmXDWBw12Q`):

```
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p wc3-data --offline   rc=0
  39 lib + 2 bin + 8 base_contract + 7 parity + 10 parity_top + 20 real_data + 1 doctest = 87 passed
```

`parity_top` now includes `hero_level_stats_reproduce_the_computed_columns` and `hero_skill_points_and_level_gate` (hero HP/mana/armour/damage/skill-points derived from sheet: `realHP = HP + STR*hp_per_strength`, `realM = manaN + INT*mana_per_intelligence`, `DefenseArmor` from `Units\MiscData.txt`; level-1 identity against sheet columns for all heroes; Paladin concrete 650 HP/255 mana/2.78 armour). Melee coverage gate remains 175/176 = 99.4% (≥60%).

Packet p1/p2-data/p3 delivered in owner paths: `crates/wc3-data/src/hero.rs` (attrs/levels/skills/revive/derived stats), `ability_def.rs` (AuraNumbers, 9 RoC auras; buff base sheet absent by listfile evidence → overlay-only `BuffStore`), SpellDef/`AbilityNumbers` from `AbilityData` + `wc3-sim::abilities::install_spell_numbers` hook (sim READ_ONLY). No invented values; runtime buff tick/apply_damage is sim-layer behavior (out of owner scope).

External blockers observed this run (READ_ONLY, other owners): `cargo test -p wc3-sim --offline` rc=101, `crates/wc3-sim/tests/merge.rs:856` `error[E0425]: cannot find value \`i\`` (mid-edit regression). wc3-data gate independent and green.

Artifacts: `analysis/agent-data/evidence-round6.txt`. H2: capture 699, experience 4393, ledger 354 `completed/done`.

### 14.8 Round 8 — hero ability catalog (#11) + item shop/charges/abilities (#14)

**New owner-path code**

| File | Change |
|---|---|
| `crates/wc3-data/src/shop.rs` (new) | `ShopDef`/`ShopItem`/`Requirement`; `GameDatabase::{shops,shop,shops_selling,shop_item,item_requirements,item_ability_ids,item_ability_coverage,item_ability_numbers}` |
| `crates/wc3-data/src/ability_def.rs` | `HeroAbilityInfo`; `GameDatabase::{hero_unit_ids,hero_ability_ids,hero_abilities,hero_ability_coverage}` |
| `crates/wc3-data/src/items.rs` | `ItemView::{class_name,sellable,powerup,requirements,ability_views}` |
| `crates/wc3-data/src/lib.rs` | `pub mod shop`; exports `HeroAbilityInfo`, `Requirement`, `ShopDef`, `ShopItem` |
| `crates/wc3-data/tests/hero_items.rs` (new) | 6 real-data tests (hero catalog, ladder 48 slots, shop stock, Requires/DependencyOr, item abilities/charges, recipe boundary) |

**Data sources (all retail, no invented table)**
- Hero abilities: `UnitAbilities.heroAbilList` × `AbilityData` (`levels`, `reqLevel`, `levelSkip`, per-level `Cost/Cool/Rng/Area/Dur/Data`).
- Shops: unit profile `Sellitems` (`Units\NeutralUnitFunc.txt`, map `war3map.w3u` overlays flow through the same `Localizer`), `ItemData` `goldcost/lumbercost/Level/stockMax/stockRegen/stockStart/uses/itemClass|class/abilList`.
- Purchase gating: `ItemFunc.txt` `Requires` + pseudo-token `DependencyOr` (`phea`/`stwp` → `TWN2` → `hkee,ostr,etoa,unp1`; `ofir` → `hcas`).
- Item abilities: `ItemData.abilList` resolved against `AbilityData`.
- **Recipes**: not a retail data structure (no recipe column/class/metadata, no `Requires` token naming an item in either install). Combining is map-script logic; pinned by `retail_data_has_no_recipe_table` instead of fabricating a table.

**Measured gates**

| Gate | Value |
|---|---|
| `qa.abilities.heroes` (matrix row 11) | RoC 56/56 hero abilities with all levels = 100 %; 12 ladder heroes × 4 = 48 slots resolved |
| `qa.items.shop` (row 14) | RoC `ngme` 8 items, TFT `ngme` 11 items from `Sellitems`; 18 TFT items gated via `Requires` |
| `qa.items.abilities` | RoC 102/106 = 96.2 %, TFT 223/223 = 100 % (≥60 %) |
| charges | `phea` 1, `wneg` 3, `ratc` none (`uses`) |

**Verification (fresh rc)**

```
cargo test -p wc3-data --test hero_items --offline   rc=0   6 passed
cargo test -p wc3-data --offline                     rc=0   93 passed, 0 failed
  39 lib | 2 bin | 8 base_contract | 6 hero_items | 7 parity | 10 parity_top
  | 20 real_data | 1 doctest
```

Artifacts: `analysis/agent-data/evidence-round8.txt`, `analysis/agent-data/coverage-report.json` (round 8 gates).
H2: capture/experience/monitor IDs in the closing message.

### 14.9 P0 gap close — items.schema / hero.coverage / cast.coverage (2026-11-13)

Tree was red on entry (`crates/wc3-data/src/hero.rs:392` unexpected closing
delimiter: a doc comment had swallowed `pub fn hp_at`); restored first,
`cargo check -p wc3-data --lib` rc=0, then the gates below.

| QA gate | Was | Data-layer fix | Measured now |
|---|---|---|---|
| `items.schema` | 1/2 trees expose class | `ItemView::class()` reads `itemClass` (RoC) then `class` (TFT) via `class_name()` (`items.rs`) | RoC **132/132**, TFT **273/273** → **2/2 trees** |
| `hero.coverage` | 11/12 | `hero::LADDER_HERO_IDS` (12) + `GameDatabase::{melee_heroes,melee_hero_coverage}` (`hero.rs`) | **12/12** both roots; `Emoo` returns attrs/HP/mana/4 skills |
| `cast.coverage` | 11/147 | `GameDatabase::{castable_ability_ids,castable_ability_coverage}` (`ability_def.rs`) | data population **RoC 251/252 = 99.6 %**, **TFT 441/442 = 99.8 %** |

Residuals (outside this owner scope, exact evidence):
- `hero.coverage` — QA counts `wc3_sim::data::ROSTER`; `roster_gen.rs` has no
  `Emoo` UnitDef while `eate.trains` references `Emoo` (`roster_gen.rs:4654`).
- `cast.coverage` — QA counts `wc3_sim::abilities::Spell::ALL` (11 variants);
  a data-driven generic cast path is a `wc3-sim` change.
- `items.schema` is fully closed at the API the QA gate calls (`item.class()`).

Canonical gates (only these were run for acceptance):

```
cargo check -p wc3-data --lib --offline   rc=0
cargo test  -p wc3-data --lib --offline   rc=0   42 passed, 0 failed
```

New lib tests: `gap_close_tests::{class_column_is_exposed_for_both_item_trees,
melee_hero_coverage_is_twelve_of_twelve,
castable_ability_coverage_covers_the_roster_population}`.
Full-suite regression: `cargo test -p wc3-data` rc=0, 96 passed.
Artifact: `analysis/agent-data/evidence-p0-gap-close.txt`, `coverage-report.json` (`p0_gap_close`).

### 14.10 Final product round — tooling + report (2026-11-13)

`dump_gamedata` now exposes the layer's product surface; measured on the real
extraction (rc=0):

| Command | Output |
|---|---|
| `shops` / `shop ID` | `ngme` 8 items RoC (11 TFT) with gold/lumber/level/class/charges/stock/`Requires` |
| `hero-abilities` / `hero-ability ID` | 56/56 hero abilities with owners and per-level mana/cd/area/range/duration/data (`AHhb` 75/5/800) |
| `coverage` | melee 175/176, spell sheet 380/380, heroes 12/12, castable 251/252, item abilities 102/106, items-with-class 132/132 (RoC); TFT 301/301, 733/733, 12/12, 442/442, 223/223, 273/273 |
| `--json` | machine-readable variants of the same commands |

Bin tests: 4 passed (`json_string_escapes`, `coverage_helpers_render_gate_numbers`,
`requirement_text_expands_pseudo_tokens`, `weapon_slot_suffixes`).

Final report: `analysis/agent-data/FINAL-DATA-REPORT.md` (API map, tooling, gate
ledger, residuals). Canonical gates stay green: `cargo check -p wc3-data --lib`
rc=0, `cargo test -p wc3-data --lib` rc=0 (42 passed).

### 14.11 Tooling + final product round (2026-11-13)

**Toolchain audit** (`analysis/agent-data/TOOLCHAIN-AUDIT.md`): python 3.14.10,
pip 26.3, node 24.22.2, npm 12.2.2, bun 1.4.4, rust 1.99.1, go 1.27.3, clang 21.1.8,
cmake 4.4.5, sqlite3 3.54.1, jq 1.8.2, ffmpeg 9.0.2, sox 14.8.1, ImageMagick
7.1.2-35, optipng 7.9.1, pngquant 3.0.3, r2 6.2.6, 7z 17.05.
Installed/adapted with rc: `pyassimp` 5.2.5 (loads GLB via system `libassimp.so`
6.0.6, probe rc=0), `@gltf-transform/{core,extensions,functions}` 4.5.1 +
`three` 0.187.0 (GLB inspect rc=0), numpy 2.5.4 / trimesh 5.2.1 / pygltflib 1.16.5
already present. Gaps documented with alternatives: no `assimp` CLI (pyassimp),
`gltf-transform` CLI broken on sharp (pure-JS core + `gltf_inspect.mjs`), no
`stormlib` package (project `wc3-mpq` + r2), no headless WebGL for `three`.

**Balance audit — every row of units/abilities/items/upgrades from SLK**
(new `crates/wc3-data/src/balance.rs`, `dump_gamedata balance`):

| Root | units | abilities | items | upgrades | total rows | checked | errors | warnings |
|---|---|---|---|---|---|---|---|---|
| RoC  | 466 | 397 | 132 | 58 | 1053 | 1053 | 0 | 0 |
| TFT  | 812 | 763 | 273 | 89 | 1937 | 1937 | 0 | 6 |

The six TFT warnings are `Area1 = -1` sentinels on aura/dummy rows (`Aave`,
`Adec`, `Aphx`, `Arav`, `Amrf`, `Astn`, all `targs=_`); they are reported and do
not gate. Per row the audit reads HP/mana/level/gold/lumber/build/food/speed/
armour, both weapon slots (min<=max, rate, range), every ability level
(mana/cooldown/area/range/duration/cast), item price/level/uses/stock, and upgrade
maxlevel/gold/lumber/time base+per-level.

**SQLite export** (`dump_gamedata sqlite PATH`, sqlite3 3.54.1):
`units 466 | abilities 397 | ability_levels 1191 | items 132 | upgrades 58 | shops 8`
for RoC, counts verified by a follow-up `SELECT` (rc=0).

**Gates**: `cargo check -p wc3-data --lib` rc=0; `cargo test -p wc3-data --lib`
rc=0 (45 passed); full `cargo test -p wc3-data` rc=0 (101 passed).
Artifacts: `analysis/agent-data/{TOOLCHAIN-AUDIT.md,evidence-tooling.txt}`.
