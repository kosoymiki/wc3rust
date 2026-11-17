//! Validate every BLP/MDX in the archive (or an extracted tree) and write the
//! catalog reports.
//!
//! Usage:
//!   asset_catalog --mpq <War3.mpq> [--index <war3.index>] [--out <report dir>]
//!                 [--extracted <dir>] [--limit N] [--skip-models]
//!
//! `--extracted` validates the loose tree with dir provenance; `--mpq` reads
//! through the MPQ reader (the production path).

use std::path::PathBuf;
use std::time::Instant;

use wc3_assetdb::catalog::{self, CatalogOptions};
use wc3_assetdb::AssetDb;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq: Option<PathBuf> = std::env::var_os("WC3_MPQ").map(PathBuf::from);
    let mut index: Option<PathBuf> = None;
    let mut report_dir = PathBuf::from("analysis/agent-assets");
    let mut extracted: Option<PathBuf> = None;
    let mut limit: Option<usize> = None;
    let mut skip_models = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = Some(PathBuf::from(&args[i]));
            }
            "--index" => {
                i += 1;
                index = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                report_dir = PathBuf::from(&args[i]);
            }
            "--extracted" => {
                i += 1;
                extracted = Some(PathBuf::from(&args[i]));
            }
            "--limit" => {
                i += 1;
                limit = Some(args[i].parse()?);
            }
            "--skip-models" => skip_models = true,
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }

    let open_start = Instant::now();
    let (db, used_index) = match (&extracted, &mpq) {
        (Some(dir), _) => (AssetDb::open_dir(dir), false),
        (None, Some(path)) => match &index {
            Some(index_path) => AssetDb::open_mpq_best(path, index_path)?,
            None => (AssetDb::open_mpq(path)?, false),
        },
        (None, None) => return Err("provide --mpq or --extracted".into()),
    };
    let open_ms = open_start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "provider: files={} open_ms={open_ms:.2} index={used_index}",
        db.file_count()
    );

    let options = CatalogOptions {
        limit,
        skip_models,
        skip_textures: false,
    };
    let report = catalog::run(&db, &options);
    catalog::write_reports(&report, &report_dir)?;

    println!(
        "textures: files={} ok={} header_fail={} decode_fail={} mip_fail={} jpeg={} paletted={} alpha={} opaque={}",
        report.textures.files,
        report.textures.ok_count(),
        report.textures.header_fail,
        report.textures.decode_fail,
        report.textures.mip_fail,
        report.textures.jpeg,
        report.textures.paletted,
        report.textures.with_alpha,
        report.textures.fully_opaque
    );
    println!(
        "models: files={} parsed={} parse_fail={} refs={} replaceable={} unresolved={} models_with_unresolved={} all_resolved={}",
        report.models.files,
        report.models.parsed,
        report.models.parse_fail,
        report.models.texture_refs,
        report.models.replaceable_refs,
        report.models.unresolved_refs,
        report.models.models_with_unresolved,
        report.models.models_all_resolved
    );
    println!(
        "icons: btn={} disabled={} paired={} missing_dis={} orphan_dis={} ok={} fail={}",
        report.icons.btn,
        report.icons.disabled,
        report.icons.paired,
        report.icons.missing_disabled,
        report.icons.orphan_disabled,
        report.icons.ok,
        report.icons.failed
    );
    println!("biomes: {}", report.biomes.len());
    for row in &report.biomes {
        println!(
            "  {} files={} ok={} failed={} bytes={}",
            row.biome, row.files, row.ok, row.failed, row.bytes
        );
    }
    println!(
        "failures={} all_ok={} elapsed={:.2}s report_dir={}",
        report.failures.len(),
        report.all_ok(),
        report.elapsed_seconds,
        report_dir.display()
    );
    for failure in report.failures.iter().take(20) {
        println!("  FAIL {} [{}] {}", failure.name, failure.kind, failure.detail);
    }
    if !report.all_ok() {
        return Err(format!("{} catalog failures", report.failures.len()).into());
    }
    Ok(())
}
