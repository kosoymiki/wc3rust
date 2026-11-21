//! `data` layer acceptance: the typed database read from the shipped SLKs is
//! compared against an **independent** SYLK reader implemented in this file.
//!
//! Why a second parser: the value of a parity test is that a bug in the crate's
//! `Sheet` reader cannot hide a row/cell drift from the check. This reader
//! implements the SYLK positional model directly from the shipped files
//! (`analysis/war3_data`, `analysis/war3x_data`) and is itself cross-checked
//! against each sheet's own `B;` bounds record, a third structural witness.
//!
//! `TEST_POLICY.md`: every test name carries its mechanic id
//! (`units.data`-equivalent table contract, `abilities.data`, `items.data`,
//! `upgrades.data`, `economy.lumber`, `heroes.attributes`). Expected numbers are
//! the cells printed by `crates/wc3-data/tools/slk_audit.py` against the same
//! files; comments quote the sheet and column they come from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::DirSource;
use wc3_data::GameDatabase;

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn roc() -> &'static GameDatabase {
    static ROC: OnceLock<GameDatabase> = OnceLock::new();
    ROC.get_or_init(|| {
        GameDatabase::load(&DirSource::new(analysis_dir().join("war3_data"))).expect("load RoC")
    })
}

fn tft() -> &'static GameDatabase {
    static TFT: OnceLock<GameDatabase> = OnceLock::new();
    TFT.get_or_init(|| {
        GameDatabase::load(&DirSource::new(analysis_dir().join("war3x_data"))).expect("load TFT")
    })
}

/// `(table name, sheet path relative to the root, primary-key header)`.
const SHEETS: [(&str, &str, &str); 10] = [
    ("UnitData", "Units/UnitData.slk", "unitID"),
    ("UnitBalance", "Units/UnitBalance.slk", "unitBalanceID"),
    ("UnitWeapons", "Units/UnitWeapons.slk", "unitWeapID"),
    ("UnitAbilities", "Units/UnitAbilities.slk", "unitAbilID"),
    ("UnitUI", "Units/unitUI.slk", "unitUIID"),
    ("AbilityData", "Units/AbilityData.slk", "alias"),
    ("ItemData", "Units/ItemData.slk", "itemID"),
    ("UpgradeData", "Units/UpgradeData.slk", "upgradeid"),
    ("DestructableData", "Units/DestructableData.slk", "DestructableID"),
    ("Doodads", "Doodads/Doodads.slk", "doodID"),
];

/// Row contract of the RoC tree: independent SLK id rows = crate `counts()`.
const ROC_ROWS: [(&str, usize); 10] = [
    ("UnitData", 466),
    ("UnitBalance", 466),
    ("UnitWeapons", 316),
    ("UnitAbilities", 324),
    ("UnitUI", 466),
    ("AbilityData", 397),
    ("ItemData", 132),
    ("UpgradeData", 58),
    ("DestructableData", 128),
    ("Doodads", 278),
];

/// Row contract of the TFT tree. `UnitWeapons` really has one row per unit
/// (812): buildings carry an empty row with `weapsOn = 0`, so a lower count
/// would mean rows were dropped, not that the sheet is narrower.
const TFT_ROWS: [(&str, usize); 10] = [
    ("UnitData", 812),
    ("UnitBalance", 812),
    ("UnitWeapons", 812),
    ("UnitAbilities", 812),
    ("UnitUI", 812),
    ("AbilityData", 763),
    ("ItemData", 273),
    ("UpgradeData", 89),
    ("DestructableData", 246),
    ("Doodads", 469),
];

/// Independent SYLK sheet: rows are 0-based (`0` is the header).
struct Slk {
    header: BTreeMap<usize, String>,
    rows: BTreeMap<usize, BTreeMap<usize, String>>,
    /// `B;` record extent as `(rows covered, columns covered)`, when present.
    bounds: Option<(usize, usize)>,
}

