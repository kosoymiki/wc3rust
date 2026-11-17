//! Print each unit's `UnitUI.unitSound` set (the per-unit voice family) and the
//! first acknowledgement definition that exists for it. Diagnostic for the
//! per-unit voice bug (a Human peasant must not resolve to an Orc set).
//!
//! Usage: `unit_sound_sets [analysis-dir]` (default `analysis/war3_full3`).

use wc3_data::source::DirSource;
use wc3_data::GameDatabase;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "analysis/war3_full3".to_string());
    let data = GameDatabase::load(&DirSource::new(root))?;
    let defs = ["hpea", "opeo", "uaco", "ewsp", "hfoo", "ogru", "ugho", "earc"];
    for def in defs {
        let unit = data.unit(def);
        println!(
            "SOUND_SET def={def} name={:?} unitSound={:?}",
            unit.as_ref().and_then(|unit| unit.name()),
            unit.as_ref().and_then(|unit| unit.sound_set()),
        );
    }
    Ok(())
}
