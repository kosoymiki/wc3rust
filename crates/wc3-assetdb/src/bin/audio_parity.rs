//! Audio parity harness: proves the game-facing sound path for every category.
//!
//! Cases (all resolved from the operator's archives, never fabricated):
//! * unit ack (`hfoo` / `FootmanWhat`), attack (`ogru` / `GruntYesAttack`)
//! * spell sound (`BlizzardWave` from `AbilitySounds.slk`)
//! * ambience loop (`AmbientSoundRain` from `AmbienceSounds.slk`, looping)
//! * music (`Human1.mp3` streamed from `Sound\Music\mp3Music`)
//! * 3D attenuation curve for a positional ack (0.5/1/2/4 x reference distance)
//! * TFT unit ack (HeroArchDruid) resolved through the War3xlocal/War3x layers
//!
//! Usage:
//!   audio_parity --install <dir> [--out DIR]
//!   audio_parity --mpq <War3.mpq> --index <war3.index> [--out DIR]
//!
//! `--install` mounts `War3Patch/patch > War3xlocal > War3x > War3` (the same
//! priority `StormFs::open_wc3_install` uses). Audio uses the null device unless
//! `H2_AUDIO` is unset/other, so CI does not touch hardware.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use glam::Vec3;

use wc3_assetdb::audio_provider::{
    build_game_audio, game_audio_stats, SharedAssetDb, WiredGameAudio,
};
use wc3_assetdb::{AssetDb, Resolution};
use wc3_audio::engine::{AudioConfig, AudioEngine, PlayParams};
use wc3_audio::{Category, VoiceId};

struct Case {
    name: &'static str,
    kind: &'static str,
    voice: Option<VoiceId>,
    peak: f32,
    resolved: Vec<String>,
    layers: Vec<String>,
    note: String,
}


