//! Consumption cross-check for every listfile entry in the operator's MPQs.
//!
//! Each entry is classified by the **actual consumer code** in this workspace:
//! `runtime` (a crate/API loads that format during gameplay), `data_only`
//! (typed data/cinematics/scan API but not the gameplay runtime), or `unused`
//! (no consumer; e.g. host DLLs). The `consumer`/`evidence` strings name the
//! implementation file and line so the table is auditable, not assumed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use wc3_mpq::MpqArchive;

use crate::error::Result;
use crate::manifest::json_str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumptionClass {
    Runtime,
    DataOnly,
    Unused,
}

impl ConsumptionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::DataOnly => "data_only",
            Self::Unused => "unused",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsumptionEntry {
    pub archive: String,
    pub name: String,
    pub size: u32,
    pub format: &'static str,
    pub class: ConsumptionClass,
    pub consumer: &'static str,
    pub evidence: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct ConsumptionReport {
    pub generated_unix: u64,
    pub archives: usize,
    pub total: usize,
    pub by_class: BTreeMap<&'static str, usize>,
    pub by_format: BTreeMap<&'static str, usize>,
    pub entries: Vec<ConsumptionEntry>,
}

impl ConsumptionReport {
    pub fn ok(&self) -> bool {
        self.total > 0
            && self.by_class.values().sum::<usize>() == self.total
            && self.by_class.get("runtime").copied().unwrap_or(0) > 0
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-consumption-v1\",\"generated_unix\":{},\"archives\":{},\"total\":{},\"ok\":{},\"by_class\":{{",
            self.generated_unix, self.archives, self.total, self.ok()
        );
        let mut first = true;
        for (key, value) in &self.by_class {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", json_str(key), value);
        }
        out.push_str("},\"by_format\":{");
        let mut first = true;
        for (key, value) in &self.by_format {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", json_str(key), value);
        }
        out.push_str("},\"entries\":[");
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"archive\":{},\"name\":{},\"size\":{},\"format\":{},\"class\":{},\"consumer\":{},\"evidence\":{}}}",
                json_str(&entry.archive),
                json_str(&entry.name),
                entry.size,
                json_str(entry.format),
                json_str(entry.class.as_str()),
                json_str(entry.consumer),
                json_str(entry.evidence)
            );
        }
        out.push_str("]}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# CONSUMPTION CROSS-CHECK — every MPQ listfile entry");
        let _ = writeln!(
            out,
            "\narchives **{}** · entries **{}** · runtime **{}** · data-only **{}** · unused **{}**",
            self.archives,
            self.total,
            self.by_class.get("runtime").copied().unwrap_or(0),
            self.by_class.get("data_only").copied().unwrap_or(0),
            self.by_class.get("unused").copied().unwrap_or(0)
        );
        let _ = writeln!(out, "\n## By format");
        let _ = writeln!(out, "| format | entries |");
        let _ = writeln!(out, "|---|---:|");
        for (format, count) in &self.by_format {
            let _ = writeln!(out, "| {format} | {count} |");
        }
        let _ = writeln!(out, "\n## Non-runtime entries");
        let _ = writeln!(out, "| archive | name | class | consumer |");
        let _ = writeln!(out, "|---|---|---|---|");
        for entry in self.entries.iter().filter(|entry| entry.class != ConsumptionClass::Runtime) {
            let _ = writeln!(
                out,
                "| `{}` | `{}` | {} | {} |",
                entry.archive,
                entry.name,
                entry.class.as_str(),
                entry.consumer
            );
        }
        out
    }
}

fn lower(name: &str) -> String {
    name.replace('/', "\\").to_ascii_lowercase()
}