impl Slk {
    fn load(root: &Path, relative: &str) -> Self {
        let path = root.join(relative);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let text = String::from_utf8_lossy(&bytes);
        let mut grid: BTreeMap<usize, BTreeMap<usize, String>> = BTreeMap::new();
        let mut bounds = None;
        let mut column = 0usize;
        let mut row = 0usize;
        let mut previous: Option<usize> = None;
        let mut positioned = false;
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            match line.as_bytes()[0] {
                b'B' => {
                    if let Some(extent) = parse_bounds(line) {
                        bounds = Some(extent);
                    }
                }
                b'C' | b'F' => {
                    let (x, y, value) = parse_record(line);
                    if x.is_some() || y.is_some() {
                        positioned = true;
                    }
                    if let Some(y) = y {
                        row = y.saturating_sub(1);
                        previous = None;
                    }
                    if let Some(x) = x {
                        let next = x.saturating_sub(1);
                        if let Some(prior) = previous {
                            if next < prior {
                                row += 1;
                                previous = None;
                            }
                        }
                        column = next;
                    } else if value.is_some() && !positioned {
                        if let Some(prior) = previous {
                            column = prior + 1;
                        }
                    }
                    let Some(value) = value else { continue };
                    grid.entry(row).or_default().insert(column, value);
                    previous = Some(column);
                    positioned = false;
                }
                _ => {}
            }
        }
        let header = grid.remove(&0).unwrap_or_default();
        Self {
            header,
            rows: grid,
            bounds,
        }
    }

    fn column(&self, name: &str) -> Option<usize> {
        self.header
            .iter()
            .find(|(_, cell)| cell.eq_ignore_ascii_case(name))
            .map(|(index, _)| *index)
    }

    fn column_or_panic(&self, name: &str) -> usize {
        self.column(name)
            .unwrap_or_else(|| panic!("sheet has no column {name:?}; header={:?}", self.header))
    }

    fn row(&self, id_column: &str, id: &str) -> &BTreeMap<usize, String> {
        let index = self.column_or_panic(id_column);
        self.rows
            .values()
            .find(|cells| cells.get(&index).is_some_and(|value| value == id))
            .unwrap_or_else(|| panic!("sheet has no row {id:?}"))
    }

    fn cell(&self, id_column: &str, id: &str, column: &str) -> &str {
        let cell_column = self.column_or_panic(column);
        self.row(id_column, id)
            .get(&cell_column)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Primary-key rows: non-empty cells of the id column below the header.
    fn ids(&self, id_column: &str) -> Vec<&str> {
        let index = self.column_or_panic(id_column);
        self.rows
            .values()
            .filter_map(|cells| cells.get(&index).map(String::as_str))
            .filter(|value| !value.is_empty())
            .collect()
    }

    /// Columns the grid actually reaches (header or data).
    fn width(&self) -> usize {
        let data = self
            .rows
            .values()
            .map(|cells| cells.keys().next_back().copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        let header = self.header.keys().next_back().copied().unwrap_or(0);
        1 + data.max(header)
    }
}

fn unquote(value: &str) -> String {
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    value.replace("\"\"", "\"")
}

/// `(x, y, value)` of a `C`/`F` record. `K` runs to end of line because the
/// shipped cells carry spreadsheet formula remnants containing `;`.
fn parse_record(line: &str) -> (Option<usize>, Option<usize>, Option<String>) {
    let mut x = None;
    let mut y = None;
    let mut value = None;
    let mut rest = line.get(2..).unwrap_or("");
    loop {
        rest = rest.trim_start_matches(';');
        if rest.is_empty() {
            break;
        }
        let (head, tail) = match rest.split_once(';') {
            Some((head, tail)) => (head, Some(tail)),
            None => (rest, None),
        };
        if let Some(digits) = head.strip_prefix('X') {
            if let Ok(parsed) = digits.parse::<usize>() {
                x = Some(parsed);
            }
        } else if let Some(digits) = head.strip_prefix('Y') {
            if let Ok(parsed) = digits.parse::<usize>() {
                y = Some(parsed);
            }
        } else if let Some(payload) = head.strip_prefix('K') {
            // Re-attach the tail: the K payload is the remainder of the line.
            let payload = match tail {
                Some(tail) => format!("{payload};{tail}"),
                None => payload.to_string(),
            };
            value = Some(unquote(&payload));
            break;
        }
        match tail {
            Some(tail) => rest = tail,
            None => break,
        }
    }
    (x, y, value)
}

fn parse_bounds(line: &str) -> Option<(usize, usize)> {
    let mut x = None;
    let mut y = None;
    for field in line.split(';').skip(1) {
        if let Some(digits) = field.strip_prefix('X') {
            if let Ok(parsed) = digits.parse::<usize>() {
                x = Some(parsed);
            }
        } else if let Some(digits) = field.strip_prefix('Y') {
            if let Ok(parsed) = digits.parse::<usize>() {
                y = Some(parsed);
            }
        }
    }
    x.zip(y).map(|(x, y)| (y, x))
}

/// Leading number of a cell (the engine's `atof` reading).
///
/// A few shipped cells carry spreadsheet formula remnants after the value
/// (`450;ERC[-1]+IF(RC[+15]=" - ",0,RC[+15]*25)` in `UnitBalance.realHP`), so a
/// numeric comparison must read the leading number, not the raw text.
fn number(cell: &str) -> Option<i64> {
    let head = cell.split(';').next().unwrap_or("").trim();
    if head.is_empty() || head == "-" || head == "_" {
        return None;
    }
    head.parse::<f64>().ok().map(|value| value as i64)
}

fn roc_sheets() -> &'static BTreeMap<&'static str, Slk> {
    static SHEETS_CACHE: OnceLock<BTreeMap<&'static str, Slk>> = OnceLock::new();
    SHEETS_CACHE.get_or_init(|| read_all(&analysis_dir().join("war3_data")))
}

fn tft_sheets() -> &'static BTreeMap<&'static str, Slk> {
    static SHEETS_CACHE: OnceLock<BTreeMap<&'static str, Slk>> = OnceLock::new();
    SHEETS_CACHE.get_or_init(|| read_all(&analysis_dir().join("war3x_data")))
}

/// Read every sheet of `root` and validate it against its own `B;` extent.
fn read_all(root: &Path) -> BTreeMap<&'static str, Slk> {
    let mut sheets = BTreeMap::new();
    for (name, relative, id_column) in SHEETS {
        let sheet = Slk::load(root, relative);
        let ids = sheet.ids(id_column);
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "{name}: duplicate primary keys");
        if let Some((bounds_rows, bounds_cols)) = sheet.bounds {
            assert!(
                bounds_rows >= sheet.rows.len(),
                "{name}: B declares {bounds_rows} rows, parsed {}",
                sheet.rows.len()
            );
            assert!(
                bounds_cols >= sheet.width(),
                "{name}: B declares {bounds_cols} columns, parsed {}",
                sheet.width()
            );
        }
        sheets.insert(name, sheet);
    }
    sheets
}

