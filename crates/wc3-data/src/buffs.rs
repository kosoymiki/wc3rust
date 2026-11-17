//! Buffs (`war3map.w3b`) — overlay driven.
//!
//! Both retail roots ship **no** `Units\AbilityBuffData.slk` and no
//! `AbilityBuffMetaData.slk` (`observed`: directory listings of
//! `analysis/war3_data/Units` and `analysis/war3x_data/Units`), so buff base rows
//! only exist in extracted data that includes them. This layer therefore stores
//! the overlay itself and exposes every change verbatim; when a base table is
//! supplied by the caller the same accessors read it.

use std::collections::BTreeMap;

use wc3_formats::gamedata::{ChangeValue, FieldTarget, ObjectDataFile, ObjectKind};

use crate::table::Table;
use crate::GameDatabase;

/// One modification of a buff row.
#[derive(Debug, Clone, PartialEq)]
pub struct BuffChange {
    /// FourCC of the metadata field (`fnam`, `ftip`, `fube`, …).
    pub field_id: String,
    /// Where the field would land, resolved through `AbilityBuffMetaData.slk`
    /// when the caller supplies it.
    pub target: FieldTarget,
    pub value: ChangeValue,
}

/// One buff entry.
#[derive(Debug, Clone, PartialEq)]
pub struct BuffEntry {
    /// Buff id (`BHbd`, `Bfro`, …); for custom buffs the new id.
    pub id: String,
    /// Prototype id of a custom buff.
    pub base_id: Option<String>,
    pub changes: Vec<BuffChange>,
}

impl BuffEntry {
    pub fn is_custom(&self) -> bool {
        self.base_id.is_some()
    }
}

/// Buff table: optional shipped base rows plus map overrides.
#[derive(Debug, Default, Clone)]
pub struct BuffStore {
    base: Option<Table>,
    entries: BTreeMap<String, BuffEntry>,
    last_modified: Option<String>,
}

