//! Headless per-def voice probe: which `UnitAckSounds` row and which archive
//! file does each reference unit actually speak?
//!
//! The probe opens the real layered install (`War3xlocal > War3x > War3`),
//! loads the retail `SoundInfo` tables plus `Units/unitUI.slk`, and then runs
//! the **hook path** (`GameAudio::unit_ack`) for every reference def. A
//! recording provider captures the path the hook resolved, so the output is
//! end-to-end evidence:
//!
//! ```text
//! VOICE_ROW  def=hpea set=Peasant row=PeasantWhat table=UnitAckSounds ...
//! VOICE_PATH def=hpea row=PeasantWhat file=Units\Human\Peasant\PeasantWhat2.wav
//! ```
//!
//! Reference set: the eight Orc melee defs the bug report named (peon, grunt,
//! headhunter, shaman, tauren, kodo, wind rider, spirit walker) plus the eight
//! Human counterparts. If two different rows resolve to the same file the
//! probe prints `VOICE_COLLISION` and exits 1 — that is the "peasant sounds
//! like an orc" defect: a shared/fallback voice instead of the def's own
//! `UnitUI.unitSound` entry.
//!
//! Usage: `unit_voice_paths [--install DIR]`

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::AssetDb;
use wc3_audio::engine::{AudioConfig, AudioEngine};
use wc3_audio::game_audio::GameAudio;
use wc3_audio::{Sound, SoundProvider};

/// Orc reference defs named by the operator bug (peon .. spirit walker).
const ORC_DEFS: [(&str, &str); 8] = [
    ("opeo", "Peon"),
    ("ogru", "Grunt"),
    ("ohun", "HeadHunter"),
    ("oshm", "Shaman"),
    ("otau", "Tauren"),
    ("okod", "KotoBeast"),
    ("owyv", "WyvernRider"),
    ("ospw", "SpiritWalker"),
];

/// Human counterparts: worker, melee, ranged, casters, siege, air.
const HUMAN_DEFS: [(&str, &str); 8] = [
    ("hpea", "Peasant"),
    ("hfoo", "Footman"),
    ("hrif", "Rifleman"),
    ("hmpr", "Priest"),
    ("hsor", "Sorceress"),
    ("hkni", "Knight"),
    ("hmtm", "MortarTeam"),
    ("hgry", "GryphonRider"),
];

/// `AssetDb` provider that records the archive path every started clip loaded.
struct RecordingProvider {
    db: AssetDb,
    last: Option<String>,
}

impl SoundProvider for RecordingProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        let sound = SoundProvider::load_sound(&mut self.db, path)?;
        self.last = Some(path.to_string());
        Some(sound)
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        SoundProvider::load_bytes(&mut self.db, path)
    }

    fn exists(&self, path: &str) -> bool {
        SoundProvider::exists(&self.db, path)
    }

    fn resolve(&self, path: &str) -> Option<String> {
        SoundProvider::resolve(&self.db, path)
    }
}

