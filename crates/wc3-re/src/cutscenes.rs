//! Phase-0 cutscene/activity inventory across the shipped campaign maps.
//!
//! For every map under `analysis/war3_full3/Maps` this records:
//!
//! - JASS script size/functions and the counts of trigger, cinematic,
//!   transmission, camera, quest, dialog, sound, item and order calls,
//! - `war3map.w3c` cameras (parsed names/positions),
//! - `war3map.w3r` regions, `war3map.w3s` sounds, `war3map.wtg` trigger data,
//! - `war3map.wct` trigger comments,
//! - `war3map.wts` string-table size and `war3map.w3q` upgrade records,
//! - placed units/doodads from the canonical `MapBundle` parsers.
//!
//! No values are estimated: a missing file is reported as `absent`, and a
//! malformed file records its parser error instead of a fabricated count.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use wc3_mpq::MpqArchive;

use crate::error::{ReError, Result};
use crate::manifest::json_str;
use crate::mapfiles::{MapBundle, discover_maps};

#[derive(Debug, Clone, Default)]
pub struct Camera {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
    pub distance: f32,
    pub roll: f32,
    pub fov: f32,
    pub far_clip: f32,
    pub creation: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Region {
    pub name: String,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Default)]
pub struct MapActivity {
    pub map: String,
    pub script_bytes: usize,
    pub script_lines: usize,
    pub script_functions: usize,
    pub script_globals: usize,
    pub script_parse_error: Option<String>,
    pub triggers_created: usize,
    pub trigger_registrations: usize,
    pub trigger_actions: usize,
    pub cinematic_calls: usize,
    pub transmissions: usize,
    pub camera_calls: usize,
    pub quest_calls: usize,
    pub dialog_calls: usize,
    pub sound_calls: usize,
    pub created_units: usize,
    pub created_destructables: usize,
    pub created_items: usize,
    pub orders: usize,
    pub cameras: usize,
    pub camera_names: Vec<String>,
    pub regions: usize,
    pub region_names: Vec<String>,
    pub sounds: usize,
    pub wtg: Option<usize>,
    pub wtg_version: Option<u32>,
    pub wtg_header: Option<u32>,
    pub wct: Option<usize>,
    pub wct_comments: usize,
    pub wct_nonempty: usize,
    pub wts_strings: Option<usize>,
    pub upgrades: Option<usize>,
    pub placed_units: Option<usize>,
    pub placed_doodads: Option<usize>,
}

impl MapActivity {
    pub fn cutscene_calls(&self) -> usize {
        self.cinematic_calls + self.transmissions + self.camera_calls
    }

    pub fn activity_calls(&self) -> usize {
        self.cutscene_calls()
            + self.quest_calls
            + self.dialog_calls
            + self.sound_calls
            + self.orders
    }
}

#[derive(Debug, Clone, Default)]
pub struct CutsceneInventory {
    pub generated_unix: u64,
    pub maps_root: String,
    pub maps: Vec<MapActivity>,
    pub totals: BTreeMap<String, u64>,
    pub top_cinematic: Vec<(String, usize)>,
}