impl BuffStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the shipped buff sheet when the data root provides one.
    pub fn set_base(&mut self, table: Table) {
        self.base = Some(table);
    }

    pub fn base(&self) -> Option<&Table> {
        self.base.as_ref()
    }

    pub fn is_base_available(&self) -> bool {
        self.base.is_some()
    }

    /// Merge a `war3map.w3b` file.
    pub fn apply(
        &mut self,
        file: &ObjectDataFile,
        meta: Option<&wc3_formats::gamedata::MetaTable>,
    ) {
        debug_assert_eq!(file.kind, ObjectKind::Buff);
        for entry in file.entries() {
            let id = entry.target_id().as_str().into_owned();
            let base_id = entry
                .is_custom()
                .then(|| entry.old_id.as_str().into_owned());
            let target = self.entries.entry(id.clone()).or_insert_with(|| BuffEntry {
                id,
                base_id,
                changes: Vec::new(),
            });
            for change in &entry.changes {
                let resolved = wc3_formats::gamedata::resolve_field(
                    meta,
                    &change.id,
                    change.level,
                    change.data_pointer,
                );
                target.changes.push(BuffChange {
                    field_id: change.id.clone(),
                    target: resolved,
                    value: change.value.clone(),
                });
            }
            self.last_modified = Some(entry.target_id().as_str().into_owned());
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len() + self.base.as_ref().map(Table::len).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Overridden/custom entries only.
    pub fn overlay_len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, id: &str) -> Option<&BuffEntry> {
        self.entries.get(id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &BuffEntry> {
        self.entries.values()
    }

    /// Base-table value when a base sheet is present.
    pub fn base_text(&self, id: &str, column: &str) -> Option<&str> {
        self.base.as_ref()?.text(id, column)
    }
}

/// Presentation of one buff row.
///
/// Retail ships **no** buff sheet (verified against both archive listfiles:
/// `grep -ic AbilityBuffData` = 0 in `list-War3-clean.txt` and
/// `list-Files100_War3x.mpq-clean.txt`), so a buff's card data lives only in
/// the profile text sections: `Bufftip` is the display name and `Buffart` the
/// icon. Measured: 85 `Bufftip` and 84 `Buffart` sections in the RoC text.
/// A `war3map.w3b` entry overrides any of the four text fields.
#[derive(Debug, Clone, PartialEq)]
pub struct BuffInfo {
    /// Buff id (`BHwe`, `Bfro`, …).
    pub id: String,
    /// `Bufftip` / `fnam` — the buff's display name.
    pub name: Option<String>,
    /// `fart` — the overlay's tooltip field (never in profile text).
    pub tip: Option<String>,
    /// `fube` — the overlay's extended tooltip field.
    pub ubertip: Option<String>,
    /// `Buffart` / `fart` — the buff icon path.
    pub icon: Option<String>,
    /// `true` when a `war3map.w3b` entry overrides or defines the id.
    pub overridden: bool,
}

impl GameDatabase {
    /// Buff ids that exist as profile sections (`Buffart`/`Bufftip`), sorted.
    ///
    /// Buffs are the one object class with no shipped sheet, so the profile
    /// text is the only complete id list; `war3map.w3b` ids are added by
    /// [`GameDatabase::buff`] on lookup.
    pub fn buff_ids(&self) -> Vec<String> {
        let mut ids = self.localizer().sections_with_key("Buffart");
        for id in self.localizer().sections_with_key("Bufftip") {
            if !ids.iter().any(|existing| existing.eq_ignore_ascii_case(&id)) {
                ids.push(id);
            }
        }
        ids.sort();
        ids
    }

    /// One buff row: profile text first, `war3map.w3b` overlay on top.
    pub fn buff(&self, id: &str) -> Option<BuffInfo> {
        let localizer = self.localizer();
        let overlay = self.buffs().get(id);
        let mut info = BuffInfo {
            id: id.to_string(),
            name: localizer.buff_tip(id),
            tip: None,
            ubertip: None,
            icon: localizer.buff_icon(id),
            overridden: overlay.is_some(),
        };
        if let Some(entry) = overlay {
            for change in &entry.changes {
                let value = change.value.as_text();
                match change.field_id.to_ascii_lowercase().as_str() {
                    "fnam" => info.name = Some(value),
                    "ftip" => info.tip = Some(value),
                    "fube" => info.ubertip = Some(value),
                    "fart" => info.icon = Some(value),
                    _ => {}
                }
            }
        }
        let empty = info.name.is_none() && info.icon.is_none() && !info.overridden;
        (!empty).then_some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_formats::gamedata::{FourCc, ObjectEntry};

    #[test]
    fn applies_custom_buff_entries() {
        let file = ObjectDataFile {
            kind: ObjectKind::Buff,
            version: 2,
            base: vec![],
            custom: vec![ObjectEntry {
                old_id: FourCc::parse("BHbd").unwrap(),
                new_id: FourCc::parse("B001").unwrap(),
                changes: vec![wc3_formats::gamedata::ObjectChange {
                    id: "fnam".to_string(),
                    value: ChangeValue::String("Custom buff".to_string()),
                    level: 0,
                    data_pointer: 0,
                }],
            }],
        };
        let mut store = BuffStore::new();
        store.apply(&file, None);
        assert_eq!(store.overlay_len(), 1);
        assert!(!store.is_base_available());
        let entry = store.get("B001").expect("entry");
        assert_eq!(entry.base_id.as_deref(), Some("BHbd"));
        assert_eq!(entry.changes[0].field_id, "fnam");
        assert_eq!(
            entry.changes[0].value,
            ChangeValue::String("Custom buff".into())
        );
        assert!(matches!(entry.changes[0].target, FieldTarget::Unknown));
    }

    /// Retail ships no buff sheet, so a buff row's card data is the profile
    /// section (`Bufftip`/`Buffart`); a `war3map.w3b` entry overrides it and the
    /// merge must reach `BuffStore` (it used to be dropped on the floor).
    #[test]
    fn profile_rows_resolve_and_w3b_overrides_win() {
        let mut source = crate::source::MemorySource::new();
        // A minimal sheet so `GameDatabase::load` accepts the root.
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"human\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\HumanAbilityFunc.txt",
            "[BHwe]\nBuffart=ReplaceableTextures\\CommandButtons\\BTNWater.blp\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\HumanAbilityStrings.txt",
            "[BHwe]\nBufftip=Water Spirit\n".as_bytes().to_vec(),
        );
        let mut gd = crate::GameDatabase::load(&source).expect("load");
        assert_eq!(gd.buff_ids(), ["BHwe"]);
        let info = gd.buff("BHwe").expect("buff row");
        assert_eq!(info.name.as_deref(), Some("Water Spirit"));
        assert!(
            info.icon
                .as_deref()
                .is_some_and(|icon| icon.ends_with("BTNWater.blp"))
        );
        assert!(!info.overridden);
        // Unknown ids are `None`, not empty rows.
        assert!(gd.buff("Bxxx").is_none());

        let file = ObjectDataFile {
            kind: ObjectKind::Buff,
            version: 2,
            base: vec![ObjectEntry {
                old_id: FourCc::parse("BHwe").unwrap(),
                new_id: FourCc::new([0; 4]),
                changes: vec![wc3_formats::gamedata::ObjectChange {
                    id: "fnam".to_string(),
                    value: ChangeValue::String("Changed spirit".to_string()),
                    level: 0,
                    data_pointer: 0,
                }],
            }],
            custom: vec![],
        };
        gd.apply_parsed("war3map.w3b".to_string(), file)
            .expect("apply w3b");
        let overridden = gd.buff("BHwe").expect("buff after override");
        assert_eq!(overridden.name.as_deref(), Some("Changed spirit"));
        assert!(overridden.overridden);
        assert_eq!(gd.buffs().overlay_len(), 1);
    }
}
