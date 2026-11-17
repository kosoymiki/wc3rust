//! `wc3re` command-line access to the reverse-engineering layer.
//!
//! Usage:
//!   wc3re scan [root] [--maps DIR] [--json OUT] [--md OUT]
//!   wc3re jass <file.j>...
//!   wc3re map <map.w3m|w3x>
//!   wc3re parity [maps-dir] [--json OUT] [--md OUT]
//!   wc3re objects [root]
//!   wc3re ai [scripts-dir]
//!   wc3re storm <archive> [inner/path]
//!   wc3re info

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wc3_re::ai::scan_ai_dir;
use wc3_re::jass::{Program, Runtime, parse};
use wc3_re::mapfiles::MapBundle;
use wc3_re::objectdata::ObjectDb;
use wc3_re::scan::ScanReport;
use wc3_re::storm::StormFs;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("info");
    let result = match command {
        "scan" => scan(&args[1..]),
        "jass" => jass(&args[1..]),
        "map" => map(&args[1..]),
        "parity" => parity(&args[1..]),
        "objects" => objects(&args[1..]),
        "ai" => ai(&args[1..]),
        "storm" => storm(&args[1..]),
        "extract" => extract(&args[1..]),
        "manifest" => manifest(&args[1..]),
        "gameplay" => gameplay(&args[1..]),
        "cutscenes" => cutscenes(&args[1..]),
        "abilities" => abilities(&args[1..]),
        "cinematics" => cinematics(&args[1..]),
        "art" => art(&args[1..]),
        "catalog" => catalog(&args[1..]),
        "consumption" => consumption(&args[1..]),
        "map-content" => map_content(&args[1..]),
        "map-boot" => map_boot(&args[1..]),
        "info" => {
            info();
            Ok(())
        }
        other => {
            eprintln!(
                "unknown command {other:?}; use scan|jass|map|parity|objects|ai|storm|extract|manifest|gameplay|cutscenes|abilities|cinematics|art|catalog|consumption|map-content|map-boot|info"
            );
            std::process::exit(2);
        }
    };
    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

fn scan(args: &[String]) -> wc3_re::Result<()> {
    let mut root = default_root();
    let mut maps: Option<PathBuf> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut index = 0;
    if let Some(first) = args.first()
        && !first.starts_with("--")
    {
        root = PathBuf::from(first);
        index = 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--maps" => {
                index += 1;
                maps = args.get(index).map(PathBuf::from);
            }
            "--json" => {
                index += 1;
                json_out = args.get(index).map(PathBuf::from);
            }
            "--md" => {
                index += 1;
                md_out = args.get(index).map(PathBuf::from);
            }
            other => {
                eprintln!("ignoring unknown scan option {other}");
            }
        }
        index += 1;
    }
    let maps = maps.unwrap_or_else(|| root.join("Maps"));
    let report = ScanReport::run(&root, &maps, 0x5EED_1234)?;
    if let Some(path) = json_out {
        std::fs::write(&path, report.to_json())?;
        eprintln!("wrote {}", path.display());
    }
    let markdown = report.to_markdown();
    if let Some(path) = md_out {
        std::fs::write(&path, &markdown)?;
        eprintln!("wrote {}", path.display());
    }
    println!("{markdown}");
    if !report.ok() {
        std::process::exit(1);
    }
    Ok(())
}

fn jass(args: &[String]) -> wc3_re::Result<()> {
    if args.is_empty() {
        return Err(wc3_re::ReError::Unsupported("usage: wc3re jass <file.j>...".into()));
    }
    let mut program = Program::default();
    let mut files = 0;
    for path in args {
        let source = std::fs::read_to_string(path)?;
        let parsed = parse(&source)
            .map_err(|err| wc3_re::ReError::Parse(format!("{path}: {err}")))?;
        files += 1;
        program.merge(parsed);
    }
    let mut runtime = Runtime::new(program, 0x5EED_1234);
    let mut results = BTreeMap::new();
    let names: Vec<String> = runtime.function_names().into_iter().collect();
    for name in names {
        if name.eq_ignore_ascii_case("main") || name.eq_ignore_ascii_case("config") {
            let value = runtime.run_function(&name);
            results.insert(name.clone(), value.map(|_| "ok"));
        }
    }
    println!("files: {files}");
    println!("functions: {}", runtime.function_names().len());
    println!("natives: {}", runtime.native_names().len());
    println!("steps: {}", runtime.steps);
    println!("output lines: {}", runtime.output.len());
    println!("entry results: {results:?}");
    println!("unimplemented natives:");
    for (name, count) in runtime.unimplemented.iter().take(50) {
        println!("  {name}: {count}");
    }
    for error in runtime.errors.iter().take(20) {
        println!("error: {error}");
    }
    Ok(())
}

