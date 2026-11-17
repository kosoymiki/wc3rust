//! Art completeness audit for the shipped object data.
//!
//! Every roster unit/building/creep/shop/destructable is checked for the five
//! presentation fields the runtime needs: model path, icon, sound set, tooltip
//! and scale. Missing values are resolved from the object data where a canonical
//! fallback exists (tooltip -> name, scale -> 1.0, destructable icon -> texture);
//! anything still missing is listed with its id, never silently dropped.
//!
//! Source: `wc3-data` typed views over `UnitData`/`unitUI`/`UnitFunc`/`UnitStrings`
//! and `DestructableData`; the same rows the game consumes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use wc3_data::GameDatabase;

use crate::error::{ReError, Result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArtField {
    pub have: usize,
    pub closed: usize,
    pub missing: Vec<String>,
}

impl ArtField {
    fn total(&self) -> usize {
        self.have + self.closed + self.missing.len()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FamilyAudit {
    pub name: String,
    pub total: usize,
    pub fields: BTreeMap<&'static str, ArtField>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtReport {
    pub generated_unix: u64,
    pub families: Vec<FamilyAudit>,
}

impl ArtReport {
    pub fn ok(&self) -> bool {
        !self.families.is_empty()
            && self.families.iter().all(|family| family.total > 0)
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-art-completeness-v1\",\"generated_unix\":{},\"ok\":{},\"families\":[",
            self.generated_unix,
            self.ok()
        );
        for (index, family) in self.families.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"family\":{},\"total\":{},\"fields\":{{",
                crate::manifest::json_str(&family.name),
                family.total
            );
            let mut first = true;
            for (field, stat) in &family.fields {
                if !first {
                    out.push(',');
                }
                first = false;
                let missing = stat
                    .missing
                    .iter()
                    .map(|id| crate::manifest::json_str(id))
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = write!(
                    out,
                    "{}:{{\"have\":{},\"closed\":{},\"missing\":[{}]}}",
                    crate::manifest::json_str(field),
                    stat.have,
                    stat.closed,
                    missing
                );
            }
            out.push_str("}}");
        }
        out.push_str("]}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# ART COMPLETENESS — object-data fields");
        let _ = writeln!(out, "\n| family | total | model | icon | sound | tooltip | scale |");
        let _ = writeln!(out, "|---|---:|---|---|---|---|---|");
        for family in &self.families {
            let cell = |name: &str| {
                family
                    .fields
                    .get(name)
                    .map_or_else(|| "—".to_string(), |field| {
                        format!(
                            "{}/{} (+{} closed, {} missing)",
                            field.have,
                            field.total(),
                            field.closed,
                            field.missing.len()
                        )
                    })
            };
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} |",
                family.name,
                family.total,
                cell("model"),
                cell("icon"),
                cell("sound"),
                cell("tooltip"),
                cell("scale")
            );
        }
        out
    }
}

fn field_mut<'a>(family: &'a mut FamilyAudit, name: &'static str) -> &'a mut ArtField {
    family.fields.entry(name).or_default()
}

fn read_sheet(data: &Path, name: &str) -> Option<crate::slk::Sheet> {
    let path = data.join("Units").join(name);
    let bytes = std::fs::read(path).ok()?;
    crate::slk::Sheet::parse(name, &bytes).ok()
}

fn sheet_num(sheet: &crate::slk::Sheet, id: &str, column: &str) -> Option<f32> {
    let row = sheet.row_index(id)?;
    sheet
        .cell(row, column)
        .and_then(|value| value.trim().parse().ok())
}

fn scale_value(sheet: &crate::slk::Sheet, id: &str) -> Option<f32> {
    sheet_num(sheet, id, "modelScale").or_else(|| sheet_num(sheet, id, "scale"))
}

fn destructable_scale(sheet: &crate::slk::Sheet, id: &str) -> Option<f32> {
    sheet_num(sheet, id, "maxScale").or_else(|| sheet_num(sheet, id, "minScale"))
}

