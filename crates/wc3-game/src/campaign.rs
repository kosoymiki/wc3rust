//! Campaign data model: campaign → mission → cutscene → trigger.
//!
//! A Warcraft III campaign is a list of missions; each mission is a map whose
//! script carries the fiction: quests (the objectives panel), cinematics
//! (camera moves, transmissions, scene text), and the triggers that sequence
//! them. The retail campaign descriptor (`war3campaign.w3f` inside a `.w3n`)
//! is not shipped in the data archives this installation carries — the mission
//! maps under `Maps\Campaign\` and `Maps\FrozenThrone\Campaign\` are — so the
//! model is built from what the maps themselves contain, and nothing is
//! invented for a mission that is absent.
//!
//! Chapter names come from the same place the retail campaign screen reads
//! them: `UI\CampaignStrings.txt` (RoC) and `UI\CampaignStrings_exp.txt` (TFT).
//! Both formats shipped by the game are parsed — the per-section
//! `TitleN`/`MissionN`/`FileN` arrays of the 1.0 tables and the
//! `MissionN="header","name","map"` rows of the 1.07+ tables. A chapter the
//! tables do not name falls back to its `war3map.w3i` name resolved through
//! `war3map.wts`, and only then to [`title_from_id`].
//!
//! The parse is deliberately script-driven, not metadata-driven: `war3map.wts`
//! holds the strings, and the quest/cinematic structure is exactly the call
//! sequence the map's `war3map.j` performs. `Human01.w3m` is the worked
//! example (`Mission::parse`), and `campaign.scaffold.parses-human01` asserts
//! the result on the real file.
//!
//! Correction for the record: `war3map.w3c` is the **camera** list (the parser
//! in `wc3-re` and the campaign cutscene inventory both read it that way), not
//! campaign metadata. Campaign metadata is `war3campaign.w3f`; cameras are
//! parsed here as cutscene support data, not as the campaign descriptor.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use wc3_re::jass::vm::{CameraEvent, Quest, SubtitleEvent, World};
use wc3_re::mapfiles::MapBundle;
use wc3_re::wts::Wts;

/// A campaign: a named sequence of missions.
#[derive(Debug, Clone, Default)]
pub struct Campaign {
    pub name: String,
    pub chapters: Vec<Chapter>,
}

/// One campaign chapter (a mission or an interlude).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    /// Stable id derived from the map file name (`Human01`).
    pub id: String,
    pub title: String,
    /// Local map path: the install file when there is one, else the extraction
    /// target in [`campaign_cache_dir`] for a chapter that lives in an archive.
    pub map: PathBuf,
    /// Interludes are cutscene-only maps (`Human02Interlude`).
    pub interlude: bool,
    /// Where the bytes come from (archive entries are extracted on demand).
    pub source: MapSource,
}

/// Where a chapter's map bytes live.
///
/// The device ships only `Human04.w3m`/`Orc01.w3m` next to `War3.mpq`; the other
/// 42 chapters are inside the mounted archive. The registry therefore has to
/// resolve a chapter to `Maps\Campaign\<id>.w3m` and read the bytes from the
/// archive, with the loose `files/` copy as a fast-path override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapSource {
    /// A `.w3m`/`.w3x` file already on disk.
    File,
    /// An entry of a mounted archive (`War3.mpq` / `War3x.mpq`).
    Mount { archive: PathBuf, entry: String },
}

impl Default for MapSource {
    fn default() -> Self {
        Self::File
    }
}

impl MapSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Mount { .. } => "mpq",
        }
    }
}

/// Chapter names read from the install's campaign string tables.
///
/// The retail campaign screen does not take a chapter name from the map file
/// stem: `Maps\Campaign\Orc01.w3m` is "Чужие берега" (`UI\CampaignStrings.txt`)
/// and `Maps\FrozenThrone\Campaign\HumanX01.w3x` is "Чужая работа"
/// (`UI\CampaignStrings_exp.txt`). Both tables are shipped in the archives, so
/// the names are read from them instead of being derived from the file id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CampaignTitles {
    /// Lowercase map stem (`orc01`) → mission name.
    missions: BTreeMap<String, String>,
    /// Lowercase campaign section (`tutorial`, `human`, …) → campaign name.
    campaigns: BTreeMap<String, String>,
}

impl CampaignTitles {
    /// Display name of a chapter id or map path, when the tables name it.
    ///
    /// `_exp.txt` may localise the file name itself (the TFT night-elf finale
    /// is `NightElfX08Финал.w3x` in the table and `NightElfX08Finale.w3x` in
    /// the archive). When no exact key exists, a unique ASCII-prefix match
    /// resolves that row without ever guessing between two chapters.
    pub fn mission(&self, id: &str) -> Option<&str> {
        let key = normalize_chapter_id(id);
        if let Some(title) = self.missions.get(&key) {
            return Some(title.as_str());
        }
        // Only a key with a localised suffix can match here; a pure-ASCII key
        // that differs from the query is a different chapter, not a match.
        let mut hits = self.missions.iter().filter(|(known, _)| {
            let prefix = ascii_prefix(known);
            !prefix.is_empty() && prefix.len() < known.len() && key.starts_with(prefix)
        });
        let first = hits.next()?;
        hits.next().is_none().then_some(first.1.as_str())
    }

    /// Display name of a campaign section (`tutorial`, `human`, `undead`, …).
    pub fn campaign(&self, section: &str) -> Option<&str> {
        self.campaigns
            .get(&section.trim().to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Number of named missions.
    pub fn len(&self) -> usize {
        self.missions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.missions.is_empty()
    }

    /// Overlays a newer table (the locale archive wins over the base archive).
    fn merge(&mut self, newer: CampaignTitles) {
        self.missions.extend(newer.missions);
        self.campaigns.extend(newer.campaigns);
    }
}

/// Lowercase map stem (`Maps\Campaign\Orc01.w3m` → `orc01`).
fn normalize_chapter_id(id: &str) -> String {
    let file = id.rsplit(['\\', '/']).next().unwrap_or(id).trim();
    let stem = match file.rsplit_once('.') {
        Some((stem, extension))
            if extension.eq_ignore_ascii_case("w3m") || extension.eq_ignore_ascii_case("w3x") =>
        {
            stem
        }
        _ => file,
    };
    stem.to_ascii_lowercase()
}

/// ASCII prefix of a key whose tail is localised (`nightelfx08финал` →
/// `nightelfx08`).
fn ascii_prefix(key: &str) -> &str {
    let end = key.find(|ch: char| !ch.is_ascii()).unwrap_or(key.len());
    &key[..end]
}

/// Value of one `key=value` line with its surrounding quotes removed.
fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

/// Splits a campaign-table value into its quoted fields.
///
/// `"Глава 1","Чужие берега","Maps\Campaign\Orc01.w3m"` → three fields, with
/// the commas inside `"Глава 7, часть 1"` kept inside the second field.
fn quoted_fields(value: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in value.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() || !fields.is_empty() {
        fields.push(current.trim().to_string());
    }
    fields
}

/// Splits `Mission10` into `("mission", 10)`; other keys keep index `0`.
fn split_key_index(key: &str) -> (&str, usize) {
    let digits = key
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digits == 0 {
        return (key, 0);
    }
    let split = key.len() - digits;
    let index = key[split..].parse::<usize>().unwrap_or(0);
    (&key[..split], index)
}

/// Parses both shipped campaign-table formats.
///
/// RoC 1.0 (`UI\CampaignStrings.txt` in `War3.mpq`) names the files in a
/// `FileN` array and the missions in a parallel `MissionN` array:
///
/// ```text
/// [Orc]
/// Name="Вторжение на Калимдор"
/// Mission0="Чужие берега"
/// File0="Orc01"
/// ```
///
/// The 1.07+ table (`UI\CampaignStrings_exp.txt`) puts header, name and map
/// path on one line:
///
/// ```text
/// Mission0="Глава 1","Наги","Maps\FrozenThrone\Campaign\NightElfX01.w3x"
/// ```
fn parse_campaign_strings(text: &str) -> CampaignTitles {
    let mut titles = CampaignTitles::default();
    let mut section = String::new();
    let mut files: BTreeMap<usize, String> = BTreeMap::new();
    let mut missions: BTreeMap<usize, String> = BTreeMap::new();
    let flush = |titles: &mut CampaignTitles,
                 files: &mut BTreeMap<usize, String>,
                 missions: &mut BTreeMap<usize, String>| {
        for (index, file) in files.iter() {
            let Some(mission) = missions.get(index) else {
                continue;
            };
            let id = normalize_chapter_id(file);
            let name = mission.trim();
            if !id.is_empty() && !name.is_empty() {
                titles.missions.insert(id, name.to_string());
            }
        }
        files.clear();
        missions.clear();
    };
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            flush(&mut titles, &mut files, &mut missions);
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("Name") {
            let name = unquote(value);
            if !section.is_empty() && !name.is_empty() {
                titles.campaigns.insert(section.clone(), name);
            }
            continue;
        }
        let (base, index) = split_key_index(key);
        if base.eq_ignore_ascii_case("File") {
            let file = unquote(value);
            if !file.is_empty() {
                files.insert(index, file);
            }
        } else if base.eq_ignore_ascii_case("Mission") {
            let fields = quoted_fields(value);
            match fields.as_slice() {
                // 1.07+ rows carry the map path in the third field.
                [_, name, map, ..] if !name.trim().is_empty() && !map.trim().is_empty() => {
                    let id = normalize_chapter_id(map);
                    if !id.is_empty() {
                        titles.missions.insert(id, name.trim().to_string());
                    }
                }
                // 1.0 rows are names only; `FileN` supplies the id.
                [name, ..] if !name.trim().is_empty() => {
                    missions.insert(index, name.trim().to_string());
                }
                _ => {}
            }
        }
    }
    flush(&mut titles, &mut files, &mut missions);
    titles
}

/// Reads every campaign table out of the mounted archives, later archives
/// (the locale layer) winning over earlier ones.
pub fn campaign_titles(archives: &[PathBuf]) -> CampaignTitles {
    let mut titles = CampaignTitles::default();
    for archive in archives {
        let Ok(mpq) = wc3_mpq::MpqArchive::open(archive) else {
            continue;
        };
        for wanted in ["UI\\CampaignStrings.txt", "UI\\CampaignStrings_exp.txt"] {
            let entry = mpq
                .listfile()
                .iter()
                .find(|name| name.eq_ignore_ascii_case(wanted))
                .cloned()
                .unwrap_or_else(|| wanted.to_string());
            let Ok(bytes) = mpq.read(&entry) else {
                continue;
            };
            titles.merge(parse_campaign_strings(&String::from_utf8_lossy(&bytes)));
        }
    }
    titles
}

/// Applies the campaign tables, then the map's own `war3map.w3i` name, and
/// leaves [`title_from_id`] as the last resort.
fn apply_chapter_titles(chapters: &mut [Chapter], titles: &CampaignTitles) {
    for chapter in chapters {
        if let Some(title) = titles.mission(&chapter.id) {
            chapter.title = title.to_string();
        } else if let Some(title) = map_info_title(chapter) {
            chapter.title = title;
        }
    }
}

/// `war3map.w3i` name of a chapter, resolved through its `war3map.wts`.
fn map_info_title(chapter: &Chapter) -> Option<String> {
    let map = chapter.resolved_map().ok()?;
    let mpq = wc3_mpq::MpqArchive::open(&map).ok()?;
    let info = wc3_formats::w3i::MapInfo::parse(&mpq.read("war3map.w3i").ok()?).ok()?;
    let mut name = info.name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    if let Ok(wts) = mpq.read("war3map.wts")
        && let Ok(wts) = Wts::parse(&wts)
    {
        name = wts.resolve(&name);
    }
    let name = name.trim();
    if name.is_empty()
        || name.starts_with("TRIGSTR_")
        || name.eq_ignore_ascii_case(&chapter.id)
    {
        None
    } else {
        Some(name.to_string())
    }
}

impl Chapter {
    /// The archive entry a mounted chapter came from (`Maps\Campaign\Human01.w3m`).
    pub fn archive_entry_path(&self) -> Option<&str> {
        match &self.source {
            MapSource::Mount { entry, .. } => Some(entry.as_str()),
            MapSource::File => None,
        }
    }

    /// The path `Assets::load` can open: the loose file, or the archive entry
    /// extracted into [`campaign_cache_dir`] (atomic temp+rename, reused when it
    /// is already there).
    pub fn resolved_map(&self) -> Result<PathBuf, String> {
        match &self.source {
            MapSource::File => {
                if self.map.is_file() {
                    Ok(self.map.clone())
                } else {
                    Err(format!("chapter {} map missing: {}", self.id, self.map.display()))
                }
            }
            MapSource::Mount { archive, entry } => {
                if self.map.is_file() {
                    return Ok(self.map.clone());
                }
                extract_archive_entry(archive, entry).map(|(path, _)| path)
            }
        }
    }
}

impl Campaign {
    /// Builds the chapter list from a directory of campaign maps.
    ///
    /// The order is the retail chapter order: prologue, the four race
    /// campaigns in their campaign order (Human, Undead, Orc, NightElf), then
    /// the credits. A file that does not match the naming falls to the end,
    /// sorted, so nothing disappears from the list.
    pub fn from_map_dir(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        // A loose chapter still gets its retail name from the install's own
        // campaign tables when the archives are present; the archive is not a
        // chapter source here, only a title source.
        Self::from_sources_with_titles(Some(dir.as_ref()), &[], &campaign_archives())
    }

    /// Builds the chapter list from the loose install files **and** the mounted
    /// archives: a chapter that exists in `Maps\Campaign\` of `War3.mpq` is part
    /// of the campaign even when its file was never copied out of the archive.
    ///
    /// Order is the retail chapter order (see [`chapter_rank`]). The archive is
    /// authoritative when it carries the chapter: the operator's install is the
    /// MPQ, and a loose `.w3m` next to it is a development copy that must not
    /// shadow the shipped map. `WC3RUST_CAMPAIGN_SOURCE=file` restores the old
    /// loose-file-first behaviour for a debug session.
    pub fn from_sources(
        disk: Option<&Path>,
        archives: &[PathBuf],
    ) -> std::io::Result<Self> {
        Self::from_sources_with_titles(disk, archives, archives)
    }

    fn from_sources_with_titles(
        disk: Option<&Path>,
        archives: &[PathBuf],
        title_archives: &[PathBuf],
    ) -> std::io::Result<Self> {
        let prefer_disk = campaign_map_preference() == CampaignMapSource::File;
        let mut chapters = Vec::new();
        if let Some(dir) = disk {
            chapters.extend(Self::disk_chapters(dir)?);
        }
        // A directory that is not the curated `Campaign` tree can hold melee
        // maps: keep only chapter-shaped ids, and fall back to everything when
        // the filter would empty a real campaign directory.
        if chapters.is_empty()
            && let Some(dir) = disk
        {
            chapters.extend(Self::disk_chapters(dir)?);
        }
        for archive in archives {
            for (entry, id) in archive_campaign_entries(archive) {
                let mounted = || Chapter {
                    title: title_from_id(&id),
                    interlude: is_interlude_id(&id),
                    map: campaign_cache_map_path(entry_file_name(&entry)),
                    id: id.clone(),
                    source: MapSource::Mount {
                        archive: archive.clone(),
                        entry: entry.clone(),
                    },
                };
                if let Some(known) = chapters
                    .iter_mut()
                    .find(|known| known.id.eq_ignore_ascii_case(&id))
                {
                    // The archive wins unless the caller asked for loose files.
                    if !prefer_disk && known.source == MapSource::File {
                        *known = mounted();
                    }
                    continue;
                }
                chapters.push(mounted());
            }
        }
        chapters.sort_by(|left, right| {
            chapter_rank(&left.id)
                .cmp(&chapter_rank(&right.id))
                .then_with(|| left.id.cmp(&right.id))
        });
        // Real chapter names come from the install's own campaign tables; the
        // file-id spelling is only the last resort when a chapter is absent
        // from every shipped table and carries no `war3map.w3i` name.
        apply_chapter_titles(&mut chapters, &campaign_titles(title_archives));
        Ok(Self {
            name: "Warcraft III: Reign of Chaos".to_string(),
            chapters,
        })
    }

