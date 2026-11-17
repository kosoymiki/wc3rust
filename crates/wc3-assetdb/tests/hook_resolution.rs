//! Gameplay hooks must use the provider's resolver: a unit whose retail sound
//! table points at a stale directory still plays, and a unit whose files are
//! genuinely absent stays silent without inventing a path.

use std::path::PathBuf;

use wc3_audio::engine::{AudioConfig, AudioEngine};
use wc3_audio::game_audio::{GameAudio, GameAudioStats};

use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::AssetDb;

fn open_db() -> Option<AssetDb> {
    let mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
        });
    if !mpq.is_file() {
        return None;
    }
    let index = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-assets/war3.index");
    match index.is_file() {
        true => AssetDb::open_mpq_best(&mpq, &index).ok().map(|(db, _)| db),
        false => AssetDb::open_mpq(&mpq).ok(),
    }
}

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 16,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.9,
    }
}

fn peak(block: &[f32]) -> f32 {
    block.iter().fold(0.0f32, |p, s| p.max(s.abs()))
}

#[test]
fn stale_table_unit_plays_through_the_resolver() {
    let Some(mut db) = open_db() else {
        eprintln!("skipping: War3.mpq not available");
        return;
    };
    let (sounds, units, tables) = load_sound_tables(&mut db);
    assert!(tables >= 6, "sound tables {tables}");
    let engine = AudioEngine::headless(config());
    let mut audio = GameAudio::new(engine, sounds, units, db);

    // npgf = PigFarm: `PigFarmWhat` points at Buildings\Orc\PigFarm\ but the file
    // ships under Buildings\Other\PigFarm\.
    let voice = audio
        .unit_ack("npgf", None)
        .expect("PigFarm ack must resolve through the listfile");
    let mut block = vec![0.0f32; config().block_samples()];
    let mut heard = 0.0f32;
    for _ in 0..8 {
        audio.render(&mut block);
        heard = heard.max(peak(&block));
    }
    println!("npgf voice={voice:?} peak={heard:.3}");
    assert!(heard > 0.001, "resolved ack must be audible, peak {heard}");
    let stats: GameAudioStats = audio.stats();
    assert_eq!(stats.acks_started, 1);
    assert_eq!(stats.acks_missing, 0);
    audio.shutdown();
}

#[test]
fn absent_unit_ack_returns_none_without_fabricating() {
    let Some(mut db) = open_db() else {
        return;
    };
    let (sounds, units, _) = load_sound_tables(&mut db);
    let engine = AudioEngine::headless(config());
    let mut audio = GameAudio::new(engine, sounds, units, db);

    // HeroArchDruid does not exist in War3.mpq (TFT content).
    assert!(audio.play_sound("HeroArchDruidWhat", None).is_none());
    let stats = audio.stats();
    assert!(stats.sounds_missing >= 1, "miss must be counted");
    audio.shutdown();
}