fn map(args: &[String]) -> wc3_re::Result<()> {
    let path = args
        .first()
        .ok_or_else(|| wc3_re::ReError::Unsupported("usage: wc3re map <file>".into()))?;
    let bundle = MapBundle::open(path)?;
    println!("map: {}", bundle.path.display());
    println!("mpq_base: 0x{:x}", bundle.mpq_base);
    println!("inner files: {}", bundle.entries.len());
    for entry in &bundle.entries {
        println!("  {:40} {} bytes", entry.name, entry.size);
    }
    if let Some(info) = bundle.localized_info() {
        println!(
            "info: name={:?} author={:?} players={:?}",
            info.name, info.author, info.recommended_players
        );
    }
    if let Some(terrain) = &bundle.terrain {
        println!("terrain: {}x{} tiles={}", terrain.width, terrain.height, terrain.tiles.len());
    }
    println!("objects: {} entries", bundle.object_entries());
    println!("script bytes: {}", bundle.script.as_ref().map_or(0, String::len));
    if !bundle.errors.is_empty() {
        println!("errors: {:?}", bundle.errors);
    }
    Ok(())
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

struct MapParity {
    map: String,
    main_ok: bool,
    functions: usize,
    triggers: usize,
    trigger_actions: usize,
    executed_triggers: usize,
    executed_actions: u32,
    map_units: usize,
    spawned_units: usize,
    matched_units: usize,
    map_doodads: usize,
    spawned_doodads: usize,
    matched_doodads: usize,
    world_units: usize,
    world_destructables: usize,
    error: Option<String>,
}

fn parity(args: &[String]) -> wc3_re::Result<()> {
    use wc3_re::engine::{Engine, EngineConfig};
    let mut maps_dir = default_root().join("Maps");
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut index = 0;
    if let Some(first) = args.first()
        && !first.starts_with("--")
    {
        maps_dir = PathBuf::from(first);
        index = 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--md" => {
                md_out = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            other => {
                return Err(wc3_re::ReError::Unsupported(format!(
                    "parity: unknown argument {other:?}"
                )));
            }
        }
    }
    let root = default_root();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("w3m") || ext.eq_ignore_ascii_case("w3x")
            }) {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&maps_dir, &mut paths);
    let mut rows = Vec::with_capacity(paths.len());
    for path in &paths {
        let mut config = EngineConfig::new(&root);
        config.map = Some(path.clone());
        config.seed = 0x5EED_1234;
        match Engine::boot(config) {
            Ok(engine) => {
                let script = engine.script_report();
                let spawn = engine.map_spawn().clone();
                let world_units = engine
                    .runtime()
                    .map_or(0, |runtime| runtime.world.units.len());
                let world_destructables = engine
                    .runtime()
                    .map_or(0, |runtime| runtime.world.destructables.len());
                rows.push(MapParity {
                    map: path.display().to_string(),
                    main_ok: script.main_ok,
                    functions: script.functions,
                    triggers: script.triggers,
                    trigger_actions: script.trigger_actions,
                    executed_triggers: script.executed_triggers,
                    executed_actions: script.executed_actions,
                    map_units: spawn.map_units,
                    spawned_units: spawn.spawned_units,
                    matched_units: spawn.matched_units,
                    map_doodads: spawn.map_doodads,
                    spawned_doodads: spawn.spawned_doodads,
                    matched_doodads: spawn.matched_doodads,
                    world_units,
                    world_destructables,
                    error: None,
                });
            }
            Err(error) => rows.push(MapParity {
                map: path.display().to_string(),
                main_ok: false,
                functions: 0,
                triggers: 0,
                trigger_actions: 0,
                executed_triggers: 0,
                executed_actions: 0,
                map_units: 0,
                spawned_units: 0,
                matched_units: 0,
                map_doodads: 0,
                spawned_doodads: 0,
                matched_doodads: 0,
                world_units: 0,
                world_destructables: 0,
                error: Some(error.to_string()),
            }),
        }
    }
    let main_ok = rows.iter().filter(|row| row.main_ok).count();
    let executed_triggers: usize = rows.iter().map(|row| row.executed_triggers).sum();
    let map_units: usize = rows.iter().map(|row| row.map_units).sum();
    let spawned_units: usize = rows.iter().map(|row| row.spawned_units).sum();
    let matched_units: usize = rows.iter().map(|row| row.matched_units).sum();
    let map_doodads: usize = rows.iter().map(|row| row.map_doodads).sum();
    let mut json = String::new();
    json.push_str(&format!(
        "{{\n  \"ok\": {},\n  \"maps\": {},\n  \"main_ok\": {},\n  \"executed_triggers\": {},\n  \"map_units\": {},\n  \"spawned_units\": {},\n  \"matched_units\": {},\n  \"map_doodads\": {},\n  \"files\": [",
        main_ok == rows.len(),
        rows.len(),
        main_ok,
        executed_triggers,
        map_units,
        spawned_units,
        matched_units,
        map_doodads
    ));
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "\n    {{\"map\": \"{}\", \"main_ok\": {}, \"functions\": {}, \"triggers\": {}, \"trigger_actions\": {}, \"executed_triggers\": {}, \"executed_actions\": {}, \"map_units\": {}, \"spawned_units\": {}, \"matched_units\": {}, \"map_doodads\": {}, \"spawned_doodads\": {}, \"matched_doodads\": {}, \"world_units\": {}, \"world_destructables\": {}{}}}",
            json_escape(&row.map),
            row.main_ok,
            row.functions,
            row.triggers,
            row.trigger_actions,
            row.executed_triggers,
            row.executed_actions,
            row.map_units,
            row.spawned_units,
            row.matched_units,
            row.map_doodads,
            row.spawned_doodads,
            row.matched_doodads,
            row.world_units,
            row.world_destructables,
            row.error
                .as_ref()
                .map(|error| format!(", \"error\": \"{}\"", json_escape(error)))
                .unwrap_or_default(),
        ));
    }
    json.push_str("\n  ]\n}\n");
    if let Some(out) = &json_out {
        std::fs::write(out, &json)?;
        println!("wrote {}", out.display());
    } else {
        print!("{json}");
    }
    if let Some(out) = &md_out {
        let mut md = String::from("# Map runtime parity — war3map.j + war3mapUnits.doo\n\n");
        md.push_str(&format!(
            "maps: **{main_ok}/{}** main ok · executed triggers: **{executed_triggers}** · placements: **{map_units}** · spawned: **{spawned_units}** · matched to script units: **{matched_units}** · doodads: **{map_doodads}**\n\n",
            rows.len()
        ));
        md.push_str(
            "| map | main | triggers | exec | map units | spawned | matched | world units |\n",
        );
        md.push_str("|---|---:|---:|---:|---:|---:|---:|---:|\n");
        for row in &rows {
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                Path::new(&row.map)
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                row.main_ok,
                row.triggers,
                row.executed_triggers,
                row.map_units,
                row.spawned_units,
                row.matched_units,
                row.world_units,
            ));
        }
        std::fs::write(out, md)?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