    /// Chapters from a directory of `.w3m`/`.w3x` files, filtered to the retail
    /// chapter naming unless the directory is a curated `Campaign` tree.
    fn disk_chapters(dir: &Path) -> std::io::Result<Vec<Chapter>> {
        let curated = dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("campaign"));
        let mut all = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("w3m") || ext.eq_ignore_ascii_case("w3x"))
            {
                continue;
            }
            let id = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            all.push(Chapter {
                title: title_from_id(&id),
                interlude: is_interlude_id(&id),
                id,
                map: path,
                source: MapSource::File,
            });
        }
        if curated {
            return Ok(all);
        }
        let filtered: Vec<Chapter> = all
            .iter()
            .filter(|chapter| is_chapter_id(&chapter.id))
            .cloned()
            .collect();
        Ok(if filtered.is_empty() { all } else { filtered })
    }

    pub fn chapter(&self, id: &str) -> Option<&Chapter> {
        self.chapters
            .iter()
            .find(|chapter| chapter.id.eq_ignore_ascii_case(id))
    }
}

fn race_order(id: &str) -> u8 {
    let lowered = id.to_ascii_lowercase();
    if lowered.starts_with("prologue") {
        0
    } else if lowered.starts_with("human") {
        1
    } else if lowered.starts_with("undead") {
        2
    } else if lowered.starts_with("orc") {
        3
    } else if lowered.starts_with("nightelf") {
        4
    } else if lowered.contains("credits") {
        6
    } else {
        5
    }
}

/// Campaign a chapter belongs to for the unlock chain.
///
/// RoC and TFT are separate campaigns within a race: `Human01` and `HumanX01`
/// are each the first chapter of their own campaign, so starting TFT must not
/// require the whole RoC human campaign to be cleared first.
fn campaign_order(id: &str) -> u8 {
    race_order(id) + if is_expansion_id(id) { 16 } else { 0 }
}

fn chapter_number(id: &str) -> u32 {
    let digits: String = id
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(99)
}

/// `true` for a TFT chapter id (`HumanX01`, `OrcX01_02`, `NightElfX08Finale`).
fn is_expansion_id(id: &str) -> bool {
    let lowered = id.to_ascii_lowercase();
    ["prologue", "human", "undead", "orc", "nightelf"]
        .iter()
        .any(|prefix| {
            lowered
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('x'))
        })
}

fn chapter_rank(id: &str) -> (u8, u8, u32, bool) {
    // Interludes follow the mission they close, so `Human02Interlude` sorts
    // after `Human02` and before `Human03`. The RoC campaign precedes the TFT
    // chapters of the same race: the base campaign is the authored order and
    // the expansion is an addendum to it.
    (
        u8::from(is_expansion_id(id)),
        race_order(id),
        chapter_number(id),
        id.to_ascii_lowercase().contains("interlude"),
    )
}

/// Retail chapter naming (`Prologue01`, `Human02Interlude`, `WarCraftIIICredits`).
///
/// The campaign screen must never list a melee map as a chapter: the app data
/// dir holds 38 melee maps in `files/Maps` and only a few loose chapters.
pub fn is_chapter_id(id: &str) -> bool {
    let lowered = id.to_ascii_lowercase();
    ["prologue", "human", "undead", "orc", "nightelf"]
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
        || (lowered.contains("credits") && !lowered.starts_with("melee"))
}

/// Cutscene-only chapters: interludes and credits reels.
pub fn is_interlude_id(id: &str) -> bool {
    let lowered = id.to_ascii_lowercase();
    lowered.contains("interlude") || lowered.contains("credits")
}

/// Campaign group index (into `ui::menu::CAMPAIGN_GROUPS`) for a chapter id.
///
/// Prologue, Human, Undead, Orc, Night Elf, then `Extra` for the credits reels
/// and any chapter that does not carry a race prefix.
pub fn chapter_group(id: &str) -> usize {
    let lowered = id.to_ascii_lowercase();
    if lowered.starts_with("prologue") {
        0
    } else if lowered.starts_with("human") {
        1
    } else if lowered.starts_with("undead") {
        2
    } else if lowered.starts_with("orc") {
        3
    } else if lowered.starts_with("nightelf") {
        4
    } else {
        5
    }
}

/// Where a chapter's map bytes may come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignMapSource {
    /// Loose `.w3m` wins over the archive (development override).
    File,
    /// The mounted archive is authoritative when it carries the chapter.
    Mount,
}

/// Chapter source preference (`WC3RUST_CAMPAIGN_SOURCE=file|mpq`).
///
/// The default is the archive: the operator's install *is* the MPQ, and a loose
/// copy next to it is a harness artefact. `file` keeps a debug session on the
/// extracted tree without touching the archive.
pub fn campaign_map_preference() -> CampaignMapSource {
    match std::env::var("WC3RUST_CAMPAIGN_SOURCE") {
        Ok(value) if value.eq_ignore_ascii_case("file") => CampaignMapSource::File,
        Ok(value) if value.eq_ignore_ascii_case("mpq") => CampaignMapSource::Mount,
        Ok(value) if !value.trim().is_empty() => {
            crate::dev_log!("CAMPAIGN_SOURCE_IGNORED value={value} (use file|mpq)");
            CampaignMapSource::Mount
        }
        _ => CampaignMapSource::Mount,
    }
}

/// Directory the archive maps are extracted to (one file per chapter, reused).
///
/// `WC3RUST_MAP_CACHE` overrides it; the default is the platform temp dir
/// (`$PREFIX/tmp` on Termux/Android), never a hardcoded `/tmp`.
pub fn campaign_cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WC3RUST_MAP_CACHE") {
        return PathBuf::from(dir);
    }
    std::env::temp_dir().join("wc3rust-maps")
}

/// Extraction target for an archive chapter (`<cache>/Maps/Campaign/<file>`).
///
/// The `Maps/Campaign` shape is not cosmetic: the game layer decides whether a
/// loaded map runs as a campaign mission from its path
/// (`game::map_file_is_campaign`), so a chapter extracted into a bare cache
/// directory was classified as a melee map and its script never booted
/// (`CAMPAIGN_LOAD … script=false`).
pub fn campaign_cache_map_path(file_name: &str) -> PathBuf {
    campaign_cache_dir()
        .join("Maps")
        .join("Campaign")
        .join(file_name)
}

/// Mounts a chapter's engine, without the map: the game's `reset_scenario`
/// enters the map afterwards, on the 32 MB script stack, and builds the bridge.
fn campaign_engine(
    map: &Path,
    archive: Option<&Path>,
) -> Result<wc3_re::engine::Engine, String> {
    let root = archive
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(campaign_install_root)
        .ok_or_else(|| "no install root for the chapter".to_string())?;
    let mut config = wc3_re::engine::EngineConfig::new(&root);
    config.seed = CAMPAIGN_SEED;
    let mut engine = wc3_re::engine::Engine::boot(config).map_err(|error| error.to_string())?;
    engine.config.map = Some(map.to_path_buf());
    Ok(engine)
}

/// Mounted archives that can carry campaign chapters, in mount priority
/// (`War3.mpq` base, then `War3x.mpq` for the TFT data, then the locale
/// archive `War3xlocal.mpq`, which carries the TFT maps and the campaign
/// string tables).
pub fn campaign_archives() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |path: PathBuf| {
        if !roots.iter().any(|known| known == &path) {
            roots.push(path);
        }
    };
    for key in [CAMPAIGN_DIR_ENV, DATA_ROOT_ENV, "H2_WC3_INSTALL"] {
        if let Some(dir) = std::env::var_os(key) {
            push(PathBuf::from(dir));
        }
    }
    push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    push(manifest.join("../../analysis/war3_full3"));
    push(manifest.join("../../analysis/agent-assets/wc3-install"));
    let mut found = Vec::new();
    for root in roots {
        for name in ["War3.mpq", "War3x.mpq", "War3xlocal.mpq"] {
            if let Some(path) = archive_in_dir(&root, name)
                && !found.iter().any(|known| known == &path)
            {
                // A candidate the process cannot read is not an install: it
                // would win the mount and then fail every extraction (and hand
                // the game an install root it cannot use).
                match std::fs::File::open(&path) {
                    Ok(_) => found.push(path),
                    Err(error) => crate::dev_log!(
                        "CAMPAIGN_ARCHIVE_UNREADABLE path={} {error}",
                        path.display()
                    ),
                }
            }
        }
    }
    found
}

/// Case-insensitive archive lookup in one directory.
fn archive_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|file| file.to_str())
                .is_some_and(|file| file.eq_ignore_ascii_case(name))
        {
            return Some(path);
        }
    }
    None
}

/// File name of an archive entry (`Maps\Campaign\Orc01.w3m` → `Orc01.w3m`).
fn entry_file_name(entry: &str) -> &str {
    entry
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("chapter.w3m")
}

/// `Maps\Campaign\*.w3m|w3x` and `Maps\FrozenThrone\Campaign\*.w3x` entries of
/// a mounted archive, as `(entry, chapter id)` pairs, sorted by the retail
/// chapter order.
///
/// RoC chapters live in `War3.mpq`; the TFT chapters are shipped in the locale
/// archive `War3xlocal.mpq` under `Maps\FrozenThrone\Campaign\`.
pub fn archive_campaign_entries(archive: &Path) -> Vec<(String, String)> {
    let Ok(mpq) = wc3_mpq::MpqArchive::open(archive) else {
        crate::dev_log!("CAMPAIGN_ARCHIVE_UNREADABLE path={}", archive.display());
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in mpq.listfile() {
        let lower = entry.to_ascii_lowercase().replace('/', "\\");
        if !lower.starts_with("maps\\campaign\\") && !lower.starts_with("maps\\frozenthrone\\campaign\\")
        {
            continue;
        }
        let is_map = lower.ends_with(".w3m") || lower.ends_with(".w3x");
        if !is_map {
            continue;
        }
        let Some(stem) = entry
            .rsplit(['\\', '/'])
            .next()
            .and_then(|file| file.rsplit_once('.').map(|(stem, _)| stem))
        else {
            continue;
        };
        if !is_chapter_id(stem) {
            continue;
        }
        out.push((entry.clone(), stem.to_string()));
    }
    out.sort_by(|left, right| {
        chapter_rank(&left.1)
            .cmp(&chapter_rank(&right.1))
            .then_with(|| left.1.cmp(&right.1))
    });
    out
}

/// Reads one archive entry and writes it to [`campaign_cache_dir`] (temp file +
/// rename, so a killed extraction never leaves a half map). Returns the path
/// and the byte count.
pub fn extract_archive_entry(archive: &Path, entry: &str) -> Result<(PathBuf, usize), String> {
    let mpq = wc3_mpq::MpqArchive::open(archive)
        .map_err(|error| format!("open {}: {error}", archive.display()))?;
    let bytes = mpq
        .read(entry)
        .map_err(|error| format!("read {entry} from {}: {error}", archive.display()))?;
    let stem = entry
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("chapter.w3m");
    let path = campaign_cache_map_path(stem);
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create {}: {error}", dir.display()))?;
    let temp = path.with_extension("w3m.part");
    std::fs::write(&temp, &bytes).map_err(|error| format!("write {}: {error}", temp.display()))?;
    std::fs::rename(&temp, &path)
        .map_err(|error| format!("rename {}: {error}", path.display()))?;
    Ok((path, bytes.len()))
}

/// `Human02Interlude` → `Human 02 Interlude`; the real title comes from the
/// map's own `war3map.w3i` when available.
pub fn title_from_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 4);
    let mut previous: Option<char> = None;
    for ch in id.chars() {
        if let Some(prev) = previous {
            let boundary = (ch.is_ascii_uppercase() && (prev.is_ascii_lowercase() || prev.is_ascii_digit()))
                || (ch.is_ascii_digit() && prev.is_ascii_alphabetic());
            if boundary {
                out.push(' ');
            }
        }
        out.push(ch);
        previous = Some(ch);
    }
    out
}

/// A parsed mission.
#[derive(Debug, Clone, Default)]
pub struct Mission {
    /// Chapter id (map file stem).
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
    pub map: PathBuf,
    /// Quests from the objectives panel, in script order.
    pub objectives: Vec<Objective>,
    /// Cutscene functions the script contains, in script order.
    pub cinematics: Vec<Cinematic>,
    /// Movie files the script requests (`PlayCinematic`).
    pub movies: Vec<String>,
    /// Camera setup variables the script applies (`gg_cam_Start01`), in first
    /// use order. The `war3map.w3c` camera definitions themselves are parsed
    /// by `wc3-re` (`cutscenes::parse_cameras`); this is the subset the
    /// mission's cutscenes actually drive.
    pub cameras: Vec<String>,
    /// Trigger function names in the order they are created.
    pub triggers: Vec<String>,
    /// Script size, for the report.
    pub script_bytes: usize,
    /// `war3map.wts`, so live script strings (`TRIGSTR_064`) resolve the same
    /// way the static parse resolved them.
    pub strings: Wts,
    /// `war3map.w3i` player slots: the human seat is the one whose controller is
    /// `1` (user), which is what the campaign HUD binds to.
    pub players: Vec<crate::sim_bridge::MapPlayerSlot>,
}

/// A quest (one entry of the objectives panel).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Objective {
    /// Script variable that holds the quest (`udg_QuestVillage`).
    pub variable: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    /// `bj_QUESTTYPE_REQ_*` — required for mission completion.
    pub required: bool,
    /// `bj_QUESTTYPE_*_DISCOVERED` — visible from the start.
    pub discovered: bool,
    pub items: Vec<QuestItem>,
}

/// One line of a quest's checklist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestItem {
    pub text: String,
}

/// One cutscene (a script function that drives camera/transmission beats).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cinematic {
    /// Function name (`Trig_Intro_Cinematic_Actions`).
    pub name: String,
    pub kind: CinematicKind,
    /// The beats, in line order.
    pub events: Vec<CinematicBeat>,
}

impl Cinematic {
    pub fn beats(&self, kind: BeatKind) -> usize {
        self.events.iter().filter(|beat| beat.kind == kind).count()
    }
}

/// What a cutscene function is for, from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CinematicKind {
    Intro,
    Outro,
    Victory,
    Defeat,
    #[default]
    Scene,
}

impl CinematicKind {
    pub fn from_name(name: &str) -> Self {
        let lowered = name.to_ascii_lowercase();
        if lowered.contains("intro") {
            Self::Intro
        } else if lowered.contains("outro") || lowered.contains("end") {
            Self::Outro
        } else if lowered.contains("victory") {
            Self::Victory
        } else if lowered.contains("defeat") || lowered.contains("fail") {
            Self::Defeat
        } else {
            Self::Scene
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intro => "intro",
            Self::Outro => "outro",
            Self::Victory => "victory",
            Self::Defeat => "defeat",
            Self::Scene => "scene",
        }
    }
}

/// One cinematic call in a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CinematicBeat {
    pub kind: BeatKind,
    /// 1-based line in `war3map.j`.
    pub line: usize,
    /// The call as written, with string arguments resolved through the WTS.
    pub text: String,
}

/// The cinematic call families the runtime records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatKind {
    Camera,
    Transmission,
    Scene,
    Movie,
    Filter,
    CinematicMode,
    Timer,
    Order,
}

impl BeatKind {
    /// True for the beats that make a function a cutscene: camera work,
    /// dialogue, scene text, a movie, a fade or the interface toggle. A
    /// function that only issues orders or registers a timer is a gameplay
    /// trigger, not a cutscene, and must not inflate the cutscene list.
    pub fn is_scene_family(self) -> bool {
        matches!(
            self,
            Self::Camera
                | Self::Transmission
                | Self::Scene
                | Self::Movie
                | Self::Filter
                | Self::CinematicMode
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Transmission => "transmission",
            Self::Scene => "scene",
            Self::Movie => "movie",
            Self::Filter => "filter",
            Self::CinematicMode => "cinematic-mode",
            Self::Timer => "timer",
            Self::Order => "order",
        }
    }
}

/// Parse failure with the map path attached.
#[derive(Debug)]
pub struct CampaignError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for CampaignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for CampaignError {}

