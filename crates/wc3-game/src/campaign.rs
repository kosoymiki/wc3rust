//! Campaign data model: campaign → mission → cutscene → trigger.
//!
//! A Warcraft III campaign is a list of missions; each mission is a map whose
//! script carries the fiction: quests (the objectives panel), cinematics
//! (camera moves, transmissions, scene text), and the triggers that sequence
//! them. The retail campaign descriptor (`war3campaign.w3f` inside a `.w3n`)
//! is not shipped in the data archives this installation carries — the mission
//! maps under `Maps\Campaign\` are — so the model is built from what the maps
//! themselves contain, and nothing is invented for a mission that is absent.
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
        Self::from_sources(Some(dir.as_ref()), &[])
    }

    /// Builds the chapter list from the loose install files **and** the mounted
    /// archives: a chapter that exists in `Maps\Campaign\` of `War3.mpq` is part
    /// of the campaign even when its file was never copied out of the archive.
    ///
    /// Order is the retail chapter order (see [`chapter_rank`]); a disk file
    /// wins over the archive copy of the same chapter.
    pub fn from_sources(
        disk: Option<&Path>,
        archives: &[PathBuf],
    ) -> std::io::Result<Self> {
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
                if chapters
                    .iter()
                    .any(|known| known.id.eq_ignore_ascii_case(&id))
                {
                    continue;
                }
                chapters.push(Chapter {
                    title: title_from_id(&id),
                    interlude: is_interlude_id(&id),
                    map: campaign_cache_dir().join(format!("{id}.w3m")),
                    id,
                    source: MapSource::Mount {
                        archive: archive.clone(),
                        entry,
                    },
                });
            }
        }
        chapters.sort_by(|left, right| {
            chapter_rank(&left.id)
                .cmp(&chapter_rank(&right.id))
                .then_with(|| left.id.cmp(&right.id))
        });
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

fn chapter_number(id: &str) -> u32 {
    let digits: String = id
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(99)
}

fn chapter_rank(id: &str) -> (u8, u32, bool) {
    // Interludes follow the mission they close, so `Human02Interlude` sorts
    // after `Human02` and before `Human03`.
    (
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

/// Mounted archives that can carry `Maps\Campaign`, in mount priority
/// (`War3.mpq` base, then `War3x.mpq` for the TFT chapters).
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
        for name in ["War3.mpq", "War3x.mpq"] {
            if let Some(path) = archive_in_dir(&root, name)
                && !found.iter().any(|known| known == &path)
            {
                found.push(path);
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

/// `Maps\Campaign\*.w3m|w3x` entries of a mounted archive, as
/// `(entry, chapter id)` pairs, sorted by the retail chapter order.
pub fn archive_campaign_entries(archive: &Path) -> Vec<(String, String)> {
    let Ok(mpq) = wc3_mpq::MpqArchive::open(archive) else {
        crate::dev_log!("CAMPAIGN_ARCHIVE_UNREADABLE path={}", archive.display());
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in mpq.listfile() {
        let lower = entry.to_ascii_lowercase().replace('/', "\\");
        if !lower.starts_with("maps\\campaign\\") {
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
    let dir = campaign_cache_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create {}: {error}", dir.display()))?;
    let stem = entry
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("chapter.w3m");
    let path = dir.join(stem);
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

/// Player slots the canonical sim carries (`Sim::resources: [i32; 2]`). A
/// campaign map may seat the human on a higher slot; the campaign HUD binds to
/// that map slot, and only a slot inside this range is handed to the sim's own
/// order/score pipeline.
pub const SIM_PLAYER_SLOTS: usize = 2;

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
        let dir = campaign_map_dir();
        let archives = campaign_archives();
        if dir.is_none() && archives.is_empty() {
            return Err(format!(
                "campaign maps not found: set {DATA_ROOT_ENV} (install root) or {CAMPAIGN_DIR_ENV}"
            ));
        }
        let save_path = CampaignProgress::default_path();
        let progress = CampaignProgress::load(&save_path).unwrap_or_else(|error| {
            crate::dev_log!("CAMPAIGN_PROGRESS_RESET {error}");
            CampaignProgress::default()
        });
        let campaign = Campaign::from_sources(dir.as_deref(), &archives)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            campaign,
            progress,
            save_path: Some(save_path),
            last_save_error: None,
        })
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
        let race = race_order(&chapter.id);
        let Some(first) = (0..self.len()).find(|candidate| {
            self.chapter(*candidate)
                .is_some_and(|known| race_order(&known.id) == race)
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
        // order. The headless VM has no coroutine suspension, so a whole cutscene
        // runs inside one tick; the cutscene clock lays the beats out on the
        // timeline using the durations the script itself declares
        // (`CAMPAIGN_CINE_CLOCK=declared-durations`).
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
            let start = beat.t.max(self.cine_clock);
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
        let mpq = self
            .assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("mpq="))
            .map(PathBuf::from);
        let assets = crate::assets::Assets::load(mpq.as_deref(), Some(&map));
        let mut fresh = crate::Game::new(assets, CAMPAIGN_SEED);
        fresh.enter_battle();
        session.script_booted = fresh.engine.is_some();
        // Provenance line the device harness greps: which chapter, from where,
        // how many bytes, and what the boot actually produced.
        let bytes = std::fs::metadata(&map).map(|meta| meta.len()).unwrap_or(0);
        crate::dev_log!(
            "CAMPAIGN_LOAD source={} path={} bytes={} units={} quests={} script={}",
            chapter.source.as_str(),
            chapter
                .archive_entry_path()
                .map(|entry| entry.to_string())
                .unwrap_or_else(|| map.display().to_string()),
            bytes,
            fresh.sim.alive_units().count(),
            session.mission.objectives.len(),
            session.script_booted,
        );
        if let Some(slot) = session.mission.human_slot().or_else(|| human_slot(&fresh)) {
            session.human_slot = Some(slot);
            let sim_bound = usize::from(slot) < SIM_PLAYER_SLOTS;
            crate::dev_log!(
                "CAMPAIGN_HUMAN_SLOT idx={index} name={} slot={slot} sim_bound={sim_bound}",
                session.mission.id
            );
            if sim_bound {
                fresh.player = slot;
            }
            // `GetLocalPlayer()` must answer the mission's human seat: Blizzard.j
            // gates every `CameraSetupApplyForPlayer`, `SetCameraFieldForPlayer`
            // and `TransmissionFromUnitWithNameBJ` block on
            // `GetLocalPlayer() == whichPlayer`, so a hardcoded slot 0 silently
            // dropped Human01's whole opening (its seat is 2).
            if let Some(runtime) = fresh
                .engine
                .as_mut()
                .and_then(|engine| engine.runtime.as_mut())
            {
                runtime.set_local_player(u32::from(slot));
            }
        }
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
}
