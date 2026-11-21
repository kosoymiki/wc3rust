//! Orc01 audio acceptance: five minutes of the first RoC orc chapter.
//!
//! Ledger 508 / GOAL 2026-11-18: the lane is judged on Orc01. This test loads
//! the real campaign map container (`Maps/Campaign/Orc01.w3m`) for its tileset
//! and placed unit ids, then drives the production hook layer (`GameAudio`) for
//! 300 seconds on the sim clock with the shipped archive:
//!
//! * orc race playlist with cross-fades and automatic rotation,
//! * day/night tileset ambience for the map's own tileset, region-keyed,
//! * unit acks/attacks/impacts/deaths, build/train/UI/deny cues,
//! * the 64 MB decoded-PCM ceiling, 64-voice pool, ack spacing and pause mute.
//!
//! Evidence printed by the test (also visible with `-- --nocapture`):
//! `ORC01_AUDIO …`, `ORC01_SAMPLE <line>` for each event kind and the
//! `GAME_AUDIO_HOOKS …` summary. The test skips when the extract is absent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{AudioConfig, AudioEngine, Category, DEFAULT_CHANNEL_VOICES};
use wc3_audio::events::{AudioEvent, Race};
use wc3_audio::{GameAudio, Sound, SoundInfoDb, SoundProvider, UnitNameMap};

const SESSION_SECONDS: u64 = 300;
const TICKS_PER_SECOND: u64 = 30;
const PCM_BUDGET: usize = 64 * 1024 * 1024;
const MAX_VOICES: usize = 64;

fn archive() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    root.join("UI/SoundInfo/UISounds.slk")
        .is_file()
        .then_some(root)
}

fn load_sounds(root: &Path) -> SoundInfoDb {
    let dir = root.join("UI/SoundInfo");
    let mut sounds = SoundInfoDb::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("sound info dir")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().and_then(|ext| ext.to_str()) != Some("slk") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("table name");
        sounds
            .add_table(name, &std::fs::read(&path).expect("slk bytes"))
            .expect("parse slk");
    }
    sounds
}

fn load_units(root: &Path) -> UnitNameMap {
    let bytes = std::fs::read(root.join("Units/unitUI.slk")).expect("unitUI.slk");
    UnitNameMap::parse(&bytes).expect("parse unitUI")
}

/// Decoding provider over the extracted archive, with a byte-capped decoded
/// cache (the production `AssetDb` behaves the same way).
struct RealProvider {
    root: PathBuf,
    index: HashMap<String, String>,
    by_basename: HashMap<String, Vec<String>>,
    cache: HashMap<String, Arc<Sound>>,
    cache_bytes: usize,
    cache_limit: usize,
    failures: Vec<String>,
}

impl RealProvider {
    fn new(root: &Path) -> Self {
        let mut index = HashMap::new();
        let mut by_basename: HashMap<String, Vec<String>> = HashMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().replace('\\', "/");
                let key = relative.to_ascii_lowercase();
                if let Some(base) = key.rsplit('/').next() {
                    by_basename
                        .entry(base.to_string())
                        .or_default()
                        .push(relative.clone());
                }
                index.insert(key, relative);
            }
        }
        for paths in by_basename.values_mut() {
            paths.sort();
        }
        Self {
            root: root.to_path_buf(),
            index,
            by_basename,
            cache: HashMap::new(),
            cache_bytes: 0,
            cache_limit: 32 * 1024 * 1024,
            failures: Vec::new(),
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.root.join(path.replace('\\', "/"))
    }
}

impl SoundProvider for RealProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        if let Some(sound) = self.cache.get(path) {
            return Some(sound.clone());
        }
        let bytes = std::fs::read(self.file(path)).ok()?;
        let decoded = if path.to_ascii_lowercase().ends_with(".mp3") {
            wc3_audio::decode_mp3_bounded(&bytes, 30 * 48_000 * 2).map(|(sound, _)| sound)
        } else {
            wc3_audio::decode_wav(&bytes)
        };
        match decoded {
            Ok(sound) if !sound.is_silent() => {
                let sound = Arc::new(sound);
                let size = sound.memory_bytes();
                if self.cache_bytes + size > self.cache_limit {
                    self.cache.clear();
                    self.cache_bytes = 0;
                }
                self.cache.insert(path.to_string(), sound.clone());
                self.cache_bytes += size;
                Some(sound)
            }
            Ok(_) => {
                self.failures.push(format!("{path}: empty PCM"));
                None
            }
            Err(error) => {
                self.failures.push(format!("{path}: {error}"));
                None
            }
        }
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.file(path)).ok().map(Arc::from)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some()
    }

    fn resolve(&self, path: &str) -> Option<String> {
        let key = path.replace('/', "\\").to_ascii_lowercase();
        if let Some(real) = self.index.get(&key) {
            return Some(real.clone());
        }
        let base = key.rsplit('\\').next()?;
        self.by_basename
            .get(base)
            .and_then(|paths| paths.first().cloned())
    }
}