impl Mission {
    /// Parses a mission from its map file.
    ///
    /// A map without a script is a data error, not an empty mission: a
    /// campaign map always carries `war3map.j` (the 44 shipped maps do), so a
    /// missing script is reported instead of producing a mission with zero
    /// objectives and zero cinematics.
    pub fn parse(path: impl AsRef<Path>) -> Result<Self, CampaignError> {
        let path = path.as_ref().to_path_buf();
        let bundle = MapBundle::open(&path).map_err(|err| CampaignError {
            path: path.clone(),
            message: err.to_string(),
        })?;
        let script = bundle.script.clone().ok_or_else(|| CampaignError {
            path: path.clone(),
            message: "map has no war3map.j".to_string(),
        })?;
        let info = bundle.localized_info().unwrap_or_default();
        let wts = bundle.wts.clone().unwrap_or_default();
        let id = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut mission = Self {
            title: if info.name.trim().is_empty() {
                title_from_id(&id)
            } else {
                info.name.trim().to_string()
            },
            id,
            author: info.author,
            description: info.description,
            recommended_players: info.recommended_players,
            map: path,
            objectives: Vec::new(),
            cinematics: Vec::new(),
            movies: Vec::new(),
            cameras: Vec::new(),
            triggers: Vec::new(),
            script_bytes: script.len(),
            strings: wts.clone(),
            players: crate::sim_bridge::map_player_slots(&bundle),
        };
        mission.apply_script(&script, &wts);
        Ok(mission)
    }

    /// Parses the script structure into the mission model. Pure, so the parse
    /// can be tested without an archive.
    pub fn apply_script(&mut self, script: &str, wts: &Wts) {
        self.objectives = parse_objectives(script, wts);
        self.cinematics = parse_cinematics(script, wts);
        self.movies = self
            .cinematics
            .iter()
            .flat_map(|cinematic| cinematic.events.iter())
            .filter(|beat| beat.kind == BeatKind::Movie)
            .map(|beat| beat.text.clone())
            .collect();
        self.triggers = parse_triggers(script);
        self.cameras = parse_camera_vars(&self.cinematics);
    }

    /// Resolves the map's `TRIGSTR_n` references in a live script string.
    pub fn resolve(&self, text: &str) -> String {
        if text.contains("TRIGSTR_") {
            self.strings.resolve(text)
        } else {
            text.to_string()
        }
    }

    /// The human seat from `war3map.w3i` (`controller == 1`).
    pub fn human_slot(&self) -> Option<u8> {
        self.players
            .iter()
            .find(|player| player.controller == 1)
            .and_then(|player| u8::try_from(player.slot).ok())
    }

    /// A compact JSON view for reports (`analysis/campaign/human01.json`).
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        json!({
            "id": self.id,
            "title": self.title,
            "author": self.author,
            "recommended_players": self.recommended_players,
            "map": self.map.display().to_string(),
            "script_bytes": self.script_bytes,
            "objectives": self.objectives.iter().map(|objective| json!({
                "variable": objective.variable,
                "title": objective.title,
                "description": objective.description,
                "required": objective.required,
                "discovered": objective.discovered,
                "items": objective.items.iter().map(|item| &item.text).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "cinematics": self.cinematics.iter().map(|cinematic| json!({
                "name": cinematic.name,
                "kind": cinematic.kind.as_str(),
                "beats": cinematic.events.len(),
                "camera": cinematic.beats(BeatKind::Camera),
                "transmissions": cinematic.beats(BeatKind::Transmission),
                "scenes": cinematic.beats(BeatKind::Scene),
                "movies": cinematic.beats(BeatKind::Movie),
                "events": cinematic.events.iter().map(|beat| json!({
                    "kind": beat.kind.as_str(),
                    "line": beat.line,
                    "call": beat.text,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "movies": self.movies,
            "cameras": self.cameras,
            "triggers": self.triggers,
        })
    }
}

/// Extracts the argument list of a call starting at `open` (the index of `(`),
/// splitting on top-level commas and keeping quoted strings intact.
fn call_args(script: &str, open: usize) -> (Vec<String>, usize) {
    let bytes = script.as_bytes();
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut index = open;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if in_string {
            current.push(ch);
            if ch == '"' && bytes.get(index.wrapping_sub(1)) != Some(&b'\\') {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                depth += 1;
                if depth > 1 {
                    current.push(ch);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if !current.trim().is_empty() {
                        args.push(current.trim().to_string());
                    }
                    return (args, index + 1);
                }
                current.push(ch);
            }
            ',' if depth == 1 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
        index += 1;
    }
    (args, index)
}

/// The string literal an argument holds, with the WTS refs resolved.
fn string_arg(arg: &str, wts: &Wts) -> Option<String> {
    let trimmed = arg.trim();
    let body = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    Some(wts.resolve(body))
}

/// Finds the first `needle(` occurrence in `script` and returns the call
/// arguments.
fn call_args_of(script: &str, needle: &str) -> Option<Vec<String>> {
    let start = script.find(needle)?;
    let open = start + needle.len();
    Some(call_args(script, open).0)
}

fn parse_objectives(script: &str, wts: &Wts) -> Vec<Objective> {
    let mut objectives: Vec<Objective> = Vec::new();
    let mut last_quest: Option<usize> = None;
    // The script binds each quest to a variable right after creating it:
    //   call CreateQuestBJ( type, "title", "description", "icon" )
    //   set udg_QuestVillage = GetLastCreatedQuestBJ()
    // Quest items are created against the variable:
    //   call CreateQuestItemBJ( udg_QuestVillage, "TRIGSTR_068" )
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("call CreateQuestBJ(") {
            if let Some(args) = call_args_of(trimmed, "CreateQuestBJ") {
                if args.len() >= 4 {
                    let kind = args[0].to_ascii_uppercase();
                    objectives.push(Objective {
                        variable: String::new(),
                        title: string_arg(&args[1], wts).unwrap_or_default(),
                        description: string_arg(&args[2], wts).unwrap_or_default(),
                        icon: string_arg(&args[3], wts).unwrap_or_default(),
                        required: kind.contains("REQ"),
                        discovered: kind.contains("DISCOVERED"),
                        items: Vec::new(),
                    });
                    last_quest = Some(objectives.len() - 1);
                }
            }
            continue;
        }
        if trimmed.starts_with("set ") && trimmed.contains("GetLastCreatedQuestBJ()") {
            if let Some(index) = last_quest {
                if let Some((variable, _)) = trimmed
                    .trim_start_matches("set ")
                    .split_once('=')
                {
                    objectives[index].variable = variable.trim().to_string();
                }
            }
            continue;
        }
        if trimmed.starts_with("call CreateQuestItemBJ(") {
            if let Some(args) = call_args_of(trimmed, "CreateQuestItemBJ") {
                let variable = args.first().map(|arg| arg.trim().to_string());
                let text = args.get(1).and_then(|arg| string_arg(arg, wts));
                if let (Some(variable), Some(text)) = (variable, text) {
                    let index = objectives
                        .iter()
                        .position(|objective| objective.variable.eq_ignore_ascii_case(&variable))
                        .or(last_quest);
                    if let Some(index) = index {
                        objectives[index].items.push(QuestItem { text });
                    }
                }
            }
        }
    }
    objectives
}

/// Camera setup variables the cinematic beats apply (`gg_cam_*`), in first use
/// order.
fn parse_camera_vars(cinematics: &[Cinematic]) -> Vec<String> {
    let mut cameras: Vec<String> = Vec::new();
    for cinematic in cinematics {
        for beat in &cinematic.events {
            if beat.kind != BeatKind::Camera {
                continue;
            }
            for token in beat.text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')) {
                if token.starts_with("gg_cam_") && !cameras.iter().any(|known| known == token) {
                    cameras.push(token.to_string());
                }
            }
        }
    }
    cameras
}

fn parse_triggers(script: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("function InitTrig_") {
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                triggers.push(name.to_string());
            }
        }
    }
    triggers
}

fn parse_cinematics(script: &str, wts: &Wts) -> Vec<Cinematic> {
    let mut cinematics: Vec<Cinematic> = Vec::new();
    let mut current: Option<Cinematic> = None;
    for (index, line) in script.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("function ") {
            if let Some(name) = rest.split_whitespace().next() {
                current = Some(Cinematic {
                    name: name.to_string(),
                    kind: CinematicKind::from_name(name),
                    events: Vec::new(),
                });
            }
        }
        if trimmed.starts_with("endfunction") {
            if let Some(cinematic) = current.take() {
                if cinematic
                    .events
                    .iter()
                    .any(|beat| beat.kind.is_scene_family())
                {
                    cinematics.push(cinematic);
                }
            }
            continue;
        }
        let Some(cinematic) = current.as_mut() else {
            continue;
        };
        if let Some((kind, text)) = classify_beat(trimmed, wts) {
            cinematic.events.push(CinematicBeat {
                kind,
                line: index + 1,
                text,
            });
        }
    }
    cinematics
}

/// Maps a script line to a cinematic beat. `None` for anything else.
fn classify_beat(line: &str, wts: &Wts) -> Option<(BeatKind, String)> {
    const CAMERA: &[&str] = &[
        "CameraSetupApplyForPlayer",
        "CameraSetupApplyForceDuration",
        "CameraSetupApply",
        "SetCameraField",
        "SetCameraFieldForPlayer",
        "PanCameraTo",
        "PanCameraToTimed",
        "SetCameraPosition",
        "SetCameraPositionForPlayer",
        "ResetToGameCamera",
        "SetCameraTargetController",
        "SetCameraQuickPosition",
    ];
    const TRANSMISSION: &[&str] = &[
        "TransmissionFromUnitWithNameBJ",
        "TransmissionFromUnitTypeWithNameBJ",
        "DoTransmissionBasicsBJ",
    ];
    const SCENE: &[&str] = &["SetCinematicScene", "SetCinematicSceneBJ", "EndCinematicScene"];
    const MOVIE: &[&str] = &["PlayCinematic", "PlayMovieBJ", "PlayModelCinematic"];
    const FILTER: &[&str] = &["CinematicFadeBJ", "CinematicFilterGenericBJ", "DisplayCineFilter"];
    const MODE: &[&str] = &["CinematicModeBJ", "CinematicModeExBJ", "ShowInterface", "EnableUserControl"];
    const TIMER: &[&str] = &[
        "TriggerRegisterTimerEvent",
        "TriggerRegisterTimerEventPeriodic",
        "TriggerRegisterTimerEventSingle",
        "StartTimerBJ",
    ];
    const ORDER: &[&str] = &["IssuePointOrder", "IssueTargetOrder", "IssueImmediateOrder"];

    let trimmed = line.trim_start_matches("call ").trim_start();
    let families: &[(&[&str], BeatKind)] = &[
        (CAMERA, BeatKind::Camera),
        (TRANSMISSION, BeatKind::Transmission),
        (SCENE, BeatKind::Scene),
        (MOVIE, BeatKind::Movie),
        (FILTER, BeatKind::Filter),
        (MODE, BeatKind::CinematicMode),
        (TIMER, BeatKind::Timer),
        (ORDER, BeatKind::Order),
    ];
    for (names, kind) in families {
        for name in *names {
            if let Some(rest) = trimmed.strip_prefix(name) {
                if rest.trim_start().starts_with('(') {
                    return Some((*kind, resolve_strings(trimmed, wts)));
                }
            }
        }
    }
    None
}

/// Resolves every `"TRIGSTR_n"` literal in a line to the string table text.
fn resolve_strings(line: &str, wts: &Wts) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        out.push_str(&rest[..start + 1]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            out.push_str(after);
            return out;
        };
        out.push_str(&wts.resolve(&after[..end]));
        out.push('"');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Objective totals per kind, for the report line.
pub fn beat_totals(mission: &Mission) -> BTreeMap<&'static str, usize> {
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    for cinematic in &mission.cinematics {
        for beat in &cinematic.events {
            *totals.entry(beat.kind.as_str()).or_insert(0) += 1;
        }
    }
    totals
}

// ===========================================================================
// Playable campaign: registry, progress save, live quest log, mission session
// ===========================================================================

/// Campaign map directory override (`.../Maps/Campaign` or a flat `.w3m` dir).
pub const CAMPAIGN_DIR_ENV: &str = "WC3RUST_CAMPAIGN";
/// Install root override (`War3.mpq` + `Maps/`); the maps are `Maps/Campaign`.
pub const DATA_ROOT_ENV: &str = "WC3RUST_DATA";
/// Campaign progress file override (tests and portable installs).
pub const SAVE_PATH_ENV: &str = "WC3RUST_CAMPAIGN_SAVE";

/// Progress container magic. The layout is the `W3SV` save layout from
/// `wc3-sim/src/save.rs` (magic, version, flags, payload length, FNV-1a
/// checksum) with the campaign file's own magic, because the payload is campaign
/// progress, not a simulation snapshot. `CampaignSession::checkpoint` writes a
/// real `W3SV` for the mission sim itself.
pub const CAMPAIGN_SAVE_MAGIC: [u8; 4] = *b"W3CP";
pub const CAMPAIGN_SAVE_VERSION: u16 = 1;
pub const CAMPAIGN_SAVE_FILE: &str = "campaign.sav";

/// Deterministic seed for campaign missions: the retail campaign is a scripted
/// single-player run, so the same mission always boots the same layout.
pub const CAMPAIGN_SEED: u64 = 0x5EED_1234;

/// `MAP_CONTROL_USER` in `common.j` (`ConvertMapControl(0)`).
pub const MAP_CONTROL_USER: u32 = 0;

/// Seats the **game layer's** own fixed arrays cover (`Game::score:
/// [SeatScore; 2]` and the direct `sim.resources[self.player]` reads in
/// `game.rs`). The sim itself now carries `wc3_sim::MAX_PLAYERS` (8) seats
/// through [`wc3_sim::Sim::bind_human_seat`]; a mission above this width is
/// bound in the sim and its local view is `Sim::local_player()`, while
/// `Game::player` stays inside these arrays until the game layer switches its
/// score/resource reads to the seat accessors.
pub const SIM_PLAYER_SLOTS: usize = 2;

/// Lane A (ledger 518) integration point: `Sim::bind_human_seat(seat)`.
///
/// The sim owns the per-seat arrays and now carries `wc3_sim::MAX_PLAYERS`
/// seats; the call makes the mission seat the local player, stamps its sight
/// and returns the seat's own gold/lumber bank. A seat outside
/// `0..MAX_PLAYERS` is a hard `CampaignSeat` failure, not a silent
/// `sim_bound=false`.
fn bind_campaign_seat(
    sim: &mut wc3_sim::Sim,
    seat: u8,
) -> Result<wc3_sim::SeatBinding, wc3_sim::SeatError> {
    sim.bind_human_seat(seat)
}

/// FNV-1a 64 over the payload — the checksum `wc3-sim`'s `W3SV` save uses.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Failure while reading or writing the campaign progress file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CampaignSaveError {
    Io(String),
    BadMagic([u8; 4]),
    Version(u16),
    Truncated { needed: usize, available: usize },
    Length { declared: usize, actual: usize },
    Checksum { expected: u64, actual: u64 },
    Corrupt(String),
}

impl std::fmt::Display for CampaignSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "CAMPAIGN_SAVE_IO {message}"),
            Self::BadMagic(found) => write!(f, "CAMPAIGN_SAVE_MAGIC {found:?}"),
            Self::Version(version) => write!(f, "CAMPAIGN_SAVE_VERSION {version}"),
            Self::Truncated { needed, available } => {
                write!(f, "CAMPAIGN_SAVE_TRUNCATED need={needed} have={available}")
            }
            Self::Length { declared, actual } => {
                write!(f, "CAMPAIGN_SAVE_LENGTH declared={declared} actual={actual}")
            }
            Self::Checksum { expected, actual } => {
                write!(f, "CAMPAIGN_SAVE_CHECKSUM stored={expected:#x} computed={actual:#x}")
            }
            Self::Corrupt(message) => write!(f, "CAMPAIGN_SAVE_CORRUPT {message}"),
        }
    }
}

impl std::error::Error for CampaignSaveError {}

impl From<std::io::Error> for CampaignSaveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Persisted campaign progress: which chapters are cleared and which mission the
/// player was on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CampaignProgress {
    /// Chapter ids (`Human01`) whose victory already fired.
    pub cleared: BTreeSet<String>,
    /// Chapter id the player is currently on, when known.
    pub current: Option<String>,
    /// Unix seconds of the last write (informational).
    pub saved_unix: u64,
}

