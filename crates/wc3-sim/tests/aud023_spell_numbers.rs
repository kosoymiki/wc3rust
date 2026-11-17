//! AUD-023 — spell numbers are an instance-owned table with a dataset identity.
//!
//! The process keeps one installed table (the numbers a live `Sim` reads through
//! `Spell::def`). `SpellNumberTable` owns that table: a first install adopts the
//! dataset, an identical reload is an idempotent success, and a *different*
//! dataset is an error that names both ids — never a silent `false`.
//!
//! These tests use their own `SpellNumberTable` instances, so they prove the
//! isolation property (two datasets in one process do not leak into each other)
//! without depending on the order of other tests that touch the global table.

use wc3_sim::abilities::{
    SPELL_DEFS, Spell, SpellDatasetId, SpellInstall, SpellNumberTable, SpellNumbers,
};

fn dataset(mana: i32, cooldown: u32, range: f32) -> Vec<SpellNumbers> {
    Spell::ALL
        .iter()
        .map(|spell| {
            let def = spell.def();
            SpellNumbers {
                id: def.id,
                mana,
                cooldown_ticks: cooldown,
                range,
                radius: def.radius,
                duration_ticks: def.duration_ticks,
            }
        })
        .collect()
}

fn def_of<'a>(table: &'a SpellNumberTable, id: &str) -> &'a wc3_sim::abilities::SpellDef {
    table
        .defs()
        .iter()
        .find(|def| def.id == id)
        .expect("spell row")
}

#[test]
fn aud023_two_tables_keep_independent_datasets_and_reject_the_other_one() {
    let first = SpellNumberTable::new();
    let second = SpellNumberTable::new();
    let dataset_a = dataset(10, 30, 500.0);
    let dataset_b = dataset(90, 300, 900.0);

    let installed_a = first.install(&dataset_a).expect("first dataset installs");
    let installed_b = second.install(&dataset_b).expect("second table is independent");
    let SpellInstall::Installed(id_a) = installed_a else {
        panic!("first install must be Installed, got {installed_a:?}");
    };
    let SpellInstall::Installed(id_b) = installed_b else {
        panic!("first install must be Installed, got {installed_b:?}");
    };
    assert_ne!(id_a, id_b, "different numbers have different identities");

    // Each table answers from its own dataset.
    assert_eq!(def_of(&first, "AHhb").mana, 10);
    assert_eq!(def_of(&second, "AHhb").mana, 90);

    // Offering the other dataset is an error that names both ids; the table
    // keeps the numbers it already had.
    let conflict = first
        .install(&dataset_b)
        .expect_err("a second, different dataset must be rejected");
    assert_eq!(conflict.installed, id_a);
    assert_eq!(conflict.incoming, id_b);
    assert_eq!(def_of(&first, "AHhb").mana, 10, "live data is unchanged");

    let reverse = second
        .install(&dataset_a)
        .expect_err("rejection is symmetric");
    assert_eq!(reverse.installed, id_b);
    assert_eq!(reverse.incoming, id_a);
    assert_eq!(second.dataset(), Some(id_b));
}

#[test]
fn aud023_repeated_identical_load_is_idempotent() {
    let table = SpellNumberTable::new();
    let numbers = dataset(25, 60, 600.0);
    let first = table.install(&numbers).expect("install");
    let id = match first {
        SpellInstall::Installed(id) => id,
        other => panic!("expected Installed, got {other:?}"),
    };
    let before: Vec<(i32, u32, f32)> = table
        .defs()
        .iter()
        .map(|def| (def.mana, def.cooldown_ticks, def.range))
        .collect();

    // Same rows, different vector order: the identity is canonical, so this is
    // the same dataset, not a conflict.
    let mut reordered = numbers.clone();
    reordered.reverse();
    let again = table.install(&reordered).expect("identical reload is not an error");
    assert_eq!(again, SpellInstall::AlreadyInstalled(id));
    assert_eq!(table.dataset(), Some(id));
    let after: Vec<(i32, u32, f32)> = table
        .defs()
        .iter()
        .map(|def| (def.mana, def.cooldown_ticks, def.range))
        .collect();
    assert_eq!(before, after, "an identical reload changes nothing");

    // Rows whose id is not a typed spell do not change the identity of the
    // table that is actually installed.
    let mut with_unknown = numbers.clone();
    with_unknown.push(SpellNumbers {
        id: "ZZZZ",
        mana: 1,
        cooldown_ticks: 1,
        range: 1.0,
        radius: 1.0,
        duration_ticks: 1,
    });
    assert_eq!(SpellDatasetId::of(&with_unknown), id);
    assert_eq!(
        table.install(&with_unknown),
        Ok(SpellInstall::AlreadyInstalled(id))
    );
}

#[test]
fn aud023_identity_is_stable_and_covers_every_numeric_field() {
    let base = dataset(10, 30, 500.0);
    let id = SpellDatasetId::of(&base);
    assert_eq!(id, SpellDatasetId::of(&base), "same input, same id");
    assert_eq!(id.hex().len(), 16, "16 hex digits");

    for (label, changed) in [
        ("mana", {
            let mut rows = base.clone();
            rows[0].mana += 1;
            rows
        }),
        ("cooldown", {
            let mut rows = base.clone();
            rows[0].cooldown_ticks += 1;
            rows
        }),
        ("range", {
            let mut rows = base.clone();
            rows[0].range += 1.0;
            rows
        }),
        ("radius", {
            let mut rows = base.clone();
            rows[0].radius += 1.0;
            rows
        }),
        ("duration", {
            let mut rows = base.clone();
            rows[0].duration_ticks += 1;
            rows
        }),
        ("missing row", {
            let mut rows = base.clone();
            rows.pop();
            rows
        }),
    ] {
        assert_ne!(
            SpellDatasetId::of(&changed),
            id,
            "{label} must be part of the dataset identity"
        );
    }
}

#[test]
fn aud023_uninstalled_table_answers_from_the_fallback_table() {
    let table = SpellNumberTable::new();
    assert_eq!(table.dataset(), None);
    assert_eq!(
        table.defs().len(),
        SPELL_DEFS.len(),
        "before an install the hand-written table is active"
    );
    assert_eq!(table.defs()[0].mana, SPELL_DEFS[0].mana);
}

/// The process table's read-only contract: the accessor and the active table
/// agree, and this test never installs into the process table (other tests in
/// the same binary read `Spell::def()`).
#[test]
fn aud023_process_table_accessors_agree() {
    match wc3_sim::abilities::installed_spell_dataset() {
        Some(id) => assert_eq!(id.hex().len(), 16, "a dataset id is 16 hex digits"),
        None => assert_eq!(
            wc3_sim::abilities::spell_defs().len(),
            SPELL_DEFS.len(),
            "without an install the fallback table is active"
        ),
    }
    assert_eq!(wc3_sim::abilities::spell_defs().len(), SPELL_DEFS.len());
}