fn id_column(table: &str) -> &'static str {
    SHEETS
        .iter()
        .find(|(name, _, _)| *name == table)
        .map(|(_, _, id_column)| *id_column)
        .unwrap_or_else(|| panic!("no sheet descriptor for {table}"))
}

/// Assert the independent id count and the crate count for the named tables.
fn assert_rows(data: &GameDatabase, sheets: &BTreeMap<&'static str, Slk>, expected: &[(&str, usize)]) {
    let counts: BTreeMap<&'static str, usize> = data
        .counts()
        .into_iter()
        .map(|(kind, rows)| (kind.name(), rows))
        .collect();
    for (table, rows) in expected {
        let sheet = sheets
            .get(*table)
            .unwrap_or_else(|| panic!("sheet {table} not read"));
        assert_eq!(
            sheet.ids(id_column(table)).len(),
            *rows,
            "{table}: independent SLK id rows"
        );
        assert_eq!(
            counts.get(*table).copied(),
            Some(*rows),
            "{table}: crate count vs SLK"
        );
    }
}

/// `units.*` data contract: every unit sheet row is read, and the footman / town
/// hall spot values are exactly the shipped cells.
#[test]
fn units_data_slk_rows_and_values_match() {
    let roc_sheets = roc_sheets();
    let tft_sheets = tft_sheets();
    let roc = roc();
    let tft = tft();
    assert_rows(roc, roc_sheets, &ROC_ROWS[..5]);
    assert_rows(tft, tft_sheets, &TFT_ROWS[..5]);

    // UnitData.slk [hfoo] (RoC)
    let unit_data = &roc_sheets["UnitData"];
    assert_eq!(unit_data.cell("unitID", "hfoo", "race"), "human");
    assert_eq!(unit_data.cell("unitID", "hfoo", "movetp"), "foot");
    assert_eq!(unit_data.cell("unitID", "hfoo", "valid"), "1");
    assert_eq!(unit_data.cell("unitID", "hfoo", "prio"), "6");

    // UnitBalance.slk [hfoo] — the footman's real numbers (RoC costs 160 gold;
    // the TFT sheet rebalances to 135).
    let balance = &roc_sheets["UnitBalance"];
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "level"), "2");
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "goldcost"), "160");
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "lumbercost"), "0");
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "HP"), "420");
    assert_eq!(
        number(balance.cell("unitBalanceID", "hfoo", "realHP")),
        Some(420),
        "realHP (formula remnant after the number)"
    );
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "def"), "2");
    assert_eq!(
        number(balance.cell("unitBalanceID", "hfoo", "realdef")),
        Some(2),
        "realdef (formula remnant after the number)"
    );
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "spd"), "270");
    assert_eq!(balance.cell("unitBalanceID", "hfoo", "sight"), "1400");
    assert_eq!(
        tft_sheets["UnitBalance"].cell("unitBalanceID", "hfoo", "goldcost"),
        "135",
        "TFT footman gold"
    );

    let defs = roc.defs();
    assert!(defs.unparsed.is_empty(), "unparsed RoC rows: {:?}", defs.unparsed);
    let footman = defs
        .units
        .iter()
        .find(|unit| unit.id == "hfoo")
        .expect("hfoo typed");
    assert_eq!(footman.race.as_deref(), Some("human"), "UnitDef.race");
    assert_eq!(footman.level, Some(2), "UnitDef.level");
    assert_eq!(footman.hp, Some(420), "UnitDef.hp");
    assert_eq!(footman.armor, Some(2), "UnitDef.armor");
    assert_eq!(footman.speed, Some(270), "UnitDef.speed");
    assert_eq!(footman.gold, Some(160), "UnitDef.gold");
    assert_eq!(footman.lumber, Some(0), "UnitDef.lumber");
    assert_eq!(footman.food_used, Some(2), "UnitDef.food_used");
    assert_eq!(footman.abilities, ["Adef"], "UnitDef.abilities");
    assert_eq!(footman.upgrades, ["Rhar", "Rhme", "Rhde"], "UnitDef.upgrades");

    let tft_defs = tft.defs();
    assert!(
        tft_defs.unparsed.is_empty(),
        "unparsed TFT rows: {:?}",
        tft_defs.unparsed
    );
    let tft_footman = tft_defs
        .units
        .iter()
        .find(|unit| unit.id == "hfoo")
        .expect("TFT hfoo typed");
    assert_eq!(tft_footman.gold, Some(135), "TFT UnitDef.gold");
    assert_eq!(tft_footman.hp, Some(420), "TFT UnitDef.hp");
    assert_eq!(tft_footman.abilities, ["Adef", "Aihn"], "TFT abilList");

    // UnitData/UnitBalance [htow]: the town hall is a building by the sheet's
    // own column (`unitUI.isbldg = 1`) and the `TownHall` type flag.
    assert_eq!(unit_data.cell("unitID", "htow", "type"), "TownHall,Mechanical");
    assert_eq!(roc_sheets["UnitUI"].cell("unitUIID", "htow", "isbldg"), "1");
    assert_eq!(balance.cell("unitBalanceID", "htow", "HP"), "1500");
    assert_eq!(balance.cell("unitBalanceID", "htow", "goldcost"), "450");
    assert_eq!(balance.cell("unitBalanceID", "htow", "lumbercost"), "150");
    assert_eq!(balance.cell("unitBalanceID", "htow", "bldtm"), "180");
    let hall = roc.unit("htow").expect("htow");
    assert_eq!(hall.type_flags(), ["TownHall", "Mechanical"], "htow type flags");
    assert!(hall.is_town_hall(), "htow TownHall flag");
    assert_eq!(
        hall.classification().unwrap(),
        wc3_data::units::Classification::Building,
        "htow classification"
    );
    assert_eq!(hall.hp().unwrap(), Some(1500), "htow hp");
    assert_eq!(hall.gold_cost().unwrap(), Some(450), "htow gold");
    assert_eq!(hall.lumber_cost().unwrap(), Some(150), "htow lumber");

    // UnitWeapons.slk [hfoo]: dmgplus1 11, dice1 1, sides1 2 → 12..13,
    // rangeN1 90, cool1 1.35, atkType1 normal, weapType1 MetalMediumSlice.
    let weapons = &roc_sheets["UnitWeapons"];
    assert_eq!(weapons.cell("unitWeapID", "hfoo", "dmgplus1"), "11");
    assert_eq!(weapons.cell("unitWeapID", "hfoo", "dice1"), "1");
    assert_eq!(weapons.cell("unitWeapID", "hfoo", "sides1"), "2");
    assert_eq!(weapons.cell("unitWeapID", "hfoo", "rangeN1"), "90");
    assert_eq!(weapons.cell("unitWeapID", "hfoo", "cool1"), "1.35");
    let weapon = defs
        .weapons
        .iter()
        .find(|weapon| weapon.unit_id == "hfoo")
        .expect("hfoo weapons typed");
    let slot = weapon.slots.first().expect("primary weapon slot");
    // `weapon_type` is `weapTp1` (normal/missile/instant); `weapType1`
    // (`MetalMediumSlice`) is the sound set and stays a raw cell above.
    assert_eq!(slot.weapon_type.as_deref(), Some("normal"), "weapTp1");
    assert_eq!(slot.attack_type.as_deref(), Some("normal"), "atkType1");
    assert_eq!(slot.min_damage, Some(12), "dmgplus1 + dice1");
    assert_eq!(slot.max_damage, Some(13), "dmgplus1 + dice1 * sides1");
    assert_eq!(slot.range, Some(90));
    let aps = slot.attacks_per_second.expect("attacks per second");
    assert!((aps - 1.0 / 1.35).abs() < 1e-6, "1 / cool1 = {aps}");

    // UnitAbilities.slk [hfoo]
    assert_eq!(
        roc_sheets["UnitAbilities"].cell("unitAbilID", "hfoo", "abilList"),
        "Adef"
    );
}