fn headless() -> AudioEngine {
    AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 16,
        master_gain: 0.8,
        ..AudioConfig::default()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut install = PathBuf::from("analysis/agent-assets/wc3-install");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--install" => {
                index += 1;
                install = PathBuf::from(args.get(index).ok_or("--install needs a path")?);
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        index += 1;
    }

    let layers: Vec<(String, PathBuf)> = ["War3xlocal.mpq", "War3x.mpq", "War3.mpq"]
        .iter()
        .map(|name| {
            (
                name.trim_end_matches(".mpq").to_string(),
                install.join(name),
            )
        })
        .filter(|(_, path)| path.is_file())
        .collect();
    let mut db = if layers.is_empty() {
        AssetDb::open_mpq(install.join("War3.mpq"))?
    } else {
        AssetDb::open_mpq_layers(&layers, "wc3-install")?
    };
    let (sounds, units, tables) = load_sound_tables(&mut db);
    println!(
        "VOICE_WIRING source={} layers={:?} tables={} defs={} units={}",
        db.source_label(),
        db.layer_labels(),
        tables,
        sounds.len(),
        units.len()
    );

    let mut audio = GameAudio::new(
        headless(),
        sounds,
        units,
        RecordingProvider { db, last: None },
    );

    let mut failures = 0usize;
    let mut resolved: Vec<(String, String, String)> = Vec::new();
    let mut by_file: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut orc_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for (group, defs) in [("orc", ORC_DEFS), ("human", HUMAN_DEFS)] {
        for (def, expected_set) in defs {
            let Some((set, row)) = audio.unit_ack_definition(def) else {
                println!("VOICE_MISSING def={def} set={expected_set} reason=no_unitSound_row");
                failures += 1;
                continue;
            };
            println!(
                "VOICE_ROW def={def} group={group} set={set} expected_set={expected_set} row={} table={} dir=\"{}\" files={} cutoff={:.0}",
                row.name,
                row.source,
                row.directory,
                row.files.len(),
                row.audible_cutoff()
            );
            audio.provider_mut().last = None;
            // `None` position: the probe measures the mapping, not the gate.
            let Some(voice) = audio.unit_ack(def, None) else {
                println!(
                    "VOICE_MISSING def={def} group={group} set={set} row={} reason=no_file_started",
                    row.name
                );
                failures += 1;
                continue;
            };
            let Some(file) = audio.provider().last.clone() else {
                println!(
                    "VOICE_MISSING def={def} group={group} row={} reason=provider_did_not_record",
                    row.name
                );
                failures += 1;
                continue;
            };
            println!("VOICE_PATH def={def} group={group} row={} file={file}", row.name);
            if set != expected_set {
                println!(
                    "VOICE_SET_MISMATCH def={def} group={group} got={set} expected={expected_set}"
                );
                failures += 1;
            }
            let key = file.to_ascii_lowercase();
            if let Some((other_def, other_row)) = by_file.get(&key) {
                if !other_row.eq_ignore_ascii_case(&row.name) {
                    println!(
                        "VOICE_COLLISION def={def} row={} file={file} also_def={other_def} also_row={other_row}",
                        row.name
                    );
                    failures += 1;
                }
            } else {
                by_file.insert(key, (def.to_string(), row.name.clone()));
            }
            if group == "orc" {
                orc_files.insert(file.to_ascii_lowercase());
            }
            resolved.push((def.to_string(), row.name.clone(), file));
            let _ = voice;
            audio.render(&mut vec![0.0f32; 512 * 2]);
        }
    }

    let rows: std::collections::BTreeSet<&str> =
        resolved.iter().map(|(_, row, _)| row.as_str()).collect();
    let files: std::collections::BTreeSet<String> =
        resolved.iter().map(|(_, _, file)| file.to_ascii_lowercase()).collect();
    println!(
        "VOICE_DISTINCT defs={} rows={} files={} collisions={}",
        resolved.len(),
        rows.len(),
        files.len(),
        resolved.len().saturating_sub(files.len())
    );
    println!(
        "VOICE_ORC_DISTINCT defs={} files={}",
        ORC_DEFS.len(),
        orc_files.len()
    );

    // Distance gate evidence: the rows' own `DistanceCutoff` (retail acks
    // 100000, UnitCombatSounds 2100) — the probe does not invent a radius.
    for (def, set) in ORC_DEFS.iter().chain(HUMAN_DEFS.iter()) {
        if let Some((_, row)) = audio.unit_ack_definition(def) {
            println!(
                "VOICE_CUTOFF def={def} set={set} row={} cutoff={:.0}",
                row.name,
                row.audible_cutoff()
            );
        }
    }
    if let Some(combat) = audio.sound_table().get("AxeMediumChopWood") {
        println!(
            "VOICE_CUTOFF def=- group=combat row={} table={} cutoff={:.0}",
            combat.name,
            combat.source,
            combat.audible_cutoff()
        );
    }

    if failures > 0 {
        return Err(format!("{failures} voice mapping failure(s)").into());
    }
    Ok(())
}
