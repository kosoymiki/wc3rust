//! Agent E (ledger 601) — per-def unit voices against the shipped object data.
//!
//! Operator bugs #4/#13: "peasant sounds like an orc" and per-def ack sets
//! collapsing into a race default. `artE_perdef_voices` proves the production
//! hook layer ([`GameAudio`]) resolves every reference def through its own
//! `unitUI.slk` `unitSound` cell into its own `UnitAckSounds.slk` row, starts a
//! file from that row, and that the resolved paths are byte-for-byte the raw
//! SLK cells as parsed by an **independent** tokenizer (not
//! `wc3_audio::parse_slk`). `artE_slk_resolution_crosscheck` extends the same
//! oracle to every unit in the shipped `unitUI.slk` that owns an ack row and
//! reports drift = 0.
//!
//! The tables are read from `analysis/war3_data` (the packet's shipped-table
//! path); the playable clips come from `analysis/war3_full3`. The three oracle
//! files are asserted byte-identical between the two trees, so the oracle and
//! the played data cannot silently diverge.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wc3_audio::engine::{AudioConfig, AudioEngine};
use wc3_audio::{GameAudio, Sound, SoundInfoDb, SoundProvider, UnitNameMap};

/// Eight reference defs: two per race, one voice family each.
const REFERENCE: [(&str, &str, &str); 8] = [
    ("hpea", "Peasant", "Units\\Human\\Peasant\\"),
    ("hfoo", "Footman", "Units\\Human\\Footman\\"),
    ("opeo", "Peon", "Units\\Orc\\Peon\\"),
    ("ogru", "Grunt", "Units\\Orc\\Grunt\\"),
    ("uaco", "Acolyte", "Units\\Undead\\Acolyte\\"),
    ("ugho", "Ghoul", "Units\\Undead\\Ghoul\\"),
    ("ewsp", "Wisp", "Units\\NightElf\\Wisp\\"),
    ("earc", "Archer", "Units\\NightElf\\Archer\\"),
];

/// Suffix lookup order the `SoundInfoDb` hooks use for an acknowledgement.
const ACK_SUFFIXES: [&str; 5] = ["What", "Yes", "YesAttack", "Pissed", "Warcry"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn oracle_root() -> Option<PathBuf> {
    let root = repo_root().join("analysis/war3_data");
    root.join("Units/unitUI.slk")
        .is_file()
        .then_some(root)
}

fn play_root() -> Option<PathBuf> {
    let root = repo_root().join("analysis/war3_full3");
    root.join("UI/SoundInfo/UISounds.slk")
        .is_file()
        .then_some(root)
}

// ---------------------------------------------------------------------------
// Independent SLK oracle (deliberately not the crate parser)
// ---------------------------------------------------------------------------

fn unquote(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_prefix('"').unwrap_or(value);
    let value = value.strip_suffix('"').unwrap_or(value);
    value.replace("\"\"", "\"")
}

/// Parse `C;Y<row>;X<col>;K"<value>"` records into row -> column -> value.
///
/// The shipped tables are line-record files. `C;` lines carry values; `F;Y`
/// formatting lines can be the only record that starts a row (the shipped
/// `DialogSounds.slk`, `UnitAckSounds.slk` and `MIDISounds.slk` do this), so
/// they move the row cursor without inserting a value. The annotation region
/// (`C;X..;A…` editor comments) contains unbalanced quote characters, so this
/// parser deliberately does not track quoting; it was verified that no `K"…"`
/// value in the tables it reads contains `;` or `""`.
fn parse_slk_oracle(text: &str) -> BTreeMap<u32, BTreeMap<u32, String>> {
    let mut rows: BTreeMap<u32, BTreeMap<u32, String>> = BTreeMap::new();
    let mut row = 1u32;
    // A value record without `X` writes the first column: SLK starts a row at
    // column 1 (`C;Y13;K"AmbientSoundTestWind"` in AmbienceSounds.slk).
    let mut col = 1u32;
    for line in text.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        let value_record = fields.first() == Some(&"C");
        if !value_record && fields.first() != Some(&"F") {
            continue;
        }
        for field in &fields[1..] {
            let field = field.trim();
            if let Some(rest) = field.strip_prefix('Y') {
                if let Ok(value) = rest.parse::<u32>() {
                    if value == 0 {
                        continue;
                    }
                    row = value;
                    col = 1;
                }
            } else if let Some(rest) = field.strip_prefix('X') {
                if let Ok(value) = rest.parse::<u32>() {
                    col = value;
                }
            } else if let Some(rest) = field.strip_prefix('K') && value_record {
                rows.entry(row).or_default().insert(col, unquote(rest));
                col += 1;
            }
        }
    }
    rows
}

