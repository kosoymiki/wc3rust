//! Dump the typed ability classification (P0-3 evidence).
//!
//! Reads the operator's data root through the RE typed API and writes one row
//! per AbilityData entry: `executable` with its effect kind, or `data_only` with
//! the exact reason. Nothing is invented here — the classification is the same
//! one the simulation consumes.
//!
//! Usage:
//!   cargo run -p wc3-sim --example ability_classification -- \
//!     [data_root] [out.json]

use std::fmt::Write as _;
use std::path::Path;

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .unwrap_or_else(|| "analysis/war3x_data".to_string());
    let out = args
        .next()
        .unwrap_or_else(|| "analysis/agent-sim/ability_classification.json".to_string());

    let defs = wc3_sim::abilities::typed_ability_defs(Path::new(&root))
        .expect("ability classification from the data root");

    let mut rows = String::new();
    let mut executable = 0usize;
    let mut data_only = 0usize;
    for (index, def) in defs.iter().enumerate() {
        if index > 0 {
            rows.push(',');
        }
        let (class, effect, reason) = if let Some(kind) = def.effect_kind() {
            executable += 1;
            ("executable", format!("{kind:?}"), String::new())
        } else {
            data_only += 1;
            (
                "data_only",
                String::new(),
                def.data_only_reason()
                    .map(|reason| format!("{reason:?}"))
                    .unwrap_or_default(),
            )
        };
        let _ = write!(
            rows,
            "{{\"id\":\"{}\",\"name\":\"{}\",\"levels\":{},\"hero\":{},\"item\":{},\"class\":\"{}\",\"effect_kind\":\"{}\",\"reason\":\"{}\"}}",
            esc(&def.id),
            esc(&def.name),
            def.levels,
            def.hero,
            def.item,
            class,
            esc(&effect),
            esc(&reason)
        );
    }

    let document = format!(
        "{{\"schema\":\"wc3-ability-classification-v1\",\"source_root\":\"{}\",\"counts\":{{\"rows\":{},\"executable\":{},\"data_only\":{}}},\"rows\":[{}]}}\n",
        esc(&root),
        defs.len(),
        executable,
        data_only,
        rows
    );

    if let Some(parent) = Path::new(&out).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out, document).expect("write classification artifact");
    println!(
        "ability_classification rows={} executable={} data_only={} out={}",
        defs.len(),
        executable,
        data_only,
        out
    );
}