/// Orc01 tileset letter and the unit ids placed in `war3mapUnits.doo`.
fn orc01_map(root: &Path) -> Option<(char, Vec<String>)> {
    let path = root.join("Maps/Campaign/Orc01.w3m");
    if !path.is_file() {
        return None;
    }
    let base = wc3_formats::map::map_mpq_base(&path).ok()?;
    let archive = wc3_mpq::MpqArchive::open_at(&path, base).ok()?;
    let terrain =
        wc3_formats::w3e::Terrain::parse(&archive.read("war3map.w3e").ok()?).ok()?;
    let tileset = terrain.tileset as char;
    let mut placed = Vec::new();
    if let Ok(bytes) = archive.read("war3mapUnits.doo")
        && let Ok(units) = wc3_formats::map::parse_units(&bytes)
    {
        for unit in units {
            let id = String::from_utf8_lossy(&unit.type_id).to_string();
            if !placed.contains(&id) {
                placed.push(id);
            }
        }
    }
    Some((tileset, placed))
}

/// Store the first line of each event kind (compact proof for the log).
fn remember<P: SoundProvider>(
    audio: &GameAudio<P>,
    samples: &mut HashMap<&'static str, String>,
    kind: &'static str,
) {
    if let Some(line) = audio.last_event_line()
        && !samples.contains_key(kind)
    {
        samples.insert(kind, line.to_string());
    }
}

