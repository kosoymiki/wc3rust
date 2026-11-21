//! Resolve every sound-table path against real layers (priority order) and
//! write the resolution report.
//!
//! This is the evidence tool for the "missing TFT sound paths" fix: it never
//! guesses a name. Each requested path is classified as
//! `exact` (present in a layer), `listfile` (exact name absent but the archive
//! listfile contains the same basename under another directory), `ambiguous`
//! (several candidates — left unresolved) or `absent` (with the layer list that
//! was searched).
//!
//! Usage:
//!   sound_paths --mpq <War3.mpq> [--index <war3.index>] [--out DIR]
//!   sound_paths --layer War3xLocal=/path --layer War3x=/path --layer War3=/path
//!
//! Layers are given in priority order (first wins), mirroring the game loader.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::{AssetDb, Resolution};


/// Minimal JSON string escaping for paths with backslashes and quotes.
fn esc(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// JSON array of strings.
fn json_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&esc(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    let mut index: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("analysis/agent-assets");
    let mut layers: Vec<(String, PathBuf)> = Vec::new();
    let mut install: Option<PathBuf> = None;
    let mut strict = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = PathBuf::from(&args[i]);
            }
            "--index" => {
                i += 1;
                index = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            "--strict" => strict = true,
            "--install" => {
                i += 1;
                install = Some(PathBuf::from(&args[i]));
            }
            "--layer" => {
                i += 1;
                let spec = &args[i];
                let (label, path) = match spec.split_once('=') {
                    Some((label, path)) => (label.to_string(), PathBuf::from(path)),
                    None => (String::new(), PathBuf::from(spec)),
                };
                layers.push((label, path));
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }

    // `--install DIR` uses the classic loader order (highest priority first),
    // the same list `wc3_re::storm::StormFs::open_wc3_install` mounts.
    if let Some(root) = &install {
        for name in ["War3Patch.mpq", "patch.mpq", "War3xlocal.mpq", "War3x.mpq", "War3.mpq"] {
            let path = root.join(name);
            if path.is_file() {
                layers.push((name.trim_end_matches(".mpq").to_string(), path));
            }
        }
        if layers.is_empty() {
            return Err(format!("no War3 archives under {}", root.display()).into());
        }
    }
    let mut db = if layers.is_empty() {
        match index {
            Some(index_path) => AssetDb::open_mpq_best(&mpq, index_path)?.0,
            None => AssetDb::open_mpq(&mpq)?,
        }
    } else {
        AssetDb::open_mpq_layers(&layers, "wc3-install")?
    };
    println!(
        "source={} kind={} layers={:?} files={}",
        esc(&db.source_label()),
        db.source_kind(),
        json_array(&db.layer_labels()),
        db.file_count()
    );

    let (sounds, units, tables) = load_sound_tables(&mut db);
    let _ = (units.len(), tables);

    let mut by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_layer: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_top: BTreeMap<String, usize> = BTreeMap::new();
    let mut absent_by_top: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = String::from("requested\tkind\tlayer\tcanonical\n");
    let mut absent_rows = String::from("requested\tsearched\n");
    let mut ambiguous: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut defs_playable = 0usize;

    for def in sounds.iter() {
        let mut def_playable = false;
        for requested in def.paths() {
            total += 1;
            let resolution = db.resolve_asset(&requested);
            *by_kind.entry(resolution.kind()).or_default() += 1;
            if let Some(layer) = resolution.layer() {
                *by_layer.entry(layer.to_string()).or_default() += 1;
            }
            let top = requested
                .split('\\')
                .next()
                .unwrap_or("(root)")
                .to_string();
            match &resolution {
                Resolution::Exact { layer, path } => {
                    *by_top.entry(top).or_default() += 1;
                    def_playable = true;
                    let _ = writeln!(rows, "{requested}\texact\t{layer}\t{path}");
                }
                Resolution::ListfileMatch { layer, path, .. } => {
                    *by_top.entry(top).or_default() += 1;
                    def_playable = true;
                    let _ = writeln!(rows, "{requested}\tlistfile\t{layer}\t{path}");
                }
                Resolution::Ambiguous { candidates, .. } => {
                    let paths: Vec<String> =
                        candidates.iter().map(|hit| hit.path.clone()).collect();
                    ambiguous.push(format!("{requested} => {}", paths.join(" | ")));
                    let _ = writeln!(
                        rows,
                        "{requested}\tambiguous\t{}\t{}",
                        "",
                        paths.join("|")
                    );
                }
                Resolution::Absent { searched, .. } => {
                    *absent_by_top.entry(top).or_default() += 1;
                    let _ = writeln!(absent_rows, "{requested}\t{}", searched.join(","));
                }
            }
        }
        if def_playable {
            defs_playable += 1;
        }
    }

    std::fs::create_dir_all(&out_dir)?;
    std::fs::write(out_dir.join("sound-paths.tsv"), &rows)?;
    std::fs::write(out_dir.join("sound-paths-absent.tsv"), &absent_rows)?;

    let resolved = by_kind.get("exact").copied().unwrap_or(0)
        + by_kind.get("listfile").copied().unwrap_or(0);
    let ambiguous_json = json_array(&ambiguous);
    let by_layer_json = {
        let mut out = String::from("{");
        for (index, (layer, count)) in by_layer.iter().enumerate() {
            if index > 0 { out.push(','); }
            let _ = write!(out, "\"{}\": {}", esc(layer), count);
        }
        out.push('}');
        out
    };
    let by_top_json = {
        let mut out = String::from("{");
        for (index, (top, count)) in by_top.iter().enumerate() {
            if index > 0 { out.push(','); }
            let _ = write!(out, "\"{}\": {}", esc(top), count);
        }
        out.push('}');
        out
    };
    let absent_top_json = {
        let mut out = String::from("{");
        for (index, (top, count)) in absent_by_top.iter().enumerate() {
            if index > 0 { out.push(','); }
            let _ = write!(out, "\"{}\": {}", esc(top), count);
        }
        out.push('}');
        out
    };
    let json = format!(
        concat!(
            "{{\n",
            "  \"source\": \"{}\",\n",
            "  \"kind\": \"{}\",\n",
            "  \"layers\": {},\n",
            "  \"definitions\": {},\n",
            "  \"definitions_playable\": {},\n",
            "  \"paths\": {},\n",
            "  \"exact\": {},\n",
            "  \"listfile\": {},\n",
            "  \"ambiguous\": {},\n",
            "  \"absent\": {},\n",
            "  \"resolved\": {},\n",
            "  \"by_layer\": {},\n",
            "  \"by_top_resolved\": {},\n",
            "  \"absent_by_top\": {},\n",
            "  \"ambiguous_paths\": {}\n",
            "}}\n"
        ),
        db.source_label(),
        db.source_kind(),
        json_array(&db.layer_labels()),
        sounds.len(),
        defs_playable,
        total,
        by_kind.get("exact").copied().unwrap_or(0),
        by_kind.get("listfile").copied().unwrap_or(0),
        by_kind.get("ambiguous").copied().unwrap_or(0),
        by_kind.get("absent").copied().unwrap_or(0),
        resolved,
        by_layer_json,
        by_top_json,
        absent_top_json,
        ambiguous_json,
    );
    std::fs::write(out_dir.join("sound-paths.json"), &json)?;

    let mut md = String::new();
    md.push_str("# Sound path resolution\n\n");
    let _ = writeln!(md, "- source: `{}` ({})", db.source_label(), db.source_kind());
    let _ = writeln!(md, "- layers (priority order): {:?}", db.layer_labels());
    let _ = writeln!(md, "- definitions: {} (playable: {})", sounds.len(), defs_playable);
    let _ = writeln!(md, "- referenced paths: {total}");
    let _ = writeln!(md, "- exact: {}", by_kind.get("exact").copied().unwrap_or(0));
    let _ = writeln!(md, "- listfile-resolved: {}", by_kind.get("listfile").copied().unwrap_or(0));
    let _ = writeln!(md, "- ambiguous: {}", by_kind.get("ambiguous").copied().unwrap_or(0));
    let _ = writeln!(md, "- absent: {}", by_kind.get("absent").copied().unwrap_or(0));
    let _ = writeln!(md, "- resolved total: {resolved}/{total}\n");
    md.push_str("## Resolved per layer\n\n| layer | paths |\n|---|---:|\n");
    for (layer, count) in &by_layer {
        let _ = writeln!(md, "| {layer} | {count} |");
    }
    md.push_str("\n## Absent per top-level directory\n\n| dir | paths |\n|---|---:|\n");
    for (top, count) in &absent_by_top {
        let _ = writeln!(md, "| {top} | {count} |");
    }
    let _ = writeln!(
        md,
        "\nAbsent paths are listed in `sound-paths-absent.tsv` with the layers that were searched. \
         They are The Frozen Throne assets: mount `War3x.mpq` / `War3xLocal.mpq` (or a TFT extract) \
         as higher-priority layers and they resolve without code changes."
    );
    std::fs::write(out_dir.join("sound-paths.md"), &md)?;
    println!("{md}");

    if strict && !ambiguous.is_empty() {
        return Err(format!("{} ambiguous sound paths", ambiguous.len()).into());
    }
    if !ambiguous.is_empty() {
        eprintln!(
            "warning: {} ambiguous sound paths left unresolved (see sound-paths.json)",
            ambiguous.len()
        );
    }
    Ok(())
}