impl CampaignProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cleared(&self, id: &str) -> bool {
        self.cleared.iter().any(|known| known.eq_ignore_ascii_case(id))
    }

    pub fn clear(&mut self, id: &str) {
        if !self.is_cleared(id) {
            self.cleared.insert(id.to_string());
        }
        self.current = Some(id.to_string());
        self.saved_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
    }

    /// `W3CP` container: header + payload + checksum, all little-endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(self.cleared.len() as u32).to_le_bytes());
        for id in &self.cleared {
            write_string(&mut payload, id);
        }
        match &self.current {
            Some(id) => {
                payload.push(1);
                write_string(&mut payload, id);
            }
            None => payload.push(0),
        }
        payload.extend_from_slice(&self.saved_unix.to_le_bytes());
        let mut out = Vec::with_capacity(payload.len() + 20);
        out.extend_from_slice(&CAMPAIGN_SAVE_MAGIC);
        out.extend_from_slice(&CAMPAIGN_SAVE_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&fnv1a(&payload).to_le_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CampaignSaveError> {
        if bytes.len() < 12 {
            return Err(CampaignSaveError::Truncated {
                needed: 12,
                available: bytes.len(),
            });
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);
        if magic != CAMPAIGN_SAVE_MAGIC {
            return Err(CampaignSaveError::BadMagic(magic));
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != CAMPAIGN_SAVE_VERSION {
            return Err(CampaignSaveError::Version(version));
        }
        let declared = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let actual = bytes.len().saturating_sub(12);
        if actual < declared + 8 {
            return Err(CampaignSaveError::Truncated {
                needed: 12 + declared + 8,
                available: bytes.len(),
            });
        }
        if actual != declared + 8 {
            return Err(CampaignSaveError::Length { declared, actual });
        }
        let payload = &bytes[12..12 + declared];
        let mut stored = [0u8; 8];
        stored.copy_from_slice(&bytes[12 + declared..12 + declared + 8]);
        let expected = u64::from_le_bytes(stored);
        let computed = fnv1a(payload);
        if expected != computed {
            return Err(CampaignSaveError::Checksum { expected, actual: computed });
        }
        let mut reader = PayloadReader { bytes: payload, pos: 0 };
        let count = reader.u32()? as usize;
        let mut cleared = BTreeSet::new();
        for _ in 0..count {
            cleared.insert(reader.string()?);
        }
        let current = match reader.u8()? {
            0 => None,
            1 => Some(reader.string()?),
            other => return Err(CampaignSaveError::Corrupt(format!("current flag {other}"))),
        };
        let saved_unix = reader.u64()?;
        if reader.pos != payload.len() {
            return Err(CampaignSaveError::Length {
                declared: reader.pos,
                actual: payload.len(),
            });
        }
        Ok(Self {
            cleared,
            current,
            saved_unix,
        })
    }

    /// Reads `path`; a missing file is a fresh campaign, not an error.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, CampaignSaveError> {
        match std::fs::read(path.as_ref()) {
            Ok(bytes) => Self::from_bytes(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(CampaignSaveError::Io(error.to_string())),
        }
    }

    /// Writes through a temporary sibling and `rename()`s it into place, so a
    /// crash mid-write can never truncate the progress of an already-cleared
    /// campaign.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), CampaignSaveError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = self.to_bytes();
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&temp, &bytes)?;
        match std::fs::rename(&temp, path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&temp);
                Err(CampaignSaveError::Io(error.to_string()))
            }
        }
    }

    /// `$WC3RUST_CAMPAIGN_SAVE`, else `<data dir>/campaign.sav`.
    pub fn default_path() -> PathBuf {
        if let Some(path) = std::env::var_os(SAVE_PATH_ENV) {
            return PathBuf::from(path);
        }
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/data/data/org.wc3rust.p0/files"));
        base.join("wc3rust").join(CAMPAIGN_SAVE_FILE)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len().min(u16::MAX as usize) as u16).to_le_bytes());
    out.extend_from_slice(&bytes[..bytes.len().min(u16::MAX as usize)]);
}

struct PayloadReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl PayloadReader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], CampaignSaveError> {
        let end = self.pos + count;
        if end > self.bytes.len() {
            return Err(CampaignSaveError::Truncated {
                needed: end,
                available: self.bytes.len(),
            });
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, CampaignSaveError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, CampaignSaveError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, CampaignSaveError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Result<u64, CampaignSaveError> {
        let bytes = self.take(8)?;
        let mut value = [0u8; 8];
        value.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(value))
    }

    fn string(&mut self) -> Result<String, CampaignSaveError> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|error| CampaignSaveError::Corrupt(format!("utf8: {error}")))
    }
}

/// Where one chapter stands for the campaign picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterState {
    Cleared,
    Unlocked,
    Locked,
}

impl ChapterState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cleared => "cleared",
            Self::Unlocked => "unlocked",
            Self::Locked => "locked",
        }
    }
}

/// One row of the mission-select screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterRow {
    pub index: usize,
    pub id: String,
    pub title: String,
    pub interlude: bool,
    pub state: ChapterState,
    pub map: PathBuf,
    /// Chapter number inside its campaign (`Human03` -> 3), 0 for interludes.
    pub number: u32,
    /// Index into `ui::menu::CAMPAIGN_GROUPS` (Prologue…Extra).
    pub group: usize,
    /// `file` or `mpq`: where the row's map bytes come from.
    pub source: &'static str,
}

/// The campaign: its chapters in retail order plus the saved progress.
#[derive(Debug, Clone, Default)]
pub struct CampaignRegistry {
    pub campaign: Campaign,
    pub progress: CampaignProgress,
    /// Where `complete()` persists; `None` keeps progress in memory (tests).
    pub save_path: Option<PathBuf>,
    /// Last persistence failure, surfaced as `CAMPAIGN_SAVE_FAILED`.
    pub last_save_error: Option<String>,
}

impl CampaignRegistry {
    /// Chapters discovered from the loose install tree **and** the mounted
    /// archives, in retail order, with the given progress.
    ///
    /// This is the discovery the game itself uses; it is public so a headless
    /// harness can install its own registry (with its own save path) instead of
    /// writing the operator's progress.
    pub fn discover(progress: CampaignProgress) -> Result<Self, String> {
        let dir = campaign_map_dir();
        let archives = campaign_archives();
        if dir.is_none() && archives.is_empty() {
            return Err(format!(
                "campaign maps not found: set {DATA_ROOT_ENV} (install root) or {CAMPAIGN_DIR_ENV}"
            ));
        }
        let campaign = Campaign::from_sources(dir.as_deref(), &archives)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            campaign,
            progress,
            save_path: None,
            last_save_error: None,
        })
    }

    /// Chapters from a directory of campaign maps plus the saved progress.
    pub fn from_map_dir(
        dir: impl AsRef<Path>,
        progress: CampaignProgress,
    ) -> std::io::Result<Self> {
        let campaign = Campaign::from_map_dir(dir)?;
        Ok(Self {
            campaign,
            progress,
            save_path: Some(CampaignProgress::default_path()),
            last_save_error: None,
        })
    }

    /// Loads the campaign from the install and the progress from disk.
    ///
    /// Chapters come from the loose `Maps/Campaign` files **and** from the
    /// mounted `War3.mpq` / `War3x.mpq` (the device ships only two loose
    /// chapters and keeps the other 42 in the archive).
    pub fn open_default() -> Result<Self, String> {
        let save_path = CampaignProgress::default_path();
        let progress = CampaignProgress::load(&save_path).unwrap_or_else(|error| {
            crate::dev_log!("CAMPAIGN_PROGRESS_RESET {error}");
            CampaignProgress::default()
        });
        let mut registry = Self::discover(progress)?;
        registry.save_path = Some(save_path);
        Ok(registry)
    }

    pub fn len(&self) -> usize {
        self.campaign.chapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.campaign.chapters.is_empty()
    }

    pub fn chapter(&self, index: usize) -> Option<&Chapter> {
        self.campaign.chapters.get(index)
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.campaign
            .chapters
            .iter()
            .position(|chapter| chapter.id.eq_ignore_ascii_case(id))
    }

    pub fn is_cleared(&self, index: usize) -> bool {
        self.chapter(index)
            .is_some_and(|chapter| self.progress.is_cleared(&chapter.id))
    }

    /// A chapter is playable when it is the first mission of its campaign
    /// (Prologue / Human / Undead / Orc / Night Elf are separate campaigns in
    /// the campaign screen) or every earlier mission of that campaign is
    /// cleared. Interludes follow the mission that opens them, so they do not
    /// gate the next mission.
    pub fn is_unlocked(&self, index: usize) -> bool {
        let Some(chapter) = self.chapter(index) else {
            return false;
        };
        if self.is_cleared(index) {
            return true;
        }
        let race = campaign_order(&chapter.id);
        let Some(first) = (0..self.len()).find(|candidate| {
            self.chapter(*candidate).is_some_and(|known| campaign_order(&known.id) == race)
        }) else {
            return false;
        };
        if index == first {
            return true;
        }
        (first..index).all(|earlier| {
            self.chapter(earlier)
                .is_some_and(|previous| previous.interlude || self.progress.is_cleared(&previous.id))
        })
    }

    /// The next chapter after `index` that is unlocked and not cleared.
    pub fn next_after(&self, index: usize) -> Option<usize> {
        ((index + 1)..self.len()).find(|candidate| {
            self.is_unlocked(*candidate) && !self.is_cleared(*candidate)
        })
    }

    /// The chapter the campaign picker should open on: the first unlocked,
    /// uncleared mission.
    pub fn first_unlocked(&self) -> Option<usize> {
        (0..self.len()).find(|index| self.is_unlocked(*index) && !self.is_cleared(*index))
    }

    pub fn rows(&self) -> Vec<ChapterRow> {
        self.campaign
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| ChapterRow {
                index,
                id: chapter.id.clone(),
                title: chapter.title.clone(),
                interlude: chapter.interlude,
                state: if self.is_cleared(index) {
                    ChapterState::Cleared
                } else if self.is_unlocked(index) {
                    ChapterState::Unlocked
                } else {
                    ChapterState::Locked
                },
                map: chapter.map.clone(),
                number: if chapter.interlude {
                    0
                } else {
                    chapter_number(&chapter.id)
                },
                group: chapter_group(&chapter.id),
                source: chapter.source.as_str(),
            })
            .collect()
    }

    /// Local path of chapter `index`, extracting an archive entry on demand.
    pub fn map_path(&self, index: usize) -> Result<PathBuf, String> {
        self.chapter(index)
            .ok_or_else(|| format!("campaign has no chapter {index}"))?
            .resolved_map()
    }

    /// Archive entry of chapter `index` (`Maps\Campaign\Human01.w3m`).
    pub fn archive_entry(&self, index: usize) -> Option<String> {
        match &self.chapter(index)?.source {
            MapSource::Mount { entry, .. } => Some(entry.clone()),
            MapSource::File => None,
        }
    }

    /// Marks `index` cleared, unlocks the next mission and persists. Returns the
    /// next chapter index when there is one.
    pub fn complete(&mut self, index: usize) -> Option<usize> {
        // The chapter id is cloned out: `self.progress` is borrowed mutably
        // below while `self.chapter` borrows the whole `self`.
        let chapter_id = self.chapter(index).map(|chapter| chapter.id.clone())?;
        self.progress.clear(&chapter_id);
        let next = self.next_after(index);
        if let Some(path) = self.save_path.clone() {
            if let Err(error) = self.progress.save(&path) {
                self.last_save_error = Some(error.to_string());
                crate::dev_log!("CAMPAIGN_SAVE_FAILED path={} {error}", path.display());
            } else {
                self.last_save_error = None;
            }
        }
        next
    }

    /// The chapter id the progress file names as current.
    pub fn current(&self) -> Option<&Chapter> {
        self.progress
            .current
            .as_deref()
            .and_then(|id| self.campaign.chapter(id))
    }
}

/// Install roots that can carry `Maps/Campaign`, in priority order.
pub fn campaign_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |path: PathBuf| {
        if !roots.iter().any(|known| known == &path) {
            roots.push(path);
        }
    };
    if let Some(dir) = std::env::var_os(CAMPAIGN_DIR_ENV) {
        push(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os(DATA_ROOT_ENV) {
        push(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("H2_WC3_INSTALL") {
        push(PathBuf::from(dir));
    }
    push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    push(manifest.join("../../analysis/war3_full3"));
    push(manifest.join("../../analysis/agent-assets/wc3-install"));
    roots
}

fn holds_maps(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.path().extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("w3m") || extension.eq_ignore_ascii_case("w3x")
            })
        })
    })
}

/// A directory that holds *campaign* chapters (retail naming), not melee maps.
fn holds_chapters(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let path = entry.path();
            let is_map = path.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("w3m") || extension.eq_ignore_ascii_case("w3x")
            });
            is_map
                && path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .is_some_and(|stem| is_chapter_id(&stem))
        })
    })
}

/// The directory that holds the campaign `.w3m` files.
///
/// Only a directory with chapter-shaped map names qualifies: `files/Maps` on
/// the device holds the 38 melee maps and must never become the campaign list.
/// The mounted archive is the second source (`Campaign::from_sources`).
pub fn campaign_map_dir() -> Option<PathBuf> {
    for root in campaign_roots() {
        for sub in ["Maps/Campaign", "maps/campaign", "Maps", "Campaign", "campaign"] {
            let dir = root.join(sub);
            if holds_chapters(&dir) {
                return Some(dir);
            }
        }
        if holds_chapters(&root) {
            return Some(root);
        }
    }
    None
}

/// The install root above `Maps/Campaign`, when the maps come from one.
pub fn campaign_install_root() -> Option<PathBuf> {
    let dir = campaign_map_dir()?;
    if dir.file_name().is_some_and(|name| name.eq_ignore_ascii_case("Campaign")) {
        return dir.parent().and_then(Path::parent).map(Path::to_path_buf);
    }
    dir.parent().map(Path::to_path_buf)
}

/// One-shot flag for `CAMPAIGN_MELEE_RULE_IGNORED` (the melee end rule has no
/// campaign gate yet; the campaign layer restores the mission once per process).
static CAMPAIGN_RULE_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

static REGISTRY: OnceLock<Mutex<Option<CampaignRegistry>>> = OnceLock::new();
static ACTIVE: OnceLock<Mutex<Option<CampaignSession>>> = OnceLock::new();

fn registry_cell() -> &'static Mutex<Option<CampaignRegistry>> {
    REGISTRY.get_or_init(|| Mutex::new(None))
}

fn active_cell() -> &'static Mutex<Option<CampaignSession>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

/// The campaign (chapters + progress), loaded once per process and kept in step
/// with `complete()`.
pub fn campaign_registry() -> Option<CampaignRegistry> {
    if let Ok(guard) = registry_cell().lock() {
        if let Some(registry) = guard.as_ref() {
            return Some(registry.clone());
        }
    }
    let registry = CampaignRegistry::open_default().ok()?;
    if let Ok(mut guard) = registry_cell().lock() {
        *guard = Some(registry.clone());
    }
    Some(registry)
}

/// Drops the cached chapter list (progress changed on disk outside this process).
pub fn reload_campaign_registry() -> Option<CampaignRegistry> {
    let registry = CampaignRegistry::open_default().ok()?;
    if let Ok(mut guard) = registry_cell().lock() {
        *guard = Some(registry.clone());
    }
    Some(registry)
}

/// The campaign mission in progress, if any.
pub fn active_campaign() -> Option<CampaignSession> {
    active_cell().lock().ok().and_then(|guard| guard.clone())
}

/// Ends the campaign session (return to the menu).
pub fn clear_active_campaign() {
    if let Ok(mut guard) = active_cell().lock() {
        *guard = None;
    }
}

fn store_active(session: CampaignSession) {
    if let Ok(mut guard) = active_cell().lock() {
        *guard = Some(session);
    }
}

/// One checklist line of a quest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestItemRow {
    pub text: String,
    pub completed: bool,
}

/// One quest of the objectives panel, live from the running map script.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestRow {
    /// Script handle; 0 while the row is only known from the static parse (the
    /// script has not created the quest yet, e.g. before `main` runs).
    pub handle: u32,
    pub variable: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub required: bool,
    pub completed: bool,
    pub discovered: bool,
    pub failed: bool,
    pub items: Vec<QuestItemRow>,
}

impl QuestRow {
    pub fn items_done(&self) -> usize {
        self.items.iter().filter(|item| item.completed).count()
    }

    /// `Title (1/3)` — the HUD formats this, the campaign does not invent UI.
    pub fn summary(&self) -> String {
        format!("{} ({}/{})", self.title, self.items_done(), self.items.len())
    }
}

/// What one `QuestLog::sync` changed, for the `QUEST add=/completed=` log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestDelta {
    pub added: Vec<String>,
    pub completed: Vec<String>,
    pub total: usize,
    pub completed_total: usize,
    pub required_total: usize,
    pub required_done: usize,
}