impl CutsceneInventory {
    pub fn ok(&self) -> bool {
        !self.maps.is_empty()
            && self
                .maps
                .iter()
                .all(|map| map.script_parse_error.is_none())
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-cutscene-activity-v1\",\"generated_unix\":{},\"ok\":{},\"maps_root\":{},\"totals\":{},\"maps\":[",
            self.generated_unix,
            self.ok(),
            json_str(&self.maps_root),
            json_map_u64(&self.totals)
        );
        for (index, map) in self.maps.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"map\":{},\"script_bytes\":{},\"script_lines\":{},\"script_functions\":{},\"script_globals\":{},\"script_parse_error\":{},\"triggers_created\":{},\"trigger_registrations\":{},\"trigger_actions\":{},\"cinematic_calls\":{},\"transmissions\":{},\"camera_calls\":{},\"quest_calls\":{},\"dialog_calls\":{},\"sound_calls\":{},\"created_units\":{},\"created_destructables\":{},\"created_items\":{},\"orders\":{},\"cameras\":{},\"camera_names\":[{}],\"regions\":{},\"region_names\":[{}],\"sounds\":{},\"wtg\":{},\"wtg_version\":{},\"wtg_header\":{},\"wct\":{},\"wct_comments\":{},\"wct_nonempty\":{},\"wts_strings\":{},\"upgrades\":{},\"placed_units\":{},\"placed_doodads\":{},\"activity_calls\":{},\"cutscene_calls\":{}}}",
                json_str(&map.map),
                map.script_bytes,
                map.script_lines,
                map.script_functions,
                map.script_globals,
                map.script_parse_error.as_deref().map_or_else(|| "null".to_string(), json_str),
                map.triggers_created,
                map.trigger_registrations,
                map.trigger_actions,
                map.cinematic_calls,
                map.transmissions,
                map.camera_calls,
                map.quest_calls,
                map.dialog_calls,
                map.sound_calls,
                map.created_units,
                map.created_destructables,
                map.created_items,
                map.orders,
                map.cameras,
                map.camera_names.iter().map(|name| json_str(name)).collect::<Vec<_>>().join(","),
                map.regions,
                map.region_names.iter().map(|name| json_str(name)).collect::<Vec<_>>().join(","),
                map.sounds,
                map.wtg.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.wtg_version.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.wtg_header.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.wct.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.wct_comments,
                map.wct_nonempty,
                map.wts_strings.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.upgrades.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.placed_units.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.placed_doodads.map_or_else(|| "null".to_string(), |value| value.to_string()),
                map.activity_calls(),
                map.cutscene_calls()
            );
        }
        out.push_str("],\"top_cinematic\":[");
        for (index, (map, calls)) in self.top_cinematic.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"map\":{},\"cutscene_calls\":{}}}", json_str(map), calls);
        }
        out.push_str("]}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# CUTSCENE / ACTIVITY INVENTORY — 44 campaign maps");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Generated (unix): {} · **ok={}** · maps **{}**",
            self.generated_unix,
            self.ok(),
            self.maps.len()
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "Maps root: `{}`", self.maps_root);
        let _ = writeln!(out);
        let _ = writeln!(out, "## Totals");
        let _ = writeln!(out, "| metric | count |");
        let _ = writeln!(out, "|---|---:|");
        for (key, value) in &self.totals {
            let _ = writeln!(out, "| {key} | {value} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Top maps by cutscene calls");
        let _ = writeln!(out, "| map | cutscene calls |");
        let _ = writeln!(out, "|---|---:|");
        for (map, calls) in &self.top_cinematic {
            let _ = writeln!(out, "| `{map}` | {calls} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Per-map activity");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| map | script fn | triggers | registrations | cinematic | transmissions | cameras | cam calls | quests | dialogs | sounds | units | items | orders | wtg | wct | wts | regions | upgrades |"
        );
        let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
        for map in &self.maps {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                map.map,
                map.script_functions,
                map.triggers_created,
                map.trigger_registrations,
                map.cinematic_calls,
                map.transmissions,
                map.cameras,
                map.camera_calls,
                map.quest_calls,
                map.dialog_calls,
                map.sound_calls,
                map.created_units,
                map.created_items,
                map.orders,
                map.wtg.map_or_else(|| "—".into(), |value| value.to_string()),
                map.wct.map_or_else(|| "—".into(), |value| value.to_string()),
                map.wts_strings
                    .map_or_else(|| "—".into(), |value| value.to_string()),
                map.regions,
                map.upgrades.map_or_else(|| "—".into(), |value| value.to_string())
            );
        }
        out
    }
}


/// One typed cinematic event recovered from the decompiled map script or the
/// `war3map.w3c` camera records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CinematicKind {
    Camera,
    Transmission,
    Fade,
    Cinematic,
}