/// `abilities.data` — AbilityData rows and Holy Light numbers from the sheet.
#[test]
fn abilities_data_slk_rows_and_values_match() {
    let roc_sheets = roc_sheets();
    let roc = roc();
    assert_rows(roc, roc_sheets, &[("AbilityData", 397)]);
    assert_rows(tft(), tft_sheets(), &[("AbilityData", 763)]);

    // AbilityData.slk [AHhb] Holy Light: hero 1, levels 3, Cost1 75, Cool1 5,
    // Rng1 800, Dur1 0 (identical in both trees).
    let abilities = &roc_sheets["AbilityData"];
    assert_eq!(abilities.cell("alias", "AHhb", "hero"), "1");
    assert_eq!(abilities.cell("alias", "AHhb", "levels"), "3");
    assert_eq!(abilities.cell("alias", "AHhb", "Cost1"), "75");
    assert_eq!(abilities.cell("alias", "AHhb", "Cool1"), "5");
    assert_eq!(abilities.cell("alias", "AHhb", "Rng1"), "800");
    assert_eq!(abilities.cell("alias", "AHhb", "Dur1"), "0");
    for data in [roc, tft()] {
        let numbers = data.ability_numbers("AHhb").expect("AHhb numbers");
        assert!(numbers.hero, "AHhb hero flag");
        assert_eq!(numbers.levels.len(), 3, "AHhb levels");
        let first = numbers.first_level().expect("level 1");
        assert_eq!(first.mana, Some(75), "level 1 Cost");
        assert_eq!(first.cooldown, Some(5.0), "level 1 Cool");
        assert_eq!(first.range, Some(800.0), "level 1 Rng");
        assert_eq!(first.duration, Some(0.0), "level 1 Dur");
    }

    // `AbilityData.slk` declares 767 rows in the TFT tree: 763 id rows + 4
    // header-less spacer rows that carry no cells, so the parsed grid has 763.
    let tft_ability = &tft_sheets()["AbilityData"];
    assert_eq!(
        tft_ability.bounds.map(|bounds| bounds.0 - 1),
        Some(767),
        "TFT AbilityData declared data rows (B; record)"
    );
    assert_eq!(tft_ability.rows.len(), 763, "TFT AbilityData cell rows");
    assert_eq!(tft_ability.ids("alias").len(), 763, "TFT AbilityData id rows");

    // The `levels` cell is the row's own level count, not the sheet-wide column
    // width: RoC declares 357 rows at 1 level and 40 at 3; TFT declares 18 at 0
    // (engine pseudo-abilities: `Aatk`, `Amov`, …), 648 at 1, 6 at 2, 77 at 3
    // and 14 at 4. Every row's typed level count must equal its cell.
    assert_levels_match(roc, &roc_sheets["AbilityData"], &[(1, 357), (3, 40)]);
    assert_levels_match(
        tft(),
        &tft_sheets()["AbilityData"],
        &[(0, 18), (1, 648), (2, 6), (3, 77), (4, 14)],
    );

    // Named rows on both sides of the old column-probe behaviour.
    assert_eq!(roc.ability("Adef").expect("Adef").max_level(), 1, "Adef 1 level");
    assert_eq!(
        roc.ability("Adef").unwrap().numbers().unwrap().levels.len(),
        1,
        "Adef numbers cover only its declared level"
    );
    assert_eq!(
        roc.ability("Adef").unwrap().level(2).unwrap(),
        None,
        "Adef has no level 2"
    );
    assert_eq!(tft().ability("Aatk").expect("Aatk").max_level(), 0, "Aatk 0 levels");
    assert!(
        tft().ability("Aatk").unwrap().numbers().unwrap().levels.is_empty(),
        "Aatk has no levels"
    );
    assert_eq!(tft().ability("Arsp").expect("Arsp").max_level(), 2, "Arsp 2 levels");
    assert_eq!(tft().ability("ANbr").expect("ANbr").max_level(), 4, "ANbr 4 levels");
}

