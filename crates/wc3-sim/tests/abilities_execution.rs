//! mechanic `abilities.execution`: generated rows match AbilityData (763) and
//! RE's typed AbilityDef classification is consumed read-only.

use std::path::Path;

use wc3_sim::{GENERIC_ABILITIES, typed_ability_defs, typed_ability_summary};

#[test]
fn abilities_execution_typed_re_classification_763() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(
        GENERIC_ABILITIES.len(),
        763,
        "generated rows == non-empty AbilityData ids"
    );
    let defs = typed_ability_defs(&root).expect("RE typed AbilityDef rows");
    let summary = typed_ability_summary(&root).expect("RE typed classification summary");
    assert_eq!(defs.len(), 763, "RE typed rows");
    assert_eq!(summary.total, 763);
    assert_eq!(
        summary.executable + summary.data_only,
        763,
        "every row classified executable or data-only"
    );
    for ability in GENERIC_ABILITIES {
        assert!(
            defs.iter().any(|def| def.id == ability.id),
            "{} missing from RE typed rows",
            ability.id
        );
    }
    // The generic runner only claims the active subset; classification counts
    // are reported, not invented.
    eprintln!(
        "ABILITIES total={} executable={} data_only={} generic_castable={}",
        summary.total,
        summary.executable,
        summary.data_only,
        wc3_sim::data_castable_ids().count()
    );
}