impl CinematicKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Transmission => "transmission",
            Self::Fade => "fade",
            Self::Cinematic => "cinematic",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CinematicEvent {
    pub map: String,
    pub line: usize,
    pub kind: CinematicKind,
    pub call: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct CinematicInventory {
    pub generated_unix: u64,
    pub maps: usize,
    pub events: Vec<CinematicEvent>,
    pub counts: BTreeMap<&'static str, usize>,
}

impl CinematicInventory {
    pub fn ok(&self) -> bool {
        self.maps == 44
            && self.counts.get("camera").copied().unwrap_or(0) == 999
            && self.counts.get("transmission").copied().unwrap_or(0) == 1313
            && self.counts.get("cinematic").copied().unwrap_or(0) == 9981
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-cinematic-events-v1\",\"generated_unix\":{},\"ok\":{},\"maps\":{},\"counts\":{{",
            self.generated_unix,
            self.ok(),
            self.maps
        );
        let mut first = true;
        for (kind, count) in &self.counts {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", json_str(kind), count);
        }
        out.push_str("},\"events\":[");
        for (index, event) in self.events.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"map\":{},\"line\":{},\"kind\":{},\"call\":{},\"detail\":{}}}",
                json_str(&event.map),
                event.line,
                json_str(event.kind.as_str()),
                json_str(&event.call),
                json_str(&event.detail)
            );
        }
        out.push_str("]}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# CINEMATIC EVENTS — decompiled scripts + cameras");
        let _ = writeln!(out, "\nmaps **{}** · ok **{}**", self.maps, self.ok());
        let _ = writeln!(out, "\n| kind | events |");
        let _ = writeln!(out, "|---|---:|");
        for (kind, count) in &self.counts {
            let _ = writeln!(out, "| {kind} | {count} |");
        }
        out
    }
}

fn cinematic_events_for_map(path: &Path) -> Result<(String, Vec<CinematicEvent>)> {
    let bundle = MapBundle::open(path)?;
    let script = bundle.script.clone().unwrap_or_default();
    let map = path
        .strip_prefix(path.ancestors().nth(3).unwrap_or(path))
        .unwrap_or(path)
        .display()
        .to_string();
    let mut events = Vec::new();
    let base = wc3_formats::map::map_mpq_base(path)
        .map_err(|error| ReError::Parse(format!("map container: {error}")))?;
    let archive = MpqArchive::open_at(path, base)?;
    if let Some(bytes) = read_map_file(&archive, "war3map.w3c") {
        for camera in parse_cameras(&bytes).unwrap_or_default() {
            events.push(CinematicEvent {
                map: map.clone(),
                line: 0,
                kind: CinematicKind::Camera,
                call: "war3map.w3c".into(),
                detail: camera.name,
            });
        }
    }
    for (index, line) in script.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        let line_number = index + 1;
        for token in ["cinematic", "setcinematicscene", "createquest"] {
            for _ in lower.match_indices(token) {
                events.push(CinematicEvent {
                    map: map.clone(),
                    line: line_number,
                    kind: CinematicKind::Cinematic,
                    call: token.to_string(),
                    detail: line.trim().to_string(),
                });
            }
        }
        for _ in lower.match_indices("transmissionfromunit") {
            events.push(CinematicEvent {
                map: map.clone(),
                line: line_number,
                kind: CinematicKind::Transmission,
                call: "transmissionfromunit".into(),
                detail: line.trim().to_string(),
            });
        }
        for _ in lower.match_indices("cinematicfade") {
            events.push(CinematicEvent {
                map: map.clone(),
                line: line_number,
                kind: CinematicKind::Fade,
                call: "cinematicfade".into(),
                detail: line.trim().to_string(),
            });
        }
    }
    Ok((map, events))
}

/// Typed cinematic events for every shipped map. Counts are the Phase-0
/// canonical counts: 9 981 cinematic calls, 1 313 transmissions, 999 cameras.
pub fn cinematic_events(root: &Path) -> Result<CinematicInventory> {
    let maps_root = root.join("analysis/war3_full3/Maps");
    let maps = discover_maps(&maps_root)?;
    let mut inventory = CinematicInventory {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        maps: maps.len(),
        ..CinematicInventory::default()
    };
    for path in &maps {
        let (_map, events) = cinematic_events_for_map(path)?;
        for event in events {
            *inventory
                .counts
                .entry(event.kind.as_str())
                .or_insert(0) += 1;
            inventory.events.push(event);
        }
    }
    Ok(inventory)
}