/// Every id row's typed level count equals its shipped `levels` cell, and the
/// cell distribution matches the independent reader.
fn assert_levels_match(data: &GameDatabase, sheet: &Slk, expected: &[(u32, usize)]) {
    let id_column = sheet.column_or_panic("alias");
    let levels_column = sheet.column_or_panic("levels");
    let mut distribution: BTreeMap<u32, usize> = BTreeMap::new();
    let mut checked = 0usize;
    for cells in sheet.rows.values() {
        let Some(id) = cells.get(&id_column) else { continue };
        if id.is_empty() {
            continue;
        }
        let declared: u32 = cells
            .get(&levels_column)
            .unwrap_or_else(|| panic!("{id}: no levels cell"))
            .parse()
            .unwrap_or_else(|value| panic!("{id}: levels cell {value:?} is not a number"));
        *distribution.entry(declared).or_default() += 1;
        let ability = data.ability(id).unwrap_or_else(|| panic!("{id}: typed ability"));
        assert_eq!(ability.max_level(), declared, "{id}: max_level vs levels cell");
        assert_eq!(
            ability.declared_levels().unwrap(),
            Some(declared as i32),
            "{id}: declared_levels"
        );
        assert_eq!(
            ability.numbers().unwrap().levels.len(),
            declared as usize,
            "{id}: ability_numbers covers the declared levels"
        );
        checked += 1;
    }
    let observed: Vec<(u32, usize)> = distribution.into_iter().collect();
    assert_eq!(observed, expected, "levels cell distribution");
    assert_eq!(checked, expected.iter().map(|(_, rows)| rows).sum::<usize>());
}

