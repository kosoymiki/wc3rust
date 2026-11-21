//! Per-chapter JASS boot ledger: which map calls which unimplemented native.
//!
//! `wc3_re::mapboot` reports the campaign-wide ledger; this example attributes
//! each fallthrough to the chapter that caused it, which is what a fix needs.
//! Boot settings match the boot matrix (`seed=0x5EED_1234`, 100 ticks at 30 Hz,
//! 32 MB script stack) so the counts line up with the aggregate.
//!
//! ```text
//! scripts/h2-cargo run -q -p wc3-jass --example campaign_boot_ledger --offline
//! ```
//!
//! Output: one `CHAPTER <file>` line per map, then the same line with the
//! unimplemented natives (`name=count`), then a `TOTAL` line.

use std::path::{Path, PathBuf};

fn install_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

fn chapter_maps() -> Vec<PathBuf> {
    let dir = install_root().join("Maps").join("Campaign");
    let mut maps: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("campaign tree {}: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "w3m" | "w3x"))
        })
        .collect();
    maps.sort();
    maps
}

fn main() {
    let root = install_root();
    let maps = chapter_maps();
    let results = std::thread::Builder::new()
        .name("campaign-boot-ledger".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut out: Vec<(String, Vec<(String, u64)>)> = Vec::new();
            for map in &maps {
                let mut config = wc3_re::engine::EngineConfig::new(&root);
                config.map = Some(map.clone());
                config.seed = 0x5EED_1234;
                let mut engine = wc3_re::engine::Engine::boot(config).expect("engine boot");
                for _ in 0..100 {
                    engine.tick(1.0 / 30.0);
                }
                let label = map
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let mut rows: Vec<(String, u64)> = engine
                    .runtime()
                    .map(|runtime| {
                        runtime
                            .unimplemented
                            .iter()
                            .map(|(name, count)| (name.clone(), *count))
                            .collect()
                    })
                    .unwrap_or_default();
                rows.sort();
                out.push((label, rows));
            }
            out
        })
        .expect("spawn")
        .join()
        .expect("join");

    let mut total: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for (label, rows) in &results {
        if rows.is_empty() {
            println!("CHAPTER {label} clean");
            continue;
        }
        let rendered: Vec<String> = rows
            .iter()
            .map(|(name, count)| format!("{name}={count}"))
            .collect();
        println!("CHAPTER {label} unimplemented={}", rendered.join(","));
        for (name, count) in rows {
            *total.entry(name.clone()).or_insert(0) += count;
        }
    }
    let rendered: Vec<String> = total
        .iter()
        .map(|(name, count)| format!("{name}={count}"))
        .collect();
    println!(
        "TOTAL maps={} distinct={} calls={} {}",
        results.len(),
        total.len(),
        total.values().sum::<u64>(),
        rendered.join(" ")
    );
}
