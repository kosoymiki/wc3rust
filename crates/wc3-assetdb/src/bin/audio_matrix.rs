//! AUDIO_MATRIX.json: per-family have/missing audit of the shipped audio data.
//!
//! Families:
//! * `units` — every `Units/unitUI.slk` id: its `unitSound` set and whether an
//!   ack definition + file resolve.
//! * `abilities` — every `AbilitySounds.slk` definition: its first file
//!   resolves through the mounted layers.
//! * `events` — every `AudioEvent` (and each race variant): a definition exists
//!   and its file resolves. Events with no retail row are listed as `gap`.
//!
//! Usage: `audio_matrix [--install DIR] [--out FILE]`

use std::fmt::Write as _;
use std::path::PathBuf;

use wc3_assetdb::audio_provider::{start_game_audio, SharedAssetDb};
use wc3_assetdb::AssetDb;
use wc3_audio::events::{event_defs, AudioEvent, Race};
use wc3_audio::{SoundInfoDb, UnitNameMap};

fn resolves(shared: &SharedAssetDb, path: &str) -> bool {
    shared.with_db(|db| {
        matches!(
            db.resolve_asset(path),
            wc3_assetdb::Resolution::Exact { .. } | wc3_assetdb::Resolution::ListfileMatch { .. }
        )
    })
}

