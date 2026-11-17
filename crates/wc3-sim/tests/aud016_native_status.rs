//! AUD-016 — the registry reports a semantic status, not "a handler exists".
//!
//! `NativeDef` keeps two facts apart: `dispatch` (a handler exists, what the VM
//! needs) and `status` + `status_reason` (how much of the native is actually
//! implemented). `registry::stats()` reports the per-status counts, the
//! coverage report (`native_status::report_rows`/`report_json`) carries every
//! row, and "no handler" is `Unsupported` — never `Partial`, never
//! `Implemented`.

use std::collections::BTreeMap;

use wc3_sim::jass::{NativeStatus, native_status, registry};

#[test]
fn aud016_every_registry_row_carries_one_status_and_a_reason_iff_it_is_a_gap() {
    let mut seen = BTreeMap::new();
    for def in registry::table() {
        assert!(
            seen.insert(def.name.to_ascii_lowercase(), def.status).is_none(),
            "duplicate registry row {}",
            def.name
        );
        if def.dispatch {
            assert_ne!(
                def.status,
                NativeStatus::Unsupported,
                "{} has a handler but is reported Unsupported",
                def.name
            );
        } else {
            assert_eq!(
                def.status,
                NativeStatus::Unsupported,
                "{} has no handler and must be Unsupported",
                def.name
            );
        }
        // A gap always explains itself; an implemented row has no reason to.
        assert_eq!(
            def.status_reason.is_empty(),
            !def.status.is_gap(),
            "{}: status={:?} reason={:?}",
            def.name,
            def.status,
            def.status_reason
        );
    }
    assert!(seen.len() >= 1600, "the registry table is complete");
}

#[test]
fn aud016_a_handler_alone_is_not_an_implementation() {
    // Natives the audit named as no-ops: the handler exists, the status says so.
    for name in [
        "SetTerrainFogExV",
        "BlzSetTerrainFogMaxLinearDensity",
        "BlzSetTerrainFogDrawOverSky",
        "SetHDWaterParamsEx",
        "SetSoundParamsFromLabel",
        "SetSoundDistances",
        "SetSoundDistanceCutoff",
        "SetSoundConeAngles",
        "SetSoundConeOrientation",
        "AddCameraBlocker",
        "BlzCameraSetupSetCameraType",
        "SetItemColor",
        "SetDestructableVertexColor",
        "SetPlayerRaceSkin",
        "SetDialogueTextKey",
        "UnitMakeAbilityPermanent",
    ] {
        // A few of these are Reforged-era handlers without a registry row
        // (`natives::HEADER_ABSENT_NATIVES`) — the status they carry is the same
        // classification, resolved from the handler table.
        let status = match registry::lookup(name) {
            Some(def) => {
                assert!(def.dispatch, "{name} has a handler");
                def.status
            }
            None => native_status::header_absent_rows()
                .iter()
                .find(|row| row.name.eq_ignore_ascii_case(name))
                .map(|row| row.status)
                .unwrap_or_else(|| panic!("{name} is neither a registry row nor a header-absent handler")),
        };
        assert_eq!(
            status,
            NativeStatus::NoOp,
            "{name} must be reported as a no-op, got {status:?}"
        );
        assert!(!registry::is_implemented(name), "{name} is not implemented");
    }

    // A row with no handler is `Unsupported` and is not counted as partial.
    let unsupported = registry::table()
        .iter()
        .filter(|def| def.status == NativeStatus::Unsupported)
        .count();
    assert!(unsupported > 0, "the registry has unimplemented rows");

    // The two metrics are different numbers now; that is the point of AUD-016.
    let stats = registry::stats();
    assert_eq!(
        stats.dispatch,
        registry::table().iter().filter(|def| def.dispatch).count()
    );
    assert!(
        stats.dispatch > stats.implemented,
        "dispatch={} implemented={} — no-op/partial/recorded rows must not be counted as implemented",
        stats.dispatch,
        stats.implemented
    );
    assert_eq!(
        stats.implemented + stats.partial + stats.recorded_only + stats.no_op + stats.unsupported,
        stats.total
    );
    assert_eq!(
        stats.partial + stats.recorded_only + stats.no_op + stats.unsupported,
        stats.total - stats.implemented,
        "the gap classes partition the non-implemented rows"
    );
}

#[test]
fn aud016_stats_and_the_status_module_agree() {
    let stats = registry::stats();
    let counts = native_status::counts();
    assert_eq!(stats.total, counts.total);
    assert_eq!(stats.implemented, counts.implemented);
    assert_eq!(stats.partial, counts.partial);
    assert_eq!(stats.recorded_only, counts.recorded_only);
    assert_eq!(stats.no_op, counts.no_op);
    assert_eq!(stats.unsupported, counts.unsupported);

    let rows = native_status::report_rows();
    assert_eq!(rows.len(), stats.total, "one report row per registry row");
    assert!(
        rows.windows(2).all(|pair| pair[0].name.to_ascii_lowercase() <= pair[1].name.to_ascii_lowercase()),
        "the report is sorted by name"
    );
    // The no-op rows appear in the report with their reason.
    let no_op = rows
        .iter()
        .find(|row| row.name.eq_ignore_ascii_case("SetSoundDistances"))
        .expect("SetSoundDistances in the report");
    assert_eq!(no_op.status, NativeStatus::NoOp);
    assert!(!no_op.reason.is_empty());
    // Every gap row carries a reason in the report too.
    for row in &rows {
        if row.status.is_gap() {
            assert!(!row.reason.is_empty(), "{} has no reason", row.name);
        }
    }
}

#[test]
fn aud016_json_report_lists_every_native_with_its_status() {
    let json = native_status::report_json();
    let stats = registry::stats();
    let handlers = stats.total + native_status::header_absent_rows().len();
    assert_eq!(
        json.matches("\"name\": \"").count(),
        handlers,
        "one object per registry row plus the header-absent handlers"
    );
    assert!(json.contains("\"summary\""));
    assert!(json.contains(&format!("\"total\": {}", stats.total)));
    assert!(json.contains(&format!("\"implemented\": {}", stats.implemented)));
    assert!(json.contains("\"status\": \"NoOp\""));
    assert!(json.contains("\"status\": \"Unsupported\""));
    // Balanced quotes are a cheap structural check without a JSON dependency.
    assert_eq!(
        json.matches('"').count() % 2,
        0,
        "quotes must pair up in the report"
    );
}