fn objects(args: &[String]) -> wc3_re::Result<()> {
    let root = args.first().map(PathBuf::from).unwrap_or_else(default_root);
    let db = ObjectDb::load_dir(&root)?;
    println!("sheets: {}", db.sheets.len());
    for (name, count) in db.sheet_counts() {
        println!("  {name}: {count} rows");
    }
    println!("objects: {}", db.objects.len());
    for (kind, count) in db.counts() {
        println!("  {kind}: {count}");
    }
    println!("metadata fields: {}", db.metadata.len());
    println!("icons: {}", db.icon_paths().len());
    if let Some(record) = db.record("hfoo") {
        println!("hfoo Art: {:?}", record.field("Art"));
        println!("hfoo HP: {:?}", record.slk_field("UnitBalance", "HP"));
    }
    Ok(())
}

fn ai(args: &[String]) -> wc3_re::Result<()> {
    let dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_root().join("Scripts"));
    let census = scan_ai_dir(&dir, 0x5EED_1234)?;
    println!("scripts: {}", census.scripts);
    println!("parsed: {}", census.parsed);
    println!("failed: {}", census.failed.len());
    for (name, error) in census.failed.iter().take(10) {
        println!("  {name}: {error}");
    }
    println!("functions: {}", census.functions);
    println!("natives: {}", census.natives);
    println!("common.ai functions: {}", census.common_ai_functions);
    println!("InitAI runs: {}", census.init_runs);
    for (name, error) in census.init_failures.iter().take(10) {
        println!("  init failure {name}: {error}");
    }
    Ok(())
}