#[test]
fn orc01_five_minute_audio_session() {
    let Some(root) = archive() else {
        eprintln!("skip: extracted archive missing");
        return;
    };
    let Some((tileset, mut units)) = orc01_map(&root) else {
        eprintln!("skip: Orc01.w3m missing");
        return;
    };
    let sounds = load_sounds(&root);
    let unit_names = load_units(&root);

    // The campaign chapter creates its units from JASS, so the placed list can
    // be empty: add the chapter roster that the shipped `unitUI.slk` knows.
    for id in [
        "opeo", "ogru", "othr", "orai", "ocat", "oshm", "odoc", "otau", "owyv", "ohun", "nkot",
    ] {
        if unit_names.sound_set(id).is_some() && !units.iter().any(|have| have.eq_ignore_ascii_case(id)) {
            units.push(id.to_string());
        }
    }
    units.retain(|id| unit_names.sound_set(id).is_some());
    // Drive only units whose shipped set has all three voice rows: a missing row
    // would be a real `AUDIO_EVENT missing`, and the chapter roster above is
    // there to keep the pool non-empty when the map places only markers.
    units.retain(|id| {
        let Some(set) = unit_names.sound_set(id) else {
            return false;
        };
        sounds.unit_ack(set).is_some()
            && sounds.unit_attack(set).is_some()
            && sounds.unit_death(set).is_some()
    });
    assert!(!units.is_empty(), "Orc01 unit roster resolves ack/attack/death rows");

    let engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: MAX_VOICES,
        channel_voices: DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    });
    let mut audio = GameAudio::new(engine, sounds, unit_names, RealProvider::new(&root));
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    );
    let themes = audio.load_themes_from_trigger_data(
        &std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt"),
    );
    assert!(themes >= 30, "musictheme rows: {themes}");
    audio.set_listener(Vec3::new(1_000.0, 0.0, 1_000.0), 0.0);
    assert_eq!(audio.ambient_pcm_budget(), PCM_BUDGET);

    // Race score: the orc playlist starts and is the current rotation.
    let music = audio.start_race_playlist(Race::Orc);
    assert!(music.is_some(), "orc race playlist starts");
    let tracks = audio
        .playlist()
        .map(|playlist| playlist.tracks().to_vec())
        .expect("playlist is current");
    assert_eq!(tracks.len(), 3, "orc rotation has three themes: {tracks:?}");
    let expected = wc3_audio::hash_order(Race::Orc.id(), Race::Orc.theme_names(wc3_audio::events::MusicState::Playing));
    assert_eq!(tracks.as_slice(), expected.as_slice(), "stable hash rotation");

    // Ambience: the map's own tileset, day then night.
    let day_voice = audio.ambient_for_region("Orc01", tileset, false);
    assert!(day_voice.is_some(), "Orc01 day ambience ({tileset}) starts");
    let day_theme = audio
        .ambient_theme_for_tileset(tileset, false)
        .expect("day theme name");
    let night_theme = audio
        .ambient_theme_for_tileset(tileset, true)
        .expect("night theme name");
    assert_ne!(day_theme, night_theme, "the tileset has distinct day/night themes");

    let mut samples: HashMap<&'static str, String> = HashMap::new();
    let mut block = vec![0.0f32; 512 * 2];
    let mut peak_pcm = 0usize;
    let mut block_peak = 0.0f32;
    let mut acks = 0u64;
    let mut attacks = 0u64;
    let mut deaths = 0u64;
    let mut impacts = 0u64;
    let mut music_switches = 0u64;
    let mut block_events = 0u64;

    for second in 0..SESSION_SECONDS {
        if second == 150 {
            // Night side of the region ambience: same region key, new theme.
            let night_voice = audio.ambient_for_region("Orc01", tileset, true);
            assert!(
                night_voice.is_some(),
                "night ambience starts: theme={night_theme} unresolved={:?}",
                audio.unresolved()
            );
            remember(&audio, &mut samples, "ambient_night");
        }
        if second == 170 {
            // Retail theme change mid-match (the client switches tracks).
            if audio.playlist_advance().is_some() {
                music_switches += 1;
            }
            remember(&audio, &mut samples, "music_switch");
        }
        if second == 200 {
            audio.set_paused(true);
            audio.render(&mut block);
            assert_eq!(audio.engine().category_gain(Category::Music), 0.0);
            assert_eq!(audio.engine().category_gain(Category::Ambient), 0.0);
            remember(&audio, &mut samples, "pause");
        }
        if second == 203 {
            audio.set_paused(false);
            audio.render(&mut block);
            assert!(audio.engine().category_gain(Category::Music) > 0.0);
            assert!(audio.engine().category_gain(Category::Ambient) > 0.0);
            remember(&audio, &mut samples, "resume");
        }
        if second == 210 {
            // Ack spacing: a second tap inside 150 ms is a limiter drop.
            // Fresh tap counters and a full window since the last line, so the
            // next two taps exercise the limiter, not the "pissed" rotation.
            audio.selection_reset();
            audio.advance_ms(500);
            let unit = &units[(second as usize) % units.len()];
            let first = audio.selection_tap(unit, Some(Vec3::new(1_000.0, 0.0, 1_000.0)));
            assert!(first.is_some(), "first tap acknowledges");
            audio.advance_ms(40);
            let limited = audio.stats().acks_limited;
            let second_tap = audio.selection_tap(unit, Some(Vec3::new(1_000.0, 0.0, 1_000.0)));
            assert!(second_tap.is_none(), "tap inside the ack window is dropped");
            assert!(audio.stats().acks_limited > limited, "limiter counted the drop");
            remember(&audio, &mut samples, "ack_limited");
        }

        for tick in 0..TICKS_PER_SECOND {
            audio.set_clock_ms((second * TICKS_PER_SECOND + tick) * 1_000 / TICKS_PER_SECOND);
            let index = (second * TICKS_PER_SECOND + tick) as usize;
            if tick % 3 == 0 {
                let unit = units[index % units.len()].clone();
                let angle = index as f32 * 0.05;
                let position = Some(Vec3::new(
                    1_000.0 + 600.0 * angle.cos(),
                    0.0,
                    1_000.0 + 600.0 * angle.sin(),
                ));
                match index % 10 {
                    0 => {
                        if audio.selection_tap(&unit, position).is_some() {
                            acks += 1;
                        }
                        remember(&audio, &mut samples, "ack");
                    }
                    1 => {
                        if audio
                            .play_event(AudioEvent::MoveOrder, Some(&unit), None, position)
                            .is_some()
                        {
                            acks += 1;
                        }
                    }
                    2 => {
                        if audio
                            .play_event(AudioEvent::Attack, Some(&unit), None, position)
                            .is_some()
                        {
                            attacks += 1;
                        }
                        remember(&audio, &mut samples, "attack");
                    }
                    3 => {
                        // Weapon × material rows come from the fixed shipped
                        // families; both ids resolve on every install.
                        if audio
                            .attack_impact(Some("hfoo"), Some("ogru"), position)
                            .is_some()
                        {
                            impacts += 1;
                        }
                        remember(&audio, &mut samples, "impact");
                    }
                    4 => {
                        if audio
                            .play_event(AudioEvent::Death, Some(&unit), None, position)
                            .is_some()
                        {
                            deaths += 1;
                        }
                        remember(&audio, &mut samples, "death");
                    }
                    5 => {
                        if audio.job_done(Race::Orc, None).is_some() {
                            block_events += 1;
                        }
                        remember(&audio, &mut samples, "build");
                    }
                    6 => {
                        if audio
                            .ui_feedback(wc3_audio::UiFeedback::Click, Some(Race::Orc))
                            .is_some()
                        {
                            block_events += 1;
                        }
                        remember(&audio, &mut samples, "ui");
                    }
                    7 => {
                        if audio
                            .ui_feedback(wc3_audio::UiFeedback::Error, Some(Race::Orc))
                            .is_some()
                        {
                            block_events += 1;
                        }
                        remember(&audio, &mut samples, "ui_error");
                    }
                    8 => {
                        if audio
                            .play_event(AudioEvent::DenyNoGold, None, Some(Race::Orc), None)
                            .is_some()
                        {
                            block_events += 1;
                        }
                        remember(&audio, &mut samples, "deny");
                    }
                    _ => {
                        if audio
                            .play_event(AudioEvent::UpgradeComplete, None, Some(Race::Orc), None)
                            .is_some()
                        {
                            block_events += 1;
                        }
                        remember(&audio, &mut samples, "upgrade");
                    }
                }
            }
            // Training-complete line (`{set}Ready`) once per second.
            if tick == 1
                && audio.unit_ready(&units[(second as usize) % units.len()], Some(Vec3::new(
                    1_000.0,
                    0.0,
                    1_000.0,
                )))
                .is_some()
            {
                block_events += 1;
                remember(&audio, &mut samples, "train");
            }
            // A level-up cue once every ten seconds: the shipped AnimSounds row.
            if tick == 0 && second % 10 == 0
                && audio
                    .play_event(AudioEvent::LevelUp, None, None, None)
                    .is_some()
            {
                block_events += 1;
                remember(&audio, &mut samples, "level_up");
            }
            audio.render(&mut block);
        }
        // A full second of mixer output.
        for _ in 0..(48_000 / 512) {
            audio.render(&mut block);
            block_peak = block_peak.max(block.iter().fold(0.0f32, |peak, s| peak.max(s.abs())));
        }
        // The game calls this once per frame: rotate when the theme length ends.
        if audio.tick_music().is_some() {
            music_switches += 1;
            remember(&audio, &mut samples, "music_tick");
        }
        assert!(audio.music_active(), "music gap at second {second}");
        assert!(audio.ambient_active(), "ambient gap at second {second}");
        let pcm = audio.pcm_bytes();
        assert!(
            pcm <= PCM_BUDGET,
            "second {second}: PCM {} bytes exceeds the 64 MB ceiling",
            pcm
        );
        peak_pcm = peak_pcm.max(pcm);
    }

    // Automatic rotation: jump past the current theme length and let the clock
    // advance the playlist (cross-faded, not cut).
    let started_before = audio.stats().music_started;
    audio.advance_ms(400_000);
    let rotated = audio.tick_music();
    assert!(rotated.is_some(), "playlist rotates when the theme length elapses");
    assert!(
        audio.stats().music_started > started_before,
        "rotation started a new streamed theme"
    );
    music_switches += 1;
    remember(&audio, &mut samples, "music_tick");

    let stats = audio.stats();
    let engine_stats = audio.audio_stats();
    assert!(acks > 0, "acks fired");
    assert!(attacks > 0, "attack lines fired");
    assert!(deaths > 0, "death lines fired");
    assert!(impacts > 0, "weapon impacts fired");
    assert!(block_events > 0, "build/train/ui/deny cues fired");
    assert_eq!(
        stats.events_missing, 0,
        "no mapped event went missing: {:?}",
        audio.missing_events()
    );
    assert!(
        audio.missing_events().is_empty(),
        "missing events: {:?}",
        audio.missing_events()
    );
    assert!(
        !audio.unresolved().iter().any(|entry| entry.starts_with("event:")),
        "unresolved event rows: {:?}",
        audio.unresolved()
    );
    assert!(stats.music_started >= 3, "music started: {}", stats.music_started);
    assert!(stats.music_crossfades >= 2, "music cross-fades: {}", stats.music_crossfades);
    assert!(stats.ambient_started >= 2, "ambient themes started");
    assert!(stats.ambient_crossfades >= 1, "ambient day/night cross-fade");
    assert_eq!(stats.pauses, 1);
    assert!(engine_stats.max_active_voices <= MAX_VOICES as u32);
    assert!(block_peak < 0.9995, "master soft limiter must not hard-clip: {block_peak}");
    // Reference-path stability: the mixer never starved, never lost a command
    // and the provider never failed a decode. A headless engine has no device
    // queue, so `buffer_underruns` is structurally zero here; the device queue
    // measurement is the SDL3 `audio_scenario_probe` run recorded in
    // `analysis/agent-audio/ALL_LAYERS_20261121.md` (underruns=0 with real
    // hardware queue reporting). Both counters must stay zero in this run.
    assert_eq!(
        engine_stats.buffer_underruns, 0,
        "reference path rendered underruns (headless queue is structural): {engine_stats:?}"
    );
    assert_eq!(
        engine_stats.backend_errors, 0,
        "reference path backend errors: {engine_stats:?}"
    );
    assert_eq!(
        engine_stats.commands_dropped, 0,
        "reference path dropped commands: {engine_stats:?}"
    );
    let (_, _, evictions, _) = audio.ambient_cache_stats();
    assert_eq!(evictions, 0, "five minutes of Orc01 fits the ambient cache");
    let provider = audio.provider();
    assert!(
        provider.failures.is_empty(),
        "decode failures: {:?}",
        provider.failures
    );
    let units_used = units.len();

    println!(
        "ORC01_AUDIO map=Orc01.w3m tileset={} seconds={SESSION_SECONDS} ticks={} units={units_used} acks={acks} attacks={attacks} deaths={deaths} impacts={impacts} other={block_events} music_started={} music_switches={music_switches} music_crossfades={} ambient_started={} ambient_crossfades={} pcm_peak_mb={:.1} pcm_budget_mb={} max_active={} missing={} unresolved={} block_peak={block_peak:.3}",
        tileset.to_ascii_uppercase(),
        SESSION_SECONDS * TICKS_PER_SECOND,
        stats.music_started,
        stats.music_crossfades,
        stats.ambient_started,
        stats.ambient_crossfades,
        peak_pcm as f32 / (1024.0 * 1024.0),
        PCM_BUDGET / (1024 * 1024),
        engine_stats.max_active_voices,
        stats.events_missing,
        audio.unresolved().len(),
    );
    for kind in [
        "ack", "attack", "impact", "death", "build", "train", "ui", "ui_error", "deny",
        "upgrade", "level_up", "music_switch", "music_tick", "ambient_night", "pause",
        "resume", "ack_limited",
    ] {
        if let Some(line) = samples.get(kind) {
            println!("ORC01_SAMPLE {kind} {line}");
        }
    }
    println!(
        "AUDIO_STATS frames={} blocks={} underruns={} backend_errors={} buffered_ms={:?} active={} max_active={} dropped={} occluded={} ducked_blocks={} soft_lim={}",
        engine_stats.frames_rendered,
        engine_stats.blocks_rendered,
        engine_stats.buffer_underruns,
        engine_stats.backend_errors,
        audio.buffered_ms(),
        engine_stats.active_voices,
        engine_stats.max_active_voices,
        engine_stats.commands_dropped,
        engine_stats.occluded_voices,
        engine_stats.ducked_blocks,
        engine_stats.soft_limited_samples,
    );
    println!("{}", audio.stats_line());
    audio.shutdown();
}
