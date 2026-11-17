//! `assets.models.all-units`: the generated audit must account for every
//! in-game roster model — each one either validated as a GLB or skipped with an
//! explicit reason. Nothing silently disappears from the count.
//!
//! Reads `analysis/agent-anim/model_audit.csv` (semicolon-separated summary of
//! `model_audit.json`) so the test has no JSON dependency.

use std::path::{Path, PathBuf};

fn audit_csv() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-anim/model_audit.csv")
}

#[derive(Debug)]
struct Row {
    model: String,
    status: String,
    reason: String,
    clips: usize,
    joints: u64,
    errors: u64,
    warnings: u64,
}

fn parse(path: &Path) -> Vec<Row> {
    let text = std::fs::read_to_string(path).expect("read model_audit.csv");
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        assert_eq!(fields.len(), 8, "bad audit row: {line}");
        rows.push(Row {
            model: fields[0].to_string(),
            status: fields[1].to_string(),
            reason: fields[2].to_string(),
            clips: fields[4].split('|').filter(|clip| !clip.is_empty()).count(),
            joints: fields[5].parse().expect("joints"),
            errors: fields[6].parse().expect("validator errors"),
            warnings: fields[7].parse().expect("validator warnings"),
        });
    }
    rows
}

#[test]
fn assets_models_all_units_audit_accounts_for_every_roster_model() {
    let path = audit_csv();
    if !path.is_file() {
        panic!(
            "model_audit.csv missing at {} (run the exporter CLI + model_audit.py)",
            path.display()
        );
    }
    let rows = parse(&path);
    // Roster model set (documented in model_audit.json):
    //   Units (non-portrait) 225 + Buildings (non-portrait) 123 + projectiles 67.
    assert_eq!(rows.len(), 415, "one audit row per in-game roster model");
    let validated = rows.iter().filter(|row| row.status == "validated").count();
    let skipped = rows.iter().filter(|row| row.status == "skipped").count();
    let failed = rows.iter().filter(|row| row.status == "failed").count();
    assert_eq!(
        validated + skipped + failed,
        rows.len(),
        "validated + skipped + failed must equal the roster set"
    );
    assert_eq!(failed, 0, "no failed roster models");
    assert!(
        validated >= 340,
        "expected nearly every roster model as a validated GLB, got {validated}/{}",
        rows.len()
    );
    for row in &rows {
        match row.status.as_str() {
            "validated" => {
                assert!(row.clips > 0, "{} validated without clips", row.model);
                assert!(row.joints > 0, "{} validated without joints", row.model);
                assert_eq!(row.errors, 0, "{} validator errors", row.model);
                assert_eq!(row.warnings, 0, "{} validator warnings", row.model);
            }
            "skipped" | "failed" => assert!(
                !row.reason.is_empty(),
                "{} is {} without a reason",
                row.model,
                row.status
            ),
            other => panic!("{}: unknown audit status {other}", row.model),
        }
    }
    eprintln!(
        "assets.models.all-units: {validated}/{} GLBs validated, {skipped} skipped with reason, {failed} failed",
        rows.len()
    );
}
