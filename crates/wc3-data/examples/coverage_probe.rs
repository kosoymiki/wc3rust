//! Ad-hoc probe: print the full coverage matrix of the installed archive.
//!
//! ```text
//! cargo run -p wc3-data --example coverage_probe [War3.mpq]
//! ```

use std::path::Path;

use wc3_data::coverage::{Field, MenuCoverage, RowCoverage};
use wc3_data::source::Source;
use wc3_data::{DataError, GameDatabase};
use wc3_mpq::MpqArchive;

struct MpqSource {
    archive: MpqArchive,
}

impl Source for MpqSource {
    fn read(&self, path: &str) -> wc3_data::Result<Vec<u8>> {
        let normalized = path.replace('/', "\\");
        self.archive.read(&normalized).map_err(|source| DataError::Io {
            path: path.to_string(),
            source: std::io::Error::other(source.to_string()),
        })
    }

    fn describe(&self) -> String {
        "mpq:War3.mpq".to_string()
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".to_string()
    });
    let archive = MpqArchive::open(Path::new(&path)).expect("open mpq");
    let source = MpqSource { archive };
    let data = GameDatabase::load(&source).expect("load database");
    let report = data.coverage();

    println!("counts = {:?}", report.counts);
    println!(
        "profile layers = {} missing = {:?}",
        data.localizer().loaded_files().len(),
        data.localizer().missing_files()
    );
    println!(
        "buffs without sheet = {} (measured ids)",
        data.buff_ids().len()
    );

    println!("\n== classes");
    for class in &report.classes {
        print_row(&class.label(), &class.rows);
    }

    println!("\n== menu");
    print_menu(&report.menu);

    println!("\n== hero learn abilities");
    print_row("heroAbilList", &report.menu.hero_abilities);
    println!("\n== unit ability buttons");
    print_row("abilList", &report.menu.unit_abilities);

    // Partition the unnamed ability rows so a resolution bug cannot hide behind
    // "the shipped data has no name": every unnamed row is classified by the
    // editor flags the sheet itself carries.
    let mut partition: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for ability in data.abilities() {
        if ability.name().is_some_and(|name| !name.trim().is_empty()) {
            continue;
        }
        let code = ability.code().unwrap_or("").trim().to_string();
        let editor = ability.use_in_editor().unwrap_or(false);
        let hero = ability.is_hero_ability().unwrap_or(false);
        let item = ability.is_item_ability().unwrap_or(false);
        let key = format!(
            "useInEditor={editor} hero={hero} item={item} code={}",
            if code.is_empty() { "-" } else { &code }
        );
        partition
            .entry(key)
            .or_default()
            .push(ability.id_str());
    }
    println!("\n== unnamed ability partition");
    let mut groups: Vec<_> = partition.into_iter().collect();
    groups.sort_by_key(|(_, ids)| std::cmp::Reverse(ids.len()));
    for (key, ids) in groups {
        println!("{:50} {} {}", key, ids.len(), {
            let mut head: Vec<&str> = ids.iter().take(6).map(String::as_str).collect();
            head.sort();
            head.join(",")
        });
    }

    // Of the rows without their own `Name`, how many resolve through the base
    // `code` row (the unified lookup is what the card sees) and how many stay
    // nameless even after inheritance.
    let mut inherited = 0usize;
    let mut genuinely_nameless = Vec::new();
    for ability in data.abilities() {
        if ability.name().is_some_and(|name| !name.trim().is_empty()) {
            continue;
        }
        match data.object(&ability.id_str()).and_then(|info| info.name) {
            Some(name) if !name.trim().is_empty() => inherited += 1,
            _ => genuinely_nameless.push(ability.id_str()),
        }
    }
    println!(
        "\n rows without own Name: {} resolved through base code, {} still nameless: {:?}",
        inherited,
        genuinely_nameless.len(),
        genuinely_nameless
    );

    // Item hotkeys only exist for items with an active ability (usable=1), so
    // measure the population instead of asserting a flat 132/132.
    let (mut usable, mut with_ability, mut with_hotkey, mut usable_hotkey) = (0, 0, 0, 0);
    for item in data.items() {
        let is_usable = item.usable().unwrap_or(false);
        let has_ability = !item.abilities().is_empty();
        let has_hotkey = item.hotkey().is_some_and(|hotkey| !hotkey.trim().is_empty());
        if is_usable {
            usable += 1;
            if has_hotkey {
                usable_hotkey += 1;
            }
        }
        if has_ability {
            with_ability += 1;
        }
        if has_hotkey {
            with_hotkey += 1;
        }
    }
    println!(
        "\n items: usable={usable} with_ability={with_ability} with_hotkey={with_hotkey} usable_with_hotkey={usable_hotkey}"
    );
}

fn print_row(label: &str, rows: &RowCoverage) {
    println!(
        "{label:26} rows={:4} name={:4} icon={:4} hotkey={:4} tooltip={:4} cost={:4} model={:4}",
        rows.rows,
        rows.present(Field::Name),
        rows.present(Field::Icon),
        rows.present(Field::Hotkey),
        rows.present(Field::Tooltip),
        rows.present(Field::Cost),
        rows.present(Field::Model),
    );
    for field in Field::ALL {
        let missing = rows.missing(field);
        if missing.is_empty() {
            continue;
        }
        println!("   no {} ({}): {:?}", field.label(), missing.len(), missing);
    }
}

fn print_menu(menu: &MenuCoverage) {
    println!(
        "producers={} builders={} researchers={} entries={} complete={} requires={} unresolved={}",
        menu.producers,
        menu.builders,
        menu.researchers,
        menu.entries,
        menu.complete,
        menu.requires,
        menu.requires_unresolved.len()
    );
    print_row("menu entries", &menu.reachable);
    for field in Field::ALL {
        if let Some(coverage) = menu.field(field) {
            println!(
                "   entry {}: {}/{} ({} missing)",
                field.label(),
                coverage.present,
                coverage.rows,
                coverage.missing.len()
            );
            for id in &coverage.missing {
                println!("      {id}");
            }
        }
    }
    for line in &menu.requires_unresolved {
        println!("   unresolved {line}");
    }
}