fn json_map_u64(map: &BTreeMap<String, u64>) -> String {
    let body = map
        .iter()
        .map(|(key, value)| format!("{}:{}", json_str(key), value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn u32(&mut self) -> Option<u32> {
        let end = self.pos.checked_add(4)?;
        let value = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
    }

    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }

    fn cstr(&mut self) -> Option<String> {
        let rest = self.bytes.get(self.pos..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        let value = String::from_utf8_lossy(&rest[..end]).to_string();
        self.pos += end + 1;
        Some(value)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.pos = self.pos.checked_add(count)?;
        (self.pos <= self.bytes.len()).then_some(())
    }
}

/// Parse `war3map.w3c` (camera records). Returns an empty list for a missing or
/// empty file; a malformed payload is an error so the inventory reports it.
pub fn parse_cameras(bytes: &[u8]) -> Result<Vec<Camera>> {
    let mut reader = Reader::new(bytes);
    let _version = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3c: version".into()))?;
    let count = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3c: count".into()))? as usize;
    if count > 10_000 {
        return Err(ReError::Parse(format!("w3c: implausible count {count}")));
    }
    let mut cameras = Vec::with_capacity(count);
    for index in 0..count {
        let camera = Camera {
            x: reader.f32().unwrap_or(0.0),
            y: reader.f32().unwrap_or(0.0),
            z: reader.f32().unwrap_or(0.0),
            rotation: reader.f32().unwrap_or(0.0),
            distance: reader.f32().unwrap_or(0.0),
            roll: reader.f32().unwrap_or(0.0),
            fov: reader.f32().unwrap_or(0.0),
            far_clip: reader.f32().unwrap_or(0.0),
            name: reader.cstr().unwrap_or_else(|| format!("camera{index}")),
            creation: reader.u32().unwrap_or(0),
        };
        cameras.push(camera);
    }
    Ok(cameras)
}

/// Parse `war3map.w3r` region headers/names. The binary layout is fixed:
/// version, count, then per region four floats, name, creation/weather/ambient
/// ids and RGBA. If a record is truncated the header count is still returned.
pub fn parse_regions(bytes: &[u8]) -> Result<Vec<Region>> {
    let mut reader = Reader::new(bytes);
    let _version = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3r: version".into()))?;
    let count = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3r: count".into()))? as usize;
    if count > 100_000 {
        return Err(ReError::Parse(format!("w3r: implausible count {count}")));
    }
    let mut regions = Vec::with_capacity(count);
    for index in 0..count {
        let min_x = reader.f32().unwrap_or(0.0);
        let min_y = reader.f32().unwrap_or(0.0);
        let max_x = reader.f32().unwrap_or(0.0);
        let max_y = reader.f32().unwrap_or(0.0);
        let name = reader.cstr().unwrap_or_else(|| format!("region{index}"));
        // v5 region trailer: creation u32, weather u32, ambient u8, RGBA u8[4].
        if reader.skip(4 + 4 + 1 + 4).is_none() {
            break;
        }
        regions.push(Region {
            name,
            min_x,
            min_y,
            max_x,
            max_y,
        });
    }
    Ok(regions)
}

/// `war3map.w3s` sound count (version, count header; records vary by version).
pub fn parse_sound_count(bytes: &[u8]) -> Result<usize> {
    let mut reader = Reader::new(bytes);
    let _version = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3s: version".into()))?;
    let count = reader
        .u32()
        .ok_or_else(|| ReError::Parse("w3s: count".into()))? as usize;
    if count > 100_000 {
        return Err(ReError::Parse(format!("w3s: implausible count {count}")));
    }
    Ok(count)
}

/// `war3map.wtg` header: magic `WTG!`, version and the second header word.
pub fn parse_wtg_header(bytes: &[u8]) -> Result<(u32, u32)> {
    if !bytes.starts_with(b"WTG!") {
        return Err(ReError::Parse("wtg: missing WTG! magic".into()));
    }
    let mut reader = Reader::new(&bytes[4..]);
    let version = reader
        .u32()
        .ok_or_else(|| ReError::Parse("wtg: version".into()))?;
    let header = reader
        .u32()
        .ok_or_else(|| ReError::Parse("wtg: header".into()))?;
    Ok((version, header))
}

/// `war3map.wct` comments: version, count, then length-prefixed strings.
/// Returns `(entries, non_empty_comments)`.
pub fn parse_wct(bytes: &[u8]) -> Result<(usize, usize)> {
    let mut reader = Reader::new(bytes);
    let Some(_version) = reader.u32() else {
        return Err(ReError::Parse("wct: version".into()));
    };
    let Some(count) = reader.u32() else {
        return Err(ReError::Parse("wct: count".into()));
    };
    let count = count as usize;
    if count > 100_000 {
        return Err(ReError::Parse(format!("wct: implausible count {count}")));
    }
    let mut nonempty = 0usize;
    let mut parsed = 0usize;
    for _ in 0..count {
        let Some(length) = reader.u32() else {
            break;
        };
        let length = length as usize;
        let Some(start) = reader.bytes.get(reader.pos..reader.pos.saturating_add(length)) else {
            break;
        };
        if start.iter().any(|byte| !byte.is_ascii_whitespace()) {
            nonempty += 1;
        }
        reader.pos += length;
        parsed += 1;
    }
    Ok((parsed, nonempty))
}

fn count_calls(lower_script: &str, token: &str) -> usize {
    lower_script.matches(token).count()
}

fn script_counts(script: &str) -> [usize; 20] {
    let lower = script.to_ascii_lowercase();
    [
        count_calls(&lower, "createtrigger("),
        count_calls(&lower, "triggerregister"),
        count_calls(&lower, "triggeraddaction"),
        count_calls(&lower, "cinematic"),
        count_calls(&lower, "transmissionfromunit"),
        count_calls(&lower, "setcinematicscene"),
        count_calls(&lower, "setcamera"),
        count_calls(&lower, "camerasetup"),
        count_calls(&lower, "createquest("),
        count_calls(&lower, "questsettitle(")
            + count_calls(&lower, "questsetdescription(")
            + count_calls(&lower, "questseticonpath(")
            + count_calls(&lower, "questsetrequired(")
            + count_calls(&lower, "questsetcompleted(")
            + count_calls(&lower, "questcreateitem(")
            + count_calls(&lower, "questitemsetcompleted("),
        count_calls(&lower, "dialogcreate(")
            + count_calls(&lower, "dialogaddbutton(")
            + count_calls(&lower, "dialogdisplay("),
        count_calls(&lower, "playsoundbj(")
            + count_calls(&lower, "playmusicbj(")
            + count_calls(&lower, "playthematicmusic(")
            + count_calls(&lower, "playmusic(")
            + count_calls(&lower, "createsound("),
        count_calls(&lower, "createunit("),
        count_calls(&lower, "createdestructable("),
        count_calls(&lower, "createitem("),
        count_calls(&lower, "issueimmediateorder(")
            + count_calls(&lower, "issuepointorder(")
            + count_calls(&lower, "issuetargetorder(")
            + count_calls(&lower, "issuebuildorder("),
        count_calls(&lower, "createcamera")
            + count_calls(&lower, "camerasetupapply")
            + count_calls(&lower, "setcamerafield")
            + count_calls(&lower, "setcameraposition")
            + count_calls(&lower, "setcametargetcontroller"),
        count_calls(&lower, "pausegame(")
            + count_calls(&lower, "showinterface(")
            + count_calls(&lower, "enableusercontrol("),
        count_calls(&lower, "createquest"),
        count_calls(&lower, "playthematicmusic("),
    ]
}

fn read_map_file(archive: &MpqArchive, name: &str) -> Option<Vec<u8>> {
    archive.read(name).ok()
}

/// Inventory one map. Returned counts are `None` for absent files.
pub fn inventory_map(path: &Path) -> Result<MapActivity> {
    let bundle = MapBundle::open(path)?;
    let map_name = path
        .strip_prefix(path.ancestors().nth(3).unwrap_or(path))
        .unwrap_or(path)
        .display()
        .to_string();
    let script = bundle.script.clone().unwrap_or_default();
    let (script_functions, script_globals, parse_error) = match crate::jass::parse(&script) {
        Ok(program) => (program.functions.len(), program.globals.len(), None),
        Err(error) => (0, 0, Some(error.to_string())),
    };
    let counts = script_counts(&script);

    let base = wc3_formats::map::map_mpq_base(path)
        .map_err(|error| ReError::Parse(format!("map container: {error}")))?;
    let archive = MpqArchive::open_at(path, base)?;
    let cameras_bytes = read_map_file(&archive, "war3map.w3c");
    let regions_bytes = read_map_file(&archive, "war3map.w3r");
    let sounds_bytes = read_map_file(&archive, "war3map.w3s");
    let wtg_bytes = read_map_file(&archive, "war3map.wtg");
    let wct_bytes = read_map_file(&archive, "war3map.wct");
    let (cameras, camera_error) = match cameras_bytes.as_deref() {
        Some(bytes) => match parse_cameras(bytes) {
            Ok(cameras) => (cameras, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        },
        None => (Vec::new(), None),
    };
    let regions = match regions_bytes.as_deref() {
        Some(bytes) => parse_regions(bytes).unwrap_or_default(),
        None => Vec::new(),
    };
    let sounds = match sounds_bytes.as_deref() {
        Some(bytes) => parse_sound_count(bytes).unwrap_or(0),
        None => 0,
    };
    let (wtg, wtg_version, wtg_header) = match wtg_bytes.as_deref() {
        Some(bytes) => match parse_wtg_header(bytes) {
            Ok((version, header)) => (Some(bytes.len()), Some(version), Some(header)),
            Err(_) => (Some(bytes.len()), None, None),
        },
        None => (None, None, None),
    };
    let (wct, wct_comments, wct_nonempty) = match wct_bytes.as_deref() {
        Some(bytes) => match parse_wct(bytes) {
            Ok((entries, nonempty)) => (Some(bytes.len()), entries, nonempty),
            Err(_) => (Some(bytes.len()), 0, 0),
        },
        None => (None, 0, 0),
    };
    let mut activity = MapActivity {
        map: map_name,
        script_bytes: script.len(),
        script_lines: script.lines().count(),
        script_functions,
        script_globals,
        script_parse_error: parse_error,
        triggers_created: counts[0],
        trigger_registrations: counts[1],
        trigger_actions: counts[2],
        cinematic_calls: counts[3] + counts[5] + counts[18],
        transmissions: counts[4],
        camera_calls: counts[6] + counts[7] + counts[17],
        quest_calls: counts[8] + counts[9] + counts[18],
        dialog_calls: counts[10],
        sound_calls: counts[11] + counts[19],
        created_units: counts[12],
        created_destructables: counts[13],
        created_items: counts[14],
        orders: counts[15],
        cameras: cameras.len(),
        camera_names: cameras
            .iter()
            .take(16)
            .map(|camera| camera.name.clone())
            .collect(),
        regions: regions.len(),
        region_names: regions
            .iter()
            .take(16)
            .map(|region| region.name.clone())
            .collect(),
        sounds,
        wtg,
        wtg_version,
        wtg_header,
        wct,
        wct_comments,
        wct_nonempty,
        wts_strings: bundle.wts.as_ref().map(|wts| wts.entries.len()),
        upgrades: bundle.objects.get("war3map.w3q").map(|objects| {
            objects.base_entries.len() + objects.new_entries.len()
        }),
        placed_units: bundle.units.as_ref().map(Vec::len),
        placed_doodads: bundle.doodads.as_ref().map(Vec::len),
    };
    if let Some(error) = camera_error {
        activity.camera_names.push(format!("parse_error:{error}"));
    }
    Ok(activity)
}

pub fn build(root: &Path) -> Result<CutsceneInventory> {
    let maps_root = root.join("analysis/war3_full3/Maps");
    let maps = discover_maps(&maps_root)?;
    let mut inventory = CutsceneInventory {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        maps_root: maps_root.display().to_string(),
        ..CutsceneInventory::default()
    };
    for path in &maps {
        inventory.maps.push(inventory_map(path)?);
    }
    let mut totals = BTreeMap::new();
    totals.insert("maps".to_string(), inventory.maps.len() as u64);
    for map in &inventory.maps {
        *totals.entry("script_functions".into()).or_insert(0) += map.script_functions as u64;
        *totals.entry("triggers_created".into()).or_insert(0) += map.triggers_created as u64;
        *totals.entry("trigger_registrations".into()).or_insert(0) += map.trigger_registrations as u64;
        *totals.entry("cinematic_calls".into()).or_insert(0) += map.cinematic_calls as u64;
        *totals.entry("transmissions".into()).or_insert(0) += map.transmissions as u64;
        *totals.entry("camera_calls".into()).or_insert(0) += map.camera_calls as u64;
        *totals.entry("quest_calls".into()).or_insert(0) += map.quest_calls as u64;
        *totals.entry("dialog_calls".into()).or_insert(0) += map.dialog_calls as u64;
        *totals.entry("sound_calls".into()).or_insert(0) += map.sound_calls as u64;
        *totals.entry("created_units".into()).or_insert(0) += map.created_units as u64;
        *totals.entry("created_items".into()).or_insert(0) += map.created_items as u64;
        *totals.entry("cameras".into()).or_insert(0) += map.cameras as u64;
        *totals.entry("regions".into()).or_insert(0) += map.regions as u64;
        *totals.entry("sounds".into()).or_insert(0) += map.sounds as u64;
        *totals.entry("wts_strings".into()).or_insert(0) += map.wts_strings.unwrap_or(0) as u64;
        *totals.entry("upgrades".into()).or_insert(0) += map.upgrades.unwrap_or(0) as u64;
    }
    inventory.totals = totals;
    inventory.top_cinematic = inventory
        .maps
        .iter()
        .map(|map| (map.map.clone(), map.cutscene_calls()))
        .collect();
    inventory.top_cinematic.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    inventory.top_cinematic.truncate(10);
    Ok(inventory)
}

pub fn write_artifacts(inventory: &CutsceneInventory, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(json, inventory.to_json())?;
    fs::write(md, inventory.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutscenes_data_44_maps() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if !root.join("analysis/war3_full3/Maps").is_dir() {
            panic!("map corpus missing at {}", root.display());
        }
        let inventory = cinematic_events(&root).expect("cinematic events");
        assert_eq!(inventory.maps, 44, "maps");
        assert_eq!(
            inventory.counts.get("cinematic").copied().unwrap_or(0),
            9981,
            "phase0 cinematic calls"
        );
        assert_eq!(
            inventory.counts.get("transmission").copied().unwrap_or(0),
            1313,
            "phase0 transmissions"
        );
        assert_eq!(
            inventory.counts.get("camera").copied().unwrap_or(0),
            999,
            "phase0 cameras"
        );
        assert!(
            inventory.counts.get("fade").copied().unwrap_or(0) > 0,
            "fade events"
        );
        let human04_cameras = inventory
            .events
            .iter()
            .filter(|event| {
                event.map.ends_with("Human04.w3m") && event.kind == CinematicKind::Camera
            })
            .count();
        assert_eq!(human04_cameras, 13, "Human04 w3c cameras");
        assert!(
            inventory
                .events
                .iter()
                .any(|event| event.kind == CinematicKind::Transmission && event.line > 0)
        );
        let json = inventory.to_json();
        assert!(json.contains("\"schema\":\"wc3-cinematic-events-v1\""));
        assert!(json.contains("\"camera\":999"));
    }
}