fn storm(args: &[String]) -> wc3_re::Result<()> {
    let Some(path) = args.first() else {
        return Err(wc3_re::ReError::Unsupported(
            "usage: wc3re storm <archive> [inner/path]".into(),
        ));
    };
    let mut fs = StormFs::new();
    let count = fs.add_archive("archive", path)?;
    println!("archive: {path}");
    println!("listfile entries: {count}");
    for (label, files, blocks) in fs.archive_file_counts() {
        println!("  {label}: listfile={files} blocks={blocks}");
    }
    if let Some(inner) = args.get(1) {
        let bytes = fs.read(inner)?;
        println!("read {inner}: {} bytes", bytes.len());
        if let Some(info) = fs.info(inner) {
            println!("info: {info:?}");
        }
    }
    Ok(())
}

fn extract(args: &[String]) -> wc3_re::Result<()> {
    if args.len() < 3 {
        return Err(wc3_re::ReError::Unsupported(
            "usage: wc3re extract <archive> <inner/path> <out>".into(),
        ));
    }
    let bytes = wc3_re::mpqex::MpqEx::read_at(&args[0], &args[1])
        .or_else(|_| {
            let mut fs = StormFs::new();
            fs.add_archive("archive", &args[0])?;
            fs.read(&args[1])
        })?;
    std::fs::write(&args[2], &bytes)?;
    println!("wrote {} bytes to {}", bytes.len(), args[2]);
    Ok(())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn phase0_dir(root: &Path) -> PathBuf {
    root.join("analysis/agent-re/phase0")
}

fn parse_phase0_args(
    args: &[String],
) -> (PathBuf, Vec<PathBuf>, Option<PathBuf>, Option<PathBuf>, PathBuf) {
    let mut root = repo_root();
    let mut archives = Vec::new();
    let mut json = None;
    let mut md = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--archive" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    archives.push(PathBuf::from(value));
                }
            }
            "--json" => {
                index += 1;
                json = args.get(index).map(PathBuf::from);
            }
            "--md" => {
                index += 1;
                md = args.get(index).map(PathBuf::from);
            }
            "--out-dir" => {
                index += 1;
                output = args.get(index).map(PathBuf::from);
            }
            other => {
                eprintln!("ignoring unknown phase0 argument {other:?}");
            }
        }
        index += 1;
    }
    let dir = output.unwrap_or_else(|| phase0_dir(&root));
    (root, archives, json, md, dir)
}

fn manifest(args: &[String]) -> wc3_re::Result<()> {
    let (root, mut archives, json, md, dir) = parse_phase0_args(args);
    let json = json.unwrap_or_else(|| dir.join("manifest.json"));
    let md = md.unwrap_or_else(|| dir.join("MANIFEST.md"));
    if archives.is_empty() {
        archives = wc3_re::manifest::default_archives(&root);
    }
    let manifest = wc3_re::manifest::build(&root, &archives)?;
    wc3_re::manifest::write_artifacts(&manifest, &json, &md)?;
    println!(
        "manifest files={} archives={} payload_bytes={} read_failed={} ok={} json={} md={}",
        manifest.total_files,
        manifest.archives.len(),
        manifest.total_payload_bytes,
        manifest.total_read_failed,
        manifest.ok(),
        json.display(),
        md.display()
    );
    Ok(())
}

fn gameplay(args: &[String]) -> wc3_re::Result<()> {
    let (root, _archives, json, md, dir) = parse_phase0_args(args);
    let json = json.unwrap_or_else(|| dir.join("constants.json"));
    let md = md.unwrap_or_else(|| dir.join("CONSTANTS.md"));
    let report = wc3_re::gameplay::build(&root)?;
    wc3_re::gameplay::write_artifacts(&report, &json, &md)?;
    println!(
        "gameplay units={} weapons={} abilities={} items={} misc={} ok={} json={} md={}",
        report.counts.get("units").copied().unwrap_or(0),
        report.counts.get("weapons").copied().unwrap_or(0),
        report.counts.get("abilities").copied().unwrap_or(0),
        report.counts.get("items").copied().unwrap_or(0),
        report.counts.get("misc_constants").copied().unwrap_or(0),
        report.ok(),
        json.display(),
        md.display()
    );
    Ok(())
}