fn def_resolves(shared: &SharedAssetDb, sounds: &SoundInfoDb, name: &str) -> bool {
    let Some(def) = sounds.get(name) else {
        return false;
    };
    def.paths().iter().any(|path| resolves(shared, path))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut install = PathBuf::from("analysis/agent-assets/wc3-install");
    let mut out = PathBuf::from("analysis/agent-assets/AUDIO_MATRIX.json");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--install" => {
                i += 1;
                install = PathBuf::from(args.get(i).ok_or("--install needs a path")?);
            }
            "--out" => {
                i += 1;
                out = PathBuf::from(args.get(i).ok_or("--out needs a path")?);
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
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
    let db = if layers.is_empty() {
        AssetDb::open_mpq(install.join("War3.mpq"))?
    } else {
        AssetDb::open_mpq_layers(&layers, "wc3-install")?
    };
    let source = db.source_label().to_string();
    let shared = SharedAssetDb::new(db);
    let (audio, wiring) = start_game_audio(shared.clone());
    let sounds = audio.sound_table();

    // ---- units ------------------------------------------------------------
    let unit_bytes = shared
        .with_db(|db| db.read("Units\\unitUI.slk").ok())
        .ok_or("unitUI.slk unreadable")?;
    let units = UnitNameMap::parse(&unit_bytes)?;
    let mut units_have = 0usize;
    let mut units_missing = Vec::new();
    for id in units.ids() {
        let Some(set) = units.sound_set(&id) else {
            units_missing.push(format!("{id}: noset"));
            continue;
        };
        match sounds.unit_ack(set) {
            Some(def) if def.paths().iter().any(|path| resolves(&shared, path)) => units_have += 1,
            Some(def) => units_missing.push(format!(
                "{id}:{set} nofile {:?}",
                def.paths().first()
            )),
            None => units_missing.push(format!("{id}:{set} norow")),
        }
    }

    // ---- abilities --------------------------------------------------------
    let mut abilities_total = 0usize;
    let mut abilities_have = 0usize;
    let mut abilities_missing = Vec::new();
    for def in sounds.iter() {
        if !def.source.to_ascii_lowercase().contains("ability") {
            continue;
        }
        abilities_total += 1;
        if def.paths().iter().any(|path| resolves(&shared, path)) {
            abilities_have += 1;
        } else {
            abilities_missing.push(def.name.clone());
        }
    }

    // ---- events -----------------------------------------------------------
    let mut events_have = 0usize;
    let mut events_gap = Vec::new();
    let mut events_missing = Vec::new();
    let unit_ack_ok = |id: &str| -> bool {
        units
            .sound_set(id)
            .and_then(|set| sounds.unit_ack(set))
            .is_some_and(|def| def.paths().iter().any(|path| resolves(&shared, path)))
    };
    let unit_attack_ok = |id: &str| -> bool {
        units
            .sound_set(id)
            .and_then(|set| sounds.unit_attack(set))
            .is_some_and(|def| def.paths().iter().any(|path| resolves(&shared, path)))
    };
    let unit_ready_ok = |id: &str| -> bool {
        units
            .sound_set(id)
            .and_then(|set| sounds.get(&format!("{set}Ready")))
            .is_some_and(|def| def.paths().iter().any(|path| resolves(&shared, path)))
    };
    for event in AudioEvent::ALL {
        // Events served by dedicated hooks (unit sets, UI rows, abilities)
        // are verified through those paths instead of `event_defs`.
        let dedicated = match event {
            AudioEvent::Attack | AudioEvent::AttackOrder => Some(unit_attack_ok("hfoo")),
            AudioEvent::Death => Some(unit_ack_ok("hfoo") && sounds.unit_death(
                units.sound_set("hfoo").unwrap_or("Footman"),
            ).is_some()),
            AudioEvent::WorkerAck | AudioEvent::MoveOrder => Some(unit_ack_ok("hpea")),
            AudioEvent::TrainComplete => Some(unit_ready_ok("hfoo")),
            AudioEvent::UiClick => Some(def_resolves(&shared, sounds, "InterfaceClick")),
            AudioEvent::UiError => Some(
                def_resolves(&shared, sounds, "InterfaceError")
                    || def_resolves(&shared, sounds, "ErrorMessage"),
            ),
            AudioEvent::AbilityCast => Some(abilities_have > 0),
            _ => None,
        };
        if let Some(ok) = dedicated {
            if ok {
                events_have += 1;
            } else {
                events_missing.push(format!("{}:dedicated", event.id()));
            }
            continue;
        }
        let races: Vec<Option<Race>> = if event.is_race_scoped() {
            Race::ALL.iter().map(|race| Some(*race)).collect()
        } else {
            vec![None]
        };
        for race in races {
            let defs = event_defs(event, race);
            if defs.is_empty() {
                events_gap.push(format!(
                    "{}:{}",
                    event.id(),
                    race.map(Race::id).unwrap_or("any")
                ));
                continue;
            }
            let hit = defs.iter().find(|name| sounds.get(name).is_some());
            match hit {
                Some(name) if def_resolves(&shared, sounds, name) => events_have += 1,
                Some(name) => events_missing.push(format!(
                    "{}:{} -> {name} nofile",
                    event.id(),
                    race.map(Race::id).unwrap_or("any")
                )),
                None => events_missing.push(format!(
                    "{}:{} norow {defs:?}",
                    event.id(),
                    race.map(Race::id).unwrap_or("any")
                )),
            }
        }
    }

    let json = {
        let mut s = String::new();
        s.push_str("{\n");
        let _ = writeln!(s, "  \"source\": {},", json_str(&source));
        let _ = writeln!(s, "  \"wiring\": {},", json_str(&wiring.summary()));
        let _ = writeln!(
            s,
            "  \"generated_epoch\": {},",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        let _ = writeln!(s, "  \"families\": {{");
        let _ = writeln!(
            s,
            "    \"units\": {{\"total\": {}, \"have\": {}, \"missing\": {}}},",
            units.len(),
            units_have,
            json_array(&units_missing)
        );
        let _ = writeln!(
            s,
            "    \"abilities\": {{\"total\": {}, \"have\": {}, \"missing\": {}}},",
            abilities_total,
            abilities_have,
            json_array(&abilities_missing)
        );
        let _ = writeln!(
            s,
            "    \"events\": {{\"have\": {}, \"gaps\": {}, \"missing\": {}}}",
            events_have,
            json_array(&events_gap),
            json_array(&events_missing)
        );
        s.push_str("  },\n");
        let _ = writeln!(
            s,
            "  \"totals\": {{\"unit_defs\": {}, \"ability_sounds\": {}, \"sound_defs\": {}}}",
            units.len(),
            abilities_total,
            sounds.len()
        );
        s.push_str("}\n");
        s
    };
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out, &json)?;

    println!(
        "AUDIO_MATRIX units={}/{} abilities={}/{} events_have={} gaps={} missing={} out={}",
        units_have,
        units.len(),
        abilities_have,
        abilities_total,
        events_have,
        events_gap.len(),
        events_missing.len(),
        out.display()
    );
    if !units_missing.is_empty() || !abilities_missing.is_empty() || !events_missing.is_empty() {
        println!("AUDIO_MATRIX_NOTE missing entries are listed in {}", out.display());
    }
    Ok(())
}

fn json_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_str(item));
    }
    out.push(']');
    out
}
