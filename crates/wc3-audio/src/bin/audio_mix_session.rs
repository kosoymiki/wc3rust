//! 60-second headless mix session (ledger 470 device check).
//!
//! Plays the real shipped content from the extracted archive: race playlist
//! (streamed MP3), region ambience (MIDI -> DLS render), unit acks, attack
//! launch + weapon impacts, deaths, build completions, UI feedback and deny
//! warnings, with a moving 3-D listener. Prints the counters the ledger asks
//! for (sounds started / culled / evicted, peak PCM) and the `AUDIO_*` lines.
//!
//! Usage: `h2-cargo run -p wc3-audio --bin audio_mix_session -- [seconds]`
//! `H2_AUDIO_LOG=1` prints every `AUDIO_*` line (the counters print either way).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{AudioConfig, AudioEngine, DEFAULT_CHANNEL_VOICES};
use wc3_audio::events::{AudioEvent, Race};
use wc3_audio::{
    GameAudio, Sound, SoundInfoDb, SoundProvider, UiFeedback, UnitNameMap, UnitWeaponSounds,
};

/// Provider over the extracted retail tree (case-insensitive, basename fallback
/// exactly like the production asset database).
struct TreeProvider {
    root: PathBuf,
    index: std::collections::HashMap<String, String>,
    by_basename: std::collections::HashMap<String, Vec<String>>,
    /// Files that resolved but did not decode (reported, never masked).
    failures: Vec<String>,
}

impl TreeProvider {
    fn new(root: &Path) -> Self {
        let mut index = std::collections::HashMap::new();
        let mut by_basename: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
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
            failures: Vec::new(),
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.root.join(path.replace('\\', "/"))
    }
}

