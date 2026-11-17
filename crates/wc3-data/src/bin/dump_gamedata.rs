//! `dump_gamedata` — inspect and validate the typed WC3 game database.
//!
//! ```text
//! dump_gamedata [--root DIR] [--tft DIR] [--listfile FILE]... [--object-data FILE]... [--json]
//!               <counts|validate|paths|unit|ability|item|upgrade|destructable|doodad|objectdata> [ID]
//! ```
//!
//! Without `--root` the RoC extraction under `analysis/war3_data` is used. Exit
//! code is 1 when validation finds errors, so the command is usable as a gate.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use wc3_data::source::{DirSource, Listfile};
use wc3_data::GameDatabase;

fn main() -> ExitCode {
    restore_sigpipe();
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("dump_gamedata: {error}");
            ExitCode::from(2)
        }
    }
}

struct Args {
    root: PathBuf,
    tft: Option<PathBuf>,
    listfiles: Vec<PathBuf>,
    wts: Option<PathBuf>,
    object_data: Vec<PathBuf>,
    json: bool,
    command: String,
    id: Option<String>,
}

fn run() -> Result<ExitCode, String> {
    let args = parse_args()?;
    let source = DirSource::new(&args.root);
    let mut data = GameDatabase::load(&source).map_err(|error| error.to_string())?;

    let mut catalog: Option<Listfile> = None;
    for path in &args.listfiles {
        let listfile = Listfile::from_file(path).map_err(|error| error.to_string())?;
        match catalog.as_mut() {
            Some(existing) => existing.extend(&listfile),
            None => catalog = Some(listfile),
        }
    }
    if let Some(catalog) = catalog {
        data.set_catalog(catalog);
    }

    if let Some(path) = &args.wts {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        data.set_wts(&bytes).map_err(|error| error.to_string())?;
    }

    for path in &args.object_data {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| format!("{}: missing extension", path.display()))?;
        let kind = wc3_formats::gamedata::ObjectKind::from_extension(extension)
            .ok_or_else(|| format!("{}: not an object-data file", path.display()))?;
        data.apply_object_data(path.display().to_string(), kind, &bytes)
            .map_err(|error| error.to_string())?;
    }

    let tft = args
        .tft
        .as_ref()
        .map(|root| GameDatabase::load(&DirSource::new(root)))
        .transpose()
        .map_err(|error| error.to_string())?;

    match args.command.as_str() {
        "counts" => {
            print_counts(&data, tft.as_ref());
            Ok(ExitCode::SUCCESS)
        }
        "validate" => {
            let report = data.validate();
            if args.json {
                print_validate_json(&report);
            } else {
                for line in report.summary_lines() {
                    println!("{line}");
                }
                for stat in &report.paths {
                    for sample in &stat.samples {
                        println!("  missing {}: {sample}", stat.class);
                    }
                }
                let mut codes: std::collections::BTreeMap<(&str, &str), usize> =
                    std::collections::BTreeMap::new();
                for issue in report
                    .issues
                    .iter()
                    .filter(|issue| issue.severity != wc3_data::validate::Severity::Info)
                {
                    *codes
                        .entry((issue.severity.as_str(), issue.code))
                        .or_insert(0) += 1;
                    println!(
                        "{} [{}] {} {}: {}",
                        issue.severity.as_str(),
                        issue.code,
                        issue.table,
                        issue.id,
                        issue.detail
                    );
                }
                for ((severity, code), count) in codes {
                    println!("summary {severity} {code}: {count}");
                }
            }
            Ok(if report.ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        "paths" => {
            let report = data.validate();
            for stat in &report.paths {
                println!(
                    "{}: checked={} resolved={} missing={}",
                    stat.class, stat.checked, stat.resolved, stat.missing
                );
                for sample in &stat.samples {
                    println!("  missing: {sample}");
                }
            }
            if let Some(catalog) = &report.catalog {
                println!(
                    "catalog {}: entries={} paths={} unit_models={} building_models={} command_icons={}",
                    catalog.origin,
                    catalog.entries,
                    catalog.paths,
                    catalog.unit_models,
                    catalog.building_models,
                    catalog.command_button_icons
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        "object" => {
            print_object(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "unit" => {
            print_unit(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "ability" => {
            print_ability(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "item" => {
            print_item(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "upgrade" => {
            print_upgrade(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "destructable" => {
            print_destructable(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "doodad" => {
            print_doodad(&data, &require_id(&args)?)?;
            Ok(ExitCode::SUCCESS)
        }
        "objectdata" => {
            for merge in data.merges() {
                println!(
                    "{}: {:?} v{} base={} custom={} applied={} shadow={} profile={} unknown={} missing_sheet={} created={}",
                    merge.source,
                    merge.kind,
                    merge.kind.extension(),
                    merge.base_entries,
                    merge.custom_entries,
                    merge.report.applied,
                    merge.report.shadow,
                    merge.report.profile,
                    merge.report.unknown_field.len(),
                    merge.report.missing_sheet.len(),
                    merge.report.created.len()
                );
                for (sheet, id) in &merge.report.created {
                    println!("  created {sheet}:{id}");
                }
                for (sheet, field) in &merge.report.unknown_field {
                    println!("  unknown field in {sheet}: {field}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "shops" => {
            print_shops(&data, args.json);
            Ok(ExitCode::SUCCESS)
        }
        "shop" => {
            print_shop(&data, &require_id(&args)?, args.json)?;
            Ok(ExitCode::SUCCESS)
        }
        "hero-abilities" => {
            print_hero_abilities(&data, args.json);
            Ok(ExitCode::SUCCESS)
        }
        "hero-ability" => {
            print_hero_ability(&data, &require_id(&args)?, args.json)?;
            Ok(ExitCode::SUCCESS)
        }
        "coverage" => {
            print_coverage(&data, tft.as_ref(), args.json);
            Ok(ExitCode::SUCCESS)
        }
        "defs" => {
            let defs = data.defs();
            let abilities = data.abilities().count();
            if args.json {
                println!(
                    "{{\"units\": {}, \"weapons\": {}, \"abilities\": {}, \"items\": {}, \"upgrades\": {}, \"effects\": {}, \"buffs\": {}, \"destructables\": {}, \"doodads\": {}, \"unparsed\": {}}}",
                    defs.units.len(),
                    defs.weapons.len(),
                    abilities,
                    defs.items.len(),
                    defs.upgrades.len(),
                    defs.effects.len(),
                    defs.buffs.len(),
                    defs.destructables.len(),
                    defs.doodads.len(),
                    defs.unparsed.len()
                );
            } else {
                println!(
                    "defs: units={} weapons={} abilities={} items={} upgrades={} effects={} buffs={} destructables={} doodads={} unparsed={}",
                    defs.units.len(),
                    defs.weapons.len(),
                    abilities,
                    defs.items.len(),
                    defs.upgrades.len(),
                    defs.effects.len(),
                    defs.buffs.len(),
                    defs.destructables.len(),
                    defs.doodads.len(),
                    defs.unparsed.len()
                );
                for row in &defs.unparsed {
                    println!("  unparsed {} {}: {}", row.family, row.id, row.reason);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "balance" => {
            print_balance(&data, args.json);
            Ok(if data.balance_audit().ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        "sqlite" => {
            let path = require_id(&args)?;
            write_sqlite(&data, &path)?;
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!(
            "unknown command `{other}`; expected counts|validate|paths|object|unit|ability|item|upgrade|destructable|doodad|objectdata|shops|shop|hero-abilities|hero-ability|coverage|defs|balance|sqlite"
        )),
    }
}

/// Rust ignores `SIGPIPE` by default, so `dump_gamedata … | head` would panic on
/// the first write to the closed pipe. A dump tool should terminate quietly.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: `signal` with `SIG_DFL` only restores the process default; no
    // handler state is dereferenced.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn parse_args() -> Result<Args, String> {
    let mut root = default_root();
    let mut tft = None;
    let mut listfiles = Vec::new();
    let mut wts = None;
    let mut object_data = Vec::new();
    let mut json = false;
    let mut command = None;
    let mut id = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                root = PathBuf::from(args.next().ok_or("--root needs a value")?);
            }
            "--tft" => {
                tft = Some(PathBuf::from(args.next().ok_or("--tft needs a value")?));
            }
            "--listfile" => {
                listfiles.push(PathBuf::from(args.next().ok_or("--listfile needs a value")?));
            }
            "--wts" => {
                wts = Some(PathBuf::from(args.next().ok_or("--wts needs a value")?));
            }
            "--object-data" => {
                object_data.push(PathBuf::from(
                    args.next().ok_or("--object-data needs a value")?,
                ));
            }
            "--json" => json = true,
            "--help" | "-h" => {
                println!("{}", HELP);
                std::process::exit(0);
            }
            other if other.starts_with("--") => return Err(format!("unknown flag `{other}`")),
            other => {
                if command.is_none() {
                    command = Some(other.to_string());
                } else if id.is_none() {
                    id = Some(other.to_string());
                } else {
                    return Err(format!("unexpected argument `{other}`"));
                }
            }
        }
    }

    Ok(Args {
        root,
        tft,
        listfiles,
        wts,
        object_data,
        json,
        command: command.unwrap_or_else(|| "counts".to_string()),
        id,
    })
}

const HELP: &str = "\
dump_gamedata — typed WC3 game data inspector

USAGE:
    dump_gamedata [OPTIONS] <COMMAND> [ID]

OPTIONS:
    --root DIR         data root (default: analysis/war3_data)
    --tft DIR          second root to report alongside (analysis/war3x_data)
    --listfile FILE    archive listfile for path validation (repeatable)
    --wts FILE         string table for TRIGSTR_ resolution (war3map.wts)
    --object-data FILE object-data file to merge (repeatable; .w3u/.w3a/…)
    --json            machine readable output for `validate`
    -h, --help        this text

COMMANDS:
    counts            row counts per table
    validate          full validation (exit code 1 on errors)
    paths             model/icon path resolution statistics
    object ID         unified lookup: name/tooltip/icon/model/stats (any kind)
    unit ID           dump a unit (e.g. hfoo)
    ability ID        dump an ability (e.g. AHbz)
    item ID           dump an item (e.g. ratc)
    upgrade ID        dump an upgrade (e.g. Rhme)
    destructable ID   dump a destructable (e.g. LTlt)
    doodad ID         dump a doodad (e.g. APct)
    objectdata        report the merged object-data files
    shops             shop units and their `Sellitems` lists
    shop ID           one shop with price/stock/charges/requirements
    hero-abilities    hero ability catalog and per-level coverage
    hero-ability ID   one hero ability with owners and per-level numbers
    coverage          data coverage gates (melee/spell/hero/castable/item)
    defs              typed definition-table counts (units/weapons/items/upgrades/effects/buffs)
    balance           validate every unit/ability/item/upgrade balance row (exit 1 on issues)
    sqlite PATH       export units/abilities/items/upgrades/shops to a SQLite database
";

fn default_root() -> PathBuf {
    // crates/wc3-data → repository root → analysis/war3_data
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../analysis/war3_data")
}

fn opt_num(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn opt_f32(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value}"))
        .unwrap_or_else(|| "-".to_string())
}

fn opt_json(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn opt_json_num(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn requirements_text(requires: &[wc3_data::Requirement]) -> String {
    if requires.is_empty() {
        return "-".to_string();
    }
    requires
        .iter()
        .map(|requirement| {
            if requirement.alternatives.is_empty() {
                requirement.token.clone()
            } else {
                format!(
                    "{}({})",
                    requirement.token,
                    requirement.alternatives.join(",")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn print_shops(data: &GameDatabase, json: bool) {
    let shops = data.shops();
    if json {
        print!("{{\"root\": {}, \"shops\": [", json_string(data.root()));
        for (index, shop) in shops.iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!(
                "{{\"unit\": {}, \"name\": {}, \"items\": [",
                json_string(&shop.unit_id),
                opt_json(shop.name.as_deref())
            );
            for (item_index, item) in shop.items.iter().enumerate() {
                if item_index > 0 {
                    print!(", ");
                }
                print!("{}", json_string(&item.item_id));
            }
            print!("]}}");
        }
        println!("]}}");
        return;
    }
    println!("shops: {}", shops.len());
    for shop in &shops {
        println!(
            "shop {} name={} items={}",
            shop.unit_id,
            shop.name.as_deref().unwrap_or("-"),
            shop.items.len()
        );
        for item in &shop.items {
            println!(
                "  {} gold={} lumber={} level={} class={} charges={} stock={}/{}/{} requires={}",
                item.item_id,
                opt_num(item.gold),
                opt_num(item.lumber),
                opt_num(item.level),
                item.class.as_deref().unwrap_or("-"),
                opt_num(item.charges),
                opt_num(item.stock_start),
                opt_num(item.stock_max),
                opt_num(item.stock_regen),
                requirements_text(&item.requires)
            );
        }
    }
}

fn print_shop(data: &GameDatabase, id: &str, json: bool) -> Result<(), String> {
    let shop = data
        .shop(id)
        .ok_or_else(|| format!("shop `{id}` not found (no Sellitems list)"))?;
    if json {
        print!(
            "{{\"unit\": {}, \"name\": {}, \"items\": [",
            json_string(&shop.unit_id),
            opt_json(shop.name.as_deref())
        );
        for (index, item) in shop.items.iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!(
                "{{\"id\": {}, \"name\": {}, \"icon\": {}, \"gold\": {}, \"lumber\": {}, \"level\": {}, \"class\": {}, \"charges\": {}, \"stock_max\": {}, \"stock_regen\": {}, \"stock_start\": {}, \"sellable\": {}, \"requires\": [",
                json_string(&item.item_id),
                opt_json(item.name.as_deref()),
                opt_json(item.icon.as_deref()),
                opt_json_num(item.gold),
                opt_json_num(item.lumber),
                opt_json_num(item.level),
                opt_json(item.class.as_deref()),
                opt_json_num(item.charges),
                opt_json_num(item.stock_max),
                opt_json_num(item.stock_regen),
                opt_json_num(item.stock_start),
                item.sellable
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string())
            );
            for (requirement_index, requirement) in item.requires.iter().enumerate() {
                if requirement_index > 0 {
                    print!(", ");
                }
                print!(
                    "{{\"token\": {}, \"alternatives\": [",
                    json_string(&requirement.token)
                );
                for (alternative_index, alternative) in requirement.alternatives.iter().enumerate() {
                    if alternative_index > 0 {
                        print!(", ");
                    }
                    print!("{}", json_string(alternative));
                }
                print!("]}}");
            }
            print!("]}}");
        }
        println!("]}}");
        return Ok(());
    }
    println!(
        "shop {} name={} items={}",
        shop.unit_id,
        shop.name.as_deref().unwrap_or("-"),
        shop.items.len()
    );
    for item in &shop.items {
        println!(
            "  {} gold={} lumber={} level={} class={} charges={} stock={}/{}/{} sellable={} requires={}",
            item.item_id,
            opt_num(item.gold),
            opt_num(item.lumber),
            opt_num(item.level),
            item.class.as_deref().unwrap_or("-"),
            opt_num(item.charges),
            opt_num(item.stock_start),
            opt_num(item.stock_max),
            opt_num(item.stock_regen),
            item.sellable
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            requirements_text(&item.requires)
        );
    }
    Ok(())
}

fn print_hero_abilities(data: &GameDatabase, json: bool) {
    let catalog = data.hero_abilities();
    let (covered, total) = data.hero_ability_coverage();
    if json {
        print!(
            "{{\"covered\": {covered}, \"total\": {total}, \"abilities\": ["
        );
        for (index, info) in catalog.iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!(
                "{{\"id\": {}, \"levels\": {}, \"required_level\": {}, \"level_skip\": {}, \"owners\": [",
                json_string(&info.id),
                info.levels,
                opt_json_num(info.required_level),
                opt_json_num(info.level_skip)
            );
            for (owner_index, owner) in info.heroes.iter().enumerate() {
                if owner_index > 0 {
                    print!(", ");
                }
                print!("{}", json_string(owner));
            }
            print!("]}}");
        }
        println!("]}}");
        return;
    }
    println!("hero abilities: {covered}/{total} with numbers for every level");
    for info in &catalog {
        println!(
            "{} levels={} req={} skip={} owners={} mana1={} cd1={} range1={}",
            info.id,
            info.levels,
            opt_num(info.required_level),
            opt_num(info.level_skip),
            info.heroes.join(","),
            opt_num(info.level(1).and_then(|level| level.mana)),
            opt_f32(info.level(1).and_then(|level| level.cooldown)),
            opt_f32(info.level(1).and_then(|level| level.range))
        );
    }
}

fn print_hero_ability(data: &GameDatabase, id: &str, json: bool) -> Result<(), String> {
    let catalog = data.hero_abilities();
    let info = catalog
        .iter()
        .find(|info| info.id == id)
        .ok_or_else(|| format!("hero ability `{id}` not found"))?;
    if json {
        print!(
            "{{\"id\": {}, \"levels\": {}, \"required_level\": {}, \"level_skip\": {}, \"owners\": [",
            json_string(&info.id),
            info.levels,
            opt_json_num(info.required_level),
            opt_json_num(info.level_skip)
        );
        for (index, owner) in info.heroes.iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!("{}", json_string(owner));
        }
        print!("], \"numbers\": [");
        for (index, level) in info.numbers.levels.iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!(
                "{{\"level\": {}, \"mana\": {}, \"cooldown\": {}, \"area\": {}, \"range\": {}, \"duration\": {}}}",
                level.level,
                opt_json_num(level.mana),
                level
                    .cooldown
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                level
                    .area
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                level
                    .range
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                level
                    .duration
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string())
            );
        }
        println!("]}}");
        return Ok(());
    }
    println!(
        "hero ability {} name={} levels={} req={} skip={} owners={}",
        info.id,
        info.name.as_deref().unwrap_or("-"),
        info.levels,
        opt_num(info.required_level),
        opt_num(info.level_skip),
        info.heroes.join(",")
    );
    for level in &info.numbers.levels {
        println!(
            "  level {} mana={} cd={} area={} range={} dur={} data={:?}",
            level.level,
            opt_num(level.mana),
            opt_f32(level.cooldown),
            opt_f32(level.area),
            opt_f32(level.range),
            opt_f32(level.duration),
            level.data
        );
    }
    Ok(())
}

fn pct(covered: usize, total: usize) -> String {
    if total == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", 100.0 * covered as f64 / total as f64)
    }
}

fn print_coverage(data: &GameDatabase, tft: Option<&GameDatabase>, json: bool) {
    let melee = data.melee_ability_coverage();
    let spell = data.ability_coverage();
    let heroes = data.melee_hero_coverage();
    let castable = data.castable_ability_coverage();
    let item_abilities = data.item_ability_coverage();
    let items: Vec<_> = data.items().collect();
    let class_present = items.iter().filter(|item| item.class().is_some()).count();

    if json {
        print!(
            "{{\"root\": {}, \"melee_abilities\": {{\"covered\": {}, \"total\": {}}}, \"spell_sheet\": {{\"covered\": {}, \"total\": {}}}, \"heroes\": {{\"present\": {}, \"total\": {}}}, \"castable\": {{\"covered\": {}, \"total\": {}}}, \"item_abilities\": {{\"covered\": {}, \"total\": {}}}, \"items\": {{\"total\": {}, \"class_present\": {}}}",
            json_string(data.root()),
            melee.0,
            melee.1,
            spell.0,
            spell.1,
            heroes.0,
            heroes.1,
            castable.0,
            castable.1,
            item_abilities.0,
            item_abilities.1,
            items.len(),
            class_present
        );
        if let Some(tft) = tft {
            let melee = tft.melee_ability_coverage();
            let spell = tft.ability_coverage();
            let heroes = tft.melee_hero_coverage();
            let castable = tft.castable_ability_coverage();
            let item_abilities = tft.item_ability_coverage();
            let items: Vec<_> = tft.items().collect();
            let class_present = items.iter().filter(|item| item.class().is_some()).count();
            print!(
                ", \"tft\": {{\"root\": {}, \"melee_abilities\": {{\"covered\": {}, \"total\": {}}}, \"spell_sheet\": {{\"covered\": {}, \"total\": {}}}, \"heroes\": {{\"present\": {}, \"total\": {}}}, \"castable\": {{\"covered\": {}, \"total\": {}}}, \"item_abilities\": {{\"covered\": {}, \"total\": {}}}, \"items\": {{\"total\": {}, \"class_present\": {}}}}}",
                json_string(tft.root()),
                melee.0,
                melee.1,
                spell.0,
                spell.1,
                heroes.0,
                heroes.1,
                castable.0,
                castable.1,
                item_abilities.0,
                item_abilities.1,
                items.len(),
                class_present
            );
        }
        println!("}}");
        return;
    }

    println!("root: {}", data.root());
    println!(
        "melee abilities: {}/{} ({})",
        melee.0,
        melee.1,
        pct(melee.0, melee.1)
    );
    println!(
        "spell sheet: {}/{} ({})",
        spell.0,
        spell.1,
        pct(spell.0, spell.1)
    );
    println!(
        "ladder heroes: {}/{} ({})",
        heroes.0,
        heroes.1,
        pct(heroes.0, heroes.1)
    );
    println!(
        "castable abilities: {}/{} ({})",
        castable.0,
        castable.1,
        pct(castable.0, castable.1)
    );
    println!(
        "item abilities: {}/{} ({})",
        item_abilities.0,
        item_abilities.1,
        pct(item_abilities.0, item_abilities.1)
    );
    println!(
        "items with class: {}/{} ({})",
        class_present,
        items.len(),
        pct(class_present, items.len())
    );
    if let Some(tft) = tft {
        let melee = tft.melee_ability_coverage();
        let spell = tft.ability_coverage();
        let heroes = tft.melee_hero_coverage();
        let castable = tft.castable_ability_coverage();
        let item_abilities = tft.item_ability_coverage();
        let items: Vec<_> = tft.items().collect();
        let class_present = items.iter().filter(|item| item.class().is_some()).count();
        println!("tft root: {}", tft.root());
        println!(
            "tft melee abilities: {}/{} ({})",
            melee.0,
            melee.1,
            pct(melee.0, melee.1)
        );
        println!(
            "tft spell sheet: {}/{} ({})",
            spell.0,
            spell.1,
            pct(spell.0, spell.1)
        );
        println!(
            "tft ladder heroes: {}/{} ({})",
            heroes.0,
            heroes.1,
            pct(heroes.0, heroes.1)
        );
        println!(
            "tft castable abilities: {}/{} ({})",
            castable.0,
            castable.1,
            pct(castable.0, castable.1)
        );
        println!(
            "tft item abilities: {}/{} ({})",
            item_abilities.0,
            item_abilities.1,
            pct(item_abilities.0, item_abilities.1)
        );
        println!(
            "tft items with class: {}/{} ({})",
            class_present,
            items.len(),
            pct(class_present, items.len())
        );
    }
}

fn print_balance(data: &GameDatabase, json: bool) {
    let audit = data.balance_audit();
    if json {
        print!(
            "{{\"root\": {}, \"rows\": {}, \"checked\": {}, \"issues\": {}, \"errors\": {}, \"warnings\": {}, \"families\": {{",
            json_string(data.root()),
            audit.rows(),
            audit.checked(),
            audit.issues(),
            audit.errors(),
            audit.warnings()
        );
        for (index, (family, stat)) in audit.families().iter().enumerate() {
            if index > 0 {
                print!(", ");
            }
            print!(
                "\"{family}\": {{\"rows\": {}, \"checked\": {}, \"issues\": [",
                stat.rows,
                stat.checked
            );
            for (issue_index, issue) in stat.issues.iter().enumerate() {
                if issue_index > 0 {
                    print!(", ");
                }
                print!(
                    "{{\"severity\": {}, \"id\": {}, \"field\": {}, \"detail\": {}}}",
                    json_string(if issue.severity == wc3_data::balance::Severity::Error {
                        "error"
                    } else {
                        "warning"
                    }),
                    json_string(&issue.id),
                    json_string(issue.field),
                    json_string(&issue.detail)
                );
            }
            print!("]}}");
        }
        println!("}}}}");
        return;
    }
    println!("root: {}", data.root());
    for (family, stat) in audit.families() {
        println!(
            "{family}: rows={} checked={} issues={} errors={} warnings={}",
            stat.rows,
            stat.checked,
            stat.issues.len(),
            stat.errors(),
            stat.warnings()
        );
        for issue in &stat.issues {
            let severity = if issue.severity == wc3_data::balance::Severity::Error {
                "error"
            } else {
                "warning"
            };
            println!("  {severity} {} [{}] {}", issue.id, issue.field, issue.detail);
        }
    }
    println!(
        "total: rows={} checked={} issues={} errors={} warnings={}",
        audit.rows(),
        audit.checked(),
        audit.issues(),
        audit.errors(),
        audit.warnings()
    );
}

fn sql_text(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("'{}'", value.replace('\'', "''")),
        None => "NULL".to_string(),
    }
}

fn sql_num<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "NULL".to_string())
}

fn write_sqlite(data: &GameDatabase, path: &str) -> Result<(), String> {
    let mut script = String::with_capacity(1 << 20);
    script.push_str(
        "PRAGMA journal_mode=WAL;\n\
         DROP TABLE IF EXISTS units;\n\
         CREATE TABLE units(id TEXT PRIMARY KEY, name TEXT, race TEXT, hp INTEGER, mana INTEGER, level INTEGER, gold INTEGER, lumber INTEGER, food_used INTEGER, food_provided INTEGER, speed INTEGER, armor INTEGER);\n\
         DROP TABLE IF EXISTS abilities;\n\
         CREATE TABLE abilities(id TEXT PRIMARY KEY, name TEXT, hero INTEGER, item INTEGER, use_in_editor INTEGER, levels INTEGER);\n\
         DROP TABLE IF EXISTS ability_levels;\n\
         CREATE TABLE ability_levels(id TEXT, level INTEGER, mana INTEGER, cooldown REAL, area REAL, range REAL, duration REAL, PRIMARY KEY(id, level));\n\
         DROP TABLE IF EXISTS items;\n\
         CREATE TABLE items(id TEXT PRIMARY KEY, name TEXT, class TEXT, level INTEGER, gold INTEGER, lumber INTEGER, charges INTEGER, stock_max INTEGER, stock_regen INTEGER, stock_start INTEGER, sellable INTEGER);\n\
         DROP TABLE IF EXISTS upgrades;\n\
         CREATE TABLE upgrades(id TEXT PRIMARY KEY, max_level INTEGER, gold_base INTEGER, gold_per_level INTEGER, lumber_base INTEGER, lumber_per_level INTEGER, time_base INTEGER, time_per_level INTEGER);\n\
         DROP TABLE IF EXISTS shops;\n\
         CREATE TABLE shops(unit TEXT, item TEXT, gold INTEGER, stock_max INTEGER, stock_regen INTEGER, requires TEXT, PRIMARY KEY(unit, item));\n\
         BEGIN;\n",
    );
    for unit in data.units() {
        script.push_str(&format!(
            "INSERT OR REPLACE INTO units VALUES({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
            sql_text(Some(&unit.id_str())),
            sql_text(unit.name().as_deref()),
            sql_text(unit.race()),
            sql_num(unit.hp().ok().flatten()),
            sql_num(unit.mana().ok().flatten()),
            sql_num(unit.level().ok().flatten()),
            sql_num(unit.gold_cost().ok().flatten()),
            sql_num(unit.lumber_cost().ok().flatten()),
            sql_num(unit.food_used().ok().flatten()),
            sql_num(unit.food_provided().ok().flatten()),
            sql_num(unit.speed().ok().flatten()),
            sql_num(unit.armor_base().ok().flatten())
        ));
    }
    for ability in data.abilities() {
        let numbers = ability.numbers().ok();
        script.push_str(&format!(
            "INSERT OR REPLACE INTO abilities VALUES({}, {}, {}, {}, {}, {});\n",
            sql_text(Some(&ability.id_str())),
            sql_text(ability.name().as_deref()),
            i32::from(ability.is_hero_ability().unwrap_or(false)),
            i32::from(ability.is_item_ability().unwrap_or(false)),
            i32::from(ability.use_in_editor().unwrap_or(false)),
            ability.max_level()
        ));
        if let Some(numbers) = numbers {
            for level in numbers.levels {
                script.push_str(&format!(
                    "INSERT OR REPLACE INTO ability_levels VALUES({}, {}, {}, {}, {}, {}, {});\n",
                    sql_text(Some(&numbers.id)),
                    level.level,
                    sql_num(level.mana),
                    sql_num(level.cooldown),
                    sql_num(level.area),
                    sql_num(level.range),
                    sql_num(level.duration)
                ));
            }
        }
    }
    for item in data.items() {
        script.push_str(&format!(
            "INSERT OR REPLACE INTO items VALUES({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});\n",
            sql_text(Some(&item.id_str())),
            sql_text(item.name().as_deref()),
            sql_text(item.class()),
            sql_num(item.level().ok().flatten()),
            sql_num(item.gold_cost().ok().flatten()),
            sql_num(item.lumber_cost().ok().flatten()),
            sql_num(item.charges().ok().flatten()),
            sql_num(item.stock_max().ok().flatten()),
            sql_num(item.stock_regen().ok().flatten()),
            sql_num(item.stock_start().ok().flatten()),
            sql_num(item.sellable().ok().flatten().map(i32::from))
        ));
    }
    for upgrade in data.upgrades() {
        script.push_str(&format!(
            "INSERT OR REPLACE INTO upgrades VALUES({}, {}, {}, {}, {}, {}, {}, {});\n",
            sql_text(Some(&upgrade.id_str())),
            sql_num(upgrade.max_level().ok().flatten()),
            sql_num(upgrade.gold_base().ok().flatten()),
            sql_num(upgrade.gold_per_level().ok().flatten()),
            sql_num(upgrade.lumber_base().ok().flatten()),
            sql_num(upgrade.lumber_per_level().ok().flatten()),
            sql_num(upgrade.time_base().ok().flatten()),
            sql_num(upgrade.time_per_level().ok().flatten())
        ));
    }
    for shop in data.shops() {
        for item in shop.items {
            let requires: Vec<String> = item
                .requires
                .iter()
                .map(|requirement| requirement.token.clone())
                .collect();
            script.push_str(&format!(
                "INSERT OR REPLACE INTO shops VALUES({}, {}, {}, {}, {}, {});\n",
                sql_text(Some(&shop.unit_id)),
                sql_text(Some(&item.item_id)),
                sql_num(item.gold),
                sql_num(item.stock_max),
                sql_num(item.stock_regen),
                sql_text(Some(&requires.join(",")))
            ));
        }
    }
    script.push_str("COMMIT;\n");

    let mut child = Command::new("sqlite3")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("sqlite3: {error} (install `sqlite3`)"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "sqlite3: no stdin".to_string())?
        .write_all(script.as_bytes())
        .map_err(|error| format!("sqlite3 stdin: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("sqlite3 wait: {error}"))?;
    if !status.success() {
        return Err(format!("sqlite3 exited with {status}"));
    }

    let verify = Command::new("sqlite3")
        .arg(path)
        .arg("SELECT 'units', COUNT(*) FROM units UNION ALL SELECT 'abilities', COUNT(*) FROM abilities UNION ALL SELECT 'ability_levels', COUNT(*) FROM ability_levels UNION ALL SELECT 'items', COUNT(*) FROM items UNION ALL SELECT 'upgrades', COUNT(*) FROM upgrades UNION ALL SELECT 'shops', COUNT(*) FROM shops;")
        .output()
        .map_err(|error| format!("sqlite3 verify: {error}"))?;
    if !verify.status.success() {
        return Err(format!("sqlite3 verify exited with {}", verify.status));
    }
    let counts = String::from_utf8_lossy(&verify.stdout);
    println!("sqlite: {path}");
    for line in counts.lines() {
        println!("  {line}");
    }
    Ok(())
}

fn require_id(args: &Args) -> Result<String, String> {    args.id
        .clone()
        .ok_or_else(|| format!("`{}` needs an identifier", args.command))
}

fn print_counts(data: &GameDatabase, tft: Option<&GameDatabase>) {
    println!("root: {} ({})", data.root(), data.tables().count());
    for (kind, rows) in data.counts() {
        let tft_rows = tft
            .and_then(|tft| tft.table(kind))
            .map(|table| table.len().to_string())
            .unwrap_or_else(|| "-".to_string());
        let (declared, spacers) = data
            .table(kind)
            .map(|table| {
                (
                    table
                        .sheet()
                        .declared_rows()
                        .map(|rows| rows.saturating_sub(1))
                        .unwrap_or(table.len()),
                    table.sheet().skipped_rows(),
                )
            })
            .unwrap_or((rows, 0));
        println!(
            "{:<18} roc={:<6} tft={:<6} declared={:<6} spacers={:<3} cols={}",
            kind.name(),
            rows,
            tft_rows,
            declared,
            spacers,
            data.table(kind).map(|table| table.columns().len()).unwrap_or(0)
        );
    }
    println!(
        "profiles: {} files loaded, {} missing",
        data.localizer().loaded_files().len(),
        data.localizer().missing_files().len()
    );
    println!(
        "object data: files={} base={} custom={}",
        data.object_data().len(),
        data.object_data().totals().0,
        data.object_data().totals().1
    );
}

fn print_validate_json(report: &wc3_data::validate::Report) {
    println!("{{");
    println!("  \"root\": {},", json_string(&report.root));
    println!("  \"rows\": {},", report.total_rows());
    println!(
        "  \"errors\": {},\n  \"warnings\": {},",
        report.error_count(),
        report.warning_count()
    );
    print!("  \"tables\": [");
    for (index, stat) in report.tables.iter().enumerate() {
        if index > 0 {
            print!(", ");
        }
        print!(
            "{{\"table\": {}, \"rows\": {}, \"columns\": {}, \"empty_ids\": {}, \"duplicate_ids\": {}}}",
            json_string(stat.kind.name()),
            stat.rows,
            stat.columns,
            stat.empty_ids,
            stat.duplicate_ids
        );
    }
    println!("],");
    print!("  \"paths\": [");
    for (index, stat) in report.paths.iter().enumerate() {
        if index > 0 {
            print!(", ");
        }
        print!(
            "{{\"class\": {}, \"checked\": {}, \"resolved\": {}, \"missing\": {}}}",
            json_string(stat.class),
            stat.checked,
            stat.resolved,
            stat.missing
        );
    }
    println!("]");
    println!("}}");
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn print_object(data: &GameDatabase, id: &str) -> Result<(), String> {
    let info = data
        .object(id)
        .ok_or_else(|| format!("object `{id}` not found"))?;
    println!(
        "object {} kind={:?} name={:?}",
        info.id, info.kind, info.name
    );
    println!(
        "  icon={:?} hotkey={:?} buttonpos={:?} model={:?}",
        info.icon, info.hotkey, info.button_pos, info.model
    );
    if info.tip.is_some() || info.ubertip.is_some() || info.description.is_some() {
        println!(
            "  tip={:?}\n  ubertip={:?}\n  description={:?}",
            info.tip, info.ubertip, info.description
        );
    }
    let stats = &info.stats;
    println!(
        "  stats: hp={:?} mana={:?} armor={:?}(+{:?}) gold={:?} lumber={:?} build={:?} level={:?}",
        stats.hp,
        stats.mana,
        stats.armor,
        stats.armor_per_upgrade,
        stats.gold_cost,
        stats.lumber_cost,
        stats.build_time,
        stats.level
    );
    println!(
        "  combat: dmg={:?}..{:?} avg={:?} aps={:?} range={:?} targets={:?}",
        stats.min_damage,
        stats.max_damage,
        stats.average_damage,
        stats.attacks_per_second,
        stats.range,
        stats.targets
    );
    println!(
        "  other: charges={:?} max_level={:?} mana_cost={:?} cooldown={:?} area={:?}",
        stats.charges, stats.max_level, stats.mana_cost, stats.cooldown, stats.area
    );
    Ok(())
}

fn print_unit(data: &GameDatabase, id: &str) -> Result<(), String> {
    let unit = data
        .unit(id)
        .ok_or_else(|| format!("unit `{id}` not found"))?;
    println!("unit {} ({:?})", unit.id_str(), unit.classification().map_err(text)?);
    println!("  name: {:?}", unit.name());
    println!(
        "  race={:?} class={:?} type={:?} hall_tier={:?}",
        unit.race(),
        unit.unit_class(),
        unit.type_flags(),
        unit.hall_tier()
    );
    println!(
        "  hp={:?} mana={:?} armor={:?}+{:?}/lvl lvl={:?} spd={:?}",
        unit.hp().map_err(text)?,
        unit.mana().map_err(text)?,
        unit.armor_base().map_err(text)?,
        unit.armor_per_upgrade().map_err(text)?,
        unit.level().map_err(text)?,
        unit.speed().map_err(text)?
    );
    println!(
        "  cost: gold={:?} lumber={:?} build={:?}s food={:?}/{:?}",
        unit.gold_cost().map_err(text)?,
        unit.lumber_cost().map_err(text)?,
        unit.build_time().map_err(text)?,
        unit.food_used().map_err(text)?,
        unit.food_provided().map_err(text)?
    );
    println!(
        "  model: {:?} exists={:?}",
        unit.model_file(),
        unit.model_exists()
    );
    println!(
        "  portrait: {:?} exists={:?}",
        unit.portrait_file(),
        unit.portrait_exists()
    );
    println!(
        "  icon: {:?} hotkey={:?} buttonpos={:?}",
        unit.icon(),
        unit.hotkey(),
        unit.button_pos()
    );
    for weapon in unit.weapons() {
        println!(
            "  weapon {:?}: {:?}/{:?} dmg={:?}..{:?} avg={:?} dps={:?} range={:?} cool={:?} targets={:?}",
            weapon.slot(),
            weapon.weapon_type(),
            weapon.attack_type(),
            weapon.min_damage().map_err(text)?,
            weapon.max_damage().map_err(text)?,
            weapon.average_damage().map_err(text)?,
            weapon.dps().map_err(text)?,
            weapon.range().map_err(text)?,
            weapon.cooldown().map_err(text)?,
            weapon.targets()
        );
    }
    println!("  abilities: {:?}", unit.abilities());
    println!("  hero abilities: {:?}", unit.hero_abilities());
    println!("  upgrades: {:?}", unit.upgrades());
    Ok(())
}

fn print_ability(data: &GameDatabase, id: &str) -> Result<(), String> {
    let ability = data
        .ability(id)
        .ok_or_else(|| format!("ability `{id}` not found"))?;
    println!("ability {} code={:?}", ability.id_str(), ability.code());
    println!("  name: {:?}", ability.name());
    println!(
        "  hero={:?} item={:?} levels={:?} max_levels={} targets={:?}",
        ability.is_hero_ability().map_err(text)?,
        ability.is_item_ability().map_err(text)?,
        ability.declared_levels().map_err(text)?,
        ability.max_level(),
        ability.target_flags()
    );
    println!(
        "  icon: {:?} hotkey={:?} buttonpos={:?}",
        ability.icon(),
        ability.hotkey(),
        ability.button_pos()
    );
    for level in ability.levels().map_err(text)? {
        println!(
            "  level {}: cool={:?} cost={:?} area={:?} range={:?} dur={:?} data={:?}",
            level.level,
            level.cooldown,
            level.mana_cost,
            level.area,
            level.range,
            level.duration,
            level.data
        );
    }
    if let Some(ubertip) = ability.resolved_ubertip() {
        println!("  ubertip: {ubertip}");
    }
    Ok(())
}

fn print_item(data: &GameDatabase, id: &str) -> Result<(), String> {
    let item = data
        .item(id)
        .ok_or_else(|| format!("item `{id}` not found"))?;
    println!("item {} class={:?} level={:?}", item.id_str(), item.class(), item.level().map_err(text)?);
    println!("  name: {:?}", item.name());
    println!(
        "  charges={:?} usable={:?} perishable={:?} droppable={:?}",
        item.charges().map_err(text)?,
        item.usable().map_err(text)?,
        item.perishable().map_err(text)?,
        item.droppable().map_err(text)?
    );
    println!(
        "  gold={:?} lumber={:?} hp={:?} armor={:?}",
        item.gold_cost().map_err(text)?,
        item.lumber_cost().map_err(text)?,
        item.hp().map_err(text)?,
        item.armor_material()
    );
    println!(
        "  model={:?} exists={:?} icon={:?}",
        item.model_file(),
        item.model_exists(),
        item.icon()
    );
    println!("  abilities: {:?}", item.abilities());
    Ok(())
}

fn print_upgrade(data: &GameDatabase, id: &str) -> Result<(), String> {
    let upgrade = data
        .upgrade(id)
        .ok_or_else(|| format!("upgrade `{id}` not found"))?;
    println!(
        "upgrade {} class={:?} race={:?} max_level={:?}",
        upgrade.id_str(),
        upgrade.class(),
        upgrade.race(),
        upgrade.max_level().map_err(text)?
    );
    println!("  name: {:?}", upgrade.name());
    println!("  comments: {:?}", upgrade.comments());
    for level in 1..=upgrade.max_level().map_err(text)?.unwrap_or(0).max(0) as u32 {
        println!(
            "  level {level}: gold={:?} lumber={:?} time={:?}s",
            upgrade.gold_cost(level).map_err(text)?,
            upgrade.lumber_cost(level).map_err(text)?,
            upgrade.research_time(level).map_err(text)?
        );
    }
    for (effect, value) in upgrade.effects_at(1).map_err(text)? {
        println!(
            "  effect {}: {:?} base={:?} mod={:?} level1={:?} {:?}",
            effect.slot, effect.code, effect.base, effect.delta, value, effect.comment
        );
    }
    Ok(())
}

fn print_destructable(data: &GameDatabase, id: &str) -> Result<(), String> {
    let object = data
        .destructable(id)
        .ok_or_else(|| format!("destructable `{id}` not found"))?;
    println!(
        "destructable {} name={:?} type={:?} armor={:?} hp={:?}",
        object.id_str(),
        object.name(),
        object.target_type(),
        object.armor_type(),
        object.hp().map_err(text)?
    );
    println!(
        "  model={:?} resolved={:?} exists={:?} variations={:?}",
        object.model_file(),
        object.resolved_model(),
        object.model_exists(),
        object.variations().map_err(text)?
    );
    Ok(())
}

fn print_doodad(data: &GameDatabase, id: &str) -> Result<(), String> {
    let object = data
        .doodad(id)
        .ok_or_else(|| format!("doodad `{id}` not found"))?;
    println!(
        "doodad {} name={:?} category={:?} variations={:?}",
        object.id_str(),
        object.name(),
        object.category(),
        object.variations().map_err(text)?
    );
    println!(
        "  model={:?} resolved={:?} exists={:?} scale={:?}",
        object.model_file(),
        object.resolved_model(),
        object.model_exists(),
        object.default_scale().map_err(text)?
    );
    Ok(())
}

fn text(error: wc3_data::DataError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_data::units::WeaponSlot;

    #[test]
    fn json_string_escapes() {
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn coverage_helpers_render_gate_numbers() {
        assert_eq!(pct(175, 176), "99.4%");
        assert_eq!(pct(0, 0), "-");
        assert_eq!(opt_num(Some(75)), "75");
        assert_eq!(opt_num(None), "-");
        assert_eq!(opt_json(None), "null");
    }

    #[test]
    fn requirement_text_expands_pseudo_tokens() {
        let requirement = wc3_data::Requirement {
            token: "TWN2".to_string(),
            alternatives: vec![
                "hkee".to_string(),
                "ostr".to_string(),
                "etoa".to_string(),
                "unp1".to_string(),
            ],
        };
        assert_eq!(requirements_text(&[requirement]), "TWN2(hkee,ostr,etoa,unp1)");
        assert_eq!(requirements_text(&[]), "-");
    }

    #[test]
    fn weapon_slot_suffixes() {
        assert_eq!(WeaponSlot::Primary.suffix(), "1");
        assert_eq!(WeaponSlot::Secondary.suffix(), "2");
    }
}
