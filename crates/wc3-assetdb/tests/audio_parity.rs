//! Audio parity at library level: every category the game needs must play from
//! the operator's archives, the 3D attenuation must follow the retail
//! inverse-square curve, and a TFT-layered install must resolve TFT-only units.

use std::path::PathBuf;

use glam::Vec3;

use wc3_assetdb::audio_provider::{build_game_audio, SharedAssetDb, WiredGameAudio};
use wc3_assetdb::{AssetDb, Resolution};
use wc3_audio::engine::{AudioConfig, AudioEngine, PlayParams};
use wc3_audio::Category;

fn analysis_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn war3_mpq() -> Option<PathBuf> {
    let path = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    path.is_file().then_some(path)
}

fn open_war3() -> Option<AssetDb> {
    let mpq = war3_mpq()?;
    let index = analysis_dir().join("agent-assets/war3.index");
    match index.is_file() {
        true => AssetDb::open_mpq_best(mpq, index).ok().map(|(db, _)| db),
        false => AssetDb::open_mpq(mpq).ok(),
    }
}

fn open_install() -> Option<AssetDb> {
    let root = analysis_dir().join("agent-assets/wc3-install");
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
        return None;
    }
    AssetDb::open_mpq_layers(&layers, "wc3-install").ok()
}

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.85,
    }
}

fn peak(block: &[f32]) -> f32 {
    block.iter().fold(0.0f32, |p, s| p.max(s.abs()))
}

fn render(audio: &mut WiredGameAudio, block: &mut [f32], blocks: usize) -> f32 {
    let mut max_peak = 0.0f32;
    for _ in 0..blocks {
        audio.render(block);
        max_peak = max_peak.max(peak(block));
    }
    max_peak
}

#[test]
fn ack_spell_ambient_and_music_play_from_the_archive() {
    let Some(db) = open_war3() else {
        eprintln!("skipping: War3.mpq not available");
        return;
    };
    let shared = SharedAssetDb::new(db);
    let (mut audio, wiring) =
        build_game_audio(shared.clone(), AudioEngine::headless(config()), "headless");
    assert!(wiring.tables >= 6, "sound tables {}", wiring.tables);
    assert!(wiring.definitions > 1000, "definitions {}", wiring.definitions);
    assert!(wiring.units > 400, "units {}", wiring.units);
    assert!(wiring.themes >= 20, "music themes {}", wiring.themes);

    let mut block = vec![0.0f32; config().block_samples()];

    // Unit ack through the unit-id -> sound-set path (RoC tables).
    let ack = audio.unit_ack("hfoo", Some(Vec3::new(20.0, 0.0, 0.0)));
    assert!(ack.is_some(), "Footman ack did not start");
    assert!(render(&mut audio, &mut block, 12) > 0.001, "ack was silent");

    // Attack acknowledgement.
    let attack = audio.unit_attack("ogru", None);
    assert!(attack.is_some(), "Grunt attack did not start");
    assert!(render(&mut audio, &mut block, 12) > 0.001, "attack was silent");

    // Spell sound from AbilitySounds.slk.
    let spell = audio.spell("BlizzardWave", Some(Vec3::new(30.0, 0.0, 0.0)));
    assert!(spell.is_some(), "BlizzardWave did not start");
    let spell_peak = render(&mut audio, &mut block, 12);
    assert!(spell_peak > 0.001, "spell was silent: {spell_peak}");
    // Retail dir is `Abilities\Spells\Human\Blizzard`, the shipped files are
    // `abilities\...` (lowercase root) — the listfile resolver must bridge it.
    let spell_path = audio
        .sound_table()
        .get("BlizzardWave")
        .and_then(|def| def.paths().into_iter().next())
        .expect("BlizzardWave path");
    assert!(
        matches!(
            shared.with_db(|db| db.resolve_asset(&spell_path)),
            Resolution::Exact { .. } | Resolution::ListfileMatch { .. }
        ),
        "spell path must resolve: {spell_path}"
    );

    // Ambience loop from AmbienceSounds.slk.
    let ambient = audio.ambient("AmbientSoundRain");
    assert!(ambient.is_some(), "AmbientSoundRain did not start");
    assert!(render(&mut audio, &mut block, 20) > 0.001, "ambience was silent");
    assert!(
        audio.audio_stats().active_voices >= 1,
        "ambience loop is not active"
    );
    if let Some(id) = ambient {
        audio.stop_voice(id);
    }
    render(&mut audio, &mut block, 6);

    // Music theme, streamed from MP3.
    let music = audio.music("Human1");
    assert!(music.is_some(), "Human1 music did not start");
    let mut nonzero = 0usize;
    for _ in 0..30 {
        audio.render(&mut block);
        if peak(&block) > 0.001 {
            nonzero += 1;
        }
    }
    assert!(nonzero >= 20, "music not continuous: {nonzero}/30");
    audio.stop_music();
    audio.shutdown();
}