fn roster_ids(root: &Path, relative: &str) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(root.join(relative)) else {
        return BTreeSet::new();
    };
    text.split("id: \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn build(root: &Path) -> Result<ArtReport> {
    let data_root = crate::catalog::data_root(root)?;
    let db = crate::catalog::database(root)?;
    let unit_ui = read_sheet(&data_root, "unitUI.slk")
        .ok_or_else(|| ReError::NotFound("Units/unitUI.slk".into()))?;
    let destruct_sheet = read_sheet(&data_root, "DestructableData.slk")
        .ok_or_else(|| ReError::NotFound("Units/DestructableData.slk".into()))?;
    let mut shop_ids = roster_ids(root, "crates/wc3-sim/src/shops_gen.rs");
    if shop_ids.is_empty() {
        shop_ids = db.shops().into_iter().map(|shop| shop.unit_id).collect();
    }
    let creep_ids = roster_ids(root, "crates/wc3-sim/src/creeps_gen.rs");
    let roster = roster_ids(root, "crates/wc3-sim/src/roster_gen.rs");
    let destructable_ids = roster_ids(root, "crates/wc3-sim/src/destructables_gen.rs");
    let mut families: BTreeMap<&'static str, FamilyAudit> = BTreeMap::new();
    for name in ["units", "buildings", "creeps", "shops", "destructables"] {
        families.insert(
            name,
            FamilyAudit {
                name: name.to_string(),
                ..FamilyAudit::default()
            },
        );
    }
    let roster_all: BTreeSet<String> = roster
        .iter()
        .chain(creep_ids.iter())
        .chain(shop_ids.iter())
        .cloned()
        .collect();
    for id in &roster_all {
        let Some(unit) = db.unit(id) else {
            continue;
        };
        let id = unit.id_str();
        let family_name = if shop_ids.contains(&id) {
            "shops"
        } else if creep_ids.contains(&id) {
            "creeps"
        } else if unit.is_building().unwrap_or(false) {
            "buildings"
        } else {
            "units"
        };
        let family = families
            .get_mut(family_name)
            .expect("family initialized");
        family.total += 1;
        if unit.model_file().as_deref().is_some_and(|value| !value.trim().is_empty()) {
            field_mut(family, "model").have += 1;
        } else {
            field_mut(family, "model").missing.push(id.clone());
        }
        if let Some(icon) = unit.icon().filter(|value| !value.trim().is_empty()) {
            let _ = icon;
            field_mut(family, "icon").have += 1;
        } else {
            field_mut(family, "icon").missing.push(id.clone());
        }
        if unit.sound_set().is_some_and(|value| !value.trim().is_empty()) {
            field_mut(family, "sound").have += 1;
        } else {
            field_mut(family, "sound").missing.push(id.clone());
        }
        if unit
            .tip()
            .or_else(|| unit.ubertip())
            .is_some_and(|value| !value.trim().is_empty())
        {
            field_mut(family, "tooltip").have += 1;
        } else if unit.name().is_some_and(|value| !value.trim().is_empty()) {
            field_mut(family, "tooltip").closed += 1;
        } else {
            field_mut(family, "tooltip").missing.push(id.clone());
        }
        if scale_value(&unit_ui, &id).is_some() {
            field_mut(family, "scale").have += 1;
        } else {
            field_mut(family, "scale").closed += 1; // canonical 1.0 fallback
        }
    }
    let destructable = families
        .get_mut("destructables")
        .expect("family initialized");
    let destructable_iter: Vec<_> = if destructable_ids.is_empty() {
        db.destructables().map(|row| row.id_str()).collect()
    } else {
        destructable_ids.iter().cloned().collect()
    };
    for row_id in destructable_iter {
        let Some(row) = db.destructable(&row_id) else {
            continue;
        };
        let id = row.id_str();
        destructable.total += 1;
        if row.model_file().as_deref().is_some_and(|value| !value.trim().is_empty()) {
            field_mut(destructable, "model").have += 1;
        } else {
            field_mut(destructable, "model").missing.push(id.clone());
        }
        if destruct_sheet
            .row_index(&id)
            .and_then(|row| destruct_sheet.cell(row, "texFile"))
            .is_some_and(|value| !value.trim().is_empty())
        {
            field_mut(destructable, "icon").have += 1;
        } else {
            field_mut(destructable, "icon").missing.push(id.clone());
        }
        if destruct_sheet
            .row_index(&id)
            .and_then(|row| destruct_sheet.cell(row, "deathSnd"))
            .is_some_and(|value| !value.trim().is_empty())
        {
            field_mut(destructable, "sound").have += 1;
        } else {
            field_mut(destructable, "sound").missing.push(id.clone());
        }
        if row.name().or_else(|| row.comment()).is_some_and(|value| !value.trim().is_empty()) {
            field_mut(destructable, "tooltip").have += 1;
        } else {
            field_mut(destructable, "tooltip").missing.push(id.clone());
        }
        if destructable_scale(&destruct_sheet, &id).is_some() {
            field_mut(destructable, "scale").have += 1;
        } else {
            field_mut(destructable, "scale").closed += 1;
        }
    }
    Ok(ArtReport {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        families: families.into_values().collect(),
    })
}

pub fn write_artifacts(report: &ArtReport, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(json, report.to_json())?;
    std::fs::write(md, report.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_completeness_families_have_required_fields() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = build(&root).expect("art report");
        assert_eq!(report.families.len(), 5);
        let by_name: BTreeMap<_, _> = report
            .families
            .iter()
            .map(|family| (family.name.as_str(), family))
            .collect();
        for name in ["units", "buildings", "creeps", "shops", "destructables"] {
            let family = by_name.get(name).expect(name);
            assert!(family.total > 0, "{name} total");
            for field in ["model", "icon", "sound", "tooltip", "scale"] {
                assert!(
                    family.fields.contains_key(field),
                    "{name}.{field} missing"
                );
            }
        }
        let units = by_name["units"];
        assert_eq!(units.fields["model"].missing.len(), 0, "unit models");
        assert!(units.fields["tooltip"].have > 0);
        let json = report.to_json();
        assert!(json.contains("\"schema\":\"wc3-art-completeness-v1\""));
    }
}