fn cutscenes(args: &[String]) -> wc3_re::Result<()> {
    let (root, _archives, json, md, dir) = parse_phase0_args(args);
    let json = json.unwrap_or_else(|| dir.join("cutscenes.json"));
    let md = md.unwrap_or_else(|| dir.join("CUTSCENES.md"));
    let inventory = wc3_re::cutscenes::build(&root)?;
    wc3_re::cutscenes::write_artifacts(&inventory, &json, &md)?;
    println!(
        "cutscenes maps={} ok={} cinematic={} transmissions={} cameras={} regions={} sounds={} json={} md={}",
        inventory.maps.len(),
        inventory.ok(),
        inventory.totals.get("cinematic_calls").copied().unwrap_or(0),
        inventory.totals.get("transmissions").copied().unwrap_or(0),
        inventory.totals.get("cameras").copied().unwrap_or(0),
        inventory.totals.get("regions").copied().unwrap_or(0),
        inventory.totals.get("sounds").copied().unwrap_or(0),
        json.display(),
        md.display()
    );
    Ok(())
}

fn abilities(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut json = false;
    let mut out: Option<PathBuf> = None;
    let mut run_demo = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--json" => json = true,
            "--out" => {
                index += 1;
                out = args.get(index).map(PathBuf::from);
            }
            "--run-demo" => run_demo = true,
            other => eprintln!("ignoring unknown abilities argument {other:?}"),
        }
        index += 1;
    }
    let defs = wc3_re::abilities::defs_for_root(&root)?;
    let summary = wc3_re::abilities::AbilitySummary::from_defs(&defs);
    if run_demo {
        use wc3_re::abilities::{
            AbilityClass, EffectKind, EffectTarget, EffectUnit, EffectWorld, apply_effect,
        };
        let mut world = EffectWorld::default();
        world.units.insert(1, EffectUnit::new(0, (0.0, 0.0), 500.0));
        world.units.insert(2, EffectUnit::new(1, (80.0, 0.0), 500.0));
        for kind in [
            EffectKind::Damage,
            EffectKind::Heal,
            EffectKind::Buff,
            EffectKind::Aura,
            EffectKind::Movement,
            EffectKind::Summon,
        ] {
            if let Some(def) = defs.iter().find(|def| matches!(def.class, AbilityClass::Executable(k) if k == kind)) {
                let target = match kind {
                    EffectKind::Damage => EffectTarget::Point(80.0, 0.0),
                    EffectKind::Heal | EffectKind::Buff | EffectKind::Movement => {
                        EffectTarget::Unit(1)
                    }
                    EffectKind::Aura | EffectKind::Summon => EffectTarget::None,
                };
                match apply_effect(def, 1, target, &mut world) {
                    Ok(outcome) => println!(
                        "abilities run {} kind={} affected={} damage={} healing={} buffs={} summons={}",
                        def.id,
                        kind.as_str(),
                        outcome.affected,
                        outcome.damage,
                        outcome.healing,
                        outcome.buffs_applied,
                        outcome.summons
                    ),
                    Err(error) => println!("abilities run {} skipped: {error}", def.id),
                }
            }
        }
    }
    if json {
        let mut body = String::new();
        body.push_str(&format!(
            "{{\"schema\":\"wc3-abilities-v1\",\"summary\":{},\"abilities\":[",
            summary.to_json()
        ));
        for (index, def) in defs.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            body.push_str(&def.to_json());
        }
        body.push_str("]}");
        if let Some(path) = out {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &body)?;
            println!("abilities total={} executable={} data_only={} wrote {}", summary.total, summary.executable, summary.data_only, path.display());
        } else {
            println!("{body}");
        }
    } else {
        println!(
            "abilities total={} executable={} data_only={}",
            summary.total, summary.executable, summary.data_only
        );
        for (kind, count) in &summary.by_effect {
            println!("  effect {kind}: {count}");
        }
        for (reason, count) in &summary.by_reason {
            println!("  data-only {reason}: {count}");
        }
        if let Some(path) = out {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, summary.to_json())?;
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}

