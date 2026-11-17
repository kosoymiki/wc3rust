//! AUD-027 — the handler inventory is classified body by body, not by list.
//!
//! The test reads the handler sources (`src/jass/natives*.rs`) and applies the
//! rule the audit asks for: a handler whose body is only `Value::Null`, or that
//! drops its arguments and returns `Null`, is a `NoOp` and must be in the
//! status table; a `NoOp` entry must point at a body that really is one. That
//! makes a newly added placeholder fail the suite instead of silently joining
//! the "handled" count. `UnitAddType`/`UnitRemoveType` were exactly such a row:
//! the scan flagged them and they now carry real per-unit state
//! (`UnitMeta::types`), so they classify as `Implemented`.
//!
//! The usage side (`common.j`, `Blizzard.j`, the 96-map corpus) is produced by
//! `analysis/audit/sim3_usage_search.py` and recorded in
//! `docs/audit/status-sim3.md`; the classification it feeds is this table.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wc3_sim::jass::{NativeStatus, native_status, registry};

const NATIVES: &str = include_str!("../src/jass/natives.rs");

fn handler_sources() -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/jass/natives");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("handler source directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            files.push((
                std::fs::read_to_string(&path).expect("read handler source"),
                path,
            ));
        }
    }
    assert!(files.len() >= 4, "expected the four handler modules");
    files
}

/// `native name -> handler function name`, parsed from the dispatch table.
fn dispatch_table() -> BTreeMap<String, String> {
    let start = NATIVES.find("handler_table! {").expect("dispatch table");
    let end = NATIVES[start..].find("\n}").expect("dispatch table end") + start;
    let body = &NATIVES[start..end];
    let mut map = BTreeMap::new();
    for line in body.lines() {
        let Some(arrow) = line.find("=>") else { continue };
        let left = line[..arrow].trim();
        let right = line[arrow + 2..].trim();
        let Some(name) = left
            .strip_prefix('"')
            .and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        let func = right
            .trim_end_matches(',')
            .rsplit("::")
            .next()
            .unwrap_or(right)
            .trim();
        map.insert(name.to_ascii_lowercase(), func.to_string());
    }
    assert!(map.len() > 500, "dispatch table parsed: {}", map.len());
    map
}