impl QuestDelta {
    pub fn changed(&self) -> bool {
        !self.added.is_empty() || !self.completed.is_empty()
    }
}

/// The objectives panel: quests and their checklist items.
#[derive(Debug, Clone, Default)]
pub struct QuestLog {
    rows: Vec<QuestRow>,
}

impl QuestLog {
    /// The panel before the script runs: the mission's static objectives.
    pub fn from_mission(mission: &Mission) -> Self {
        let rows = mission
            .objectives
            .iter()
            .map(|objective| QuestRow {
                handle: 0,
                variable: objective.variable.clone(),
                title: objective.title.clone(),
                description: objective.description.clone(),
                icon: objective.icon.clone(),
                required: objective.required,
                completed: false,
                discovered: objective.discovered,
                failed: false,
                items: objective
                    .items
                    .iter()
                    .map(|item| QuestItemRow {
                        text: item.text.clone(),
                        completed: false,
                    })
                    .collect(),
            })
            .collect();
        Self { rows }
    }

    pub fn rows(&self) -> &[QuestRow] {
        &self.rows
    }

    pub fn into_rows(self) -> Vec<QuestRow> {
        self.rows
    }

    pub fn required_total(&self) -> usize {
        self.rows.iter().filter(|row| row.required).count()
    }

    pub fn required_done(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.required && row.completed)
            .count()
    }

    pub fn completed_total(&self) -> usize {
        self.rows.iter().filter(|row| row.completed).count()
    }

    /// Every required objective is complete (and there is at least one).
    pub fn all_required_done(&self) -> bool {
        let total = self.required_total();
        total > 0 && self.required_done() == total
    }

    /// Merges the live VM quest table into the panel.
    ///
    /// A live quest binds to the static objective it was created from — by
    /// title first (the WTS-resolved text is identical), then by script order —
    /// so the panel keeps the mission's variable name and icon while the state
    /// (title/description/required/completed/failed/items) comes from the VM.
    pub fn sync(&mut self, quests: &BTreeMap<u32, Quest>, mission: &Mission) -> QuestDelta {
        let mut delta = QuestDelta::default();
        let mut claimed = vec![false; self.rows.len()];
        let mut ordered: Vec<(u32, &Quest)> = quests.iter().map(|(handle, quest)| (*handle, quest)).collect();
        ordered.sort_by_key(|(handle, _)| *handle);
        let title_of = |quest: &Quest| mission.resolve(&quest.title);
        let description_of = |quest: &Quest| mission.resolve(&quest.description);
        for (handle, quest) in ordered {
            let live_title = title_of(quest);
            let existing = self.rows.iter().position(|row| row.handle == handle);
            let index = match existing {
                Some(index) => index,
                None => {
                    let by_title = self.rows.iter().enumerate().position(|(position, row)| {
                        row.handle == 0
                            && !claimed[position]
                            && !row.title.is_empty()
                            && !live_title.is_empty()
                            && row.title == live_title
                    });
                    let by_slot = self
                        .rows
                        .iter()
                        .enumerate()
                        .position(|(position, row)| row.handle == 0 && !claimed[position]);
                    let index = match by_title.or(by_slot) {
                        Some(index) => {
                            self.rows[index].handle = handle;
                            index
                        }
                        None => {
                            self.rows.push(QuestRow {
                                handle,
                                ..QuestRow::default()
                            });
                            self.rows.len() - 1
                        }
                    };
                    let title = if live_title.is_empty() {
                        mission
                            .objectives
                            .get(index)
                            .map(|objective| objective.title.clone())
                            .unwrap_or_default()
                    } else {
                        live_title.clone()
                    };
                    delta.added.push(title);
                    index
                }
            };
            claimed[index] = true;
            let live_description = description_of(quest);
            let row = &mut self.rows[index];
            let was_completed = row.completed;
            if !live_title.is_empty() {
                row.title = live_title.clone();
            }
            if !live_description.is_empty() {
                row.description = live_description.clone();
            }
            if !quest.icon.is_empty() {
                row.icon = quest.icon.clone();
            }
            row.required = quest.required;
            row.discovered = quest.discovered;
            row.failed = quest.failed;
            row.completed = quest.completed;
            if !quest.items.is_empty() {
                let static_items: Vec<String> =
                    row.items.iter().map(|item| item.text.clone()).collect();
                row.items = quest
                    .items
                    .iter()
                    .enumerate()
                    .map(|(position, (text, completed))| QuestItemRow {
                        text: if text.trim().is_empty() {
                            static_items.get(position).cloned().unwrap_or_default()
                        } else {
                            mission.resolve(text)
                        },
                        completed: *completed,
                    })
                    .collect();
            }
            if row.completed && !was_completed {
                delta.completed.push(row.title.clone());
            }
        }
        delta.total = self.rows.len();
        delta.completed_total = self.completed_total();
        delta.required_total = self.required_total();
        delta.required_done = self.required_done();
        delta
    }

    pub fn row(&self, title: &str) -> Option<&QuestRow> {
        self.rows.iter().find(|row| row.title == title)
    }
}

/// Camera/fade/scene beats observed since the mission started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CutsceneLedger {
    pub events: usize,
    pub fades: usize,
    pub scenes: usize,
    pub cameras: usize,
    /// The last beat still holds the camera away from the gameplay view.
    pub active: bool,
    observed: usize,
}

impl CutsceneLedger {
    /// Consumes the camera events appended since the previous call; returns how
    /// many were new.
    pub fn observe(&mut self, events: &[CameraEvent]) -> usize {
        let mut fresh = 0;
        for event in events.iter().skip(self.observed) {
            fresh += 1;
            match event.kind.as_str() {
                "fade" => self.fades += 1,
                "scene" => self.scenes += 1,
                "reset" | "stop" => {}
                _ => self.cameras += 1,
            }
            self.active = !matches!(event.kind.as_str(), "reset" | "stop");
        }
        self.observed = events.len();
        self.events += fresh;
        fresh
    }
}

/// One campaign mission running in the game.
#[derive(Debug, Clone)]
pub struct CampaignSession {
    pub registry: CampaignRegistry,
    pub index: usize,
    pub mission: Mission,
    pub quests: QuestLog,
    pub cutscene: CutsceneLedger,
    /// Names the script asked for via `PlayCinematic`, in call order.
    pub movies_requested: Vec<String>,
    /// Names resolved to a file on disk.
    pub movies_played: Vec<String>,
    /// Names the install does not carry (`MOVIE_MISSING <name>`).
    pub missing_movies: Vec<String>,
    /// A resolved movie the presentation layer has not started yet.
    pub pending_movie: Option<String>,
    pub victory: bool,
    pub victory_reason: Option<String>,
    pub defeat: bool,
    pub completed: bool,
    pub next: Option<usize>,
    /// The map's `war3map.j` ran in the engine.
    pub script_booted: bool,
    /// The mission's human seat (`war3map.w3i` controller 1).
    pub human_slot: Option<u8>,
    /// Camera shot stream the renderer consumes.
    pub director: CameraDirector,
    /// Cutscene subtitle band.
    pub subtitles: SubtitleQueue,
    /// Campaign frames advanced through `Game::campaign_frame`.
    pub frames: u64,
    /// Mission clock (`world.game_time`) of the last observation.
    pub time: f64,
    /// End of the laid-out cutscene timeline (see `observe_cinematic_stream`).
    pub cine_clock: f64,
    /// Mission time of the last beat consumed from `world.cinematic_log`.
    pub last_beat_time: f64,
    /// Contract lines: CAMPAIGN_MISSION / QUEST / CAMPAIGN_DONE / MOVIE_MISSING.
    pub log: Vec<String>,
    observed_movies: usize,
    observed_beats: usize,
    /// High-water marks into the VM's typed tables: the beat log repeats an
    /// entry when the engine re-runs a script phase into the same world, and an
    /// append-only index makes the stream idempotent.
    last_camera_index: Option<usize>,
    last_subtitle_index: Option<usize>,
}

impl CampaignSession {
    /// Opens `index` from a loaded registry (no `Game`, no window).
    pub fn open_with(registry: CampaignRegistry, index: usize) -> Result<Self, String> {
        let chapter = registry
            .chapter(index)
            .cloned()
            .ok_or_else(|| format!("campaign has no chapter {index}"))?;
        let mission = Mission::parse(&chapter.map).map_err(|error| error.to_string())?;
        let quests = QuestLog::from_mission(&mission);
        let mut session = Self {
            registry,
            index,
            mission,
            quests,
            cutscene: CutsceneLedger::default(),
            movies_requested: Vec::new(),
            movies_played: Vec::new(),
            missing_movies: Vec::new(),
            pending_movie: None,
            victory: false,
            victory_reason: None,
            defeat: false,
            completed: false,
            next: None,
            script_booted: false,
            human_slot: None,
            director: CameraDirector::default(),
            subtitles: SubtitleQueue::default(),
            frames: 0,
            time: 0.0,
            cine_clock: 0.0,
            last_beat_time: -1.0,
            log: Vec::new(),
            observed_movies: 0,
            observed_beats: 0,
            last_camera_index: None,
            last_subtitle_index: None,
        };
        session.note(format!(
            "CAMPAIGN_MISSION idx={} name={} title={} map={}",
            index,
            chapter.id,
            session.mission.title,
            chapter.map.display()
        ));
        Ok(session)
    }

    /// Opens the first playable chapter of the install.
    pub fn open_first() -> Result<Self, String> {
        let registry = campaign_registry().ok_or_else(|| "campaign maps not found".to_string())?;
        let index = registry
            .first_unlocked()
            .ok_or_else(|| "campaign has no playable chapter".to_string())?;
        Self::open_with(registry, index)
    }

    /// Records a contract line: kept for the HUD/evidence and sent to the
    /// platform log.
    pub fn note(&mut self, line: String) {
        crate::dev_log!("{line}");
        self.log.push(line);
    }

    pub fn chapter(&self) -> Option<&Chapter> {
        self.registry.chapter(self.index)
    }

    pub fn chapter_title(&self) -> String {
        if self.mission.title.trim().is_empty() {
            self.chapter().map(|chapter| chapter.title.clone()).unwrap_or_default()
        } else {
            self.mission.title.clone()
        }
    }

    /// Reads the running mission's VM world: quests, camera beats, movie
    /// requests, victory/defeat triggers.
    pub fn observe(&mut self, world: &World) -> QuestDelta {
        let delta = self.quests.sync(&world.quests, &self.mission);
        for title in &delta.added {
            self.note(format!("QUEST add={title}"));
        }
        for title in &delta.completed {
            self.note(format!("QUEST completed={title}"));
        }
        self.time = world.game_time;
        self.cutscene.observe(&world.camera_events);
        // The ordered cinematic stream: camera moves, fades and lines in script
        // order, stamped with the mission time the VM recorded when the script
        // made the call. Beats that share one tick fall back to the durations
        // the script declares, so a cutscene that never sleeps still lays out
        // (`CAMPAIGN_CINE_CLOCK=tick-durations`).
        let fresh_subtitles = self.observe_cinematic_stream(world);
        if fresh_subtitles > 0 {
            let lines: Vec<(f64, String, f64, String)> = self
                .subtitles
                .lines()
                .iter()
                .rev()
                .take(fresh_subtitles)
                .map(|line| (line.t, line.speaker.clone(), line.shown_for, line.text.clone()))
                .collect();
            for (t, speaker, shown_for, text) in lines.into_iter().rev() {
                self.note(format!(
                    "CUTSCENE subtitle t={t:.2} speaker=\"{speaker}\" dur={shown_for:.2} text=\"{text}\""
                ));
            }
        }
        while self.observed_movies < world.movie_requests.len() {
            let name = world.movie_requests[self.observed_movies].clone();
            self.observed_movies += 1;
            self.movies_requested.push(name.clone());
            if crate::cinematic::movies().find(&name).is_some() {
                self.movies_played.push(name.clone());
                self.pending_movie = Some(name);
            } else {
                self.missing_movies.push(name.clone());
                self.note(format!("MOVIE_MISSING {name}"));
            }
        }
        if !self.defeat {
            if let Some(action) = trigger_matching(world, "defeat") {
                self.defeat = true;
                self.note(format!("CAMPAIGN_DEFEAT idx={} trigger={action}", self.index));
            }
        }
        if !self.completed && !self.defeat {
            let reason = match trigger_matching(world, "victory") {
                Some(action) => Some(format!("trigger:{action}")),
                None if self.quests.all_required_done() => Some("objectives".to_string()),
                None => None,
            };
            if let Some(reason) = reason {
                self.victory = true;
                self.completed = true;
                self.victory_reason = Some(reason.clone());
                let next = self.registry.complete(self.index);
                self.next = next;
                let next_label = next
                    .map(|index| {
                        self.registry
                            .chapter(index)
                            .map(|chapter| chapter.id.clone())
                            .unwrap_or_else(|| index.to_string())
                    })
                    .unwrap_or_else(|| "none".to_string());
                self.note(format!(
                    "CAMPAIGN_DONE idx={} name={} next={} reason={}",
                    self.index, self.mission.id, next_label, reason
                ));
                if let Some(error) = self.registry.last_save_error.clone() {
                    self.note(format!("CAMPAIGN_SAVE_FAILED {error}"));
                }
                sync_global_registry(&self.registry);
            }
        }
        delta
    }

    /// Walks the VM's ordered cinematic log, stamping each beat with the
    /// cutscene clock and handing it to the director / subtitle band. Returns the
    /// number of subtitle lines added.
    fn observe_cinematic_stream(&mut self, world: &World) -> usize {
        let mut new_subtitles = 0;
        while self.observed_beats < world.cinematic_log.len() {
            let beat = world.cinematic_log[self.observed_beats];
            self.observed_beats += 1;
            // Real mission time is the truth: the script's own
            // `TriggerSleepAction`s space the beats across ticks, and the
            // timestamps are what the player sees. The declared-duration clock
            // only separates beats that share one tick (a script that never
            // sleeps, which used to be the only case this VM could produce).
            let same_tick = beat.t <= self.last_beat_time + 1e-9;
            let start = if same_tick {
                beat.t.max(self.cine_clock)
            } else {
                beat.t
            };
            self.last_beat_time = beat.t;
            match beat.kind {
                wc3_re::jass::vm::CineBeatKind::Camera(index) => {
                    if self.last_camera_index.is_some_and(|last| index <= last) {
                        continue;
                    }
                    let Some(event) = world.camera_events.get(index) else {
                        continue;
                    };
                    self.last_camera_index = Some(index);
                    self.director.push(world, event, start);
                    self.cine_clock = start + CameraDirector::hold_seconds(event);
                }
                wc3_re::jass::vm::CineBeatKind::Subtitle(index) => {
                    if self.last_subtitle_index.is_some_and(|last| index <= last) {
                        continue;
                    }
                    let Some(event) = world.subtitles.get(index) else {
                        continue;
                    };
                    self.last_subtitle_index = Some(index);
                    self.subtitles.push(world, event, &self.mission, start);
                    new_subtitles += 1;
                    let shown = self
                        .subtitles
                        .lines()
                        .last()
                        .map(|line| line.shown_for)
                        .unwrap_or(0.0);
                    self.cine_clock = start + shown;
                }
            }
        }
        new_subtitles
    }

    pub fn quest_rows(&self) -> Vec<QuestRow> {
        self.quests.rows().to_vec()
    }

    /// The one-line cutscene report the acceptance gate reads:
    /// `CAMPAIGN_CINE frames=… cameras=N fades=N transmissions=N subtitles=[…]`.
    pub fn cine_summary(&self) -> String {
        let subtitles: Vec<String> = self
            .subtitles
            .lines()
            .iter()
            .map(|line| {
                format!(
                    "{:.2}s {}: {} ({:.2}s)",
                    line.t,
                    if line.speaker.is_empty() { "scene" } else { &line.speaker },
                    line.text,
                    line.shown_for
                )
            })
            .collect();
        format!(
            "CAMPAIGN_CINE frames={} cameras={} fades={} transmissions={} subtitles=[{}]",
            self.frames,
            self.director.keyframes.len(),
            self.director.fades.len(),
            self.subtitles.transmissions(),
            subtitles.join(" | ")
        )
    }

    /// The full report the storyboard generator writes.
    pub fn report(&self) -> MissionSessionReport {
        MissionSessionReport {
            mission: self.mission.id.clone(),
            title: self.chapter_title(),
            frames: self.frames,
            time: self.time,
            cine_summary: self.cine_summary(),
            quests: self.quest_rows(),
            keyframes: self.director.keyframes.clone(),
            fade_events: self.director.fades.clone(),
            subtitles: self.subtitles.lines().to_vec(),
            human_slot: self.human_slot,
            script_booted: self.script_booted,
        }
    }