fn read_table(root: &Path, relative: &str) -> BTreeMap<u32, BTreeMap<u32, String>> {
    let bytes = std::fs::read(root.join(relative))
        .unwrap_or_else(|err| panic!("read {relative}: {err}"));
    // SLK is a text format; the shipped files are Latin-1 clean.
    let text = String::from_utf8_lossy(&bytes);
    let rows = parse_slk_oracle(&text);
    assert!(
        rows.len() > 10,
        "{relative}: independent parser found only {} rows",
        rows.len()
    );
    rows
}

/// One shipped unit row from the oracle.
struct OracleUnit {
    set: String,
    name: String,
}

/// One shipped sound row from the oracle.
struct OracleRow {
    directory: String,
    files: Vec<String>,
}

impl OracleRow {
    /// Raw retail path of every variant: exactly one separator between the
    /// `DirectoryBase` cell and each file name. A file cell that already
    /// carries a path is used as-is (retail tables normalise `/` to `\`).
    fn paths(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|file| {
                if file.contains('\\') || file.contains('/') {
                    return file.replace('/', "\\");
                }
                if self.directory.is_empty() {
                    file.clone()
                } else if self.directory.ends_with('\\') || self.directory.ends_with('/') {
                    format!("{}{file}", self.directory.trim_end_matches('/'))
                } else {
                    format!("{}\\{file}", self.directory)
                }
            })
            .collect()
    }
}

fn cells_to_row(cells: &BTreeMap<u32, String>) -> OracleRow {
    let files: Vec<String> = cells
        .get(&2)
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    OracleRow {
        directory: cells.get(&3).cloned().unwrap_or_default(),
        files,
    }
}

struct Oracle {
    units: BTreeMap<String, OracleUnit>,
    ack: BTreeMap<String, OracleRow>,
}

fn load_oracle(root: &Path) -> Oracle {
    let unitui = read_table(root, "Units/unitUI.slk");
    let header = unitui.get(&1).expect("unitUI.slk header row");
    let header_cols: HashMap<String, u32> = header
        .iter()
        .map(|(col, value)| (value.to_ascii_lowercase(), *col))
        .collect();
    assert_eq!(
        header_cols.get("unituiid"),
        Some(&1),
        "TFT unitUI.slk has the unitUIID header"
    );
    let sound_col = header_cols.get("unitsound").copied();
    let name_col = header_cols.get("name").copied().unwrap_or(3);

    let mut units = BTreeMap::new();
    for (row, cells) in &unitui {
        if *row == 1 {
            continue;
        }
        let id = cells.get(&1).cloned().unwrap_or_default();
        if id.len() != 4 || id.eq_ignore_ascii_case("unitUIID") {
            continue;
        }
        let name = cells.get(&name_col).cloned().unwrap_or_default();
        let set = sound_col
            .and_then(|col| cells.get(&col))
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| name.clone());
        units.insert(
            id.to_ascii_lowercase(),
            OracleUnit {
                set,
                name,
            },
        );
    }

    let ack_rows = read_table(root, "UI/SoundInfo/UnitAckSounds.slk");
    let mut ack = BTreeMap::new();
    for cells in ack_rows.values() {
        let name = cells.get(&1).cloned().unwrap_or_default();
        if name.is_empty() || name.eq_ignore_ascii_case("SoundName") {
            continue;
        }
        let row = cells_to_row(cells);
        if row.files.is_empty() {
            continue;
        }
        ack.insert(name.to_ascii_lowercase(), row);
    }

    assert!(
        units.len() > 400,
        "shipped unitUI.slk rows: {}",
        units.len()
    );
    assert!(
        ack.len() > 600,
        "shipped UnitAckSounds.slk rows: {}",
        ack.len()
    );
    Oracle { units, ack }
}

