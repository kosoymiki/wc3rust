//! Event -> retail file table (ledger 470 / 508 evidence).
//!
//! Plays every `AudioEvent` through the real hook path against the operator's
//! extracted archive and prints the row and file that actually started:
//! `EVENT_ROW event=… group=… def=… file=… kind=…`.
//!
//! The provider decodes the shipped bytes for real. A WAV that does not decode
//! is a failure (`missing=1`), never a synthetic success: this table is the
//! evidence that an event resolves to a playable file.
//!
//! Usage: `h2-cargo run -p wc3-audio --bin audio_event_table`

use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{AudioConfig, AudioEngine, DEFAULT_CHANNEL_VOICES};
use wc3_audio::events::{AudioEvent, AudioGroup, Race};
use wc3_audio::{GameAudio, Sound, SoundInfoDb, SoundProvider, UnitNameMap};

struct TreeProvider {
    root: PathBuf,
    index: std::collections::HashMap<String, String>,
    by_basename: std::collections::HashMap<String, Vec<String>>,
    /// Files the resolver found but could not decode (reported, not masked).
    decode_failures: Vec<String>,
}

impl TreeProvider {
    fn new(root: &std::path::Path) -> Self {
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
            decode_failures: Vec::new(),
        }
    }
}

impl SoundProvider for TreeProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        let file = self.root.join(path.replace('\\', "/"));
        let bytes = std::fs::read(&file).ok()?;
        let decoded = if path.to_ascii_lowercase().ends_with(".mp3") {
            wc3_audio::decode_mp3_bounded(&bytes, 30 * 48_000 * 2).map(|(sound, _)| sound)
        } else {
            wc3_audio::decode_wav(&bytes)
        };
        match decoded {
            Ok(sound) if sound.frames() > 0 => Some(Arc::new(sound)),
            Ok(_) => {
                self.decode_failures.push(format!("{path}: empty PCM"));
                None
            }
            Err(error) => {
                self.decode_failures
                    .push(format!("{path}: {error}"));
                None
            }
        }
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.root.join(path.replace('\\', "/")))
            .ok()
            .map(Arc::from)
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

fn load_tables(root: &std::path::Path) -> (SoundInfoDb, UnitNameMap) {
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
        sounds
            .add_table(name, &std::fs::read(path).expect("slk bytes"))
            .expect("parse slk");
    }
    let units = UnitNameMap::parse(&std::fs::read(root.join("Units/unitUI.slk")).expect("unitUI"))
        .expect("parse unitUI");
    (sounds, units)
}

/// The context the game supplies for each event (unit id, race, weapon/target,
/// ability row), exactly like the production call sites.
fn play_event_with_context(
    audio: &mut GameAudio<TreeProvider>,
    event: AudioEvent,
) -> Option<wc3_audio::engine::VoiceId> {
    match event {
        AudioEvent::Attack | AudioEvent::AttackOrder => {
            audio.play_event(event, Some("hfoo"), None, None)
        }
        AudioEvent::AttackImpact => audio.attack_impact(Some("hfoo"), Some("ogru"), None),
        AudioEvent::Death => audio.play_event(event, Some("hfoo"), None, None),
        AudioEvent::WorkerAck | AudioEvent::MoveOrder | AudioEvent::Selected => {
            audio.play_event(event, Some("hpea"), None, None)
        }
        AudioEvent::TrainComplete => audio.play_event(event, Some("hfoo"), None, None),
        AudioEvent::AbilityCast => audio.spell("BlizzardWave", None),
        AudioEvent::HeroDeath => audio.play_event(event, Some("hfoo"), Some(Race::Human), None),
        event if event.is_race_scoped() => audio.play_event(event, None, Some(Race::Human), None),
        other => audio.play_event(other, None, None, None),
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    if !root.join("UI/SoundInfo/UISounds.slk").is_file() {
        eprintln!("EVENT_TABLE skipped=no-archive root={}", root.display());
        std::process::exit(2);
    }
    let (sounds, units) = load_tables(&root);
    let engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 32,
        channel_voices: DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    });
    let mut audio = GameAudio::new(engine, sounds, units, TreeProvider::new(&root));
    if let Ok(bytes) = std::fs::read(root.join("Units/UnitWeapons.slk")) {
        let rows = audio.load_weapon_table(&bytes);
        println!("EVENT_TABLE weapon_rows={rows}");
    }
    if let Ok(bytes) = std::fs::read(root.join("UI/TriggerData.txt")) {
        let themes = audio.load_themes_from_trigger_data(&bytes);
        println!("EVENT_TABLE themes={themes}");
    }
    audio.set_event_log(std::env::var_os("H2_AUDIO_LOG").is_some());
    audio.set_listener(Vec3::ZERO, 0.0);

    let mut rows = 0usize;
    let mut missing = Vec::new();
    let mut gaps = Vec::new();
    let mut per_group: std::collections::BTreeMap<&'static str, usize> =
        AudioGroup::ALL.iter().map(|group| (group.id(), 0)).collect();
    for event in AudioEvent::ALL {
        audio.advance_ms(500);
        if event.documented_gap() {
            gaps.push(event.id());
            println!(
                "EVENT_ROW event={} group={} gap=1 (no shipped cue)",
                event.id(),
                event.group().id()
            );
            continue;
        }
        let voice = play_event_with_context(&mut audio, event);
        let line = audio.last_event_line().unwrap_or("-").to_string();
        let file = audio.last_sound_file().unwrap_or("-").to_string();
        let def = line
            .split_whitespace()
            .find_map(|token| token.strip_prefix("def="))
            .unwrap_or("-")
            .to_string();
        let kind = if voice.is_none() {
            "none"
        } else if event.direct_file().is_some() {
            "direct"
        } else if file.to_ascii_lowercase().ends_with(".mp3") {
            "mp3"
        } else {
            "wav"
        };
        if voice.is_some() {
            rows += 1;
            *per_group.entry(event.group().id()).or_default() += 1;
            println!(
                "EVENT_ROW event={} group={} def={def} file={file} kind={kind}",
                event.id(),
                event.group().id()
            );
        } else {
            missing.push(event.id());
            println!(
                "EVENT_ROW event={} group={} missing=1 line={line} tried={file}",
                event.id(),
                event.group().id()
            );
        }
    }
    let stats = audio.stats();
    let provider_failures = audio.provider().decode_failures.clone();
    println!(
        "EVENT_GROUPS {}",
        per_group
            .iter()
            .map(|(group, count)| format!("{group}={count}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "EVENT_TABLE rows={rows} missing={missing:?} gaps={gaps:?} events_played={} impacts={} ui_feedback={} unresolved={} decode_failures={:?}",
        stats.events_played,
        stats.impacts_played,
        stats.ui_feedback,
        audio.unresolved().len(),
        provider_failures
    );
    if !missing.is_empty() {
        eprintln!("EVENT_TABLE FAIL: {} mapped events did not start", missing.len());
        std::process::exit(1);
    }
    if rows + gaps.len() != AudioEvent::ALL.len() {
        eprintln!(
            "EVENT_TABLE FAIL: {} rows + {} gaps != {} events",
            rows,
            gaps.len(),
            AudioEvent::ALL.len()
        );
        std::process::exit(1);
    }
    if per_group.values().any(|count| *count == 0) {
        eprintln!("EVENT_TABLE FAIL: an event group has no playable row: {per_group:?}");
        std::process::exit(1);
    }
    if !provider_failures.is_empty() {
        eprintln!("EVENT_TABLE FAIL: decode failures: {provider_failures:?}");
        std::process::exit(1);
    }
    audio.shutdown();
}