    /// Emits [`Self::cine_summary`] into the contract log.
    pub fn note_cine_summary(&mut self) -> String {
        let summary = self.cine_summary();
        self.note(summary.clone());
        summary
    }

    /// Mid-mission snapshot: the canonical `W3SV` save of the mission sim
    /// (`wc3-sim/src/save.rs`), resumable with [`wc3_sim::Sim::from_save_path`].
    pub fn checkpoint(&self, sim: &wc3_sim::Sim, path: impl AsRef<Path>) -> Result<(), String> {
        sim.save_to_path(path).map_err(|error| error.to_string())
    }

    pub fn view(&self) -> CampaignView {
        CampaignView {
            index: self.index,
            id: self.mission.id.clone(),
            title: self.chapter_title(),
            interlude: self.chapter().is_some_and(|chapter| chapter.interlude),
            script_booted: self.script_booted,
            human_slot: self.human_slot,
            quests: self.quest_rows(),
            chapters: self.registry.rows(),
            cutscene: self.cutscene.clone(),
            time: self.time,
            camera: self.director.sample(self.cine_clock),
            keyframes: self.director.keyframes.len(),
            fades: self.director.fades.len(),
            transmissions: self.subtitles.transmissions(),
            subtitle: self.subtitles.active_at(self.cine_clock).cloned(),
            victory: self.victory,
            victory_reason: self.victory_reason.clone(),
            defeat: self.defeat,
            completed: self.completed,
            next: self.next,
            movies_requested: self.movies_requested.clone(),
            movies_played: self.movies_played.clone(),
            missing_movies: self.missing_movies.clone(),
            log: self.log.clone(),
        }
    }
}

/// Installs `registry` as the process-wide campaign registry.
///
/// The running game reads the registry through [`campaign_registry`]; a headless
/// harness (or a test) that must not write the operator's progress installs its
/// own copy with a temporary `save_path` first.
pub fn set_campaign_registry(registry: CampaignRegistry) {
    sync_global_registry(&registry);
}

fn sync_global_registry(registry: &CampaignRegistry) {
    if let Ok(mut guard) = registry_cell().lock() {
        *guard = Some(registry.clone());
    }
}

/// The first executed trigger whose action function names `needle`
/// (`victory` / `defeat`), ignoring the cancel/cheat variants.
fn trigger_matching(world: &World, needle: &str) -> Option<String> {
    world
        .triggers
        .values()
        .filter(|trigger| trigger.exec_count > 0)
        .flat_map(|trigger| trigger.actions.iter())
        .find(|action| {
            let lowered = action.to_ascii_lowercase();
            lowered.contains(needle) && !lowered.contains("cancel") && !lowered.contains("cheat")
        })
        .cloned()
}

/// `camerafield` ids from `common.j` (`ConvertCameraField(n)`).
const CAM_FIELD_TARGET_DISTANCE: i64 = 0;
const CAM_FIELD_ANGLE_OF_ATTACK: i64 = 2;
const CAM_FIELD_ROTATION: i64 = 5;
const CAM_FIELD_ZOFFSET: i64 = 6;

/// Sampling rate the camera path is measured at (the sim tick).
pub const CAMERA_SAMPLE_HZ: f64 = 30.0;
/// A shot never snaps the view faster than this between two frames.
pub const CAMERA_MAX_STEP_UNITS: f64 = 400.0;
/// Shortest blend for a cut, so a hard `CameraSetupApply(…, 0.00)` still reads as
/// a film cut on a 30 Hz renderer instead of a teleport.
pub const CAMERA_MIN_BLEND_S: f64 = 0.25;
/// Longest automatic blend the director will insert between two shots.
pub const CAMERA_MAX_BLEND_S: f64 = 2.5;

/// Where the cutscene camera looks, in world units/degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CameraPose {
    pub target: [f64; 2],
    pub z: f64,
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
}

impl CameraPose {
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Self {
            target: [
                mix(self.target[0], other.target[0]),
                mix(self.target[1], other.target[1]),
            ],
            z: mix(self.z, other.z),
            distance: mix(self.distance, other.distance),
            yaw: mix(self.yaw, other.yaw),
            pitch: mix(self.pitch, other.pitch),
        }
    }

    /// Ground-plane distance between two camera targets.
    pub fn target_distance(&self, other: &Self) -> f64 {
        let dx = other.target[0] - self.target[0];
        let dy = other.target[1] - self.target[1];
        (dx * dx + dy * dy).sqrt()
    }
}

/// One camera move the renderer plays, in mission time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraKeyframe {
    /// Mission time (seconds) the shot starts.
    pub t: f64,
    pub pose: CameraPose,
    /// Seconds the camera takes to reach `pose`.
    pub blend: f64,
    /// Where the camera already is when the shot starts. Pinned at push time
    /// (not read from the previous keyframe) so two shots whose blends overlap
    /// still join without a jump.
    pub from: Option<CameraPose>,
    /// The setup handle the move came from, when it was a `CameraSetupApply*`.
    pub setup: Option<u32>,
}

/// A `CinematicFadeBJ` in the render path: the screen fades from
/// `start_alpha` to `end_alpha` over `duration`, through `color`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FadeEvent {
    pub t: f64,
    pub duration: f64,
    pub color: [f64; 3],
    pub start_alpha: f64,
    pub end_alpha: f64,
}

impl FadeEvent {
    /// Screen tint at `t`: `(alpha, colour)`.
    pub fn at(&self, t: f64) -> (f64, [f64; 3]) {
        if self.duration <= 0.0 {
            return (self.end_alpha, self.color);
        }
        let u = ((t - self.t) / self.duration).clamp(0.0, 1.0);
        (
            self.start_alpha + (self.end_alpha - self.start_alpha) * u,
            self.color,
        )
    }
}

/// The camera stream: every `CameraSetupApply*`, `SetCameraField`,
/// `PanCameraTo*` and `CinematicFadeBJ` the mission script executed, stamped with
/// mission time and turned into a keyframe track the renderer samples.
#[derive(Debug, Clone, Default)]
pub struct CameraDirector {
    pub keyframes: Vec<CameraKeyframe>,
    pub fades: Vec<FadeEvent>,
    pub scenes: usize,
    pub noise: usize,
    pub smoothings: usize,
    observed_events: usize,
    last_pose: CameraPose,
}

impl CameraDirector {
    /// Reads the events the script appended since the previous call, stamping
    /// each with the mission clock (`world.game_time`).
    pub fn observe(&mut self, world: &World) -> usize {
        let mut fresh = 0;
        for event in world.camera_events.iter().skip(self.observed_events) {
            fresh += 1;
            self.push(world, event, event.t);
        }
        self.observed_events = world.camera_events.len();
        fresh
    }

    /// Consumes the camera events appended since the previous call in typed
    /// order (see [`CameraDirector`]'s `push`).
    pub fn push(&mut self, world: &World, event: &CameraEvent, t: f64) {
        {
            match event.kind.as_str() {
                "setup" | "setup_z" => {
                    let mut pose = setup_pose(world, event.setup, &self.last_pose);
                    if event.kind == "setup_z" {
                        pose.z += event.value;
                    }
                    self.push_pose(t, pose, event.duration, event.setup);
                }
                "position" | "pan" => {
                    let mut pose = self.last_pose;
                    pose.target = [event.x, event.y];
                    self.push_pose(t, pose, event.duration, None);
                }
                "pan_z" => {
                    let mut pose = self.last_pose;
                    pose.target = [event.x, event.y];
                    pose.z = event.value;
                    self.push_pose(t, pose, event.duration, None);
                }
                "field" => {
                    if let Some(field) = event.field {
                        let mut pose = self
                            .keyframes
                            .last()
                            .map(|key| key.pose)
                            .unwrap_or(self.last_pose);
                        apply_pose_field(&mut pose, field, event.value);
                        if let Some(last) = self.keyframes.last_mut() {
                            last.pose = pose;
                        } else {
                            self.keyframes.push(CameraKeyframe {
                                t,
                                pose,
                                blend: 0.0,
                                from: Some(pose),
                                setup: None,
                            });
                        }
                        self.last_pose = pose;
                    }
                }
                "fade" => {
                    let start_alpha = event
                        .field
                        .map(|value| (value as f64 / 255.0).clamp(0.0, 1.0))
                        .unwrap_or(1.0);
                    self.fades.push(FadeEvent {
                        t,
                        duration: event.duration,
                        color: [event.x / 255.0, event.y / 255.0, event.value / 255.0],
                        start_alpha,
                        end_alpha: (event.value / 255.0).clamp(0.0, 1.0),
                    });
                }
                "scene" => self.scenes += 1,
                "noise" => self.noise += 1,
                "smoothing" => self.smoothings += 1,
                _ => {}
            }
        }
    }

    /// How long the beat holds the cutscene before the next one starts, from the
    /// durations the script declared (the engine's own pacing data).
    pub fn hold_seconds(event: &CameraEvent) -> f64 {
        match event.kind.as_str() {
            "setup" | "setup_z" => {
                if event.duration > 0.0 {
                    event.duration
                } else {
                    CAMERA_MIN_BLEND_S
                }
            }
            "pan" | "pan_z" | "position" => event.duration.max(0.0),
            "fade" => event.duration.max(0.0),
            _ => 0.0,
        }
    }

    fn push_pose(&mut self, t: f64, pose: CameraPose, requested_blend: f64, setup: Option<u32>) {
        let blend = |from: CameraPose, to: CameraPose, requested: f64| {
            let distance = from.target_distance(&to);
            // A shot may not move the view faster than the frame budget: extend
            // the blend until 30 Hz sampling stays inside
            // `CAMERA_MAX_STEP_UNITS`.
            let budget = distance / (CAMERA_MAX_STEP_UNITS * CAMERA_SAMPLE_HZ);
            let blend = if requested > 0.0 {
                requested.max(budget)
            } else if distance > 0.0 {
                budget.max(CAMERA_MIN_BLEND_S)
            } else {
                0.0
            };
            blend.min(CAMERA_MAX_BLEND_S)
        };
        // Two setups applied in the same instant are one shot: keep the newest
        // pose and blend it from the pose before, instead of stacking a
        // zero-length transition that would teleport.
        // The transition starts where the camera actually is at `t`: a shot that
        // begins before the previous blend finished must not snap forward.
        let from = if self.keyframes.is_empty() {
            pose
        } else {
            self.sample(t)
        };
        if let Some(last) = self.keyframes.last() {
            if (t - last.t).abs() < 1e-6 {
                let blend = blend(from, pose, requested_blend);
                if let Some(last) = self.keyframes.last_mut() {
                    last.pose = pose;
                    last.blend = blend;
                    if setup.is_some() {
                        last.setup = setup;
                    }
                }
                self.last_pose = pose;
                return;
            }
        }
        let blend = blend(from, pose, requested_blend);
        self.keyframes.push(CameraKeyframe {
            t,
            pose,
            blend,
            from: Some(from),
            setup,
        });
        self.last_pose = pose;
    }

    /// The camera pose at mission time `t`: the last shot, blended in over its
    /// `blend` seconds with a smoothstep.
    pub fn sample(&self, t: f64) -> CameraPose {
        let Some(index) = self
            .keyframes
            .iter()
            .rposition(|key| key.t <= t + 1e-9)
        else {
            return self.keyframes.first().map(|key| key.pose).unwrap_or_default();
        };
        let key = &self.keyframes[index];
        if index > 0 && key.blend > 0.0 {
            let u = ((t - key.t) / key.blend).clamp(0.0, 1.0);
            if u < 1.0 {
                // Linear interpolation: the blend was sized from this exact
                // segment, so the per-frame movement stays inside
                // `CAMERA_MAX_STEP_UNITS` (a smoothstep curve peaks at 1.5x the
                // linear speed and would break the budget).
                let from = key
                    .from
                    .unwrap_or_else(|| self.keyframes[index - 1].pose);
                return from.lerp(&key.pose, u);
            }
        }
        key.pose
    }

    /// The pose track between two mission times at `hz`.
    pub fn path(&self, from: f64, to: f64, hz: f64) -> Vec<(f64, CameraPose)> {
        let step = 1.0 / hz.max(1.0);
        let mut out = Vec::new();
        let mut t = from;
        while t <= to + 1e-9 {
            out.push((t, self.sample(t)));
            t += step;
        }
        out
    }

    /// The largest target movement between two consecutive samples (the number
    /// the `campaign.cine.camera-path` gate checks).
    pub fn max_step(&self, from: f64, to: f64, hz: f64) -> f64 {
        let path = self.path(from, to, hz);
        path.windows(2)
            .map(|pair| pair[0].1.target_distance(&pair[1].1))
            .fold(0.0f64, f64::max)
    }

    /// The screen tint `CinematicFadeBJ` asks for at mission time `t`.
    pub fn fade_at(&self, t: f64) -> Option<(f64, [f64; 3])> {
        self.fades
            .iter()
            .rev()
            .find(|fade| fade.t <= t + 1e-9)
            .map(|fade| fade.at(t))
    }
}

fn setup_pose(world: &World, handle: Option<u32>, fallback: &CameraPose) -> CameraPose {
    let Some(setup) = handle.and_then(|handle| world.camera_setups.get(&handle)) else {
        return *fallback;
    };
    let field = |id: i64, default: f64| setup.fields.get(&id).copied().unwrap_or(default);
    let (x, y) = setup
        .dest
        .unwrap_or((fallback.target[0], fallback.target[1]));
    CameraPose {
        target: [x, y],
        z: field(CAM_FIELD_ZOFFSET, fallback.z),
        distance: field(CAM_FIELD_TARGET_DISTANCE, fallback.distance),
        yaw: field(CAM_FIELD_ROTATION, fallback.yaw),
        pitch: field(CAM_FIELD_ANGLE_OF_ATTACK, fallback.pitch),
    }
}

fn apply_pose_field(pose: &mut CameraPose, field: i64, value: f64) {
    match field {
        CAM_FIELD_TARGET_DISTANCE => pose.distance = value,
        CAM_FIELD_ANGLE_OF_ATTACK => pose.pitch = value,
        CAM_FIELD_ROTATION => pose.yaw = value,
        CAM_FIELD_ZOFFSET => pose.z = value,
        _ => {}
    }
}

/// Where a subtitle came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleKind {
    /// `TransmissionFromUnitWithNameBJ` → `SetCinematicScene`.
    Transmission,
    /// A bare `SetCinematicScene`.
    Scene,
    /// `DisplayTimedTextToPlayer` / `DisplayTextToPlayer`.
    Message,
}

impl SubtitleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transmission => "transmission",
            Self::Scene => "scene",
            Self::Message => "message",
        }
    }
}

/// One subtitle line on the cutscene band.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleLine {
    /// Mission time the line appears.
    pub t: f64,
    pub speaker: String,
    pub text: String,
    /// Duration the script asked for.
    pub duration: f64,
    /// Duration the queue keeps the line on screen (reading-time floor when the
    /// voiceover length is unknown).
    pub shown_for: f64,
    pub kind: SubtitleKind,
    /// Live unit handle of the speaker portrait (`None` when the type has no
    /// unit on the map).
    pub portrait: Option<u32>,
    /// `portraitUnitId` rawcode, as the script passed it.
    pub portrait_unit_id: i64,
    pub sound: Option<u32>,
}

impl SubtitleLine {
    pub fn ends_at(&self) -> f64 {
        self.t + self.shown_for
    }
}

/// The cutscene subtitle band: every transmission, in order, with the window it
/// is on screen.
#[derive(Debug, Clone, Default)]
pub struct SubtitleQueue {
    lines: Vec<SubtitleLine>,
    observed: usize,
    transmissions: usize,
}

impl SubtitleQueue {
    /// Minimum reading time for a line whose voiceover length is not decoded.
    pub fn reading_time(text: &str) -> f64 {
        1.4 + 0.055 * text.chars().count() as f64
    }

    pub fn observe(&mut self, world: &World, mission: &Mission) -> usize {
        let mut fresh = 0;
        for event in world.subtitles.iter().skip(self.observed) {
            fresh += 1;
            self.push(world, event, mission, event.t);
        }
        self.observed = world.subtitles.len();
        fresh
    }