/// Normalise a path for comparison: retail tables use `\`, the extracted tree
/// uses `/`; the data is case-insensitive.
fn norm_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// Exact-path provider over the extracted tree (no basename fallback)
// ---------------------------------------------------------------------------

struct ExactProvider {
    root: PathBuf,
    index: HashMap<String, String>,
}

impl ExactProvider {
    fn new(root: &Path) -> Self {
        let mut index = HashMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().replace('\\', "/");
                index.insert(relative.to_ascii_lowercase(), relative);
            }
        }
        Self {
            root: root.to_path_buf(),
            index,
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.root.join(path.replace('\\', "/"))
    }
}

impl SoundProvider for ExactProvider {
    fn load_sound(&mut self, _path: &str) -> Option<Arc<Sound>> {
        Some(Arc::new(Sound {
            sample_rate: 22_050,
            channels: 1,
            samples: vec![0.2; 512],
        }))
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.file(path)).ok().map(Arc::from)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some()
    }

    fn resolve(&self, path: &str) -> Option<String> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        self.index.get(&key).cloned()
    }
}

fn headless() -> AudioEngine {
    AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
        block_frames: 512,
        max_voices: 16,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    })
}

/// Load every `SoundInfo` table exactly like the game does (production order is
/// irrelevant because `UnitAckSounds` owns the ack rows).
fn load_sounds(root: &Path) -> SoundInfoDb {
    let dir = root.join("UI/SoundInfo");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("sound info dir")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    let mut sounds = SoundInfoDb::new();
    for path in entries {
        if path.extension().and_then(|ext| ext.to_str()) != Some("slk") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("table name");
        sounds
            .add_table(name, &std::fs::read(&path).expect("slk bytes"))
            .expect("parse slk");
    }
    sounds
}

fn load_units(root: &Path) -> UnitNameMap {
    let bytes = std::fs::read(root.join("Units/unitUI.slk")).expect("unitUI.slk");
    UnitNameMap::parse(&bytes).expect("parse unitUI")
}

/// The oracle and the played tree must carry identical object data for the
/// tables this test trusts.
fn assert_same_object_data(oracle: &Path, play: &Path) {
    for relative in [
        "Units/unitUI.slk",
        "UI/SoundInfo/UnitAckSounds.slk",
        "UI/SoundInfo/AnimSounds.slk",
    ] {
        let a = std::fs::read(oracle.join(relative)).unwrap_or_else(|err| {
            panic!("oracle {relative}: {err}");
        });
        let b = std::fs::read(play.join(relative)).unwrap_or_else(|err| {
            panic!("played tree {relative}: {err}");
        });
        assert_eq!(
            a, b,
            "{relative}: analysis/war3_data and analysis/war3_full3 disagree"
        );
    }
}