fn runtime_slk(path: &str) -> bool {
    [
        "units\\unitdata.slk",
        "units\\unitbalance.slk",
        "units\\unitweapons.slk",
        "units\\unitabilities.slk",
        "units\\unitui.slk",
        "units\\abilitydata.slk",
        "units\\itemdata.slk",
        "units\\upgradedata.slk",
        "units\\destructabledata.slk",
        "units\\abilitymetadata.slk",
        "units\\unitmetadata.slk",
        "units\\upgrademetadata.slk",
        "units\\upgradeeffectmetadata.slk",
        "units\\miscmetadata.slk",
        "units\\unitmetadata.slk",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn runtime_txt(path: &str) -> bool {
    path.ends_with("ui\\worldeditstrings.txt")
        || path.ends_with("ui\\commandstrings.txt")
        || path.ends_with("ui\\commandfunc.txt")
        || path.ends_with("ui\\miscdata.txt")
        || (path.starts_with("units\\")
            && (path.ends_with("func.txt") || path.ends_with("strings.txt")))
        || (path.starts_with("units\\") && path.contains("abilityfunc.txt"))
}

fn classify(name: &str) -> (&'static str, ConsumptionClass, &'static str, &'static str) {
    let path = lower(name);
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "blp" => (
            "BLP",
            ConsumptionClass::Runtime,
            "wc3-game assets texture loader; wc3-re::blp",
            "crates/wc3-game/src/assets.rs:16; crates/wc3-re/src/blp.rs:225",
        ),
        "mdx" | "mdl" => (
            "MDX",
            ConsumptionClass::Runtime,
            "wc3-game model_render/assets; wc3-re::mdx",
            "crates/wc3-game/src/model_render.rs:20; crates/wc3-game/src/assets.rs:18",
        ),
        "slk" => {
            if runtime_slk(&path) {
                (
                    "SLK",
                    ConsumptionClass::Runtime,
                    "wc3-data GameDatabase typed tables",
                    "crates/wc3-data/src/lib.rs:196; crates/wc3-re/src/catalog.rs:database",
                )
            } else {
                (
                    "SLK",
                    ConsumptionClass::DataOnly,
                    "wc3-re::slk scanner",
                    "crates/wc3-re/src/slk.rs:26",
                )
            }
        }
        "txt" => {
            if runtime_txt(&path) {
                (
                    "TXT",
                    ConsumptionClass::Runtime,
                    "wc3-data profiles/localizer",
                    "crates/wc3-data/src/lib.rs:196; crates/wc3-game/src/assets.rs:507",
                )
            } else {
                (
                    "TXT",
                    ConsumptionClass::DataOnly,
                    "wc3-re::txt scanner",
                    "crates/wc3-re/src/txt.rs:48",
                )
            }
        }
        "fdf" | "toc" => (
            "FDF",
            ConsumptionClass::Runtime,
            "wc3-formats::fdf; wc3-game ui",
            "crates/wc3-formats/src/fdf.rs:504; crates/wc3-game/src/ui.rs",
        ),
        "wav" => (
            "WAV",
            ConsumptionClass::Runtime,
            "wc3-audio GameAudio",
            "crates/wc3-audio/src/game_audio.rs:94",
        ),
        "mp3" => (
            "MP3",
            ConsumptionClass::Runtime,
            "wc3-audio GameAudio",
            "crates/wc3-audio/src/game_audio.rs:94",
        ),
        "j" | "ai" => (
            "JASS",
            ConsumptionClass::Runtime,
            "wc3-re::jass / Engine::enter_map",
            "crates/wc3-re/src/jass/vm.rs:599; crates/wc3-re/src/engine.rs:240",
        ),
        "w3m" | "w3x" => (
            "MAP",
            ConsumptionClass::Runtime,
            "wc3-re::mapfiles / Engine",
            "crates/wc3-re/src/mapfiles.rs:87; crates/wc3-re/src/engine.rs:240",
        ),
        "w3e" | "w3i" | "wpm" | "doo" => (
            "MAPDATA",
            ConsumptionClass::Runtime,
            "wc3-re::mapfiles + terrain/pathing",
            "crates/wc3-re/src/mapfiles.rs:134; crates/wc3-formats/src/map.rs:102",
        ),
        "w3u" | "w3t" | "w3a" | "w3b" | "w3d" | "w3h" | "w3q" => (
            "OBJDATA",
            ConsumptionClass::Runtime,
            "wc3-re::mapfiles object data",
            "crates/wc3-re/src/mapfiles.rs:154",
        ),
        "wts" => (
            "WTS",
            ConsumptionClass::Runtime,
            "wc3-re::wts map strings",
            "crates/wc3-re/src/wts.rs:18",
        ),
        "w3c" | "w3r" | "w3s" | "wtg" | "wct" => (
            "CUTSCENE",
            ConsumptionClass::DataOnly,
            "wc3-re::cutscenes / mapcontent",
            "crates/wc3-re/src/cutscenes.rs:408; crates/wc3-re/src/mapcontent.rs",
        ),
        "w3n" => (
            "CAMPAIGN",
            ConsumptionClass::DataOnly,
            "wc3-re::mapfiles listfile only",
            "crates/wc3-re/src/mapfiles.rs:49",
        ),
        "dls" | "mid" => (
            "AUDIO_DATA",
            ConsumptionClass::DataOnly,
            "DirectMusic/ambient data; no game runtime decoder",
            "none",
        ),
        "mrf" => (
            "MRF",
            ConsumptionClass::DataOnly,
            "model reference list; not read by the runtime",
            "none",
        ),
        "ifl" => (
            "IFL",
            ConsumptionClass::DataOnly,
            "water texture flip list; renderer uses BLP atlas",
            "crates/wc3-game/src/terrain_render.rs",
        ),
        "ttf" => (
            "FONT",
            ConsumptionClass::Runtime,
            "wc3-game font loader",
            "crates/wc3-game/src/assets.rs load_game_font",
        ),
        "pld" => (
            "PLD",
            ConsumptionClass::DataOnly,
            "credits preload (no gameplay consumer)",
            "crates/wc3-re/src/jass/natives.rs preloader",
        ),
        "tga" | "dds" | "png" | "jpg" => (
            "IMAGE",
            ConsumptionClass::DataOnly,
            "manifest/scan only; runtime uses BLP",
            "crates/wc3-re/src/scan.rs",
        ),
        "mpq" => (
            "MPQ",
            ConsumptionClass::DataOnly,
            "nested archive metadata only",
            "crates/wc3-re/src/manifest.rs:252",
        ),
        "dll" | "exe" => (
            "PE",
            ConsumptionClass::Unused,
            "host/installer binary, not game data",
            "none",
        ),
        "bin" | "dat" | "w3g" => (
            "BIN",
            ConsumptionClass::Unused,
            "no game-data consumer",
            "none",
        ),
        _ => (
            "UNKNOWN",
            ConsumptionClass::DataOnly,
            "no dedicated runtime consumer",
            "none",
        ),
    }
}

pub fn build(root: &Path) -> Result<ConsumptionReport> {
    let archives = crate::manifest::default_archives(root);
    let mut report = ConsumptionReport {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        archives: archives.len(),
        ..ConsumptionReport::default()
    };
    for path in &archives {
        let archive = MpqArchive::open(path)
            .map_err(|error| crate::error::ReError::Parse(format!("consumption {}: {error}", path.display())))?;
        let label = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        for name in archive.listfile() {
            let size = archive.meta(name).map_or(0, |(_, meta)| meta.size);
            let (format, class, consumer, evidence) = classify(name);
            *report.by_class.entry(class.as_str()).or_insert(0) += 1;
            *report.by_format.entry(format).or_insert(0) += 1;
            report.total += 1;
            report.entries.push(ConsumptionEntry {
                archive: label.clone(),
                name: name.clone(),
                size,
                format,
                class,
                consumer,
                evidence,
            });
        }
    }
    Ok(report)
}

pub fn write_artifacts(report: &ConsumptionReport, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(json, report.to_json())?;
    std::fs::write(md, report.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_mpq_consumption_covers_7451_entries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = build(&root).expect("consumption report");
        assert_eq!(report.total, 7_451, "listfile entries");
        assert_eq!(report.archives, 18, "archives");
        assert_eq!(report.by_class.values().sum::<usize>(), report.total);
        assert!(report.by_class.get("runtime").copied().unwrap_or(0) > 0);
        assert!(report.by_class.get("data_only").copied().unwrap_or(0) > 0);
        assert!(report.entries.iter().all(|entry| entry.format != "UNKNOWN"));
        let json = report.to_json();
        assert!(json.contains("\"schema\":\"wc3-consumption-v1\""));
        assert!(json.contains("\"total\":7451"));
    }
}