    /// Adds one subtitle line at mission time `t`.
    pub fn push(&mut self, world: &World, event: &SubtitleEvent, mission: &Mission, t: f64) {
        let text = mission.resolve(event.text.trim()).trim().to_string();
        if text.is_empty() {
            return;
        }
        let kind = if !event.speaker.trim().is_empty() || event.portrait_unit_id != 0 {
            if event.portrait_unit_id != 0 {
                SubtitleKind::Transmission
            } else {
                SubtitleKind::Scene
            }
        } else {
            SubtitleKind::Message
        };
        if kind == SubtitleKind::Transmission {
            self.transmissions += 1;
        }
        // The script's `sceneDuration` is the voiceover length plus the portrait
        // hang time; this VM does not decode the voice file, so a line that would
        // vanish before it can be read keeps the reading-time floor instead.
        let shown_for = event.duration.max(Self::reading_time(&text));
        let portrait = portrait_handle(world, event.portrait_unit_id);
        let speaker = mission.resolve(event.speaker.trim()).trim().to_string();
        self.lines.push(SubtitleLine {
            t,
            speaker: if speaker.is_empty() {
                portrait
                    .and_then(|handle| world.units.get(&handle))
                    .map(|unit| mission.resolve(&unit.name))
                    .unwrap_or_default()
            } else {
                speaker
            },
            text,
            duration: event.duration,
            shown_for,
            kind,
            portrait,
            portrait_unit_id: event.portrait_unit_id,
            sound: event.sound,
        });
    }

    pub fn lines(&self) -> &[SubtitleLine] {
        &self.lines
    }

    pub fn transmissions(&self) -> usize {
        self.transmissions
    }

    /// The line on screen at mission time `t` (the newest one still running).
    pub fn active_at(&self, t: f64) -> Option<&SubtitleLine> {
        self.lines
            .iter()
            .rev()
            .find(|line| line.t <= t + 1e-9 && t < line.ends_at())
    }
}

fn portrait_handle(world: &World, rawcode: i64) -> Option<u32> {
    if rawcode == 0 {
        return None;
    }
    let id = String::from_utf8_lossy(&(rawcode as u32).to_le_bytes()).to_string();
    world
        .units
        .iter()
        .find(|(_, unit)| unit.id == id)
        .map(|(handle, _)| *handle)
}

/// Everything the report/storyboard generator needs about the running mission.
#[derive(Debug, Clone, Default)]
pub struct MissionSessionReport {
    pub mission: String,
    pub title: String,
    pub frames: u64,
    pub time: f64,
    pub cine_summary: String,
    pub quests: Vec<QuestRow>,
    pub keyframes: Vec<CameraKeyframe>,
    pub fade_events: Vec<FadeEvent>,
    pub subtitles: Vec<SubtitleLine>,
    pub human_slot: Option<u8>,
    pub script_booted: bool,
}

/// The HUD-facing snapshot of the running campaign mission.
#[derive(Debug, Clone, Default)]
pub struct CampaignView {
    pub index: usize,
    pub id: String,
    pub title: String,
    pub interlude: bool,
    pub script_booted: bool,
    /// Mission human seat (`None` when the map names no user slot).
    pub human_slot: Option<u8>,
    pub quests: Vec<QuestRow>,
    pub chapters: Vec<ChapterRow>,
    pub cutscene: CutsceneLedger,
    /// Mission time the snapshot was taken at.
    pub time: f64,
    /// Camera pose at `time`.
    pub camera: CameraPose,
    /// Number of camera keyframes so far.
    pub keyframes: usize,
    pub fades: usize,
    pub transmissions: usize,
    /// The subtitle on screen at `time`.
    pub subtitle: Option<SubtitleLine>,
    pub victory: bool,
    pub victory_reason: Option<String>,
    pub defeat: bool,
    pub completed: bool,
    pub next: Option<usize>,
    pub movies_requested: Vec<String>,
    pub movies_played: Vec<String>,
    pub missing_movies: Vec<String>,
    pub log: Vec<String>,
}

impl CampaignView {
    pub fn quest(&self, title: &str) -> Option<&QuestRow> {
        self.quests.iter().find(|row| row.title == title)
    }

    pub fn required_done(&self) -> usize {
        self.quests
            .iter()
            .filter(|row| row.required && row.completed)
            .count()
    }

    pub fn required_total(&self) -> usize {
        self.quests.iter().filter(|row| row.required).count()
    }
}

fn campaign_world(game: &crate::Game) -> Option<&World> {
    game.engine
        .as_ref()
        .and_then(|engine| engine.runtime())
        .map(|runtime| &runtime.world)
}

/// The human slot of the running mission: the `war3map.w3i` user seat, else the
/// slot the script marked `MAP_CONTROL_USER`, else the owner of the most map
/// units (campaign maps place the hero there).
fn human_slot(game: &crate::Game) -> Option<u8> {
    if let Some(session) = active_campaign() {
        if let Some(slot) = session.mission.human_slot() {
            return Some(slot);
        }
    }
    if let Some(world) = campaign_world(game) {
        if let Some(index) = world
            .players
            .iter()
            .position(|player| player.controller == MAP_CONTROL_USER)
        {
            return u8::try_from(index).ok();
        }
        // The map's own start-location markers (`sloc`) belong to the seat the
        // map was authored for, and the campaign intro's first camera shot looks
        // at the protagonist. A roster count is the wrong signal: Human01's
        // human owns 4 units and the enemy 27, so "the biggest army" answered
        // seat 2 while the mission's human is seat 1 (measured: the opening only
        // runs with `GetLocalPlayer() == Player(1)`).
        let opening = world.camera_setups.values().find_map(|setup| setup.dest);
        let mut markers: Vec<(f64, i32)> = world
            .units
            .values()
            .filter(|unit| unit.id.eq_ignore_ascii_case("sloc") && (0..12).contains(&unit.owner))
            .map(|unit| {
                let distance =
                    opening.map_or(0.0, |(x, y)| (unit.x - x).powi(2) + (unit.y - y).powi(2));
                (distance, unit.owner)
            })
            .collect();
        markers.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        if let Some((_, owner)) = markers.first() {
            return u8::try_from(*owner).ok();
        }
    }
    let mut counts = [0usize; 16];
    for unit in game.sim.alive_units() {
        let owner = unit.owner as usize;
        if owner < counts.len() {
            counts[owner] += 1;
        }
    }
    let mut best: Option<(u8, usize)> = None;
    for (owner, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if best.is_none_or(|(_, previous)| *count > previous) {
            best = Some((owner as u8, *count));
        }
    }
    best.map(|(owner, _)| owner)
}

impl crate::Game {
    /// Loads campaign chapter `index` into this game: the map's terrain, its
    /// units, its script entry points (`main`, `InitCustomTriggers`,
    /// `RunInitializationTriggers`) and its quest panel.
    pub fn start_campaign_mission(&mut self, index: usize) -> Result<usize, String> {
        let registry = campaign_registry()
            .ok_or_else(|| "campaign maps not found (set WC3RUST_DATA)".to_string())?;
        if !registry.is_unlocked(index) {
            return Err(format!("chapter {index} is locked"));
        }
        let chapter = registry
            .chapter(index)
            .cloned()
            .ok_or_else(|| format!("chapter {index} has no map"))?;
        // Archive chapters (`Maps\Campaign\Human01.w3m` inside `War3.mpq`) are
        // extracted to the cache before anything parses them; the loose
        // `files/` copy wins when it exists. A dead entry is an error here.
        let map = registry.map_path(index)?;
        // `open_with` parses `war3map.w3i`/`war3map.wts`/`war3map.j` from the
        // chapter path, so the extraction has to exist first.
        let mut session = CampaignSession::open_with(registry, index)?;
        // The engine runs `war3map.j` inside `Game::new`, and Blizzard.j gates
        // every camera/transmission block on `GetLocalPlayer() == whichPlayer`:
        // the seat has to be known before the boot, not after it.
        if let Some(slot) = session.mission.human_slot() {
            wc3_re::jass::vm::Runtime::set_default_local_player(u32::from(slot));
        }
        // RoC campaign maps are `war3map.w3i` v18 and do not name the user seat;
        // single-player wildcard mode makes every "local player" block the
        // human's block (see `World::local_player_any`).
        wc3_re::jass::vm::set_local_player_any_default(session.mission.human_slot().is_none());
        // The archive a mounted chapter came from *is* the install root: using
        // the previous game's `mpq=` line made a chapter that was extracted to
        // the cache depend on whatever map happened to be loaded before it.
        let mpq = match &chapter.source {
            MapSource::Mount { archive, .. } => Some(archive.clone()),
            MapSource::File => self.assets.mpq_path().map(Path::to_path_buf),
        };
        let assets = crate::assets::Assets::load(mpq.as_deref(), Some(&map));
        let mut fresh = crate::Game::new(assets, CAMPAIGN_SEED);
        fresh.enter_battle();
        // A chapter always owns its script. Two game-layer rules can suppress
        // the boot for a chapter map:
        //   * the melee-arena stem list (`game.rs:1331`, which names `Orc01`
        //     and `Human04` — the two maps the device ships loose);
        //   * the install-root probe, which only sees an install when the
        //     archive is named by a `mpq=` debug line, while an install-shaped
        //     root logs `storm_layers=` instead (`assets::open_asset_db`).
        // Mount the chapter's engine here and let the game's own
        // `reset_scenario` run the map: it enters the script on the 32 MB
        // stack, creates the sim bridge and adopts the map's units. The two
        // one-line fixes for the owners of `game.rs` are written up in
        // `analysis/campaign/ORC01-MELEE-STEM-PATCH.md`.
        if fresh.engine.is_none() {
            fresh.melee = false;
            match campaign_engine(&map, mpq.as_deref()) {
                Ok(engine) => {
                    fresh.engine = Some(engine);
                    fresh.restart();
                }
                Err(error) => {
                    session.note(format!(
                        "CAMPAIGN_SCRIPT_BOOT_FAILED map={} {error}",
                        map.display()
                    ));
                }
            }
        }
        session.script_booted = fresh
            .engine
            .as_ref()
            .and_then(|engine| engine.runtime())
            .is_some();
        // The mission's human seat is bound before the provenance line, so the
        // line states whether the sim actually got the seat. The wave-2 defect
        // was a `sim_bound=false` warning emitted after a load that had already
        // left the mission on the wrong owner.
        let title = if chapter.title.trim().is_empty() {
            session.mission.title.clone()
        } else {
            chapter.title.clone()
        };
        let mut sim_bound = false;
        let mut sim_bound_units = 0usize;
        let mut sim_bound_visible = 0usize;
        if let Some(slot) = session.mission.human_slot().or_else(|| human_slot(&fresh)) {
            session.human_slot = Some(slot);
            // Lane A (ledger 518): the sim makes the seat the local player,
            // stamps its sight and reports what it owns/sees. A seat outside
            // the sim's `MAX_PLAYERS` is a hard mission-start failure: a
            // campaign cannot run its hero's seat on a player the sim does not
            // carry.
            match bind_campaign_seat(&mut fresh.sim, slot) {
                Ok(binding) => {
                    sim_bound = true;
                    sim_bound_units = binding.units;
                    sim_bound_visible = binding.visible_units;
                    // `Game::player` indexes the game layer's own two-seat
                    // arrays (`score`, `sim.resources`); seats above that are
                    // the sim's `local_player()` until the game layer switches
                    // those reads to the seat accessors.
                    if usize::from(slot) < SIM_PLAYER_SLOTS {
                        fresh.player = slot;
                    }
                }
                Err(error) => {
                    let line = format!("CAMPAIGN_SEAT_UNBOUND idx={index} slot={slot} {error}");
                    crate::dev_log!("{line}");
                    return Err(line);
                }
            }
            crate::dev_log!(
                "CAMPAIGN_HUMAN_SLOT idx={index} name={} slot={slot} sim_bound={sim_bound} units={sim_bound_units} visible={sim_bound_visible}",
                session.mission.id
            );
            // `GetLocalPlayer()` must answer the mission's human seat: Blizzard.j
            // gates every `CameraSetupApplyForPlayer`, `SetCameraFieldForPlayer`
            // and `TransmissionFromUnitWithNameBJ` block on
            // `GetLocalPlayer() == whichPlayer`.
            //
            // Only a seat the *map* declares is answered exactly. A RoC map
            // (`war3map.w3i` v18) carries no player records at all — Human01's
            // parsed `players` is empty — so the seat is inferred, and narrowing
            // the single-player wildcard to an inferred seat dropped every gated
            // block the script reached *after* its first `TriggerSleepAction`
            // (the opening played two camera shots and no transmission). The
            // wildcard is the campaign's own rule: there is exactly one human,
            // so the human's blocks are the local player's blocks.
            if let Some(declared) = session.mission.human_slot() {
                if let Some(runtime) = fresh
                    .engine
                    .as_mut()
                    .and_then(|engine| engine.runtime.as_mut())
                {
                    runtime.set_local_player(u32::from(declared));
                }
            }
        }
        // Provenance line the device harness greps: which chapter, from where,
        // how many bytes, and what the boot actually produced. It goes through
        // the session log as well as the platform log so the HUD and the
        // evidence run read the same line.
        let bytes = std::fs::metadata(&map).map(|meta| meta.len()).unwrap_or(0);
        session.note(format!(
            "CAMPAIGN_LOAD source={} path={} bytes={} units={} quests={} script={} title={} sim_bound={} seat_units={} seat_visible={}",
            chapter.source.as_str(),
            chapter
                .archive_entry_path()
                .map(|entry| entry.to_string())
                .unwrap_or_else(|| map.display().to_string()),
            bytes,
            fresh.sim.alive_units().count(),
            session.mission.objectives.len(),
            session.script_booted,
            title,
            sim_bound,
            sim_bound_units,
            sim_bound_visible,
        ));
        *self = fresh;
        if let Some(world) = campaign_world(self) {
            session.observe(world);
        }
        store_active(session);
        Ok(index)
    }

    /// Loads a chapter by its map id (`Human01`).
    pub fn start_campaign_mission_by_id(&mut self, id: &str) -> Result<usize, String> {
        let registry = campaign_registry()
            .ok_or_else(|| "campaign maps not found (set WC3RUST_DATA)".to_string())?;
        let index = registry
            .index_of(id)
            .ok_or_else(|| format!("campaign has no chapter {id}"))?;
        self.start_campaign_mission(index)
    }

    /// The slot the running mission gives the human player
    /// (`MAP_CONTROL_USER`, else the owner of the most map units).
    pub fn campaign_human_slot(&self) -> Option<u8> {
        human_slot(self)
    }

    /// Chapter index of a map id in the loaded campaign.
    pub fn campaign_index_of(&self, id: &str) -> Option<usize> {
        campaign_registry().and_then(|registry| registry.index_of(id))
    }

    /// Mission-select rows: every chapter with its cleared/unlocked/locked state.
    pub fn campaign_chapters(&self) -> Vec<ChapterRow> {
        if let Some(session) = active_campaign() {
            return session.registry.rows();
        }
        campaign_registry()
            .map(|registry| registry.rows())
            .unwrap_or_default()
    }

    /// The live quest panel (`Game::quest_rows` for the HUD screen lane).
    pub fn quest_rows(&self) -> Vec<QuestRow> {
        self.sync_campaign(false)
            .map(|view| view.quests)
            .unwrap_or_default()
    }

    /// The running mission snapshot, synced with the VM first.
    pub fn campaign_view(&self) -> Option<CampaignView> {
        self.sync_campaign(false)
    }