/// `items.data` — ItemData rows and the Claws of Attack row per tree.
#[test]
fn items_data_slk_rows_and_values_match() {
    let roc_sheets = roc_sheets();
    let tft_sheets = tft_sheets();
    assert_rows(roc(), roc_sheets, &[("ItemData", 132)]);
    assert_rows(tft(), tft_sheets, &[("ItemData", 273)]);

    // RoC spells the class column `itemClass` and stores a short class code.
    let items = &roc_sheets["ItemData"];
    assert_eq!(items.cell("itemID", "ratc", "itemClass"), "ringa");
    assert_eq!(items.cell("itemID", "ratc", "Level"), "7");
    assert_eq!(items.cell("itemID", "ratc", "goldcost"), "500");
    assert_eq!(items.cell("itemID", "ratc", "abilList"), "AItc");
    assert_eq!(items.cell("itemID", "ratc", "droppable"), "1");

    // TFT spells it `class` with a text class, and rebalances `Level` to 5.
    let tft_items = &tft_sheets["ItemData"];
    assert_eq!(tft_items.cell("itemID", "ratc", "class"), "Permanent");
    assert_eq!(tft_items.cell("itemID", "ratc", "Level"), "5");
    assert_eq!(tft_items.cell("itemID", "ratc", "goldcost"), "500");

    let roc_claw = roc()
        .defs()
        .items
        .into_iter()
        .find(|item| item.id == "ratc")
        .expect("RoC ratc typed");
    assert_eq!(roc_claw.class.as_deref(), Some("ringa"), "RoC ItemDef.class");
    assert_eq!(roc_claw.level, Some(7), "RoC ItemDef.level");
    assert_eq!(roc_claw.gold, Some(500), "RoC ItemDef.gold");
    assert_eq!(roc_claw.lumber, Some(0), "RoC ItemDef.lumber");
    assert!(roc_claw.droppable, "RoC ItemDef.droppable");
    assert_eq!(roc_claw.abilities, ["AItc"], "RoC ItemDef.abilities");

    let tft_claw = tft()
        .defs()
        .items
        .into_iter()
        .find(|item| item.id == "ratc")
        .expect("TFT ratc typed");
    assert_eq!(tft_claw.class.as_deref(), Some("Permanent"), "TFT ItemDef.class");
    assert_eq!(tft_claw.level, Some(5), "TFT ItemDef.level");
    assert_eq!(tft_claw.gold, Some(500), "TFT ItemDef.gold");
}