impl SoundProvider for TreeProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        // Decode the shipped bytes for real (WAV, MP3 rows in UISounds); a file
        // that does not decode is reported, never replaced with synthetic PCM.
        let bytes = std::fs::read(self.file(path)).ok()?;
        let decoded = if path.to_ascii_lowercase().ends_with(".mp3") {
            wc3_audio::decode_mp3_bounded(&bytes, 30 * 48_000 * 2).map(|(sound, _)| sound)
        } else {
            wc3_audio::decode_wav(&bytes)
        };
        match decoded {
            Ok(sound) => Some(Arc::new(sound)),
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

fn load_tables(root: &Path) -> (SoundInfoDb, UnitNameMap, UnitWeaponSounds, usize) {
    let dir = root.join("UI/SoundInfo");
    let mut sounds = SoundInfoDb::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("UI/SoundInfo")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in &entries {
        if path.extension().and_then(|ext| ext.to_str()) != Some("slk") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("table name");
        let bytes = std::fs::read(path).expect("slk bytes");
        sounds.add_table(name, &bytes).expect("parse slk");
    }
    let units = UnitNameMap::parse(&std::fs::read(root.join("Units/unitUI.slk")).expect("unitUI"))
        .expect("parse unitUI");
    let weapons = UnitWeaponSounds::parse(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons"),
    )
    .expect("parse UnitWeapons");
    (sounds, units, weapons, entries.len())
}

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(60)
        .clamp(5, 600);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    if !root.join("UI/SoundInfo/UISounds.slk").is_file() {
        eprintln!("MIX_SESSION skipped=no-archive root={}", root.display());
        std::process::exit(2);
    }
    let (sounds, units, weapons, tables) = load_tables(&root);
    let engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    });
    let mut audio = GameAudio::new(engine, sounds, units, TreeProvider::new(&root));
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons"),
    );
    let themes = audio.load_themes_from_trigger_data(
        &std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt"),
    );
    audio.set_event_log(true);

    // Regions that actually render on this install.
    let mut regions: Vec<char> = Vec::new();
    for letter in [
        'Y', 'L', 'X', 'D', 'A', 'I', 'F', 'N', 'C', 'J', 'G', 'K', 'B', 'O', 'Q', 'V', 'W', 'Z',
    ] {
        if audio.ambient_theme_for_tileset(letter, false).is_some() {
            regions.push(letter);
        }
    }
    if regions.is_empty() {
        eprintln!("MIX_SESSION skipped=no-ambient-themes");
        std::process::exit(2);
    }
    println!(
        "MIX_SESSION begin seconds={seconds} tables={tables} defs={} units={} weapons={} themes={themes} regions={}",
        audio.sound_table().len(),
        audio.unit_names().len(),
        weapons.len(),
        regions.len()
    );

    audio.start_race_playlist(Race::Human);
    let mut block = vec![0.0f32; 512 * 2];
    let frames_per_second = 48_000usize;
    let block_frames = 512usize;
    let mut peak_pcm = 0usize;
    let mut block_peak = 0.0f32;
    let mut started_events = 0u64;
    let mut started_impacts = 0u64;
    let mut started_music = 0u64;
    let mut paused_rms = 0.0f32;
    let mut playing_rms = 0.0f32;
    let mut paused_acc = 0.0f64;
    let mut paused_samples = 0u64;
    let mut resumed_acc = 0.0f64;
    let mut resumed_samples = 0u64;
    let mut music_bus = 1.0f32;
    let mut ambient_bus = 1.0f32;
    let unit_ids = ["hfoo", "hpea", "ogru", "opeo", "hrif", "uaco", "earc", "hkni"];
    let mut event_index = 0usize;

    for second in 0..seconds {
        // Region change every 10 s, race/music rotation naturally cross-faded.
        if second % 10 == 0 {
            let letter = regions[(second / 10) as usize % regions.len()];
            let night = (second / 10) % 2 == 1;
            let region = format!("Region{}", second / 10);
            if audio.ambient_for_region(&region, letter, night).is_some() {
                println!(
                    "MIX_REGION second={second} region={region} tileset={} night={night}",
                    letter.to_ascii_uppercase()
                );
            }
        }
        // Pause window: retail silences the score, not the interface. The
        // measurement render happens right after the bus change so the number
        // is the paused mix, not a tail of the previous second.
        if second == 30 {
            audio.set_paused(true);
            // Isolate the score: drop the one-shots so the measurement is the
            // music+ambience buses only (the paused interface is still live,
            // which is the retail behaviour and would otherwise mask the mute).
            for category in [
                wc3_audio::Category::Voice,
                wc3_audio::Category::Sfx,
                wc3_audio::Category::Ui,
            ] {
                audio.engine().stop_category(category);
            }
            // One block drains the queued commands before the gains are read.
            audio.render(&mut block);
            music_bus = audio.engine().category_gain(wc3_audio::Category::Music);
            ambient_bus = audio.engine().category_gain(wc3_audio::Category::Ambient);
            let mut sum = 0.0f32;
            let mut samples = 0usize;
            for _ in 0..(frames_per_second / block_frames) {
                audio.render(&mut block);
                sum += block.iter().map(|sample| sample * sample).sum::<f32>();
                samples += block.len();
            }
            paused_rms = (sum / samples.max(1) as f32).sqrt();
            println!(
                "MIX_SCORE paused=1 music_bus={music_bus} ambient_bus={ambient_bus} rms={paused_rms:.5}"
            );
        }
        if second == 35 {
            audio.set_paused(false);
            audio.render(&mut block);
            let mut sum = 0.0f32;
            let mut samples = 0usize;
            for _ in 0..(frames_per_second / block_frames) {
                audio.render(&mut block);
                sum += block.iter().map(|sample| sample * sample).sum::<f32>();
                samples += block.len();
            }
            let resumed_rms = (sum / samples.max(1) as f32).sqrt();
            playing_rms = resumed_rms;
            println!(
                "MIX_SCORE paused=0 music_bus={} ambient_bus={} rms={resumed_rms:.5}",
                audio.engine().category_gain(wc3_audio::Category::Music),
                audio.engine().category_gain(wc3_audio::Category::Ambient)
            );
        }
        // Music rotation every 20 s (hash-ordered playlist, cross-faded).
        if second > 0 && second % 20 == 0 {
            if audio.playlist_advance().is_some() {
                started_music += 1;
                let race = if (second / 20) % 2 == 0 {
                    Race::Human
                } else {
                    Race::Orc
                };
                audio.start_race_playlist(race);
                let tracks = audio.playlist().map(|p| p.tracks().len()).unwrap_or(0);
                println!("MIX_MUSIC second={second} race={} tracks={tracks}", race.id());
            }
        }
        // Eight gameplay events per second on the sim clock.
        for _ in 0..8 {
            audio.advance_ms(125);
            let index = event_index;
            event_index += 1;
            let unit = unit_ids[index % unit_ids.len()];
            let position = Vec3::new(
                ((index * 137) % 3_000) as f32,
                0.0,
                ((index * 71) % 3_000) as f32,
            );
            let voice = match index % 10 {
                0 => audio.play_event(AudioEvent::MoveOrder, Some(unit), None, Some(position)),
                1 => audio.play_event(AudioEvent::Attack, Some(unit), None, Some(position)),
                2 => audio.attack_impact(Some("hfoo"), Some("ogru"), Some(position)),
                3 => audio.play_event(AudioEvent::Death, Some(unit), None, Some(position)),
                4 => audio.play_event(AudioEvent::BuildComplete, None, Some(Race::Human), None),
                5 => audio.ui_feedback(UiFeedback::Click, Some(Race::Human)),
                6 => audio.ui_feedback(UiFeedback::Error, Some(Race::Human)),
                7 => audio.play_event(AudioEvent::DenyNoGold, None, Some(Race::Human), None),
                8 => audio.ui_feedback(UiFeedback::Waypoint, Some(Race::Human)),
                _ => audio.play_event(AudioEvent::TrainComplete, Some(unit), None, Some(position)),
            };
            if voice.is_some() {
                started_events += 1;
                if index % 10 == 2 {
                    started_impacts += 1;
                }
            }
            // Listener walks a circle around the map centre.
            let angle = (index as f32) * 0.05;
            audio.set_listener(
                Vec3::new(1_500.0 + 900.0 * angle.cos(), 0.0, 1_500.0 + 900.0 * angle.sin()),
                angle,
            );
            let mut out = vec![0.0f32; 512 * 2];
            audio.render(&mut out);
        }
        // One second of mixer output.
        let mut rms = 0.0f32;
        let mut samples = 0usize;
        for _ in 0..(frames_per_second / block_frames) {
            audio.render(&mut block);
            let peak = block.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            block_peak = block_peak.max(peak);
            let sum: f32 = block.iter().map(|sample| sample * sample).sum();
            rms += sum;
            samples += block.len();
        }
        let rms = (rms / samples.max(1) as f32).sqrt();
        // Whole-window averages: the 5 s pause and the 4 s after resume are
        // more stable evidence than a single second next to a region change.
        if audio.is_paused() {
            paused_acc += rms as f64 * rms as f64 * samples as f64;
            paused_samples += samples as u64;
        } else if (36..=39).contains(&second) {
            resumed_acc += rms as f64 * rms as f64 * samples as f64;
            resumed_samples += samples as u64;
        }
        let pcm = audio.pcm_bytes();
        peak_pcm = peak_pcm.max(pcm);
        if pcm > audio.ambient_pcm_budget() {
            println!(
                "MIX_BUDGET_VIOLATION second={second} pcm={pcm} budget={}",
                audio.ambient_pcm_budget()
            );
        }
    }

    if paused_samples > 0 {
        paused_rms = (paused_acc / paused_samples as f64).sqrt() as f32;
    }
    if resumed_samples > 0 {
        playing_rms = (resumed_acc / resumed_samples as f64).sqrt() as f32;
    }
    let stats = audio.stats();
    let engine_stats = audio.audio_stats();
    let (entries, cache_bytes, evictions, budget) = audio.ambient_cache_stats();
    let culled = stats.voices_gated + stats.events_gated + stats.acks_gated;
    let decode_failures = audio.provider().failures.clone();
    println!(
        "MIX_SESSION done seconds={seconds} started={started_events} impacts={started_impacts} music_rotations={started_music} culled={culled} evicted={evictions} peak_pcm_mb={:.1} cache_entries={entries} cache_mb={:.1} budget_mb={} active={} max_active={} chan_full={} chan_preempt={} crossfades_music={} crossfades_ambient={} acks_limited={} pauses={} missing={} unresolved={} paused_rms={paused_rms:.4} playing_rms={playing_rms:.4} music_bus_paused={music_bus} ambient_bus_paused={ambient_bus} block_peak={block_peak:.3} decode_failures={}",
        peak_pcm as f32 / (1024.0 * 1024.0),
        cache_bytes as f32 / (1024.0 * 1024.0),
        budget / (1024 * 1024),
        engine_stats.active_voices,
        engine_stats.max_active_voices,
        engine_stats.voices_channel_full,
        engine_stats.voices_channel_preempted,
        stats.music_crossfades,
        stats.ambient_crossfades,
        stats.acks_limited,
        stats.pauses,
        stats.events_missing,
        audio.unresolved().len(),
        decode_failures.len(),
    );
    assert!(
        decode_failures.is_empty(),
        "shipped files that did not decode: {decode_failures:?}"
    );
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
    assert!(
        peak_pcm <= audio.ambient_pcm_budget(),
        "PCM ceiling exceeded: {peak_pcm} > {}",
        audio.ambient_pcm_budget()
    );
    audio.shutdown();
}