    /// One frame of campaign flow: read the mission's VM world, hand a requested
    /// movie to the decoder and advance it. Safe to call every frame; it never
    /// blocks the mission on a missing movie.
    pub fn campaign_frame(&mut self, dt: f32) -> Option<CampaignView> {
        let view = self.sync_campaign(true);
        if let Some(session) = active_campaign() {
            // A campaign mission is decided by its own victory/defeat trigger,
            // not by the melee "no base, no workers" rule: Human01 seats the
            // player on slot 2 and the map's scripted units carry no halls, so
            // `Game::evaluate_match` finishes the match on frame one. Until the
            // rule grows a campaign gate (`game.rs:3388`), the campaign layer
            // restores the running state; `outcome` is written once, so this is
            // a single correction per mission, not a per-tick fight.
            if !session.victory
                && !session.defeat
                && (self.phase != crate::game::Phase::Playing
                    || self.screen != crate::game::Screen::Playing)
            {
                self.phase = crate::game::Phase::Playing;
                self.screen = crate::game::Screen::Playing;
                if !CAMPAIGN_RULE_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    crate::dev_log!(
                        "CAMPAIGN_MELEE_RULE_IGNORED idx={} reason=scripted-mission-no-base game.rs=3388",
                        session.index
                    );
                }
            }
        }
        if let Some(name) = self.take_pending_movie() {
            match self.cinematic_start(&name) {
                Ok(()) => {}
                Err(error) => crate::dev_log!("{error}"),
            }
        }
        if self.cinematic_active() {
            let _ = self.cinematic_frame(dt);
        }
        view
    }

    /// Pops the movie request the campaign has not handed to the decoder yet.
    pub fn take_pending_movie(&self) -> Option<String> {
        active_cell()
            .lock()
            .ok()
            .and_then(|mut guard| guard.as_mut().and_then(|session| session.pending_movie.take()))
    }

    /// Loads the chapter after the running one (the victory screen's Next).
    pub fn campaign_next_mission(&mut self) -> Result<Option<usize>, String> {
        let session = active_campaign().ok_or_else(|| "no campaign mission is running".to_string())?;
        match session.next {
            Some(next) => {
                self.start_campaign_mission(next)?;
                Ok(Some(next))
            }
            None => Ok(None),
        }
    }

    /// Marks the running chapter cleared. The victory trigger does this
    /// automatically; this is the explicit entry point for a skip/cheat path and
    /// for tests.
    pub fn campaign_complete_mission(&mut self) -> Option<usize> {
        let mut guard = active_cell().lock().ok()?;
        let session = guard.as_mut()?;
        if session.completed {
            return session.next;
        }
        session.victory = true;
        session.completed = true;
        session.victory_reason = Some("forced".to_string());
        let next = session.registry.complete(session.index);
        session.next = next;
        let line = format!(
            "CAMPAIGN_DONE idx={} name={} next={} reason=forced",
            session.index,
            session.mission.id,
            next.map(|index| index.to_string()).unwrap_or_else(|| "none".to_string())
        );
        session.note(line);
        let registry = session.registry.clone();
        drop(guard);
        sync_global_registry(&registry);
        next
    }

    /// Contract lines of the running mission (CAMPAIGN_MISSION / QUEST / …).
    pub fn campaign_log(&self) -> Vec<String> {
        active_campaign().map(|session| session.log).unwrap_or_default()
    }

    fn sync_campaign(&self, advance_frame: bool) -> Option<CampaignView> {
        let world = campaign_world(self);
        let mut guard = active_cell().lock().ok()?;
        let session = guard.as_mut()?;
        if let Some(world) = world {
            session.observe(world);
        }
        if advance_frame {
            session.frames += 1;
        }
        Some(session.view())
    }

    /// The camera keyframe stream the renderer plays a cutscene from.
    pub fn camera_director(&self) -> Option<CameraDirector> {
        active_campaign().map(|session| session.director)
    }

    /// Where the cutscene camera looks right now.
    pub fn cutscene_camera(&self) -> Option<CameraPose> {
        let session = active_campaign()?;
        Some(session.director.sample(session.cine_clock))
    }

    /// The `CinematicFadeBJ` tint the render path composites, if a fade is
    /// running at the current mission time.
    pub fn screen_fade(&self) -> Option<(f64, [f64; 3])> {
        let session = active_campaign()?;
        session.director.fade_at(session.cine_clock)
    }

    /// The subtitle the HUD band shows right now.
    pub fn subtitle_now(&self) -> Option<SubtitleLine> {
        let session = active_campaign()?;
        session.subtitles.active_at(session.cine_clock).cloned()
    }

    /// A cutscene is on screen: the script moved the camera away from the
    /// gameplay view and has not reset it.
    pub fn cutscene_active(&self) -> bool {
        active_campaign().is_some_and(|session| session.cutscene.active)
    }

    /// `ShowInterface(false, …)`: the script hid the console for a cutscene.
    pub fn cutscene_ui_hidden(&self) -> bool {
        campaign_world(self).is_some_and(|world| !world.interface_shown)
    }

    /// The mission report (quests, camera keyframes, fades, subtitles).
    pub fn campaign_session_report(&self) -> Option<MissionSessionReport> {
        self.sync_campaign(false)?;
        active_campaign().map(|session| session.report())
    }

    /// `CAMPAIGN_CINE …` for the running mission.
    pub fn campaign_cine_summary(&self) -> Option<String> {
        self.sync_campaign(false).map(|_| {
            active_campaign()
                .map(|session| session.cine_summary())
                .unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
function InitTrig_Intro_Cinematic takes nothing returns nothing
    set gg_trg_Intro_Cinematic = CreateTrigger(  )
    call TriggerAddAction( gg_trg_Intro_Cinematic, function Trig_Intro_Cinematic_Actions )
endfunction

function Trig_Intro_Cinematic_Actions takes nothing returns nothing
    call CinematicFadeBJ( bj_CINEFADETYPE_FADEOUT, 0.00, "White" )
    call CameraSetupApplyForPlayer( true, gg_cam_Start01, udg_AAAP_Arthas, 0.00 )
    call CinematicModeBJ( true, udg_AAAPG_Arthas )
    call TransmissionFromUnitWithNameBJ( udg_AAAPG_Arthas, udg_Uther, "TRIGSTR_020", null, "TRIGSTR_021", bj_TIMETYPE_ADD, 0.00, false )
    call EndCinematicScene()
    call CinematicModeBJ( false, udg_AAAPG_Arthas )
endfunction

function InitTrig_SetupQuests takes nothing returns nothing
endfunction
"#;

    #[test]
    fn campaign_scaffold_parses_objectives_and_beats() {
        let mut wts = Wts::default();
        wts.entries.insert(20, "Uther".to_string());
        wts.entries.insert(21, "Glad you came.".to_string());
        wts.entries.insert(64, "The Defense of Strahnbrad".to_string());
        wts.entries.insert(65, "Save the village.".to_string());
        wts.entries.insert(68, "Travel to Strahnbrad".to_string());
        let script = format!(
            "{SCRIPT}\nfunction Trig_SetupQuests_Actions takes nothing returns nothing\n    call CreateQuestBJ( bj_QUESTTYPE_REQ_DISCOVERED, \"TRIGSTR_064\", \"TRIGSTR_065\", \"icon.tga\" )\n    set udg_QuestVillage = GetLastCreatedQuestBJ()\n    call CreateQuestItemBJ( udg_QuestVillage, \"TRIGSTR_068\" )\nendfunction\n"
        );
        let mut mission = Mission {
            id: "Human01".into(),
            ..Mission::default()
        };
        mission.apply_script(&script, &wts);
        assert_eq!(mission.objectives.len(), 1);
        let quest = &mission.objectives[0];
        assert_eq!(quest.title, "The Defense of Strahnbrad");
        assert_eq!(quest.variable, "udg_QuestVillage");
        assert!(quest.required);
        assert_eq!(quest.items.len(), 1);
        assert_eq!(quest.items[0].text, "Travel to Strahnbrad");
        let intro = mission
            .cinematics
            .iter()
            .find(|cinematic| cinematic.name == "Trig_Intro_Cinematic_Actions")
            .expect("intro cutscene");
        assert_eq!(intro.kind, CinematicKind::Intro);
        assert_eq!(intro.beats(BeatKind::Camera), 1);
        assert_eq!(intro.beats(BeatKind::Transmission), 1);
        assert_eq!(intro.beats(BeatKind::CinematicMode), 2);
        assert_eq!(intro.beats(BeatKind::Filter), 1);
        assert_eq!(intro.beats(BeatKind::Scene), 1);
        let transmission = intro
            .events
            .iter()
            .find(|beat| beat.kind == BeatKind::Transmission)
            .unwrap();
        assert!(
            transmission.text.contains("Uther") && transmission.text.contains("Glad you came."),
            "WTS refs resolved: {}",
            transmission.text
        );
        assert_eq!(mission.triggers, vec!["Intro_Cinematic", "SetupQuests"]);
        assert_eq!(mission.cameras, vec!["gg_cam_Start01"]);
    }

    #[test]
    fn campaign_chapter_order_follows_the_retail_campaign() {
        assert!(chapter_rank("Prologue01") < chapter_rank("Human01"));
        assert!(chapter_rank("Human02") < chapter_rank("Human02Interlude"));
        assert!(chapter_rank("Human02Interlude") < chapter_rank("Human03"));
        assert!(chapter_rank("Human09") < chapter_rank("Undead01"));
        assert!(chapter_rank("NightElf09") < chapter_rank("WarCraftIIICredits"));
        assert_eq!(title_from_id("Human02Interlude"), "Human 02 Interlude");
    }

    /// The shipped campaign tree (`analysis/war3_full3/Maps/Campaign`).
    ///
    /// All 44 RoC chapters ship as loose maps in the extracted install, so this
    /// is the oracle for the campaign layer: the registry has to see exactly
    /// these 44, in retail order, with no file dropped and none invented.
    fn shipped_campaign_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps/Campaign")
    }

    fn shipped_map_stems(dir: &Path) -> Vec<String> {
        let mut stems: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let extension = path.extension()?.to_str()?.to_ascii_lowercase();
                if !matches!(extension.as_str(), "w3m" | "w3x") {
                    return None;
                }
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
            })
            .collect();
        stems.sort();
        stems
    }

    fn shipped_registry() -> CampaignRegistry {
        let dir = shipped_campaign_dir();
        assert!(
            dir.is_dir(),
            "shipped campaign tree missing at {}",
            dir.display()
        );
        let mut registry = CampaignRegistry::from_map_dir(&dir, CampaignProgress::default())
            .unwrap_or_else(|error| panic!("registry from {}: {error}", dir.display()));
        // A headless sweep must never write the operator's progress file.
        registry.save_path = None;
        registry
    }

    #[test]
    fn campaign_registry_lists_all_44_shipped_chapters() {
        let dir = shipped_campaign_dir();
        let stems = shipped_map_stems(&dir);
        assert_eq!(stems.len(), 44, "the shipped Campaign tree holds 44 maps");
        let registry = shipped_registry();
        assert_eq!(registry.len(), 44, "44 chapters discovered");
        assert_eq!(registry.len(), stems.len(), "44 = 44: no chapter dropped");
        let mut ids: Vec<String> = registry
            .rows()
            .iter()
            .map(|row| row.id.clone())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 44, "chapter ids are unique");
        ids.sort();
        assert_eq!(ids, sorted, "no id appears twice");
        // Every shipped file is a chapter, and every chapter is a shipped file.
        let mut discovered: Vec<String> = registry
            .rows()
            .iter()
            .map(|row| row.id.to_ascii_lowercase())
            .collect();
        discovered.sort();
        let expected: Vec<String> = stems.iter().map(|id| id.to_ascii_lowercase()).collect();
        assert_eq!(discovered, expected, "44 chapter files = 44 registry rows");

        // Retail order: prologue first, the two credits reels last, each
        // interlude directly after the mission it closes.
        let rows = registry.rows();
        assert_eq!(rows[0].id, "Prologue01");
        let tail: Vec<&str> = rows[42..].iter().map(|row| row.id.as_str()).collect();
        assert_eq!(tail, vec!["BonusCredits", "WarCraftIIICredits"]);
        let interludes: Vec<&str> = rows
            .iter()
            .filter(|row| row.interlude)
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(
            interludes,
            vec![
                "Human02Interlude",
                "Human05Interlude",
                "Human06Interlude",
                "Undead02Interlude",
                "Undead05interlude",
                "Orc02Interlude",
                "Orc04Interlude",
                "NightElf06Interlude",
                "BonusCredits",
                "WarCraftIIICredits",
            ],
            "8 interludes + 2 credits reels, in retail race order (Human, Undead, Orc, NightElf)"
        );
        for row in registry.rows() {
            assert!(
                is_chapter_id(&row.id),
                "{} is a chapter id, not a melee map",
                row.id
            );
            assert_eq!(
                row.interlude,
                is_interlude_id(&row.id),
                "{} interlude flag matches its kind",
                row.id
            );
            assert_eq!(
                row.number,
                if row.interlude {
                    0
                } else {
                    chapter_number(&row.id)
                },
                "{} chapter number",
                row.id
            );
        }
    }

    /// The campaign flow itself: the shipped chain is walked chapter by chapter.
    ///
    /// A fresh save unlocks the first mission of each campaign (the five RoC
    /// campaigns plus the credits group); every victory unlocks exactly the next
    /// chapter of that campaign, interludes included, and the 44th victory ends
    /// the chain. That is the retail progression, proved on the shipped order.
    #[test]
    fn campaign_flow_clears_all_44_chapters_in_retail_order() {
        let mut registry = shipped_registry();
        assert_eq!(registry.len(), 44);
        let fresh_rows = registry.rows();
        let fresh: Vec<&str> = fresh_rows
            .iter()
            .filter(|row| row.state == ChapterState::Unlocked)
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(
            fresh,
            vec![
                "Prologue01",
                "Human01",
                "Undead01",
                "Orc01",
                "NightElf01",
                "BonusCredits",
                "WarCraftIIICredits",
            ],
            "one open mission per campaign at a fresh save; the credits reels are\
             interludes of the Extra group, so they never gate"
        );
        assert_eq!(registry.first_unlocked(), Some(0));

        let mut cleared = 0usize;
        loop {
            let Some(index) = registry.first_unlocked() else {
                break;
            };
            let id = registry.chapter(index).expect("chapter").id.clone();
            assert!(registry.is_unlocked(index), "{id} is playable");
            let next = registry.complete(index);
            cleared += 1;
            let expected = (index + 1 < registry.len()).then_some(index + 1);
            assert_eq!(
                next, expected,
                "clearing {id} (index {index}) opens exactly the next chapter"
            );
            if let Some(next) = next {
                assert!(
                    campaign_order(&registry.chapter(next).unwrap().id)
                        >= campaign_order(&id),
                    "the chain never walks backwards"
                );
            }
            assert!(registry.is_cleared(index), "{id} persisted as cleared");
            assert!(cleared <= 44, "the chain cannot clear more than 44 chapters");
        }
        assert_eq!(cleared, 44, "all 44 shipped chapters are reachable");
        assert_eq!(registry.first_unlocked(), None, "the campaign is complete");
        for row in registry.rows() {
            assert_eq!(
                row.state,
                ChapterState::Cleared,
                "{} is cleared at the end of the campaign",
                row.id
            );
            assert!(
                registry.next_after(row.index).is_none(),
                "nothing unlocks after the campaign"
            );
        }
    }

    /// Every chapter resolves to a real map whose script the mission model can
    /// read: 44/44 `war3map.j` scripts, each with its `war3map.w3i` metadata.
    #[test]
    fn campaign_every_chapter_parses_into_a_mission() {
        let registry = shipped_registry();
        assert_eq!(registry.len(), 44);
        let mut parsed = 0usize;
        let mut script_bytes = 0usize;
        let mut objectives = 0usize;
        let mut cutscenes = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for chapter in &registry.campaign.chapters {
            let map = match chapter.resolved_map() {
                Ok(map) => map,
                Err(error) => {
                    failures.push(format!("{}: {error}", chapter.id));
                    continue;
                }
            };
            match Mission::parse(&map) {
                Ok(mission) => {
                    if mission.script_bytes == 0 {
                        failures.push(format!("{}: empty war3map.j", chapter.id));
                        continue;
                    }
                    if !mission.id.eq_ignore_ascii_case(&chapter.id) {
                        failures.push(format!(
                            "{}: parsed as {}",
                            chapter.id, mission.id
                        ));
                        continue;
                    }
                    parsed += 1;
                    script_bytes += mission.script_bytes;
                    objectives += mission.objectives.len();
                    cutscenes += mission.cinematics.len();
                }
                Err(error) => failures.push(format!("{}: {error}", chapter.id)),
            }
        }
        assert!(failures.is_empty(), "44 chapter parse failures: {failures:?}");
        assert_eq!(parsed, 44, "every shipped chapter is a mission");
        assert!(
            script_bytes > 44 * 1024,
            "44 scripts carry real code ({script_bytes} bytes)"
        );
        assert!(
            objectives >= 40,
            "the missions declare their objectives ({objectives} quests)"
        );
        assert!(
            cutscenes >= 20,
            "the missions carry their cutscenes ({cutscenes} cinematics)"
        );
        println!(
            "GATE campaign.chapter-parse PASS chapters={parsed} script_bytes={script_bytes} objectives={objectives} cinematics={cutscenes}"
        );
    }
}