/// `upgrades.data` — UpgradeData rows and the Melee Weapons cost curve per tree.
#[test]
fn upgrades_data_slk_rows_and_values_match() {
    let roc_sheets = roc_sheets();
    let tft_sheets = tft_sheets();
    assert_rows(roc(), roc_sheets, &[("UpgradeData", 58)]);
    assert_rows(tft(), tft_sheets, &[("UpgradeData", 89)]);

    // UpgradeData.slk [Rhme] RoC: maxlevel 3, goldbase 100, goldmod 150,
    // lumberbase 50, lumbermod 50, timebase 60, timemod 15.
    let upgrades = &roc_sheets["UpgradeData"];
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "class"), "melee");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "race"), "human");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "maxlevel"), "3");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "goldbase"), "100");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "goldmod"), "150");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "lumberbase"), "50");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "lumbermod"), "50");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "timebase"), "60");
    assert_eq!(upgrades.cell("upgradeid", "Rhme", "timemod"), "15");

    // TFT rebalances the step: +75 gold / +125 lumber, same base and time.
    let tft_upgrades = &tft_sheets["UpgradeData"];
    assert_eq!(tft_upgrades.cell("upgradeid", "Rhme", "goldmod"), "75");
    assert_eq!(tft_upgrades.cell("upgradeid", "Rhme", "lumbermod"), "125");
    assert_eq!(tft_upgrades.cell("upgradeid", "Rhme", "timebase"), "60");
    assert_eq!(tft_upgrades.cell("upgradeid", "Rhme", "timemod"), "15");

    let roc_melee = roc()
        .defs()
        .upgrades
        .into_iter()
        .find(|upgrade| upgrade.id == "Rhme")
        .expect("RoC Rhme typed");
    assert_eq!(roc_melee.class.as_deref(), Some("melee"), "UpgradeDef.class");
    assert_eq!(roc_melee.race.as_deref(), Some("human"), "UpgradeDef.race");
    assert_eq!(roc_melee.max_level, Some(3), "UpgradeDef.max_level");
    assert_eq!(roc_melee.gold_base, Some(100), "goldbase");
    assert_eq!(roc_melee.gold_per_level, Some(150), "goldmod");
    assert_eq!(roc_melee.lumber_base, Some(50), "lumberbase");
    assert_eq!(roc_melee.lumber_per_level, Some(50), "lumbermod");
    assert_eq!(roc_melee.time_base, Some(60), "timebase");
    assert_eq!(roc_melee.time_per_level, Some(15), "timemod");

    let tft_melee = tft()
        .defs()
        .upgrades
        .into_iter()
        .find(|upgrade| upgrade.id == "Rhme")
        .expect("TFT Rhme typed");
    assert_eq!(tft_melee.gold_base, Some(100), "TFT goldbase");
    assert_eq!(tft_melee.gold_per_level, Some(75), "TFT goldmod");
    assert_eq!(tft_melee.lumber_base, Some(50), "TFT lumberbase");
    assert_eq!(tft_melee.lumber_per_level, Some(125), "TFT lumbermod");
    assert_eq!(tft_melee.time_base, Some(60), "TFT timebase");
    assert_eq!(tft_melee.time_per_level, Some(15), "TFT timemod");
}

