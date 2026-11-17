//! Warcraft III sound tables (`UI/SoundInfo/*.slk`) and unit sound-set mapping.
//!
//! Gameplay audio is data-driven: `UnitAckSounds.slk` defines 740 named sounds
//! (each with a file list, directory base, volume, pitch and 3D distances),
//! `UnitCombatSounds.slk` defines the material hit sounds, and `unitUI.slk`
//! maps a unit id (`hfoo`) to its display name (`Footman`), which is the prefix
//! of its acknowledgement sounds (`FootmanWhat`, `FootmanYesAttack`, ...).
//!
//! Everything here is parsed from those files, so `unit_ack("hfoo")` plays the
//! exact files the retail game would.

use std::collections::HashMap;

use crate::AudioError;

/// One row of a sound table.
/// SoundData parser default `DistanceCutoff` in WC3 (`Game.dll` `0x6f3a2530`):
/// a row that stores 0 is parsed as 3000 and still culled at that distance.
pub const DEFAULT_DISTANCE_CUTOFF: f32 = 3_000.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SoundDef {
    pub name: String,
    pub files: Vec<String>,
    pub directory: String,
    /// 0..=1 (the SLK stores 0..127).
    pub volume: f32,
    pub pitch: f32,
    pub pitch_variance: f32,
    pub priority: u32,
    pub channel: String,
    pub flags: String,
    pub min_distance: f32,
    pub max_distance: f32,
    pub distance_cutoff: f32,
    /// Table the definition came from ("UnitAckSounds", ...).
    pub source: String,
}

