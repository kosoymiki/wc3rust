//! End-to-end gameplay audio against the real archive.
//!
//! Opens `War3.mpq`, loads the retail sound tables, plays a Footman ack, a Grunt
//! attack, a death line, UI feedback and a music theme through the headless
//! engine, and asserts that audio actually comes out of the mixer. Coverage
//! numbers (units playable from this archive) are printed as evidence.

use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;
use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::AssetDb;
use wc3_audio::engine::{AudioConfig, AudioEngine, PlayParams};
use wc3_audio::game_audio::GameAudio;
use wc3_audio::Sound;

fn mpq_path() -> Option<PathBuf> {
    let path = PathBuf::from(
        std::env::var_os("WC3_MPQ").unwrap_or_else(|| {
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()
        }),
    );
    path.is_file().then_some(path)
}

fn index_path() -> Option<PathBuf> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-assets/war3.index");
    path.is_file().then_some(path)
}

fn open_db() -> Option<AssetDb> {
    let mpq = mpq_path()?;
    match index_path() {
        Some(index) => AssetDb::open_mpq_best(mpq, index).ok().map(|(db, _)| db),
        None => AssetDb::open_mpq(mpq).ok(),
    }
}

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 32,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.9,
    }
}

fn peak(block: &[f32]) -> f32 {
    block.iter().fold(0.0f32, |p, s| p.max(s.abs()))
}

#[test]
fn real_unit_ack_attack_death_ui_and_music() {
    let Some(db) = open_db() else {
        eprintln!("skipping: War3.mpq not available");
        return;
    };
    let mut db = db;
    let (sounds, units, tables) = load_sound_tables(&mut db);
    assert!(tables >= 6, "expected the retail sound tables, got {tables}");
    assert!(sounds.len() > 1000, "definitions {}", sounds.len());

    let engine = AudioEngine::headless(config());
    let mut audio = GameAudio::new(engine, sounds, units, db);
    let theme_bytes = audio
        .sound_table()
        .get("FootmanWhat")
        .cloned()
        .expect("FootmanWhat definition");

    // Unit acknowledgement: hfoo -> Footman -> FootmanWhat1..4.wav
    let ack = audio
        .unit_ack("hfoo", Some(Vec3::new(10.0, 0.0, 0.0)))
        .expect("Footman ack must play from War3.mpq");
    let mut block = vec![0.0f32; config().block_samples()];
    let mut ack_peak = 0.0f32;
    for _ in 0..6 {
        audio.render(&mut block);
        ack_peak = ack_peak.max(peak(&block));
    }
    println!(
        "ack voice={ack:?} peak={ack_peak:.3} def={} files={:?}",
        theme_bytes.name, theme_bytes.files
    );
    assert!(ack_peak > 0.001, "ack produced silence: {ack_peak}");

    // Attack and death lines.
    let attack = audio.unit_attack("ogru", None).expect("Grunt attack");
    let death = audio.unit_death("hfoo", None).expect("Footman death");
    println!("attack={attack:?} death={death:?}");

    // UI feedback.
    let click = audio.ui_click().expect("InterfaceClick");
    let error = audio.ui_error().expect("InterfaceError");
    println!("click={click:?} error={error:?}");

    // Music theme from TriggerData: Human1.mp3 streamed.
    let trigger = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/UI/TriggerData.txt"),
    )
    .expect("trigger data");
    let themes = audio.load_themes_from_trigger_data(&trigger);
    println!("themes loaded {themes}");
    assert!(themes >= 20, "music themes {themes}");
    let music = audio.music("Human1").expect("Human1 music theme");
    println!("music voice {music:?}");
    let mut music_peak = 0.0f32;
    let mut music_nonzero_blocks = 0;
    for _ in 0..200 {
        audio.render(&mut block);
        let p = peak(&block);
        music_peak = music_peak.max(p);
        if p > 0.001 {
            music_nonzero_blocks += 1;
        }
    }
    println!(
        "music peak={music_peak:.3} nonzero_blocks={music_nonzero_blocks} stats={:?}",
        audio.audio_stats()
    );
    assert!(
        music_nonzero_blocks > 100,
        "music must stream continuously, got {music_nonzero_blocks} blocks"
    );

    let stats = audio.stats();
    assert_eq!(stats.acks_started, 1);
    assert_eq!(stats.acs_missing_placeholder(), 0);
    assert_eq!(stats.music_started, 1);
    audio.stop_music();
    // After stopping, further blocks decay to silence.
    for _ in 0..20 {
        audio.render(&mut block);
    }
    let silence_peak = peak(&block);
    println!("after stop_music peak={silence_peak}");
    assert!(silence_peak < 0.01, "music did not stop: {silence_peak}");
    let audio_stats = audio.audio_stats();
    assert_eq!(audio_stats.backend_errors, 0);
    audio.shutdown();
}