/// Bugs #4/#13 acceptance: at least eight defs, one deterministic
/// def -> set -> row -> file record each, no collisions, and the peasant's
/// file lives under `Units\Human\Peasant\` (never the Orc tree).
#[test]
#[allow(non_snake_case)] // the packet pins this exact acceptance-test name
fn artE_perdef_voices() {
    let Some(oracle_root) = oracle_root() else {
        eprintln!("SKIP artE_perdef_voices: analysis/war3_data absent");
        return;
    };
    let Some(play_root) = play_root() else {
        eprintln!("SKIP artE_perdef_voices: analysis/war3_full3 absent");
        return;
    };
    assert_same_object_data(&oracle_root, &play_root);
    let oracle = load_oracle(&oracle_root);
    assert!(REFERENCE.len() >= 8, "at least eight defs are exercised");

    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&play_root),
        load_units(&play_root),
        ExactProvider::new(&play_root),
    );
    audio.clear_unit_voice_log();

    let mut sets = std::collections::HashSet::new();
    let mut rows = std::collections::HashSet::new();
    let mut files = std::collections::HashSet::new();
    let mut expected_files: HashMap<String, (String, Vec<String>)> = HashMap::new();

    for (def, expected_set, expected_dir) in REFERENCE {
        // 1. Independent oracle: the raw unitUI.slk row names the set, the raw
        //    UnitAckSounds.slk row owns the file list and directory base.
        let unit = oracle
            .units
            .get(def)
            .unwrap_or_else(|| panic!("{def}: absent from shipped unitUI.slk"));
        assert_eq!(unit.set, expected_set, "{def}: raw unitSound cell");
        let raw = oracle
            .ack
            .get(&format!("{}what", expected_set.to_ascii_lowercase()))
            .unwrap_or_else(|| panic!("{def}: shipped UnitAckSounds has no {expected_set}What"));
        assert!(
            raw.directory.eq_ignore_ascii_case(expected_dir),
            "{def}: {expected_set}What directory {} != {expected_dir}",
            raw.directory
        );
        let raw_paths = raw.paths();
        assert!(!raw_paths.is_empty(), "{def}: empty file list");

        // 2. The production resolver must select that exact set and row.
        let (set, row) = audio
            .resolve_unit_voice(def)
            .unwrap_or_else(|| panic!("{def}: no unit voice resolution"));
        assert_eq!(set, expected_set, "{def}: resolved unitSound set");
        assert!(
            row.eq_ignore_ascii_case(&format!("{expected_set}What")),
            "{def}: resolved row {row}, want {expected_set}What"
        );
        let def_row = audio
            .sound_table()
            .get(&row)
            .unwrap_or_else(|| panic!("{def}: row {row} missing from the loaded tables"));
        assert_eq!(
            def_row.source, "UnitAckSounds",
            "{def}: ack row must come from UnitAckSounds"
        );

        // 3. The resolved paths equal the raw SLK cells (independent parser).
        let resolved_paths: Vec<String> = def_row.paths().iter().map(|p| norm_path(p)).collect();
        let expected_paths: Vec<String> = raw_paths.iter().map(|p| norm_path(p)).collect();
        assert_eq!(
            resolved_paths, expected_paths,
            "{def}: resolved paths drifted from the shipped SLK cells"
        );

        // 4. The command path the game calls starts a file from that row.
        let voice = audio
            .unit_ack(def, None)
            .unwrap_or_else(|| panic!("{def}: ack command did not start a voice"));
        let file = audio
            .last_sound_file()
            .unwrap_or_else(|| panic!("{def}: no file recorded for the ack"))
            .to_string();
        assert!(
            expected_paths.contains(&norm_path(&file)),
            "{def}: started {file}, not one of {raw_paths:?}"
        );
        assert!(
            norm_path(&file).starts_with(&norm_path(expected_dir)),
            "{def}: {file} is outside the shipped {expected_set} directory"
        );
        println!(
            "ART_E_PERDEF def={def} set={set} row={row} file={file} voice={}",
            voice.0
        );

        sets.insert(set.clone());
        rows.insert(row.to_ascii_lowercase());
        files.insert(norm_path(&file));
        expected_files.insert(
            def.to_string(),
            (file.clone(), expected_paths.clone()),
        );
    }

    // 5. Eight defs -> eight sets -> eight rows -> eight files, no collisions.
    assert_eq!(sets.len(), 8, "eight defs must own eight distinct sets");
    assert_eq!(rows.len(), 8, "eight defs must own eight distinct rows");
    assert_eq!(files.len(), 8, "eight defs must start eight distinct files");
    assert_eq!(
        audio.stats().acks_limited,
        0,
        "distinct defs must not hit the ack limiter"
    );

    let log = audio.unit_voice_log();
    let acks: Vec<_> = log.iter().filter(|row| row.kind == "ack").collect();
    assert_eq!(acks.len(), 8, "one deterministic log record per command");
    for row in &acks {
        assert!(
            row.voice.is_some(),
            "logged command started a voice: {row:?}"
        );
        let (file, raw_paths) = expected_files
            .get(&row.def_id)
            .unwrap_or_else(|| panic!("{}: no expected file", row.def_id));
        let logged = row
            .file
            .as_deref()
            .unwrap_or_else(|| panic!("{}: log row has no file", row.def_id));
        assert_eq!(logged, file, "{}: log file", row.def_id);
        assert!(
            raw_paths.contains(&norm_path(logged)),
            "{}: logged {logged} is not a row variant",
            row.def_id
        );
        println!(
            "ART_E_PERDEF_LOG def={} kind={} set={} row={} file={}",
            row.def_id, row.kind, row.set, row.ack_def, logged
        );
    }
    let by_def: HashMap<&str, &str> = acks
        .iter()
        .map(|row| (row.def_id.as_str(), row.set.as_str()))
        .collect();
    for (def, expected_set, _) in REFERENCE {
        assert_eq!(
            by_def.get(def).copied(),
            Some(expected_set),
            "{def}: log set"
        );
    }
    assert_ne!(
        by_def.get("hpea").copied(),
        Some("Grunt"),
        "peasant must never resolve to the orc grunt set"
    );
    assert_ne!(
        files
            .iter()
            .find(|file| file.contains("peasantwhat"))
            .cloned(),
        files.iter().find(|file| file.contains("gruntwhat")).cloned(),
        "peasant and grunt must not share an ack file"
    );

    let summary: Vec<String> = log
        .iter()
        .map(|row| {
            format!(
                "{}/{}:{}/{}:{}",
                row.def_id,
                row.kind,
                row.set,
                row.ack_def,
                row.file.as_deref().unwrap_or("none")
            )
        })
        .collect();
    println!("ART_E_PERDEF_LOG_ALL count={} {}", log.len(), summary.join(" | "));
}