impl SoundDef {
    /// Full archive paths for every variant of this sound.
    ///
    /// `DirectoryBase` is inconsistent between retail tables: `UnitAckSounds`
    /// ends with a backslash, `AbilitySounds`/`AmbienceSounds` do not. Join with
    /// exactly one separator or every ability sound becomes
    /// `...\BlizzardBlizzardTarget1.wav`.
    pub fn paths(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|file| {
                if file.contains('\\') || file.contains('/') {
                    return file.replace('/', "\\");
                }
                match self.directory.as_str() {
                    "" => file.clone(),
                    dir if dir.ends_with('\\') || dir.ends_with('/') => {
                        format!("{}{}", dir.trim_end_matches('/'), file)
                    }
                    dir => format!("{dir}\\{file}"),
                }
            })
            .collect()
    }

    /// Distance used to build the 3D attenuation curve.
    pub fn reference_distance(&self) -> f32 {
        if self.max_distance > 0.0 {
            self.max_distance
        } else {
            1000.0
        }
    }

    /// DistanceCutoff as the engine uses it: the row's own value, or the
    /// parsed SoundData default (3000) when the row stores 0.
    pub fn audible_cutoff(&self) -> f32 {
        if self.distance_cutoff > 0.0 {
            self.distance_cutoff
        } else {
            DEFAULT_DISTANCE_CUTOFF
        }
    }

    /// Retail audibility test: a 3-D sound never starts when
    /// `distance² > DistanceCutoff²` (`StartSound` `0x6f102980`, `fcom`
    /// `0x6f102b0b`). Equality is still audible (`>` not `>=`).
    pub fn is_audible_distance(&self, distance: f32) -> bool {
        let cutoff = self.audible_cutoff();
        distance * distance <= cutoff * cutoff
    }

    /// `Channel` column as the mixer sees it (numeric channel number).
    ///
    /// Retail rows store a single integer (`1`..`4` for `UnitAckSounds`); a
    /// non-numeric or empty cell parses as 0, i.e. ungrouped.
    pub fn channel_number(&self) -> u16 {
        self.channel
            .trim()
            .split(|c: char| !c.is_ascii_digit())
            .find(|token| !token.is_empty())
            .and_then(|token| token.parse::<u16>().ok())
            .unwrap_or(0)
    }

    /// True when the `Flags` column carries `flag` (case-insensitive).
    ///
    /// Shipped values are comma separated, e.g.
    /// `WANT3D,NODUPLICATES,CHANNELFULLPREEMPT,SCALEPRIORITY`.
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags
            .split([',', '|', ' '])
            .any(|token| token.trim().eq_ignore_ascii_case(flag))
    }

    /// Stable key for the `NODUPLICATES` rule (0 = no key).
    ///
    /// `NODUPLICATES` means "never two instances of this row at once"; the key
    /// is an FNV-1a hash of the lowercased row name so it stays `Copy` and
    /// deterministic across runs.
    pub fn dedup_key(&self) -> u64 {
        if !self.has_flag("NODUPLICATES") && !self.has_flag("NODUPEUSERNAMES") {
            return 0;
        }
        fnv1a(self.name.as_bytes())
    }

    /// `(near radius, far radius)` for the mixer's attenuation curve.
    ///
    /// Retail semantics, verified against the shipped `UnitAckSounds` /
    /// `AnimSounds` rows: `MinDistance` is the near radius where the sound is
    /// still at unity gain (`PeonWhat` 3000, `PeonDeath` 600), `MaxDistance` is
    /// the far end where it becomes inaudible (`10000` for both), and
    /// `DistanceCutoff` culls it outright (`PeonDeath` 3000). `fallback` is
    /// used when a row has no `MinDistance`; `far = 0` keeps the legacy
    /// inverse-square tail.
    pub fn playback_distances(&self, fallback: f32) -> (f32, f32) {
        let reference = if self.min_distance > 0.0 {
            self.min_distance
        } else {
            fallback.max(1.0)
        };
        let far = if self.max_distance > reference {
            self.max_distance
        } else if self.distance_cutoff > reference {
            self.distance_cutoff
        } else {
            0.0
        };
        (reference, far)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoundInfoDb {
    defs: HashMap<String, SoundDef>,
    /// Number of definitions per source table, for reports.
    pub table_counts: Vec<(String, usize)>,
}

impl SoundInfoDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse one SLK table and add its definitions.
    ///
    /// Only tables that follow the `SoundInfo` schema are ingested: a row must
    /// list actual sound files (an extension on every entry). Tables such as
    /// `EnvironmentSounds.slk` (environment types) and `EAXDefs.slk` share the
    /// directory but not the columns, and would otherwise turn their first
    /// column into bogus "sounds".
    pub fn add_table(&mut self, table_name: &str, bytes: &[u8]) -> Result<usize, AudioError> {
        let rows = parse_slk(bytes)?;
        let headers: Vec<String> = rows
            .first()
            .map(|row| row.values().map(|v| v.to_ascii_lowercase()).collect())
            .unwrap_or_default();
        // Schema check at table level: a SoundInfo table lists audio filenames.
        // Environment/EAX tables share the directory but not the columns, and
        // would otherwise produce bogus "sounds" from their first column.
        let data_rows = rows.len().saturating_sub(1).max(1);
        let dotted = rows
            .iter()
            .filter(|row| {
                row.get(&2)
                    .map(|files| files.split(',').any(|file| file.contains('.')))
                    .unwrap_or(false)
            })
            .count();
        if dotted * 2 < data_rows {
            return Ok(0);
        }
        let mut added = 0usize;
        for row in rows {
            let name = row.get(&1).cloned().unwrap_or_default();
            if name.is_empty() || headers.contains(&name.to_ascii_lowercase()) {
                continue;
            }
            let files: Vec<String> = row
                .get(&2)
                .map(|list| {
                    list.split(',')
                        .map(|file| file.trim())
                        .filter(|file| !file.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if files.is_empty() {
                continue;
            }
            // A file entry without an extension is still a valid retail entry
            // (the engine appends nothing, so it simply will not resolve), and
            // is kept so coverage reports stay honest.

            let def = SoundDef {
                name: name.clone(),
                files,
                directory: row.get(&3).cloned().unwrap_or_default(),
                volume: number(&row, 4).map(|v| (v / 127.0).clamp(0.0, 1.0)).unwrap_or(1.0),
                pitch: number(&row, 5).unwrap_or(1.0),
                pitch_variance: number(&row, 6).unwrap_or(0.0),
                priority: number(&row, 7).unwrap_or(0.0) as u32,
                channel: row.get(&8).cloned().unwrap_or_default(),
                flags: row.get(&9).cloned().unwrap_or_default(),
                min_distance: number(&row, 10).unwrap_or(0.0),
                max_distance: number(&row, 11).unwrap_or(0.0),
                distance_cutoff: number(&row, 12).unwrap_or(0.0),
                source: table_name.to_string(),
            };
            self.defs.insert(name.to_ascii_lowercase(), def);
            added += 1;
        }
        if added > 0 {
            self.table_counts.push((table_name.to_string(), added));
        }
        Ok(added)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Iterate every definition (catalog/coverage tooling).
    pub fn iter(&self) -> impl Iterator<Item = &SoundDef> {
        self.defs.values()
    }

    pub fn get(&self, name: &str) -> Option<&SoundDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// First variant path, if the sound exists.
    pub fn path(&self, name: &str) -> Option<String> {
        self.get(name).and_then(|def| def.paths().into_iter().next())
    }

    /// All variant paths (retail picks one at random per event).
    pub fn paths(&self, name: &str) -> Vec<String> {
        self.get(name).map(SoundDef::paths).unwrap_or_default()
    }

    /// Resolve a unit display name to an actual acknowledgement definition.
    pub fn unit_ack(&self, unit_name: &str) -> Option<&SoundDef> {
        for suffix in ["What", "Yes", "YesAttack", "Pissed", "Warcry"] {
            let candidate = format!("{unit_name}{suffix}");
            if let Some(def) = self.get(&candidate) {
                return Some(def);
            }
        }
        None
    }

    /// Resolve an attack acknowledgement for a unit display name.
    pub fn unit_attack(&self, unit_name: &str) -> Option<&SoundDef> {
        for suffix in ["YesAttack", "Attack", "What"] {
            let candidate = format!("{unit_name}{suffix}");
            if let Some(def) = self.get(&candidate) {
                return Some(def);
            }
        }
        None
    }

    /// Resolve a death sound for a unit display name.
    ///
    /// Death lines live in `AnimSounds.slk` (`FootmanDeath`), not in
    /// `UnitAckSounds.slk`; both tables are loaded into the same database, so
    /// the suffix lookup covers them. There is no `What` fallback: an ack row
    /// has `DistanceCutoff` 100000 and would make a building/unit death audible
    /// 12 000 units away (`GreatHallWhat` had no `GreatHallDeath` row).
    pub fn unit_death(&self, unit_name: &str) -> Option<&SoundDef> {
        self.get(&format!("{unit_name}Death"))
    }

    /// Resolve the "pissed" line (`{set}Pissed`), played after repeated
    /// selection taps.
    pub fn unit_pissed(&self, unit_name: &str) -> Option<&SoundDef> {
        for suffix in ["Pissed", "What", "Yes"] {
            let candidate = format!("{unit_name}{suffix}");
            if let Some(def) = self.get(&candidate) {
                return Some(def);
            }
        }
        None
    }

    /// Resolve the training-complete line (`{set}Ready`).
    pub fn unit_ready(&self, unit_name: &str) -> Option<&SoundDef> {
        for suffix in ["Ready", "What"] {
            let candidate = format!("{unit_name}{suffix}");
            if let Some(def) = self.get(&candidate) {
                return Some(def);
            }
        }
        None
    }
}

/// Unit id -> weapon sound family, parsed from `Units/UnitWeapons.slk`.
///
/// `UnitCombatSounds.slk` names its rows `<weapon><material>` (for example
/// `MetalMediumSlice` + `Flesh` -> `MetalMediumSliceFlesh`), and the weapon half
/// is the unit's own `weapType1`/`weapType2` cell. This table is that mapping,
/// so the impact sound is read from shipped data instead of guessed from the
/// unit's race or class.
#[derive(Debug, Clone, Default)]
pub struct UnitWeaponSounds {
    /// id -> `weapType1` (`MetalMediumSlice`, `AxeMediumChop`, ...).
    types: HashMap<String, String>,
    /// id -> `weapType2` (dual-wielding units ship a second family).
    secondary: HashMap<String, String>,
}

impl UnitWeaponSounds {
    /// Parse `Units/UnitWeapons.slk`.
    ///
    /// RoC has no header row and its columns are fixed: X1 = unit id,
    /// X4 = display name, X6 = `weapType1`, X7 = its target-class list.
    /// Empirically `hfoo` -> `MetalMediumSlice`, `ogru` -> `MetalMediumChop`,
    /// `hpea` -> `MetalLightChop`; a TFT header that names `weapType1`/`weapType2`
    /// overrides the shipped indices, and a `weapType2` cell is used only when
    /// the primary cell is empty (`_`).
    pub fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        let rows = parse_slk(bytes)?;
        let has_header = rows
            .first()
            .and_then(|row| row.get(&1))
            .map(|value| value.eq_ignore_ascii_case("unitWeapID"))
            .unwrap_or(false);
        let mut primary_col = 6usize;
        let mut secondary_col: Option<usize> = None;
        if has_header {
            if let Some(header) = rows.first() {
                for (col, value) in header {
                    match value.to_ascii_lowercase().as_str() {
                        "weaptype1" => primary_col = *col,
                        "weaptype2" => secondary_col = Some(*col),
                        _ => {}
                    }
                }
            }
        }
        let mut types = HashMap::new();
        let mut secondary = HashMap::new();
        for row in rows {
            let id = row.get(&1).cloned().unwrap_or_default();
            if id.len() != 4
                || id.eq_ignore_ascii_case("unitWeapID")
                || id.eq_ignore_ascii_case("id")
            {
                continue;
            }
            let key = id.to_ascii_lowercase();
            if let Some(primary) = weapon_cell(row.get(&primary_col)) {
                types.insert(key.clone(), primary);
            }
            if let Some(second) = secondary_col.and_then(|col| weapon_cell(row.get(&col))) {
                secondary.insert(key, second);
            }
        }
        Ok(Self { types, secondary })
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Primary weapon sound family for a unit id.
    pub fn weapon_type(&self, unit_id: &str) -> Option<&str> {
        let key = unit_id.to_ascii_lowercase();
        self.types
            .get(&key)
            .map(String::as_str)
            .or_else(|| self.secondary.get(&key).map(String::as_str))
    }

    /// Secondary family (dual-wield) when the primary cell is empty.
    pub fn secondary_type(&self, unit_id: &str) -> Option<&str> {
        self.secondary
            .get(&unit_id.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// `weapType` cells use `_`/`-` for "no weapon"; those are not sound families.
fn weapon_cell(value: Option<&String>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value == "_" || value == "-" {
        return None;
    }
    Some(value.to_string())
}

/// Map/campaign unit ids that are not rows in the shipped `Units/unitUI.slk`
/// but belong to a retail unit's voice family.///
/// `hgrunt` is the campaign/map Grunt id (the melee Grunt is `ogru`); maps
/// spawn it under the player's control, so the id must still resolve to the
/// `Grunt` sound set instead of going silent. Aliases are resolved by id only —
/// a unit with its own `unitSound` row always uses that row.
pub const DEFAULT_UNIT_ALIASES: &[(&str, &str)] = &[("hgrunt", "ogru")];

/// One unit's voice profile: the `unitSound` family plus the display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitSoundProfile {
    /// Four-letter unit id (`hfoo`).
    pub unit_id: String,
    /// Sound-set prefix from `unitUI.unitSound` (`Footman` -> `FootmanWhat`).
    pub set: String,
    /// Display name from `unitUI.name` (`footman`).
    pub name: String,
}

/// Unit id -> display name and sound set, parsed from `Units/unitUI.slk`.
///
/// Two retail schemas exist:
/// * RoC: `X1=id, X2=file, X3=name` (the display name doubles as the sound-set
///   prefix, e.g. `Footman` -> `FootmanWhat`).
/// * TFT (`War3x.mpq`): a header row (`unitUIID … file … unitSound … name`)
///   with `unitSound` (e.g. `Footman`) and `name` in separate columns. Using the
///   TFT `name` column ("footman") works case-insensitively, but `unitSound` is
///   the authoritative prefix and is preferred by the hooks.
#[derive(Debug, Clone)]
pub struct UnitNameMap {
    names: HashMap<String, String>,
    sound_sets: HashMap<String, String>,
    /// id -> target-surface class (`Flesh`, `Wood`, `Metal`, ...) from
    /// `unitUI.slk`; drives the weapon-impact row suffix.
    materials: HashMap<String, String>,
    /// id -> base id for ids the shipped table does not contain.
    aliases: HashMap<String, String>,
}

impl Default for UnitNameMap {
    fn default() -> Self {
        let mut map = Self {
            names: HashMap::new(),
            sound_sets: HashMap::new(),
            materials: HashMap::new(),
            aliases: HashMap::new(),
        };
        for (id, base) in DEFAULT_UNIT_ALIASES {
            map.add_alias(*id, *base);
        }
        map
    }
}

impl UnitNameMap {
    pub fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        let rows = parse_slk(bytes)?;
        // Header detection: TFT unitUI starts with `C;X1;Y1;K"unitUIID"`.
        let has_header = rows
            .first()
            .and_then(|row| row.get(&1))
            .map(|value| value.eq_ignore_ascii_case("unitUIID"))
            .unwrap_or(false);
        let mut name_col = 3usize;
        let mut sound_col: Option<usize> = None;
        // `unitUI.slk` column 34 is the target-surface class used to pick a
        // weapon-impact row (`MetalMediumSlice` + `Flesh` -> `...Flesh`). The RoC
        // table has no header row, so the shipped column index is the fallback;
        // a TFT header that names the column overrides it.
        let mut material_col = 34usize;
        if has_header {
            if let Some(header) = rows.first() {
                for (col, value) in header {
                    match value.to_ascii_lowercase().as_str() {
                        "name" => name_col = *col,
                        "unitsound" => sound_col = Some(*col),
                        "armortype" | "material" | "soundmaterial" => material_col = *col,
                        _ => {}
                    }
                }
            }
        }
        let mut names = HashMap::new();
        let mut sound_sets = HashMap::new();
        let mut materials = HashMap::new();
        for row in rows {
            if has_header && row.get(&1).map(|v| v.eq_ignore_ascii_case("unitUIID")).unwrap_or(false) {
                continue;
            }
            let id = row.get(&1).cloned().unwrap_or_default();
            let name = row.get(&name_col).cloned().unwrap_or_default();
            if id.len() != 4 || name.is_empty() || name.eq_ignore_ascii_case("name") {
                continue;
            }
            let key = id.to_ascii_lowercase();
            names.insert(key.clone(), name.clone());
            let set = sound_col
                .and_then(|col| row.get(&col))
                .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("unitSound"))
                .cloned()
                .unwrap_or(name);
            if let Some(material) = row
                .get(&material_col)
                .map(|value| value.trim())
                .filter(|value| crate::events::Material::from_class(value).is_some())
            {
                materials.insert(key.clone(), material.to_string());
            }
            sound_sets.insert(key, set);
        }
        Ok(Self {
            names,
            sound_sets,
            materials,
            aliases: DEFAULT_UNIT_ALIASES
                .iter()
                .map(|(id, base)| (id.to_ascii_lowercase(), base.to_ascii_lowercase()))
                .collect(),
        })
    }

    /// Register a map/campaign id alias (`hgrunt` -> `ogru`).
    ///
    /// Later aliases win; an alias whose base is itself aliased resolves
    /// through the chain (bounded, so a cycle cannot hang the lookup).
    pub fn add_alias(&mut self, unit_id: &str, base_id: &str) {
        if unit_id.eq_ignore_ascii_case(base_id) {
            return;
        }
        self.aliases
            .insert(unit_id.to_ascii_lowercase(), base_id.to_ascii_lowercase());
    }

    /// Registered aliases (`id -> base`), sorted for reports.
    pub fn aliases(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .aliases
            .iter()
            .map(|(id, base)| (id.clone(), base.clone()))
            .collect();
        out.sort();
        out
    }

    /// Follow aliases until the id names a row in the shipped table (or the
    /// chain ends). Returns the resolved id.
    pub fn resolve_id(&self, unit_id: &str) -> Option<String> {
        let mut id = unit_id.to_ascii_lowercase();
        for _ in 0..DEFAULT_UNIT_ALIASES.len().max(4) {
            if self.names.contains_key(&id) || self.sound_sets.contains_key(&id) {
                return Some(id);
            }
            match self.aliases.get(&id) {
                Some(base) => id = base.clone(),
                None => return None,
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn name(&self, unit_id: &str) -> Option<&str> {
        let key = self.resolve_id(unit_id)?;
        self.names.get(&key).map(String::as_str)
    }

    /// Target-surface class of a unit (`Flesh`, `Wood`, `Metal`, `Stone`,
    /// `Ethereal`) from `unitUI.slk`.
    pub fn material(&self, unit_id: &str) -> Option<crate::events::Material> {
        let key = self.resolve_id(unit_id)?;
        self.materials
            .get(&key)
            .and_then(|value| crate::events::Material::from_class(value))
    }

    /// Number of rows that carry a material class (report/coverage line).
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Sound-set prefix for a unit (`Footman` -> `FootmanWhat`). Falls back to
    /// the display name when the table has no dedicated sound column.
    ///
    /// Map/campaign ids absent from the shipped table resolve through
    /// [`UnitNameMap::aliases`] (`hgrunt` -> `ogru` -> `Grunt`).
    pub fn sound_set(&self, unit_id: &str) -> Option<&str> {        let key = self.resolve_id(unit_id)?;
        if let Some(set) = self.sound_sets.get(&key) {
            return Some(set.as_str());
        }
        self.names.get(&key).map(String::as_str)
    }

    /// Sound-set candidates for a unit id, authoritative column first.
    ///
    /// TFT `UnitUI.slk` has a dedicated `unitSound` column (`hpea` -> `Peasant`);
    /// RoC `unitUI.slk` has no such column, so its display `name` doubles as the
    /// set. Rows that store an empty `unitSound` cell fall back to `name` the
    /// same way. Both values come from the shipped object data — the list never
    /// contains an invented or race-generic set, and a def with neither value
    /// yields an empty list (silent, not another unit's voice).
    ///
    /// The hook layer walks the list and uses the first set whose
    /// `UnitAckSounds` row exists, so a stale `unitSound` cell on one schema
    /// cannot silence a unit that still has a usable display-name set.
    pub fn sound_set_candidates(&self, unit_id: &str) -> Vec<&str> {
        let Some(key) = self.resolve_id(unit_id) else {
            return Vec::new();
        };
        let mut out: Vec<&str> = Vec::with_capacity(2);
        if let Some(set) = self.sound_sets.get(&key).map(String::as_str)
            && !set.is_empty()
        {
            out.push(set);
        }
        if let Some(name) = self.names.get(&key).map(String::as_str)
            && !name.is_empty()
            && !out.iter().any(|set| set.eq_ignore_ascii_case(name))
        {
            out.push(name);
        }
        out
    }

    /// Raw `unitSound` value stored for the id, without alias resolution.
    pub fn raw_sound_set(&self, unit_id: &str) -> Option<&str> {
        self.sound_sets
            .get(&unit_id.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Full voice profile for a unit id (resolved through aliases).
    pub fn profile(&self, unit_id: &str) -> Option<UnitSoundProfile> {
        let key = self.resolve_id(unit_id)?;
        let set = self.sound_set(unit_id)?.to_string();
        Some(UnitSoundProfile {
            unit_id: unit_id.to_ascii_lowercase(),
            set,
            name: self
                .names
                .get(&key)
                .cloned()
                .unwrap_or_else(|| unit_id.to_string()),
        })
    }

    /// Ids that are present in the shipped table (no aliases).
    pub fn contains(&self, unit_id: &str) -> bool {
        self.names.contains_key(&unit_id.to_ascii_lowercase())
    }

    /// All unit ids in the table (catalog/coverage tooling).
    pub fn ids(&self) -> Vec<String> {
        self.names.keys().cloned().collect()
    }

    /// Reverse lookup: unit display name -> unit id (e.g. `HeroArchDruid` -> `Earh`).
    /// Returns the first id in stable (sorted) order when several units share a name.
    pub fn id_for_name(&self, name: &str) -> Option<&str> {
        let mut ids: Vec<(&String, &String)> = self
            .names
            .iter()
            .filter(|(id, value)| {
                value.eq_ignore_ascii_case(name)
                    || self
                        .sound_sets
                        .get(*id)
                        .map(|set| set.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
            })
            .collect();
        ids.sort_by(|a, b| a.0.cmp(b.0));
        ids.first().map(|(id, _)| id.as_str())
    }
}

/// Parse an SLK sheet into rows of `(column, value)`.
///
/// SLK cells look like `C;Y3;X2;K"text"`; `Y` starts a row, `X` selects the
/// column, and only the changed coordinates are repeated. Formatting records
/// (`F;Y10;X1`) also carry coordinates and can be the *only* record that starts
/// a row — `MIDISounds.slk` and `UnitAckSounds.slk` do this for 9 and 8 rows,
/// and ignoring them merged those rows into the previous one (a lost sound).
/// Formatting records therefore move the cursor but never insert a value.
/// `P;` palette and `E` end records are skipped.
pub fn parse_slk(bytes: &[u8]) -> Result<Vec<HashMap<usize, String>>, AudioError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AudioError::Corrupt("SLK is not UTF-8"))?;
    let mut rows: Vec<HashMap<usize, String>> = Vec::new();
    let mut row_index: Option<usize> = None;
    let mut column: usize = 0;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let value_record = line.starts_with("C;");
        if !value_record && !line.starts_with("F;") {
            continue;
        }
        let mut fields = line.split(';');
        let _ = fields.next();
        let mut y: Option<usize> = None;
        let mut x: Option<usize> = None;
        let mut value: Option<String> = None;
        for field in fields {
            let mut chars = field.chars();
            match chars.next() {
                Some('Y') => y = chars.as_str().parse().ok(),
                Some('X') => x = chars.as_str().parse().ok(),
                Some('K') if value_record => {
                    let raw = chars.as_str();
                    let parsed = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                        raw[1..raw.len() - 1].replace("\"\"", "\"")
                    } else {
                        raw.to_string()
                    };
                    value = Some(parsed);
                }
                _ => {}
            }
        }
        if let Some(y) = y {
            // Y is 1-based; grow the vector to fit.
            if y == 0 {
                continue;
            }
            while rows.len() < y {
                rows.push(HashMap::new());
            }
            row_index = Some(y - 1);
            column = 1;
        }
        let Some(row) = row_index else {
            continue;
        };
        if let Some(x) = x {
            column = x;
        }
        if let Some(value) = value {
            rows[row].insert(column, value);
            column += 1;
        }
    }
    Ok(rows)
}

fn number(row: &HashMap<usize, String>, column: usize) -> Option<f32> {
    row.get(&column).and_then(|value| value.trim().parse().ok())
}

/// FNV-1a over lowercased bytes: stable `NODUPLICATES` key.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn extracted(name: &str) -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3")
            .join(name);
        path.is_file().then_some(path)
    }

    #[test]
    fn parses_slk_rows_and_cells() {
        let slk = b"ID;PWXL;N;E\nC;Y1;X1;K\"SoundName\"\nC;X2;K\"FileNames\"\nC;Y2;X1;K\"TestWhat\"\nC;X2;K\"TestWhat1.wav,TestWhat2.wav\"\nC;X3;K\"Units\\\\Test\\\\\"\nC;X4;K127\n";
        let rows = parse_slk(slk).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].get(&1).map(String::as_str), Some("TestWhat"));
        assert_eq!(rows[1].get(&4).map(String::as_str), Some("127"));
    }

    #[test]
    fn parses_real_ack_table_and_resolves_paths() {
        let Some(path) = extracted("UI/SoundInfo/UnitAckSounds.slk") else {
            eprintln!("skipping: extracted sound tables missing");
            return;
        };
        let bytes = std::fs::read(path).expect("read slk");
        let mut db = SoundInfoDb::new();
        let added = db.add_table("UnitAckSounds", &bytes).expect("parse");
        println!("defs={added} tables={:?}", db.table_counts);
        // 747 retail rows: 739 are `C;Y`-started and eight (`HuntressWarcry`,
        // `MurlocReady`, `OgreReady`, `OgreWarcry`, `SatyrReady`,
        // `SatyrWarcry`, `ChaosWarlordReady`, `ChaosWarlordWarcry`) start with
        // an `F;Y` formatting record only. Before `parse_slk` honoured those,
        // the eight rows were lost.
        assert_eq!(added, 747, "retail table has 747 sound definitions");
        let footman = db.get("FootmanWhat").expect("FootmanWhat");
        assert_eq!(footman.files.len(), 4);
        assert_eq!(footman.directory, "Units\\Human\\Footman\\");
        assert!((footman.volume - 1.0).abs() < 0.01, "volume {}", footman.volume);
        let paths = footman.paths();
        assert_eq!(paths[0], "Units\\Human\\Footman\\FootmanWhat1.wav");
        assert!(db.unit_ack("Footman").is_some());
        assert!(db.unit_attack("Footman").is_some());
    }

    #[test]
    fn directory_base_join_handles_both_retail_spellings() {
        let with_sep = SoundDef {
            name: "A".into(),
            files: vec!["AWhat1.wav".into()],
            directory: "Units\\Human\\Footman\\".into(),
            ..Default::default()
        };
        assert_eq!(with_sep.paths(), vec!["Units\\Human\\Footman\\AWhat1.wav"]);
        let without_sep = SoundDef {
            name: "BlizzardTarget".into(),
            files: vec!["BlizzardTarget1.wav".into()],
            directory: "Abilities\\Spells\\Human\\Blizzard".into(),
            ..Default::default()
        };
        assert_eq!(
            without_sep.paths(),
            vec!["Abilities\\Spells\\Human\\Blizzard\\BlizzardTarget1.wav"]
        );
        let absolute = SoundDef {
            name: "Abs".into(),
            files: vec!["Sound\\Music\\x.mp3".into()],
            directory: "ignored\\".into(),
            ..Default::default()
        };
        assert_eq!(absolute.paths(), vec!["Sound\\Music\\x.mp3"]);
    }

    #[test]
    fn unit_names_come_from_unitui() {        let Some(path) = extracted("Units/unitUI.slk") else {
            return;
        };
        let bytes = std::fs::read(path).expect("read");
        let names = UnitNameMap::parse(&bytes).expect("parse");
        println!("units={}", names.len());
        // RoC `unitUI.slk` has a header row: X3 = unitSound, X6 = name.
        assert_eq!(names.sound_set("hfoo"), Some("Footman"));
        assert_eq!(names.sound_set("HFOO"), Some("Footman"));
        assert!(
            names.name("hfoo").map(|n| n.eq_ignore_ascii_case("footman")).unwrap_or(false),
            "display name: {:?}",
            names.name("hfoo")
        );
        assert_eq!(names.id_for_name("Footman"), Some("hfoo"));
        assert!(names.len() > 300);
    }

    /// `soundinfo.channel-number`: the `Channel` column is the mixer channel
    /// (`play_instance+0x70`, mixer `param[0x13]`). Non-numeric cells are
    /// ungrouped.
    #[test]
    fn channel_number_parses_the_slk_cell() {
        let def = SoundDef {
            channel: "3".into(),
            ..Default::default()
        };
        assert_eq!(def.channel_number(), 3);
        let def = SoundDef {
            channel: " 12 ".into(),
            ..Default::default()
        };
        assert_eq!(def.channel_number(), 12);
        for raw in ["", "-", "none", "0"] {
            let def = SoundDef {
                channel: raw.into(),
                ..Default::default()
            };
            assert_eq!(def.channel_number(), 0, "channel {raw:?} is ungrouped");
        }
    }

    /// `soundinfo.flags`: the shipped flags are a comma separated list
    /// (`UnitAckSounds` rows carry `CHANNELFULLPREEMPT,RANDOMPITCH`,
    /// `AmbienceSounds` rows `NODUPLICATES,SCALEPRIORITY`).
    #[test]
    fn flags_drive_preempt_and_dedup() {
        let ack = SoundDef {
            name: "FootmanWhat".into(),
            flags: "WANT3D,NODUPEUSERNAMES,CHANNELFULLPREEMPT,RANDOMPITCH".into(),
            ..Default::default()
        };
        assert!(ack.has_flag("CHANNELFULLPREEMPT"));
        assert!(ack.has_flag("channelfullpreempt"));
        assert!(!ack.has_flag("NODUPLICATES"));
        assert_ne!(ack.dedup_key(), 0, "NODUPEUSERNAMES is a dedup flag");
        let ambient = SoundDef {
            name: "AmbientSoundRain".into(),
            flags: "WANT3D,LOOPING,NODUPLICATES,SCALEPRIORITY".into(),
            ..Default::default()
        };
        assert!(ambient.has_flag("NODUPLICATES"));
        assert_ne!(ambient.dedup_key(), 0);
        assert_eq!(ambient.dedup_key(), ambient.dedup_key());
        let plain = SoundDef {
            name: "UnitAckWhat".into(),
            flags: "WANT3D".into(),
            ..Default::default()
        };
        assert_eq!(plain.dedup_key(), 0, "no dedup flag means no key");
    }

    /// Real shipped rows: `UnitAckSounds` groups lines by *meaning* on the
    /// mixer channel (What/Pissed/Warcry = 1, YesAttack = 2, Yes = 3,
    /// Ready = 4), every row priority 1000 with `CHANNELFULLPREEMPT`.
    #[test]
    fn real_ack_rows_carry_channel_and_priority() {
        let Some(path) = extracted("UI/SoundInfo/UnitAckSounds.slk") else {
            return;
        };
        let mut db = SoundInfoDb::new();
        db.add_table("UnitAckSounds", &std::fs::read(path).expect("read"))
            .expect("parse");
        let what = db.get("FootmanWhat").expect("FootmanWhat");
        assert_eq!(what.channel_number(), 1, "What rows use channel 1");
        assert_eq!(what.priority, 1000, "retail ack priority");
        assert!(what.has_flag("CHANNELFULLPREEMPT"));
        // The four channels are the four semantic groups of the table.
        let mut groups = std::collections::BTreeMap::new();
        for (suffix, channel) in [
            ("What", 1u16),
            ("Pissed", 1),
            ("Warcry", 1),
            ("YesAttack", 2),
            ("Yes", 3),
            ("Ready", 4),
        ] {
            let found: std::collections::BTreeSet<u16> = db
                .iter()
                .filter(|def| def.name.ends_with(suffix))
                .map(SoundDef::channel_number)
                .collect();
            assert_eq!(found, [channel].into_iter().collect(), "{suffix} channel");
            groups.insert(suffix, channel);
        }
        println!("ack channel groups={groups:?}");
    }
}