fn cinematics(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut out = PathBuf::from("analysis/agent-re/phase0/cinematics.json");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--out" | "--json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    out = PathBuf::from(value);
                }
            }
            other => eprintln!("ignoring unknown cinematics argument {other:?}"),
        }
        index += 1;
    }
    let inventory = wc3_re::cutscenes::cinematic_events(&root)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, inventory.to_json())?;
    println!(
        "cinematics maps={} cinematic={} transmissions={} cameras={} fades={} events={} ok={} out={}",
        inventory.maps,
        inventory.counts.get("cinematic").copied().unwrap_or(0),
        inventory.counts.get("transmission").copied().unwrap_or(0),
        inventory.counts.get("camera").copied().unwrap_or(0),
        inventory.counts.get("fade").copied().unwrap_or(0),
        inventory.events.len(),
        inventory.ok(),
        out.display()
    );
    Ok(())
}

fn art(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut json = PathBuf::from("analysis/agent-re/phase0/art.json");
    let mut md = PathBuf::from("analysis/agent-re/phase0/ART.md");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    json = PathBuf::from(value);
                }
            }
            "--md" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    md = PathBuf::from(value);
                }
            }
            other => eprintln!("ignoring unknown art argument {other:?}"),
        }
        index += 1;
    }
    let report = wc3_re::art::build(&root)?;
    wc3_re::art::write_artifacts(&report, &json, &md)?;
    println!("art families={} json={} md={}", report.families.len(), json.display(), md.display());
    for family in &report.families {
        let missing: usize = family.fields.values().map(|field| field.missing.len()).sum();
        println!(
            "  {} total={} missing={} fields={}",
            family.name,
            family.total,
            missing,
            family.fields.len()
        );
        for (field, stat) in &family.fields {
            if !stat.missing.is_empty() || stat.closed > 0 {
                println!(
                    "    {field}: have={} closed={} missing={}",
                    stat.have,
                    stat.closed,
                    stat.missing.len()
                );
            }
        }
    }
    Ok(())
}

fn catalog(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut json = false;
    let mut out: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--json" => json = true,
            "--out" => {
                index += 1;
                out = args.get(index).map(PathBuf::from);
            }
            other => eprintln!("ignoring unknown catalog argument {other:?}"),
        }
        index += 1;
    }
    let summary = wc3_re::catalog::summary_for_root(&root)?;
    if json || out.is_some() {
        let body = format!(
            "{{\"schema\":\"wc3-catalog-v1\",\"summary\":{}}}",
            summary.to_json()
        );
        if let Some(path) = out {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &body)?;
            println!("catalog {} wrote {}", summary.to_json(), path.display());
            return Ok(());
        }
        println!("{body}");
    } else {
        println!(
            "catalog abilities={} units={} weapons={} items={} upgrades={} effects={} buffs={} unparsed={}",
            summary.abilities,
            summary.units,
            summary.weapons,
            summary.items,
            summary.upgrades,
            summary.effects,
            summary.buffs,
            summary.unparsed
        );
    }
    Ok(())
}

fn consumption(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut json = PathBuf::from("analysis/agent-re/phase0/consumption.json");
    let mut md = PathBuf::from("analysis/agent-re/phase0/CONSUMPTION.md");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    json = PathBuf::from(value);
                }
            }
            "--md" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    md = PathBuf::from(value);
                }
            }
            other => eprintln!("ignoring unknown consumption argument {other:?}"),
        }
        index += 1;
    }
    let report = wc3_re::consumption::build(&root)?;
    wc3_re::consumption::write_artifacts(&report, &json, &md)?;
    println!(
        "consumption entries={} archives={} runtime={} data_only={} unused={} json={} md={}",
        report.total,
        report.archives,
        report.by_class.get("runtime").copied().unwrap_or(0),
        report.by_class.get("data_only").copied().unwrap_or(0),
        report.by_class.get("unused").copied().unwrap_or(0),
        json.display(),
        md.display()
    );
    Ok(())
}

