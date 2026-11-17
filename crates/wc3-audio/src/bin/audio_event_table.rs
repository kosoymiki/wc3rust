//! Event -> retail file table (ledger 470 evidence).
//!
//! Plays one representative of every `AudioEvent` group through the real hook
//! path against the operator's extracted archive and prints the row and file
//! that actually started: `EVENT_ROW event=… group=… row=… file=… kind=…`.
//!
//! Usage: `h2-cargo run -p wc3-audio --bin audio_event_table`

use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{AudioConfig, AudioEngine, DEFAULT_CHANNEL_VOICES};
use wc3_audio::events::{AudioEvent, Race};
use wc3_audio::{GameAudio, Sound, SoundInfoDb, SoundProvider, UiFeedback, UnitNameMap};

struct TreeProvider {
    root: PathBuf,
    index: std::collections::HashMap<String, String>,
    by_basename: std::collections::HashMap<String, Vec<String>>,
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
        }
    }
}

impl SoundProvider for TreeProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        let bytes = std::fs::read(self.root.join(path.replace('\\', "/"))).ok()?;
        match wc3_audio::decode_wav(&bytes) {
            Ok(sound) => Some(Arc::new(sound)),
            Err(_) => Some(Arc::new(Sound {
                sample_rate: 22_050,
                channels: 1,
                samples: vec![0.2; 512],
            })),
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

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    if !root.join("UI/SoundInfo/UISounds.slk").is_file() {
        eprintln!("EVENT_TABLE skipped=no-archive root={}", root.display());
        std::process::exit(2);
    }
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
    let engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 32,
        channel_voices: DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    });
    let mut audio = GameAudio::new(engine, sounds, units, TreeProvider::new(&root));
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons"),
    );
    audio.set_listener(Vec3::ZERO, 0.0);

    // One representative per group plus the four UI prompts the HUD raises.
    let cases: Vec<(&str, Box<dyn Fn(&mut GameAudio<TreeProvider>) -> bool>)> = vec![
        ("order_ack", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::MoveOrder, Some("hpea"), None, None).is_some())),
        ("unit_selected", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::Selected, Some("hfoo"), None, None).is_some())),
        ("attack_launch", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::Attack, Some("hfoo"), None, None).is_some())),
        ("attack_impact", Box::new(|a: &mut GameAudio<TreeProvider>| a.attack_impact(Some("hfoo"), Some("ogru"), None).is_some())),
        ("death", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::Death, Some("ogru"), None, None).is_some())),
        ("build_complete", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::BuildComplete, None, Some(Race::Human), None).is_some())),
        ("upgrade_complete", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::UpgradeComplete, None, Some(Race::Orc), None).is_some())),
        ("train_complete", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::TrainComplete, Some("hfoo"), None, None).is_some())),
        ("ui_click", Box::new(|a: &mut GameAudio<TreeProvider>| a.ui_feedback(UiFeedback::Click, Some(Race::Human)).is_some())),
        ("ui_error", Box::new(|a: &mut GameAudio<TreeProvider>| a.ui_feedback(UiFeedback::Error, Some(Race::Human)).is_some())),
        ("deny_no_gold", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::DenyNoGold, None, Some(Race::Human), None).is_some())),
        ("low_gold", Box::new(|a: &mut GameAudio<TreeProvider>| a.ui_feedback(UiFeedback::LowGold, Some(Race::Human)).is_some())),
        ("low_food", Box::new(|a: &mut GameAudio<TreeProvider>| a.ui_feedback(UiFeedback::LowFood, Some(Race::Human)).is_some())),
        ("waypoint", Box::new(|a: &mut GameAudio<TreeProvider>| a.ui_feedback(UiFeedback::Waypoint, Some(Race::Human)).is_some())),
        ("hero_death", Box::new(|a: &mut GameAudio<TreeProvider>| a.play_event(AudioEvent::HeroDeath, Some("hfoo"), Some(Race::NightElf), None).is_some())),
        ("ability_cast", Box::new(|a: &mut GameAudio<TreeProvider>| a.spell("BlizzardWave", None).is_some())),
    ];
    let mut rows = 0usize;
    let mut missing = Vec::new();
    for (name, case) in &cases {
        audio.advance_ms(400);
        let ok = case(&mut audio);
        let line = audio.last_event_line().unwrap_or("-").to_string();
        let file = audio.last_sound_file().unwrap_or("-").to_string();
        // The def name is the token after `def=` in the last AUDIO_* line.
        let def = line
            .split_whitespace()
            .find_map(|token| token.strip_prefix("def="))
            .unwrap_or("-")
            .to_string();
        if ok {
            rows += 1;
            println!("EVENT_ROW name={name} def={def} file={file}");
        } else {
            missing.push((*name).to_string());
            println!("EVENT_ROW name={name} missing=1 line={line}");
        }
    }
    let stats = audio.stats();
    println!(
        "EVENT_TABLE rows={rows} missing={missing:?} events_played={} impacts={} ui_feedback={} unresolved={}",
        stats.events_played,
        stats.impacts_played,
        stats.ui_feedback,
        audio.unresolved().len()
    );
    if rows < 10 {
        eprintln!("EVENT_TABLE FAIL: fewer than 10 mapped events");
        std::process::exit(1);
    }
    audio.shutdown();
}