/// Whole-table drift scan: every shipped unit that owns an ack row must resolve
/// through the candidate order `unitSound` -> `name` to the first shipped
/// `{set}{suffix}` row, with paths equal to the raw `UnitAckSounds.slk` cells.
#[test]
#[allow(non_snake_case)] // `artE_*` is the layer's mandated test prefix
fn artE_slk_resolution_crosscheck() {
    let Some(oracle_root) = oracle_root() else {
        eprintln!("SKIP artE_slk_resolution_crosscheck: analysis/war3_data absent");
        return;
    };
    let Some(play_root) = play_root() else {
        eprintln!("SKIP artE_slk_resolution_crosscheck: analysis/war3_full3 absent");
        return;
    };
    assert_same_object_data(&oracle_root, &play_root);
    let oracle = load_oracle(&oracle_root);
    let audio = GameAudio::new(
        headless(),
        load_sounds(&play_root),
        load_units(&play_root),
        ExactProvider::new(&play_root),
    );

    let mut checked = 0usize;
    let mut silent = 0usize;
    let mut drift: Vec<String> = Vec::new();
    for (def, unit) in &oracle.units {
        // Candidate order mirrors `UnitNameMap::sound_set_candidates`:
        // authoritative `unitSound`, then the display-name set for RoC rows.
        let mut candidates = vec![unit.set.clone()];
        if !unit.name.is_empty() && !unit.name.eq_ignore_ascii_case(&unit.set) {
            candidates.push(unit.name.clone());
        }
        let raw = candidates.iter().find_map(|set| {
            ACK_SUFFIXES.iter().find_map(|suffix| {
                let name = format!("{set}{suffix}").to_ascii_lowercase();
                oracle.ack.get(&name).map(|row| (set.clone(), name, row))
            })
        });
        let Some((raw_set, raw_name, raw_row)) = raw else {
            silent += 1;
            continue;
        };
        checked += 1;

        let Some((set, row_name)) = audio.resolve_unit_voice(def) else {
            drift.push(format!("{def}: no resolution, want {raw_set}/{raw_name}"));
            continue;
        };
        if set != raw_set {
            drift.push(format!("{def}: set {set} != raw {raw_set}"));
            continue;
        }
        if !row_name.eq_ignore_ascii_case(&raw_name) {
            drift.push(format!(
                "{def}: row {row_name} != raw {raw_name} (set {set})"
            ));
            continue;
        }
        let Some(def_row) = audio.sound_table().get(&row_name) else {
            drift.push(format!("{def}: resolved row {row_name} not loaded"));
            continue;
        };
        let resolved: Vec<String> = def_row.paths().iter().map(|p| norm_path(p)).collect();
        let expected: Vec<String> = raw_row.paths().iter().map(|p| norm_path(p)).collect();
        if resolved != expected {
            drift.push(format!(
                "{def}: {row_name} paths {resolved:?} != raw {expected:?}"
            ));
        }
    }

    println!(
        "ART_E_SLK_CROSSCHECK units={} with_ack_row={checked} silent={silent} drift={}",
        oracle.units.len(),
        drift.len()
    );
    assert!(
        checked >= 300,
        "the scan must cover the shipped ack families, got {checked}"
    );
    assert!(
        drift.is_empty(),
        "resolution drift vs shipped UI/SoundInfo SLK: {drift:#?}"
    );
}