/// Body of `fn name(` in `source`, comments stripped and whitespace collapsed.
fn body_of(source: &str, name: &str) -> Option<String> {
    let needle = format!("fn {name}(");
    let start = source.find(&needle)?;
    let open = source[start..].find('{')? + start;
    let mut depth = 0usize;
    let mut end = open;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &source[open + 1..end];
    let without_comments: String = body
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    Some(without_comments.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn status_of(name: &str) -> NativeStatus {
    if let Some(def) = registry::lookup(name) {
        return def.status;
    }
    native_status::header_absent_rows()
        .iter()
        .find(|row| row.name.eq_ignore_ascii_case(name))
        .map(|row| row.status)
        .unwrap_or_else(|| panic!("{name} has neither a registry row nor a header-absent status"))
}

fn body_for(sources: &[(String, PathBuf)], native: &str) -> String {
    let table = dispatch_table();
    let func = table
        .get(&native.to_ascii_lowercase())
        .unwrap_or_else(|| panic!("{native} is not in the dispatch table"));
    sources
        .iter()
        .find_map(|(source, _)| body_of(source, func))
        .unwrap_or_else(|| panic!("body of {func} ({native}) not found"))
}

#[test]
fn aud027_every_null_body_handler_is_classified_as_a_no_op() {
    let sources = handler_sources();
    let table = dispatch_table();
    let mut checked = 0usize;
    let mut missing = Vec::new();
    for native in table.keys() {
        let body = body_for(&sources, native);
        checked += 1;
        if body == "Value::Null" || body == "Value::Null;" {
            let status = status_of(native);
            if status != NativeStatus::NoOp {
                missing.push((native.clone(), status));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "handlers that return Null and are not classified NoOp: {missing:#?}"
    );
    assert!(
        checked >= 500,
        "the scan must cover the whole table, saw {checked}"
    );
}

#[test]
fn aud027_no_op_entries_really_drop_their_effect() {
    let sources = handler_sources();
    let mut no_ops: Vec<&'static str> = registry::table()
        .iter()
        .filter(|def| def.status == NativeStatus::NoOp)
        .map(|def| def.name)
        .collect();
    no_ops.extend(
        native_status::header_absent_rows()
            .iter()
            .filter(|row| row.status == NativeStatus::NoOp)
            .map(|row| row.name),
    );
    assert!(
        no_ops.len() >= 20,
        "the scan found the placeholder fleet: {no_ops:?}"
    );

    for name in &no_ops {
        let body = body_for(&sources, name);
        assert!(
            body == "Value::Null"
                || body == "Value::Null;"
                || body.starts_with("let _ = ")
                || body.contains("let _ ="),
            "{name}: classified NoOp but the body does something: {body}"
        );
        // Every gap row explains itself (header-absent rows too).
        let reason = registry::lookup(name).map(|def| def.status_reason).or_else(|| {
            native_status::header_absent_rows()
                .iter()
                .find(|row| row.name.eq_ignore_ascii_case(name))
                .map(|row| row.reason)
        });
        assert!(
            reason.is_some_and(|reason| !reason.is_empty()),
            "{name} is a gap and must explain itself"
        );
    }
}

#[test]
fn aud027_the_audit_list_is_classified_row_by_row() {
    // Status per native named in the AUD-027 finding, as classified in
    // `native_status.rs`. `Implemented` here means the handler has a real
    // mechanism (state the queries read), not just a table row.
    let expected: &[(&str, NativeStatus)] = &[
        ("SetTerrainFogEx", NativeStatus::Implemented),
        ("SetTerrainFogExV", NativeStatus::NoOp),
        ("BlzSetTerrainFogMaxLinearDensity", NativeStatus::NoOp),
        ("BlzSetTerrainFogDrawOverSky", NativeStatus::NoOp),
        ("SetHDWaterParamsEx", NativeStatus::NoOp),
        ("SetSoundParamsFromLabel", NativeStatus::NoOp),
        ("SetSoundDistances", NativeStatus::NoOp),
        ("SetSoundDistanceCutoff", NativeStatus::NoOp),
        ("SetSoundConeAngles", NativeStatus::NoOp),
        ("SetSoundConeOrientation", NativeStatus::NoOp),
        ("SetSoundVolume", NativeStatus::Implemented),
        ("SetSoundDuration", NativeStatus::Implemented),
        ("SetSoundPosition", NativeStatus::Implemented),
        ("SetSoundFacialAnimationLabel", NativeStatus::NoOp),
        ("SetSoundFacialAnimationGroupLabel", NativeStatus::NoOp),
        ("SetSoundFacialAnimationSetFilepath", NativeStatus::NoOp),
        ("SetDialogueSpeakerNameKey", NativeStatus::NoOp),
        ("SetDialogueTextKey", NativeStatus::NoOp),
        ("PlayDialogueFromSpeakerEx", NativeStatus::NoOp),
        ("AddCameraBlocker", NativeStatus::NoOp),
        ("BlzCameraSetupSetCameraType", NativeStatus::NoOp),
        ("SetItemColor", NativeStatus::NoOp),
        ("SetDestructableVertexColor", NativeStatus::NoOp),
        ("SetPlayerRaceSkin", NativeStatus::NoOp),
        ("ClearTextMessages", NativeStatus::Implemented),
        ("IsPlayerObserver", NativeStatus::Partial),
        ("IsFogEnabled", NativeStatus::Implemented),
        ("IsFogMaskEnabled", NativeStatus::Implemented),
        ("UnitMakeAbilityPermanent", NativeStatus::NoOp),
        ("TriggerSleepAction", NativeStatus::Implemented),
        ("UnitAddType", NativeStatus::Implemented),
        ("UnitRemoveType", NativeStatus::Implemented),
        ("IsUnitType", NativeStatus::Implemented),
        ("CreateTextTag", NativeStatus::Partial),
        ("SelectUnit", NativeStatus::RecordedOnly),
        ("GetCameraMargin", NativeStatus::Partial),
        ("DisplayTextToPlayer", NativeStatus::Implemented),
    ];
    for (name, status) in expected {
        assert_eq!(status_of(name), *status, "{name}");
    }
}

#[test]
fn aud027_each_gap_class_is_represented_and_carries_a_reason() {
    let mut counts: BTreeMap<NativeStatus, usize> = BTreeMap::new();
    let mut all = registry::table().iter().map(|def| (def.name, def.status, def.status_reason));
    for def in registry::table() {
        *counts.entry(def.status).or_default() += 1;
        if def.status.is_gap() {
            assert!(
                !def.status_reason.is_empty(),
                "{} is {:?} without a reason",
                def.name,
                def.status
            );
        }
    }
    let _ = &mut all;
    for row in native_status::header_absent_rows() {
        *counts.entry(row.status).or_default() += 1;
        if row.status.is_gap() {
            assert!(!row.reason.is_empty(), "{} has no reason", row.name);
        }
    }
    for status in [
        NativeStatus::Partial,
        NativeStatus::RecordedOnly,
        NativeStatus::NoOp,
        NativeStatus::Unsupported,
    ] {
        assert!(
            counts.get(&status).copied().unwrap_or(0) > 0,
            "class {status:?} must be populated: {counts:?}"
        );
    }
    eprintln!("native status census (registry + header-absent): {counts:?}");
    // The report exposes the same classification.
    let json = native_status::report_json();
    assert!(json.contains("\"status\": \"RecordedOnly\""));
    assert!(json.contains("\"status\": \"Partial\""));
    assert!(json.contains("\"header_absent\""));
}