#[test]
fn attenuation_matches_inverse_square() {
    let Some(db) = open_war3() else {
        return;
    };
    let shared = SharedAssetDb::new(db);
    let (mut audio, _wiring) =
        build_game_audio(shared.clone(), AudioEngine::headless(config()), "headless");
    let mut block = vec![0.0f32; config().block_samples()];

    let probe_path = shared
        .with_db(|db| db.sound_path("Units\\Human\\Footman\\FootmanWhat1.wav"))
        .expect("probe path");
    let probe = shared
        .with_db(|db| db.sound_shared(&probe_path))
        .expect("probe sound");
    assert!(probe.peak() > 0.1);

    let reference = 600.0f32;
    let mut energies = Vec::new();
    for distance in [reference, reference * 2.0, reference * 4.0] {
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
        for step in 0..18 {
            audio.render(&mut block);
            if step >= 12 {
                for frame in block.chunks_exact(2) {
                    energy += frame[0] * frame[0] + frame[1] * frame[1];
                }
            }
        }
        energies.push(energy);
        audio.engine().stop(id);
        render(&mut audio, &mut block, 6);
    }
    println!("attenuation energies at 1x/2x/4x reference: {energies:?}");
    let unity = energies[0];
    assert!(unity > 0.0, "no energy at unity distance");
    // Inverse square: amplitude 1/2 -> energy 1/16; 1/4 -> 1/256.
    assert!(
        (energies[1] - unity / 16.0).abs() <= unity / 16.0 * 0.25,
        "2x distance energy {} is not unity/16 ({})",
        energies[1],
        unity / 16.0
    );
    assert!(
        energies[2] <= unity / 256.0 * 1.5,
        "4x distance energy {} is not ~unity/256 ({})",
        energies[2],
        unity / 256.0
    );
    audio.shutdown();
}

#[test]
fn tft_install_resolves_a_unit_ack_from_war3x() {
    let Some(db) = open_install() else {
        eprintln!("skipping: TFT install layers not available");
        return;
    };
    let shared = SharedAssetDb::new(db);
    let (mut audio, wiring) =
        build_game_audio(shared.clone(), AudioEngine::headless(config()), "headless");
    assert!(
        wiring.layers.contains(&"War3x".to_string())
            || wiring.layers.contains(&"War3xlocal".to_string()),
        "install layers: {:?}",
        wiring.layers
    );
    let mut block = vec![0.0f32; config().block_samples()];
    let mut found: Option<(String, String, String)> = None;
    for id in audio.unit_names().ids() {
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
    let (id, def_name, layer) = found.expect("no TFT-unit ack resolved from a TFT layer");
    println!("TFT unit ack: unit={id} def={def_name} layer={layer}");
    let voice = audio.unit_ack(&id, Some(Vec3::new(-40.0, 0.0, 0.0)));
    assert!(voice.is_some(), "TFT unit ack did not start");
    let heard = render(&mut audio, &mut block, 16);
    assert!(heard > 0.001, "TFT unit ack was silent: {heard}");
    audio.shutdown();
}