/// Minimal JSON string escaping for paths with backslashes and quotes.
fn esc(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// JSON array of strings.
fn json_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&esc(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    let mut index: Option<PathBuf> = None;
    let mut install: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("analysis/agent-assets");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = PathBuf::from(&args[i]);
            }
            "--index" => {
                i += 1;
                index = Some(PathBuf::from(&args[i]));
            }
            "--install" => {
                i += 1;
                install = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    if std::env::var_os("H2_AUDIO").is_none() {
        // SAFETY: single-threaded setup before the render thread starts.
        unsafe { std::env::set_var("H2_AUDIO", "null") };
    }

    let db = match &install {
        Some(root) => {
            let mut layers: Vec<(String, PathBuf)> = Vec::new();
            for name in [
                "War3Patch.mpq",
                "patch.mpq",
                "War3xlocal.mpq",
                "War3x.mpq",
                "War3.mpq",
            ] {
                let path = root.join(name);
                if path.is_file() {
                    layers.push((name.trim_end_matches(".mpq").to_string(), path));
                }
            }
            if layers.is_empty() {
                return Err(format!("no War3 archives under {}", root.display()).into());
            }
            AssetDb::open_mpq_layers(&layers, "wc3-install")?
        }
        None => match &index {
            Some(path) => AssetDb::open_mpq_best(&mpq, path)?.0,
            None => AssetDb::open_mpq(&mpq)?,
        },
    };
    let layers = db.layer_labels();
    let source = db.source_label().to_string();
    let shared = SharedAssetDb::new(db);

    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.85,
    };
    let (mut audio, wiring) = build_game_audio(
        shared.clone(),
        AudioEngine::headless(config),
        "headless",
    );
    // QA device-log gate: emit AUDIO_ACK / AUDIO_MUSIC / ... lines on stderr.
    audio.set_event_log(true);
    println!("wiring: {}", wiring.summary());

    let block_frames = config.block_frames;
    let channels = config.channels as usize;
    let mut block = vec![0.0f32; block_frames * channels];

    let mut play = |audio: &mut WiredGameAudio,
                    block: &mut [f32],
                    shared: &SharedAssetDb,
                    name: &'static str,
                    kind: &'static str,
                    sound_def: Option<&str>,
                    theme: Option<&str>,
                    blocks: usize|
     -> Case {
        let mut resolved = Vec::new();
        let mut layers_used = Vec::new();
        if let Some(def_name) = sound_def {
            if let Some(def) = audio.sound_table().get(def_name) {
                for path in def.paths() {
                    if let Resolution::Exact { layer, path } | Resolution::ListfileMatch { layer, path, .. } =
                        shared.with_db(|db| db.resolve_asset(&path))
                    {
                        resolved.push(path);
                        layers_used.push(layer);
                    }
                }
            }
        }
        if let Some(theme_name) = theme {
            if let Some((_name, path)) = audio
                .themes()
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(theme_name))
            {
                if let Resolution::Exact { layer, path } | Resolution::ListfileMatch { layer, path, .. } =
                    shared.with_db(|db| db.resolve_asset(path))
                {
                    resolved.push(path);
                    layers_used.push(layer);
                }
            }
        }
        let voice = match (sound_def, theme) {
            (Some(def_name), None) if kind == "ambient" => audio.ambient(def_name),
            (Some(_), None) if kind == "ack" => audio.unit_ack("hfoo", None),
            (Some(_), None) if kind == "attack" => audio.unit_attack("ogru", None),
            (Some(def_name), None) if kind == "spell" => {
                audio.spell(def_name, Some(Vec3::new(30.0, 0.0, 0.0)))
            }
            (Some(def_name), None) => audio.play_sound(def_name, None),
            (None, None) if kind == "ui" => audio.ui_click(),
            (None, Some(theme_name)) => audio.music(theme_name),
            _ => None,
        };
        let mut peak = 0.0f32;
        for _ in 0..blocks {
            audio.render(block);
            peak = peak.max(block.iter().fold(0.0f32, |p, s| p.max(s.abs())));
        }
        let note = if voice.is_some() {
            String::new()
        } else if kind == "ui" {
            "ui click returned no voice (no shipped variant resolved)".to_string()
        } else if resolved.is_empty() {
            "no resolvable variant in the mounted layers".to_string()
        } else {
            "voice did not start".to_string()
        };
        Case {
            name,
            kind,
            voice,
            peak,
            resolved,
            layers: layers_used,
            note,
        }
    };

    // --- acknowledgement / attack -----------------------------------------
    let ack = play(
        &mut audio,
        &mut block,
        &shared,
        "FootmanWhat",
        "ack",
        Some("FootmanWhat"),
        None,
        12,
    );
    let attack = play(
        &mut audio,
        &mut block,
        &shared,
        "GruntYesAttack",
        "attack",
        Some("GruntYesAttack"),
        None,
        12,
    );
    // --- UI click (UISounds fallback chain) --------------------------------
    let ui = play(
        &mut audio,
        &mut block,
        &shared,
        "InterfaceClick",
        "ui",
        None,
        None,
        8,
    );
    // --- spell -------------------------------------------------------------
    let spell = play(
        &mut audio,
        &mut block,
        &shared,
        "BlizzardWave",
        "spell",
        Some("BlizzardWave"),
        None,
        12,
    );
    // --- ambience (looping) ------------------------------------------------
    let ambient = play(
        &mut audio,
        &mut block,
        &shared,
        "AmbientSoundRain",
        "ambient",
        Some("AmbientSoundRain"),
        None,
        20,
    );
    let ambient_active = audio.audio_stats().active_voices;
    if let Some(id) = ambient.voice {
        audio.stop_voice(id);
    }
    for _ in 0..6 {
        audio.render(&mut block);
    }
    // --- music (streamed) --------------------------------------------------
    let music = play(
        &mut audio,
        &mut block,
        &shared,
        "Human1",
        "music",
        None,
        Some("Human1"),
        40,
    );
    let mut music_nonzero = 0usize;
    {
        // Re-render to count non-silent blocks (the case above already consumed
        // its blocks; measure a fresh window while the loop is still running).
        for _ in 0..40 {
            audio.render(&mut block);
            if block.iter().fold(0.0f32, |p, s| p.max(s.abs())) > 0.001 {
                music_nonzero += 1;
            }
        }
    }
    audio.stop_all();
    for _ in 0..6 {
        audio.render(&mut block);
    }

    // --- 3D attenuation ----------------------------------------------------
    audio.set_listener(Vec3::ZERO, 0.0);
    let ack_ref = audio
        .sound_table()
        .get("FootmanWhat")
        .map(|def| {
            if def.max_distance > 0.0 {
                def.max_distance
            } else {
                600.0
            }
        })
        .unwrap_or(600.0);
    let mut attenuation: Vec<(f32, f32, f32)> = Vec::new(); // (ref multiplier, peak, energy)
    // Deterministic probe: the random ack variants have very different attack
    // shapes, so attenuation is measured with one looping decoded sound from
    // the archive and explicit PlayParams (reference distance 600).
    let probe_path = shared
        .with_db(|db| db.sound_path("Units\\Human\\Footman\\FootmanWhat1.wav"))
        .ok_or("FootmanWhat1.wav did not resolve")?;
    let probe = shared
        .with_db(|db| db.sound_shared(&probe_path))
        .map_err(|err| format!("decode {probe_path}: {err}"))?;
    let reference = 600.0f32;
    for multiplier in [0.5f32, 1.0, 2.0, 4.0] {
        let distance = reference * multiplier;
        let id = audio.engine().play(
            probe.clone(),
            PlayParams {
                gain: 1.0,
                position: Some(Vec3::new(distance, 0.0, 0.0)),
                looping: true,
                reference_distance: reference,
                category: Category::Sfx,
                ..Default::default()
            },
        );
        let mut energy = 0.0f32;
        let mut peak = 0.0f32;
        // Ack WAVs have a quiet lead-in; skip 0.25 s before measuring.
        for step in 0..18 {
            audio.render(&mut block);
            if step >= 12 {
                for frame in block.chunks_exact(2) {
                    energy += frame[0] * frame[0] + frame[1] * frame[1];
                    peak = peak.max(frame[0].abs().max(frame[1].abs()));
                }
            }
        }
        attenuation.push((multiplier, peak, energy));
        audio.engine().stop(id);
        for _ in 0..6 {
            audio.render(&mut block);
        }
    }
    let energies: Vec<f32> = attenuation.iter().map(|row| row.2).collect();
    assert!(energies[0] > 0.0, "no energy at the near distance");
    assert!(
        (energies[1] - energies[0]).abs() <= energies[0] * 0.10,
        "inside the reference radius gain must stay at unity: {energies:?}"
    );
    assert!(
        energies[2] < energies[1] * 0.10,
        "2x reference distance must fall off sharply: {energies:?}"
    );
    assert!(
        energies[3] < energies[2] * 0.20,
        "4x reference distance must fall off again: {energies:?}"
    );

    // --- TFT unit ack ------------------------------------------------------
    // Pick a unit whose acknowledgement resolves from a TFT layer
    // (War3x/War3xlocal) through its retail `unitSound` column, then play it
    // through the same `unit_ack` hook the game calls.
    let mut tft = String::from("skipped (no TFT layers mounted)");
    let mut tft_ok = false;
    if matches!(install, Some(_)) {
        let mut found: Option<(String, String, String)> = None; // id, def, layer
        let ids = audio.unit_names().ids();
        for id in ids {
            let Some(set) = audio.unit_names().sound_set(&id).map(str::to_string) else {
                continue;
            };
            let def_name = format!("{set}What");
            let Some(def) = audio.sound_table().get(&def_name) else {
                continue;
            };
            for path in def.paths() {
                if let Resolution::Exact { layer, .. } | Resolution::ListfileMatch { layer, .. } =
                    shared.with_db(|db| db.resolve_asset(&path))
                {
                    if layer == "War3x" || layer == "War3xlocal" {
                        found = Some((id.clone(), def_name.clone(), layer));
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        match found {
            Some((id, def_name, layer)) => {
                let voice = audio.unit_ack(&id, Some(Vec3::new(-40.0, 0.0, 0.0)));
                let mut peak = 0.0f32;
                for _ in 0..16 {
                    audio.render(&mut block);
                    peak = peak.max(block.iter().fold(0.0f32, |p, s| s.abs().max(p)));
                }
                tft_ok = voice.is_some() && peak > 0.001;
                tft = format!(
                    "unit={id} sound_set={def_name} layer={layer} voice={voice:?} peak={peak:.4} ok={tft_ok}"
                );
            }
            None => {
                tft = "no unit ack resolved from a TFT layer".to_string();
            }
        }
    }

    // --- assertions --------------------------------------------------------
    for case in [&ack, &attack, &spell, &ambient, &music] {
        assert!(
            case.voice.is_some(),
            "{} ({}) did not start: {}",
            case.name,
            case.kind,
            case.note
        );
        assert!(
            case.peak > 0.001,
            "{} ({}) produced silence",
            case.name,
            case.kind
        );
        assert!(
            !case.resolved.is_empty(),
            "{} ({}) resolved no archive path",
            case.name,
            case.kind
        );
    }
    assert!(ambient_active >= 1, "ambience loop was not active");
    assert!(
        music_nonzero >= 30,
        "music stream is not continuous: {music_nonzero}/40"
    );
    if matches!(install, Some(_)) {
        assert!(tft_ok, "TFT unit ack failed: {tft}");
    }

    // --- report ------------------------------------------------------------
    let mut json = String::new();
    let _ = writeln!(json, "{{");
    let _ = writeln!(json, "  \"source\": \"{source}\",");
    let _ = writeln!(json, "  \"layers\": {layers:?},");
    let _ = writeln!(json, "  \"wiring\": \"{}\",", esc(&wiring.summary()));
    let _ = writeln!(json, "  \"cases\": [");
    let rows: Vec<&Case> = vec![&ack, &attack, &ui, &spell, &ambient, &music];
    for (index, case) in rows.iter().enumerate() {
        let _ = write!(
            json,
            "    {{\"name\": \"{}\", \"kind\": \"{}\", \"voice\": {}, \"peak\": {:.4}, \"resolved\": {}, \"layers\": {}, \"note\": \"{}\"}}{}\n",
            esc(case.name),
            esc(case.kind),
            case.voice.map(|v| v.0 as i64).unwrap_or(-1),
            case.peak,
            json_array(&case.resolved),
            json_array(&case.layers),
            esc(&case.note),
            if index + 1 == rows.len() { "" } else { "," }
        );
    }
    let _ = writeln!(json, "  ],");
    let _ = writeln!(json, "  \"ambient_active_voices\": {ambient_active},");
    let _ = writeln!(json, "  \"music_nonzero_blocks\": {music_nonzero},");
    let _ = writeln!(json, "  \"reference_distance\": {ack_ref:.1},");
    let _ = writeln!(json, "  \"attenuation\": [");
    for (index, (multiplier, peak, energy)) in attenuation.iter().enumerate() {
        let _ = write!(
            json,
            "    {{\"ref_multiple\": {multiplier:.1}, \"peak\": {peak:.4}, \"energy\": {energy:.6}}}{}\n",
            if index + 1 == attenuation.len() { "" } else { "," }
        );
    }
    let _ = writeln!(json, "  ],");
    let _ = writeln!(json, "  \"tft_unit_ack\": \"{}\",", esc(&tft));
    let _ = writeln!(json, "  \"hook_stats\": \"{:?}\",", game_audio_stats(&audio));
    let _ = writeln!(json, "  \"wired\": true");
    let _ = writeln!(json, "}}");
    println!("{json}");
    std::fs::create_dir_all(&out_dir)?;
    std::fs::write(out_dir.join("audio-parity.json"), &json)?;

    audio.shutdown();
    Ok(())
}