/// Catalog-wide path drift check: every `SoundDef` the game actually loaded
/// (2 025 definitions in the production wiring) must resolve to exactly the
/// `DirectoryBase` + file cells of the shipped table it came from.
#[test]
#[allow(non_snake_case)]
fn artE_soundinfo_paths_match_slk() {
    let Some(oracle_root) = oracle_root() else {
        eprintln!("SKIP artE_soundinfo_paths_match_slk: analysis/war3_data absent");
        return;
    };
    let sounds = load_sounds(&oracle_root);

    let mut sources: Vec<String> = sounds
        .iter()
        .map(|def| def.source.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    sources.sort();
    let mut raw: HashMap<String, BTreeMap<String, OracleRow>> = HashMap::new();
    for source in &sources {
        let mut rows = BTreeMap::new();
        for cells in read_table(&oracle_root, &format!("UI/SoundInfo/{source}.slk")).values() {
            let name = cells.get(&1).cloned().unwrap_or_default();
            if name.is_empty() || name.eq_ignore_ascii_case("SoundName") {
                continue;
            }
            rows.insert(name.to_ascii_lowercase(), cells_to_row(cells));
        }
        raw.insert(source.clone(), rows);
    }

    let mut drift: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for def in sounds.iter() {
        let Some(table) = raw.get(&def.source) else {
            drift.push(format!("{}: source table {} was not parsed", def.name, def.source));
            continue;
        };
        let Some(row) = table.get(&def.name.to_ascii_lowercase()) else {
            drift.push(format!("{}: absent from raw {}", def.name, def.source));
            continue;
        };
        checked += 1;
        let resolved: Vec<String> = def.paths().iter().map(|p| norm_path(p)).collect();
        let expected: Vec<String> = row.paths().iter().map(|p| norm_path(p)).collect();
        if resolved != expected {
            drift.push(format!(
                "{} ({}): {resolved:?} != raw {expected:?}",
                def.name, def.source
            ));
        }
    }

    println!(
        "ART_E_SOUNDINFO_PATH tables={sources:?} defs={} checked={checked} drift={}",
        sounds.len(),
        drift.len()
    );
    assert!(
        checked >= 1_500,
        "catalog path scan must cover the loaded tables, got {checked}"
    );
    assert!(
        drift.is_empty(),
        "catalog path drift vs shipped UI/SoundInfo SLK: {drift:#?}"
    );
}