/// `economy.lumber` — the destructable and doodad sheets behind the shipped
/// melee map: row counts plus the lumber tree's life/model/pathing cells.
#[test]
fn economy_lumber_slk_rows_and_values_match() {
    let roc_sheets = roc_sheets();
    let tft_sheets = tft_sheets();
    assert_rows(
        tft(),
        tft_sheets,
        &[("DestructableData", 246), ("Doodads", 469)],
    );
    assert_rows(
        roc(),
        roc_sheets,
        &[("DestructableData", 128), ("Doodads", 278)],
    );

    // DestructableData.slk [LTlt] — the Lordaeron summer tree.
    let destructables = &tft_sheets["DestructableData"];
    assert_eq!(destructables.cell("DestructableID", "LTlt", "HP"), "50");
    assert_eq!(destructables.cell("DestructableID", "LTlt", "armor"), "Wood");
    assert_eq!(destructables.cell("DestructableID", "LTlt", "targType"), "tree");
    assert_eq!(destructables.cell("DestructableID", "LTlt", "numVar"), "10");
    let tree = tft()
        .defs()
        .destructables
        .into_iter()
        .find(|def| def.id == "LTlt")
        .expect("LTlt typed");
    assert_eq!(tree.hp, Some(50), "DestructableDef.hp");
    assert_eq!(tree.armor_type.as_deref(), Some("Wood"), "armor type");
    assert!(
        tree.model
            .as_deref()
            .is_some_and(|model| model.contains("LordaeronTree")),
        "model {:?}",
        tree.model
    );
    assert_eq!(
        tree.path_texture.as_deref(),
        Some("PathTextures\\4x4Default.tga"),
        "pathTex"
    );

    // Doodads.slk [APct] — a prop on the same map. `file` stores the full model
    // path in the TFT sheet.
    let doodads = &tft_sheets["Doodads"];
    assert_eq!(doodads.cell("doodID", "APct", "category"), "E");
    assert_eq!(
        doodads.cell("doodID", "APct", "file"),
        "Doodads\\Ashenvale\\Plants\\AshenCatTail\\AshenCatTail"
    );
    assert_eq!(doodads.cell("doodID", "APct", "defScale"), "1");
    let cat = tft()
        .defs()
        .doodads
        .into_iter()
        .find(|def| def.id == "APct")
        .expect("APct typed");
    assert_eq!(cat.category.as_deref(), Some("E"), "DoodadDef.category");
    assert!(
        cat.model
            .as_deref()
            .is_some_and(|model| model.contains("AshenCatTail")),
        "model {:?}",
        cat.model
    );
}

/// `heroes.attributes` — `UnitBalance` stores the level-1 base in `HP`/`manaN`
/// and the computed pool in `realHP`/`realM`. The typed accessors must expose
/// both and keep them distinct (a hero's mana pool is not `manaN`, which is the
/// per-level gain that is 0 on the level-1 row).
#[test]
fn heroes_attributes_hero_pool_columns_are_distinct() {
    let sheets = roc_sheets();
    let balance = &sheets["UnitBalance"];
    assert_eq!(number(balance.cell("unitBalanceID", "Hamg", "HP")), Some(100));
    assert_eq!(
        number(balance.cell("unitBalanceID", "Hamg", "realHP")),
        Some(450),
        "realHP (formula remnant after the number)"
    );
    assert_eq!(number(balance.cell("unitBalanceID", "Hamg", "manaN")), Some(0));
    assert_eq!(
        number(balance.cell("unitBalanceID", "Hamg", "realM")),
        Some(285),
        "realM (formula remnant after the number)"
    );
    assert_eq!(number(balance.cell("unitBalanceID", "Hamg", "STR")), Some(14));
    assert_eq!(number(balance.cell("unitBalanceID", "Hamg", "INT")), Some(19));
    assert_eq!(number(balance.cell("unitBalanceID", "Hamg", "AGI")), Some(17));
    assert_eq!(balance.cell("unitBalanceID", "Hamg", "Primary"), "INT");

    let archmage = roc().unit("Hamg").expect("Hamg");
    assert_eq!(archmage.hp().unwrap(), Some(100), "raw HP cell");
    assert_eq!(archmage.real_hp().unwrap(), Some(450), "realHP cell");
    assert_eq!(archmage.mana().unwrap(), Some(0), "manaN cell");
    assert_eq!(archmage.real_mana().unwrap(), Some(285), "realM cell");

    let hero = roc().hero("Hamg").expect("Hamg hero def");
    assert_eq!(hero.attributes.intelligence, 19, "INT");
    assert_eq!(hero.hp_at(1), 450, "hero level-1 hp uses realHP");
    assert_eq!(hero.mana_at(1), 285, "hero level-1 mana uses realM");
}