fn map_content(args: &[String]) -> wc3_re::Result<()> {
    let mut root = repo_root();
    let mut one: Option<PathBuf> = None;
    let mut json = PathBuf::from("analysis/agent-re/phase0/mapcontent.json");
    let mut md = PathBuf::from("analysis/agent-re/phase0/MAPCONTENT.md");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    root = PathBuf::from(value);
                }
            }
            "--map" => {
                index += 1;
                one = args.get(index).map(PathBuf::from);
            }
            "--json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    json = PathBuf::from(value);
                }
            }
            "--md" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    md = PathBuf::from(value);
                }
            }
            other => eprintln!("ignoring unknown map-content argument {other:?}"),
        }
        index += 1;
    }
    let db = wc3_re::catalog::database(&root)?;
    if let Some(path) = one {
        let content = wc3_re::mapcontent::inspect(&path, Some(&db))?;
        println!(
            "map-content map={} starts={} mines={} shops={} creeps={} units={} doodads={} regions={} cameras={} sounds={}",
            content.path,
            content.starts.len(),
            content.gold_mines,
            content.shops,
            content.creeps,
            content.units,
            content.doodads,
            content.regions,
            content.cameras,
            content.sounds
        );
        if let Some(parent) = json.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&json, content.to_json())?;
    } else {
        let report = wc3_re::mapcontent::all(&root)?;
        wc3_re::mapcontent::write_artifacts(&report, &json, &md)?;
        println!(
            "map-content maps={} json={} md={}",
            report.maps.len(),
            json.display(),
            md.display()
        );
        if let Some(ponds) = report
            .maps
            .iter()
            .find(|map| map.path.to_ascii_lowercase().contains("ponds"))
        {
            println!(
                "map-content ponds starts={} mines={} units={} doodads={}",
                ponds.starts.len(),
                ponds.gold_mines,
                ponds.units,
                ponds.doodads
            );
        }
    }
    Ok(())
}

fn map_boot(args: &[String]) -> wc3_re::Result<()> {
    let root = repo_root();
    let mut install_root = root.join("analysis/war3_full3");
    let mut maps_dir = install_root.join("Maps");
    let mut json = PathBuf::from("analysis/tech-lead/map_boot_matrix.json");
    let mut native_json = PathBuf::from("analysis/agent-re/native_coverage.json");
    let mut md: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    install_root = PathBuf::from(value);
                    maps_dir = install_root.join("Maps");
                }
            }
            "--maps" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    maps_dir = PathBuf::from(value);
                }
            }
            "--json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    json = PathBuf::from(value);
                }
            }
            "--native-json" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    native_json = PathBuf::from(value);
                }
            }
            "--md" => {
                index += 1;
                md = args.get(index).map(PathBuf::from);
            }
            other => eprintln!("ignoring unknown map-boot argument {other:?}"),
        }
        index += 1;
    }
    let matrix = wc3_re::mapboot::boot_all(&install_root, &maps_dir)?;
    wc3_re::mapboot::write_json(&matrix, &json)?;
    wc3_re::mapboot::write_native_coverage(&matrix, &native_json)?;
    if let Some(md) = md {
        wc3_re::mapboot::write_markdown(&matrix, &md)?;
    }
    println!(
        "map-boot maps={} ok={} main_ok={} executed_triggers={} unique_natives={} unimplemented={} json={} native_json={}",
        matrix.maps.len(),
        matrix.totals.get("ok").copied().unwrap_or(0),
        matrix.totals.get("main_ok").copied().unwrap_or(0),
        matrix.totals.get("executed_triggers").copied().unwrap_or(0),
        matrix.native_calls.len(),
        matrix.unimplemented.len(),
        json.display(),
        native_json.display()
    );
    if !matrix.ok() {
        for map in matrix.maps.iter().filter(|map| !map.ok) {
            eprintln!("map-boot FAIL {} errors={:?}", map.path, map.errors);
        }
        return Err(wc3_re::ReError::Unsupported("map boot matrix has failures".into()));
    }
    Ok(())
}

fn info() {
    println!("wc3-re {}", env!("CARGO_PKG_VERSION"));
    println!("storm exports: {}", wc3_re::storm_api::STORM_EXPORTS.len());
    println!("game→storm imports: {}", wc3_re::storm_api::GAME_IMPORTS_STORM.len());
    println!("game→war3 host imports: {}", wc3_re::host::HOST_IMPORTS.len());
    println!("storm named: 19 (donors/StormLib/storm.dll/storm.def)");
}