#[test]
fn unit_coverage_from_this_archive() {
    let Some(mut db) = open_db() else {
        return;
    };
    let (sounds, units, _) = load_sound_tables(&mut db);
    let ids = units.ids();
    let mut named = 0usize;
    let mut with_ack = 0usize;
    let mut playable = 0usize;
    let mut missing_examples = Vec::new();
    for id in &ids {
        // `unitSound` is the authoritative ack prefix (RoC and TFT both have it);
        // the display name is only the fallback for tables without that column.
        let Some(name) = units.sound_set(id).or_else(|| units.name(id)) else {
            continue;
        };
        named += 1;
        if let Some(def) = sounds.unit_ack(name) {
            with_ack += 1;
            let paths = def.paths();
            if paths.iter().any(|path| db.contains(path)) {
                playable += 1;
            } else if missing_examples.len() < 5 {
                missing_examples.push((id.clone(), name.to_string(), paths.first().cloned()));
            }
        }
    }
    println!(
        "units named={named} with_ack={with_ack} playable={playable} missing_examples={missing_examples:?}"
    );
    assert!(named > 400, "unit names {named}");
    assert!(with_ack > 350, "units with ack {with_ack}");
    assert!(
        playable >= with_ack - 10,
        "all but expansion-only units must be playable: {playable}/{with_ack}"
    );
}

#[test]
fn loads_real_wav_variants_and_reports_codecs() {
    let Some(mut db) = open_db() else {
        return;
    };
    let mut loaded = 0usize;
    let mut bytes = 0usize;
    let mut frames = 0u64;
    for name in [
        "Units\\Human\\Footman\\FootmanWhat1.wav",
        "Sound\\Interface\\Click.wav",
        "Sound\\Buildings\\Orc\\OrcBuildingBirthWhat1.wav",
        "Doodads\\Barrens\\Terrain\\WyvernCage\\WyvernCageDeath1.wav",
    ] {
        let sound: Arc<Sound> = db.sound_shared(name).expect(name);
        println!(
            "{name}: {} Hz, {} ch, {} frames, {:.2} s, peak {:.3}",
            sound.sample_rate,
            sound.channels,
            sound.frames(),
            sound.duration_seconds(),
            sound.peak()
        );
        assert!(sound.frames() > 100);
        assert!(sound.peak() > 0.01 || sound.is_silent());
        loaded += 1;
        bytes += sound.memory_bytes();
        frames += sound.frames() as u64;
    }
    assert_eq!(loaded, 4);
    println!("decoded {loaded} sounds, {bytes} bytes, {frames} frames");
    assert!(db.sound_bytes() > 0);
}

#[test]
fn engine_plays_a_sound_loaded_from_the_archive_with_3d_pan() {
    let Some(mut db) = open_db() else {
        return;
    };
    let sound = db
        .sound_shared("Units\\Orc\\Grunt\\GruntWhat1.wav")
        .expect("GruntWhat1");
    let mut engine = AudioEngine::headless(config());
    engine.set_listener(Vec3::ZERO, 0.0);
    engine.play(
        sound,
        PlayParams {
            position: Some(Vec3::new(-300.0, 0.0, 0.0)),
            reference_distance: 600.0,
            ..Default::default()
        },
    );
    let mut block = vec![0.0f32; config().block_samples()];
    let mut left_energy = 0.0f32;
    let mut right_energy = 0.0f32;
    for _ in 0..10 {
        engine.render(&mut block);
        for frame in block.chunks_exact(2) {
            left_energy += frame[0] * frame[0];
            right_energy += frame[1] * frame[1];
        }
    }
    println!("3D pan: left_energy={left_energy:.4} right_energy={right_energy:.4}");
    assert!(left_energy > 0.0, "no audio rendered");
    assert!(
        left_energy > right_energy * 1.5,
        "a source to the listener's left must favour the left channel"
    );
}
