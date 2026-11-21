//! Mechanic tests (audio group): `audio.events` (event -> retail sound),
//! `audio.race-themes` (race/state selection), `audio.spatial` (distance ->
//! gain invariants) and `audio.stability` (queue policy behind the device
//! measurement). Data comes from the operator's extracted archive
//! (`analysis/war3_full3`); each test skips when the extract is absent.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::Vec3;
use wc3_audio::engine::{clamp_buffer_ms, AudioConfig, AudioEngine, PlayParams, DEFAULT_BUFFER_MS};
use wc3_audio::events::{event_defs, AudioEvent, AudioGroup, MusicState, Race};
use wc3_audio::{GameAudio, MapSounds, Sound, SoundInfoDb, SoundProvider, UnitNameMap};

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
        let bytes = std::fs::read(&path).expect("slk bytes");
        sounds.add_table(name, &bytes).expect("parse slk");
    }
    sounds
}

fn load_units(root: &Path) -> UnitNameMap {
    let bytes = std::fs::read(root.join("Units/unitUI.slk")).expect("unitUI.slk");
    UnitNameMap::parse(&bytes).expect("parse unitUI")
}

/// Provider over the extracted archive.
///
/// The retail `SoundInfo` tables carry stale directory bases and mixed case
/// (`Abilities\Spells\...` vs the extracted `abilities/...`), so like the
/// production `AssetDb` this provider resolves exactly, case-insensitively and
/// by basename before giving up. WAVs decode through a synthetic sound so the
/// hook layer is exercised; MP3s are read from the archive for real streaming.
struct ArchiveProvider {
    root: PathBuf,
    index: std::collections::HashMap<String, String>,
    by_basename: std::collections::HashMap<String, Vec<String>>,
}

impl ArchiveProvider {
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
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.root.join(path.replace('\\', "/"))
    }
}

impl SoundProvider for ArchiveProvider {
    fn load_sound(&mut self, _path: &str) -> Option<Arc<Sound>> {
        Some(Arc::new(Sound {
            sample_rate: 22_050,
            channels: 1,
            samples: vec![0.2; 512],
        }))
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
        self.by_basename.get(base).and_then(|paths| paths.first().cloned())
    }
}

/// Provider that corrupts one music path, for the probe-before-fade regression.
struct CorruptMusicProvider {
    inner: ArchiveProvider,
    corrupt: String,
}

impl SoundProvider for CorruptMusicProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        self.inner.load_sound(path)
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        if !self.corrupt.is_empty() && path.eq_ignore_ascii_case(&self.corrupt) {
            return Some(Arc::from(&b"not an mpeg stream"[..]));
        }
        self.inner.load_bytes(path)
    }

    fn exists(&self, path: &str) -> bool {
        self.inner.exists(path)
    }

    fn resolve(&self, path: &str) -> Option<String> {
        self.inner.resolve(path)
    }
}

fn headless() -> AudioEngine {
    AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
        block_frames: 512,
        max_voices: 16,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    })
}

fn unit_name(units: &UnitNameMap, id: &str) -> Option<String> {
    units
        .sound_set(id)
        .or_else(|| units.name(id))
        .map(str::to_string)
}

/// Play one event through its production entry point with the context the game
/// supplies for that event (unit id, race, weapon/target, ability row).
fn play_event_with_context(
    audio: &mut GameAudio<ArchiveProvider>,
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
        AudioEvent::BuildStart
        | AudioEvent::BuildLoop
        | AudioEvent::BuildComplete
        | AudioEvent::UpgradeComplete
        | AudioEvent::DenyNoGold
        | AudioEvent::DenyNoLumber
        | AudioEvent::DenyNoFood
        | AudioEvent::LowGold
        | AudioEvent::LowFood
        | AudioEvent::GoldMineCollapse => {
            audio.play_event(event, None, Some(Race::Human), None)
        }
        other => audio.play_event(other, None, None, None),
    }
}

/// `audio.events`: every event the game can raise has retail data except the
/// three documented gaps, and each event's definitions exist in the shipped
/// tables.
#[test]
fn audio_events_every_retail_event_has_a_mapped_sound() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let units = load_units(&root);
    assert!(
        sounds.len() > 2_000,
        "retail sound tables loaded: {}",
        sounds.len()
    );
    assert!(units.len() > 400, "unit names loaded: {}", units.len());
    // The weapon-impact path needs both shipped tables.
    let weapons = wc3_audio::UnitWeaponSounds::parse(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    )
    .expect("parse UnitWeapons");
    assert!(weapons.len() >= 140, "weapon rows: {}", weapons.len());
    assert!(
        units.material_count() * 2 > units.len(),
        "material classes: {} of {} rows",
        units.material_count(),
        units.len()
    );

    let mut mapped = Vec::new();
    let mut absent = Vec::new();
    for event in AudioEvent::ALL {
        let ok = match event {
            AudioEvent::Attack => ["hfoo", "hpea", "ogru"].iter().any(|id| {
                unit_name(&units, id)
                    .and_then(|name| sounds.unit_attack(&name))
                    .is_some()
            }),
            AudioEvent::AttackImpact => {
                // `<weapType1><material>` must exist for a reference attacker.
                weapons.weapon_type("hfoo").is_some_and(|weapon| {
                    units
                        .material("ogru")
                        .is_some_and(|material| sounds.get(&format!("{weapon}{}", material.suffix())).is_some())
                })
            }
            AudioEvent::Death => ["hfoo", "hpea", "ogru"].iter().any(|id| {
                unit_name(&units, id)
                    .and_then(|name| sounds.unit_death(&name))
                    .is_some()
            }),
            AudioEvent::WorkerAck | AudioEvent::Selected => {
                ["hpea", "opeo", "uaco", "ewsp"].iter().any(|id| {
                    unit_name(&units, id)
                        .and_then(|name| sounds.unit_ack(&name))
                        .is_some()
                })
            }
            AudioEvent::AbilityCast => sounds.get("BlizzardWave").is_some(),
            AudioEvent::MoveOrder | AudioEvent::AttackOrder => ["hfoo", "hpea"].iter().any(|id| {
                unit_name(&units, id).is_some_and(|name| {
                    sounds.unit_attack(&name).is_some() || sounds.unit_ack(&name).is_some()
                })
            }),
            AudioEvent::TrainComplete => ["hfoo", "hpea"].iter().any(|id| {
                unit_name(&units, id).is_some_and(|name| sounds.get(&format!("{name}Ready")).is_some())
            }),
            AudioEvent::UiClick => sounds.get("InterfaceClick").is_some(),
            AudioEvent::UiError => sounds
                .get("InterfaceError")
                .or_else(|| sounds.get("ErrorMessage"))
                .is_some(),
            AudioEvent::DepositGold => event
                .direct_file()
                .is_some_and(|file| root.join(file.replace('\\', "/")).is_file()),
            other => event_defs(other, None)
                .iter()
                .any(|name| sounds.get(name).is_some()),
        };
        if ok {
            mapped.push(event.id());
        } else {
            absent.push(event.id());
        }
    }

    assert_eq!(mapped.len(), 33, "mapped={mapped:?} absent={absent:?}");
    assert_eq!(
        absent,
        vec!["deposit_lumber"],
        "no shipped lumber-receipt cue exists; it stays silent and is reported once"
    );

    // Every definition named for an event exists in the loaded tables.
    for event in AudioEvent::ALL {
        let defs = event_defs(event, None);
        if !defs.is_empty() {
            assert!(
                defs.iter().any(|name| sounds.get(name).is_some()),
                "{}: none of {defs:?} exists",
                event.id()
            );
        }
    }
    // Race-specific rows all exist (building complete, upgrade, deny, warnings).
    for race in Race::ALL {
        let defs = event_defs(AudioEvent::BuildComplete, Some(race));
        assert_eq!(defs.len(), 1, "{race:?}");
        assert!(sounds.get(defs[0]).is_some(), "{race:?}: {}", defs[0]);
        for event in [
            AudioEvent::UpgradeComplete,
            AudioEvent::DenyNoGold,
            AudioEvent::DenyNoLumber,
            AudioEvent::DenyNoFood,
            AudioEvent::LowGold,
            AudioEvent::LowFood,
            AudioEvent::HeroDeath,
            AudioEvent::GoldMineCollapse,
        ] {
            let defs = event_defs(event, Some(race));
            assert!(
                defs.first().is_some_and(|name| sounds.get(name).is_some()),
                "{} {race:?}: {defs:?}",
                event.id()
            );
        }
    }
}

/// `audio.events`: playing every mapped event through the hook layer starts a
/// voice; only the documented gaps stay silent.
#[test]
fn audio_events_every_mapped_event_starts_a_voice() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    );

    let mut started = 0;
    for event in AudioEvent::ALL {
        let voice = play_event_with_context(&mut audio, event);
        match event {
            AudioEvent::DepositLumber => {
                assert!(
                    voice.is_none(),
                    "{} must stay silent (no shipped cue)",
                    event.id()
                );
            }
            _ => {
                assert!(voice.is_some(), "{} stayed silent despite data", event.id());
                started += 1;
            }
        }
        // The ack limiter is a per-unit window: separate units per event and a
        // gapped clock keep this matrix a mapping test, not a limiter test.
        audio.advance_ms(500);
    }
    assert_eq!(started, 33, "events with a started voice");
    assert_eq!(
        audio.missing_events(),
        &["deposit_lumber"],
        "only the documented gap is reported missing"
    );
}

/// Every mapped event must name a shipped file that actually decodes.
///
/// The mapping matrix above uses a synthetic PCM buffer for speed, so it cannot
/// catch a row whose WAV is absent or corrupt. This walks the same mapping to
/// the real archive and decodes the bytes (WAV fully, MP3 header probe): one
/// missing file is a defect, not a silent fallback.
#[test]
fn audio_events_every_mapped_event_has_a_decodable_file() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let units = load_units(&root);
    let weapons = wc3_audio::UnitWeaponSounds::parse(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    )
    .expect("parse UnitWeapons");
    let provider = ArchiveProvider::new(&root);

    fn unit_def_paths(
        sounds: &SoundInfoDb,
        units: &UnitNameMap,
        unit_id: &str,
        kind: &str,
    ) -> Vec<String> {
        let Some(set) = units.sound_set(unit_id) else {
            return Vec::new();
        };
        let def = match kind {
            "attack" => sounds.unit_attack(set),
            "death" => sounds.unit_death(set),
            "ack" => sounds.unit_ack(set),
            "ready" => sounds.unit_ready(set),
            _ => None,
        };
        def.map(|def| def.paths()).unwrap_or_default()
    }

    let candidates = |event: AudioEvent| -> Vec<String> {
        match event {
            AudioEvent::Attack | AudioEvent::AttackOrder => {
                unit_def_paths(&sounds, &units, "hfoo", "attack")
            }
            AudioEvent::Death => unit_def_paths(&sounds, &units, "ogru", "death"),
            AudioEvent::WorkerAck | AudioEvent::MoveOrder | AudioEvent::Selected => {
                unit_def_paths(&sounds, &units, "hpea", "ack")
            }
            AudioEvent::TrainComplete => unit_def_paths(&sounds, &units, "hfoo", "ready"),
            AudioEvent::AttackImpact => {
                let weapon = weapons.weapon_type("hfoo").expect("hfoo weapon family");
                let material = units.material("ogru").unwrap_or(wc3_audio::events::Material::Flesh);
                sounds
                    .get(&format!("{weapon}{}", material.suffix()))
                    .map(|def| def.paths())
                    .unwrap_or_default()
            }
            AudioEvent::AbilityCast => sounds
                .get("BlizzardWave")
                .map(|def| def.paths())
                .unwrap_or_default(),
            AudioEvent::UiClick => [
                "InterfaceClick",
                "GlueScreenClick",
                "MenuButtonClick",
                "AutoCastButtonClick",
            ]
            .iter()
            .filter_map(|name| sounds.get(name))
            .flat_map(|def| def.paths())
            .collect(),
            AudioEvent::UiError => ["InterfaceError", "ErrorMessage"]
                .iter()
                .filter_map(|name| sounds.get(name))
                .flat_map(|def| def.paths())
                .collect(),
            other => match other.direct_file() {
                Some(file) => vec![file.to_string()],
                None => event_defs(other, Some(Race::Human))
                    .iter()
                    .filter_map(|name| sounds.get(name))
                    .flat_map(|def| def.paths())
                    .collect(),
            },
        }
    };

    let mut checked = 0usize;
    for event in AudioEvent::ALL {
        if event.documented_gap() {
            continue;
        }
        let paths = candidates(event);
        assert!(!paths.is_empty(), "{}: no candidate path in the shipped tables", event.id());
        let mut decoded = None;
        let mut failures = Vec::new();
        for requested in &paths {
            let Some(path) = provider.resolve(requested) else {
                failures.push(format!("{requested}: unresolved"));
                continue;
            };
            let bytes = std::fs::read(root.join(path.replace('\\', "/"))).expect("read sound file");
            if path.to_ascii_lowercase().ends_with(".mp3") {
                match wc3_audio::probe_mp3(&bytes).ok() {
                    Some(info) if info.frames > 0 => {
                        decoded = Some(path);
                        break;
                    }
                    _ => failures.push(format!("{path}: mp3 probe failed")),
                }
            } else {
                match wc3_audio::decode_wav(&bytes) {
                    Ok(sound) if sound.frames() > 0 && !sound.is_silent() => {
                        decoded = Some(path);
                        break;
                    }
                    Ok(_) => failures.push(format!("{path}: empty/silent PCM")),
                    Err(error) => failures.push(format!("{path}: {error}")),
                }
            }
        }
        assert!(
            decoded.is_some(),
            "{}: no decodable shipped file; tried {failures:?}",
            event.id()
        );
        checked += 1;
    }
    assert_eq!(checked, 33, "every mapped event decoded a real file");
    println!("EVENT_FILES decoded={checked} documented_gaps=1");
}

/// `audio.events.ack`: order/selection/training acknowledgements resolve through
/// the unit's own `unitSound` set and start a voice.
#[test]
fn audio_events_ack_group_maps_order_selection_and_ready() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let mut rows = Vec::new();
    for event in AudioEvent::ALL.iter().copied().filter(|event| event.group() == AudioGroup::Ack) {
        let voice = play_event_with_context(&mut audio, event).unwrap_or_else(|| {
            panic!("{} starts no acknowledgement", event.id())
        });
        let line = audio.last_event_line().unwrap_or_default().to_string();
        rows.push(format!("{} voice={} {line}", event.id(), voice.0));
        audio.advance_ms(500);
    }
    assert_eq!(rows.len(), 5, "ack group covers 5 events: {rows:?}");
    let stats = audio.stats();
    assert_eq!(stats.acks_started, stats.acks_requested, "every ack started");
    assert_eq!(stats.acks_missing, 0);
    // The set is the unit's own shipped family, not a race default.
    assert!(rows.iter().any(|row| row.contains("def=FootmanYesAttack")), "{rows:?}");
    assert!(rows.iter().any(|row| row.contains("def=PeasantWhat")), "{rows:?}");
    assert!(rows.iter().any(|row| row.contains("def=FootmanReady")), "{rows:?}");
}

/// `audio.events.attack`: the launch line is the unit's `{set}YesAttack` row and
/// the impact is the shipped `<weapType1><material>` combat row.
#[test]
fn audio_events_attack_group_launch_and_impact_rows() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    );
    assert_eq!(audio.weapon_table().weapon_type("hfoo"), Some("MetalMediumSlice"));
    assert_eq!(audio.weapon_table().weapon_type("ogru"), Some("MetalMediumChop"));
    assert_eq!(audio.unit_names().material("ogru"), Some(wc3_audio::Material::Flesh));
    assert_eq!(audio.unit_names().material("hfoo"), Some(wc3_audio::Material::Metal));

    let launch = audio
        .play_event(AudioEvent::Attack, Some("hfoo"), None, None)
        .expect("launch line");
    assert!(launch.0 > 0);
    let impact = audio
        .attack_impact(Some("hfoo"), Some("ogru"), None)
        .expect("impact row");
    println!(
        "ATTACK_GROUP launch={} impact={} line={}",
        launch.0,
        impact.0,
        audio.last_event_line().unwrap_or_default()
    );
    let line = audio.last_event_line().unwrap_or_default().to_string();
    assert!(line.contains("row=MetalMediumSliceFlesh"), "{line}");
    assert_eq!(audio.stats().impacts_played, 1);
    // Order/launch limiter: the same unit inside the window does not stack.
    assert!(audio.play_event(AudioEvent::AttackOrder, Some("hfoo"), None, None).is_none());
    assert_eq!(audio.stats().acks_limited, 1);
}

/// `audio.events.death`: unit death, hero death per race and mine collapse all
/// resolve; the death row is not replaced by the map-wide ack row.
#[test]
fn audio_events_death_group_rows() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let mut lines = Vec::new();
    for event in AudioEvent::ALL.iter().copied().filter(|event| event.group() == AudioGroup::Death) {
        for race in Race::ALL {
            let voice = play_event_with_context_race(&mut audio, event, race);
            assert!(voice.is_some(), "{} {race:?} stayed silent", event.id());
            lines.push(format!("{} {} {:?}", event.id(), race.id(), audio.last_event_line()));
            audio.advance_ms(400);
        }
    }
    assert_eq!(lines.len(), 12, "3 death events x 4 races");
    assert!(lines.iter().any(|line| line.contains("HeroDiesHuman")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("GoldMineCollapseOrc")), "{lines:?}");
    // Death rows are their own family (`{set}Death`), never the ack row.
    assert!(lines.iter().any(|line| line.contains("def=FootmanDeath")), "{lines:?}");
}

fn play_event_with_context_race(
    audio: &mut GameAudio<ArchiveProvider>,
    event: AudioEvent,
    race: Race,
) -> Option<wc3_audio::engine::VoiceId> {
    match event {
        AudioEvent::Death | AudioEvent::HeroDeath => {
            audio.play_event(event, Some("hfoo"), Some(race), None)
        }
        other => audio.play_event(other, None, Some(race), None),
    }
}

/// `audio.events.build`: construction start/loop, repair, completion and upgrade
/// all map to their per-race rows.
#[test]
fn audio_events_build_group_rows() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let mut lines = Vec::new();
    for event in AudioEvent::ALL.iter().copied().filter(|event| event.group() == AudioGroup::Build) {
        for race in Race::ALL {
            let voice = audio
                .play_event(event, None, Some(race), None)
                .unwrap_or_else(|| panic!("{} {} stayed silent", event.id(), race.id()));
            assert!(voice.0 > 0);
            lines.push(format!("{} {} {:?}", event.id(), race.id(), audio.last_event_line()));
        }
    }
    assert_eq!(lines.len(), 20, "5 build events x 4 races");
    assert!(lines.iter().any(|line| line.contains("JobDoneSoundHuman")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("UpgradeCompleteUndead")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("BuildingConstructionLoop")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("Repair")), "{lines:?}");
    // Race scoping is data, not a guess: without a race the event is not played.
    assert!(audio.play_event(AudioEvent::BuildComplete, None, None, None).is_none());
}

/// `audio.events.ui`: click, waypoint, item and quest cues are UI-bus rows, and
/// a `UiAction` result string classifies onto the same prompts.
#[test]
fn audio_events_ui_group_rows() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let mut lines = Vec::new();
    for event in AudioEvent::ALL.iter().copied().filter(|event| event.group() == AudioGroup::Ui) {
        let voice = match event {
            AudioEvent::AbilityCast => play_event_with_context(&mut audio, event),
            other => audio.play_event(other, None, None, None),
        }
        .unwrap_or_else(|| panic!("{} stayed silent", event.id()));
        assert!(voice.0 > 0);
        lines.push(format!("{} {voice:?} {:?}", event.id(), audio.last_event_line()));
    }
    assert_eq!(lines.len(), 11, "UI group covers 11 events");
    assert!(lines.iter().any(|line| line.contains("RallyPointPlace")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("ItemGet")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("QuestCompleted")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("Levelupcaster")), "{lines:?}");

    // A `UiAction` result string drives the same prompts.
    for (result, feedback) in [
        ("waypoint set", wc3_audio::UiFeedback::Waypoint),
        ("not enough gold", wc3_audio::UiFeedback::LowGold),
        ("not enough food", wc3_audio::UiFeedback::LowFood),
        ("cannot place there", wc3_audio::UiFeedback::Error),
        ("ok", wc3_audio::UiFeedback::Click),
    ] {
        assert_eq!(wc3_audio::UiFeedback::for_result(result), Some(feedback), "{result}");
        assert!(
            audio.ui_feedback(feedback, Some(Race::Human)).is_some(),
            "ui_feedback({}) stayed silent",
            feedback.id()
        );
    }
    assert!(audio.stats().ui_feedback >= 5);
}

/// `audio.events.error`: deny/warning cues resolve per race, and a missing event
/// is reported exactly once (`AUDIO_EVENT missing=…`) instead of every frame.
#[test]
fn audio_events_error_group_and_missing_once() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let mut lines = Vec::new();
    for event in AudioEvent::ALL.iter().copied().filter(|event| event.group() == AudioGroup::Error) {
        if matches!(event, AudioEvent::DepositGold | AudioEvent::DepositLumber) {
            continue;
        }
        let voice = audio
            .play_event(event, None, Some(Race::NightElf), None)
            .unwrap_or_else(|| panic!("{} stayed silent", event.id()));
        assert!(voice.0 > 0);
        lines.push(format!("{} {voice:?} {:?}", event.id(), audio.last_event_line()));
    }
    assert_eq!(lines.len(), 6, "error group covers 6 resolvable events: {lines:?}");
    assert!(lines.iter().any(|line| line.contains("NoGoldNightElf")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("GoldMineLowNightElf")), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("NoFoodNightElf")), "{lines:?}");

    // Lumber receipt has no shipped cue: report once, keep counting.
    assert!(audio.play_event(AudioEvent::DepositLumber, None, Some(Race::Human), None).is_none());
    let first = audio.last_event_line().unwrap_or_default().to_string();
    assert!(first.contains("missing=deposit_lumber"), "{first}");
    assert_eq!(audio.stats().events_missing, 1);
    assert!(audio.play_event(AudioEvent::DepositLumber, None, Some(Race::Human), None).is_none());
    assert_eq!(audio.stats().events_missing, 2, "every gap is counted");
    assert_eq!(
        audio.missing_events(),
        &["deposit_lumber"],
        "one missing record per event id: the AUDIO_EVENT missing line is not repeated"
    );
    println!("ERROR_GROUP lines={lines:?} missing_once={first}");
}

/// `audio.race-themes`: playing/victory/defeat themes resolve for all four
/// races from `TriggerData.txt`, and the shipped MP3 exists for each.
#[test]
fn audio_race_themes_victory_defeat_resolve() {
    let Some(root) = archive() else {
        return;
    };
    let trigger = std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt");
    let mut audio = GameAudio::new(
        headless(),
        SoundInfoDb::new(),
        UnitNameMap::default(),
        ArchiveProvider::new(&root),
    );
    let added = audio.load_themes_from_trigger_data(&trigger);
    assert!(added >= 30, "musictheme rows: {added} (RoC 30, TFT 42)");
    let themes: Vec<(String, String)> = audio.themes().to_vec();

    let mut checked = 0;
    for race in Race::ALL {
        for state in [
            MusicState::Playing,
            MusicState::Victory,
            MusicState::Defeat,
        ] {
            let names = race.theme_names(state);
            let found = names.iter().find_map(|name| {
                themes
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
            });
            let (key, path) =
                found.unwrap_or_else(|| panic!("{race:?} {state:?} missing in {names:?}"));
            assert!(
                root.join(path.replace('\\', "/")).is_file(),
                "{key}: {path} missing from the archive"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 12, "4 races x playing/victory/defeat");

    for race in Race::ALL {
        assert!(
            audio.music_for_race(race, MusicState::Victory).is_some(),
            "{race:?} victory stinger did not start"
        );
        assert!(
            audio.music_for_race(race, MusicState::Defeat).is_some(),
            "{race:?} defeat stinger did not start"
        );
    }
}

fn energy_at(distance: f32, reference_distance: f32) -> f32 {
    let mut engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 4,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    });
    engine.set_listener(Vec3::ZERO, 0.0);
    let sound = Arc::new(Sound {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.25; 48_000],
    });
    engine.play(
        sound,
        PlayParams {
            position: Some(Vec3::new(distance, 0.0, 0.0)),
            reference_distance,
            looping: true,
            ..Default::default()
        },
    );
    let mut block = vec![0.0f32; 512 * 2];
    let mut energy = 0.0f32;
    for _ in 0..8 {
        engine.render(&mut block);
        energy += block.iter().map(|sample| sample * sample).sum::<f32>();
    }
    energy
}

/// `audio.spatial`: unity inside the reference radius, inverse-square beyond,
/// and strict monotonic decay with distance.
#[test]
fn audio_spatial_attenuation_follows_reference_distance() {
    let reference = 100.0;
    let near = energy_at(50.0, reference);
    let at_reference = energy_at(100.0, reference);
    let twice = energy_at(200.0, reference);
    let four = energy_at(400.0, reference);

    assert!(at_reference > 0.0, "audible at the reference distance");
    assert!(
        (near - at_reference).abs() <= at_reference * 0.02,
        "inside the reference radius is unity: near={near} at_ref={at_reference}"
    );
    // gain = (ref/d)^2 beyond the radius, energy = gain^2.
    assert!(
        twice < at_reference * 0.10 && twice > 0.0,
        "2x reference: {twice} vs {at_reference}"
    );
    assert!(
        four < twice * 0.10 && four > 0.0,
        "4x reference: {four} vs {twice}"
    );
}

/// A larger reference radius must keep a source audible farther away.
#[test]
fn audio_spatial_larger_reference_radius_raises_far_gain() {
    let small = energy_at(400.0, 100.0);
    let large = energy_at(400.0, 200.0);
    assert!(
        large > small * 10.0,
        "reference 200 vs 100 at 400 units: {large} vs {small}"
    );
}

/// `audio.stability`: mixer policy behind the device measurement
/// (underruns=0, queue ~100-130 ms observed on device).
#[test]
fn audio_stability_buffer_policy_is_a_150ms_queue() {
    assert_eq!(DEFAULT_BUFFER_MS, 150, "device queue target");
    assert!(
        (100..=200).contains(&DEFAULT_BUFFER_MS),
        "target inside the device-verified band"
    );
    assert_eq!(clamp_buffer_ms(0), 20, "never a 1 ms queue");
    assert_eq!(clamp_buffer_ms(5_000), 1_000, "never seconds of latency");
    assert_eq!(clamp_buffer_ms(150), 150);
    // The pacing keeps the queue below the target while topping up at
    // target - 2 blocks; for the device config that is the 100-130 ms band.
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        ..Default::default()
    };
    let block_ms = (config.block_frames as f64 / config.sample_rate as f64 * 1_000.0).ceil() as u32;
    let low = DEFAULT_BUFFER_MS.saturating_sub(block_ms * 2).max(block_ms / 2);
    assert!(
        (100..=130).contains(&low),
        "low watermark {low} ms sits in the device-measured band"
    );
}

/// `audio.unit-voices`: every unit resolves to its own `UnitUI.unitSound` set;
/// the four workers resolve to four distinct sets and each mapped
/// ack/attack/death definition exists in the shipped `SoundInfo` tables.
/// Prints `UNIT_VOICE def=<id> set=<set> first_ack=<def>` for the 8 reference
/// defs (run with `--nocapture` for the device-report table).
#[test]
fn audio_unit_voices_each_unit_uses_its_own_sound_set() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let units = load_units(&root);

    let reference = [
        ("hpea", "Peasant"),
        ("opeo", "Peon"),
        ("uaco", "Acolyte"),
        ("ewsp", "Wisp"),
        ("hfoo", "Footman"),
        ("ogru", "Grunt"),
        ("ugho", "Ghoul"),
        ("earc", "Archer"),
    ];
    let mut sets = std::collections::HashSet::new();
    for (def, expected) in reference {
        let set = units
            .sound_set(def)
            .unwrap_or_else(|| panic!("{def}: no unitSound row"));
        let name = units.name(def).unwrap_or("<no name>");
        let ack = sounds
            .unit_ack(set)
            .unwrap_or_else(|| panic!("{def}: set {set} has no ack definition"));
        println!(
            "UNIT_VOICE def={def} name={name} set={set} first_ack={}",
            ack.name
        );
        assert_eq!(set, expected, "{def} must use its own retail sound set");
        assert!(
            sounds.get(&ack.name).is_some(),
            "{def}: {} missing from the table",
            ack.name
        );
        assert!(
            ack.name.to_ascii_lowercase().starts_with(&set.to_ascii_lowercase()),
            "{def}: {} does not belong to set {set}",
            ack.name
        );
        sets.insert(set.to_string());
    }
    assert_eq!(sets.len(), 8, "the 8 reference units use 8 distinct sets");

    let workers = ["hpea", "opeo", "uaco", "ewsp"];
    let worker_sets: std::collections::HashSet<String> = workers
        .iter()
        .map(|def| units.sound_set(def).expect("worker set").to_string())
        .collect();
    assert_eq!(worker_sets.len(), 4, "four workers -> four distinct sets");

    for def in workers {
        let set = units.sound_set(def).expect("worker set");
        let attack = sounds
            .unit_attack(set)
            .unwrap_or_else(|| panic!("{def}: set {set} has no attack definition"));
        let death = sounds
            .unit_death(set)
            .unwrap_or_else(|| panic!("{def}: set {set} has no death definition"));
        println!(
            "UNIT_VOICE_EVENTS def={def} set={set} attack={} death={}",
            attack.name, death.name
        );
        assert!(attack.name.to_ascii_lowercase().starts_with(&set.to_ascii_lowercase()));
        assert!(death.name.to_ascii_lowercase().starts_with(&set.to_ascii_lowercase()));
    }

    // The hook layer plays the set-based definitions end to end: ack on
    // selection, attack on order and death on death, all from the unit's set.
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    for def in workers {
        let set = units.sound_set(def).expect("worker set");
        let (ack, _) = audio
            .unit_ack_set(set, None)
            .unwrap_or_else(|| panic!("{def}: set {set} ack did not start"));
        let (attack, _) = audio
            .unit_attack_set(set, None)
            .unwrap_or_else(|| panic!("{def}: set {set} attack did not start"));
        let (death, _) = audio
            .unit_death_set(set, None)
            .unwrap_or_else(|| panic!("{def}: set {set} death did not start"));
        println!("UNIT_VOICE_PLAY def={def} set={set} ack={ack} attack={attack} death={death}");
        for name in [&ack, &attack, &death] {
            assert!(
                name.to_ascii_lowercase().starts_with(&set.to_ascii_lowercase()),
                "{def}: {name} is not from set {set}"
            );
        }
    }
}

/// `audio.unit-voices`: the exact per-unit set table the acceptance gate reads.
/// Five reference ids (`opeo`, `hgrunt`, `hfoo`, `hpea`, `nftr`) must resolve to
/// five distinct shipped `unitSound` sets, and each set's ack/attack/death/
/// pissed rows must exist in the loaded `SoundInfo` tables.
///
/// `hgrunt` is the campaign/map Grunt id: it has no `unitUI.slk` row and must
/// resolve through the documented alias to `ogru`'s `Grunt` family instead of
/// falling back to a generic kind sound.
#[test]
fn audio_unit_sets_table_matches_shipped_rows() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let units = load_units(&root);

    let expected = [
        ("opeo", "Peon"),
        ("hgrunt", "Grunt"),
        ("hfoo", "Footman"),
        ("hpea", "Peasant"),
        ("nftr", "ForestTroll"),
    ];
    let mut sets = std::collections::HashSet::new();
    for (def, set) in expected {
        let resolved = units
            .sound_set(def)
            .unwrap_or_else(|| panic!("{def}: no sound set"));
        assert_eq!(resolved, set, "{def} resolves to its shipped unitSound row");
        let profile = units.profile(def).expect("profile");
        assert_eq!(profile.set, set, "{def} profile set");
        let ack = sounds
            .unit_ack(set)
            .unwrap_or_else(|| panic!("{def}: {set} has no ack row"));
        let attack = sounds
            .unit_attack(set)
            .unwrap_or_else(|| panic!("{def}: {set} has no attack row"));
        let death = sounds
            .unit_death(set)
            .unwrap_or_else(|| panic!("{def}: {set} has no death row"));
        let pissed = sounds
            .unit_pissed(set)
            .unwrap_or_else(|| panic!("{def}: {set} has no pissed row"));
        for row in [&ack.name, &attack.name, &death.name, &pissed.name] {
            assert!(
                row.to_ascii_lowercase().starts_with(&set.to_ascii_lowercase()),
                "{def}: {row} does not belong to set {set}"
            );
            assert!(sounds.get(row).is_some(), "{def}: {row} missing");
        }
        println!(
            "UNIT_SET_TABLE def={def} set={set} ack={} attack={} death={} pissed={}",
            ack.name, attack.name, death.name, pissed.name
        );
        sets.insert(set.to_string());
    }
    assert_eq!(sets.len(), 5, "five ids -> five distinct sets");

    // `hgrunt` has no raw row: the alias is what keeps it audible.
    assert!(
        units.raw_sound_set("hgrunt").is_none(),
        "hgrunt is not a shipped unitUI row"
    );
    assert_eq!(units.resolve_id("hgrunt").as_deref(), Some("ogru"));
    assert_eq!(
        units.aliases(),
        vec![("hgrunt".to_string(), "ogru".to_string())],
        "documented alias table"
    );

    // The hook layer resolves the same sets by def id and actually starts the
    // shipped clips (not a shared/kind sound).
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    for (def, set) in expected {
        let ack = audio
            .unit_ack(def, None)
            .unwrap_or_else(|| panic!("{def}: ack did not start"));
        // The ack limiter is per unit and shared with the attack line.
        audio.advance_ms(200);
        let attack = audio
            .unit_attack(def, None)
            .unwrap_or_else(|| panic!("{def}: attack did not start"));
        let death = audio
            .unit_death(def, None)
            .unwrap_or_else(|| panic!("{def}: death did not start"));
        let pissed = audio
            .unit_pissed(def, None)
            .unwrap_or_else(|| panic!("{def}: pissed did not start"));
        let line = audio.last_event_line().unwrap_or_default();
        assert!(
            line.contains(&format!("set={set}")),
            "{def}: last line {line} does not name the set"
        );
        println!(
            "UNIT_SET_PLAY def={def} set={set} voices={},{},{},{}",
            ack.0, attack.0, death.0, pissed.0
        );
    }
}

/// `audio.selection-taps`: the retail pissed counter (every 4th tap on the same
/// unit) and the stats/log line the device asserts.
#[test]
fn audio_selection_taps_ack_then_pissed() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    for tap in 1..=4 {
        // The ack limiter runs on the deterministic gameplay clock; a human
        // double-tap cadence is ~300 ms apart.
        audio.advance_ms(300);
        let voice = audio
            .selection_tap("hfoo", None)
            .unwrap_or_else(|| panic!("tap {tap} stayed silent"));
        let line = audio.last_event_line().unwrap_or_default();
        println!("TAP {tap} voice={} line={line}", voice.0);
        if tap < 4 {
            assert!(line.starts_with("AUDIO_ACK"), "tap {tap}: {line}");
        } else {
            assert!(line.starts_with("AUDIO_PISSED"), "tap {tap}: {line}");
            assert!(line.contains("def=FootmanPissed"), "tap {tap}: {line}");
        }
    }
    assert_eq!(audio.stats().selection_taps, 4);
    audio.selection_reset();
    audio.advance_ms(300);
    assert!(audio.selection_tap("hfoo", None).is_some(), "counter reset");
    assert!(audio.last_event_line().unwrap_or_default().starts_with("AUDIO_ACK"));
}

/// `audio.tileset-ambient`: the Justice tileset ('L' = LordaeronSummer) resolves
/// to the shipped day/night MIDI + DLS pair, renders audible PCM through the
/// real bank and starts a looped ambient voice. No clip here is synthetic.
#[test]
fn audio_tileset_ambient_renders_lordaeron_summer() {    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    assert_eq!(
        audio.ambient_theme_for_tileset('L', false).as_deref(),
        Some("LordaeronSummerDay")
    );
    assert_eq!(
        audio.ambient_theme_for_tileset('L', true).as_deref(),
        Some("LordaeronSummerNight")
    );
    let names = audio.ambient_theme_names();
    assert!(names.len() >= 20, "ambient themes: {}", names.len());

    let voice = audio
        .ambient_for_tileset('L', false)
        .expect("Justice tileset ambience must start");
    let line = audio.last_event_line().unwrap_or_default().to_string();
    println!("TILESET_AMBIENT voice={} line={line}", voice.0);
    assert!(
        line.contains("theme=LordaeronSummerDay"),
        "resolved clip line: {line}"
    );
    assert!(line.contains(".mid"), "clip name is logged: {line}");
    assert!(line.contains(".dls"), "bank name is logged: {line}");
    let stats = audio.stats();
    assert_eq!(stats.ambient_started, 1);
    assert_eq!(stats.ambient_missing, 0);
    assert!(
        audio.unresolved().is_empty(),
        "unresolved clips: {:?}",
        audio.unresolved()
    );

    // The voice is real audio, not a placeholder silence. The loop fades in
    // over the region cross-fade window, so render past the fade.
    let mut block = vec![0.0f32; 512];
    let mut peak = 0.0f32;
    for _ in 0..120 {
        audio.render(&mut block);
        peak = block.iter().fold(peak, |peak, sample| peak.max(sample.abs()));
    }
    assert!(peak > 0.01, "ambience rendered samples (peak {peak})");
    let line = audio.stats_line();
    println!("{line}");
    assert!(line.contains("GAME_AUDIO_HOOKS"));
    assert!(line.contains("ambient=1/0"), "hook counters: {line}");
    assert!(line.contains("underruns=0"), "engine counters: {line}");

    // A second call replaces the loop instead of stacking voices.
    assert!(audio.ambient_for_tileset('L', true).is_some());
    assert_eq!(audio.stats().ambient_started, 2);
}

/// `audio.race-themes`: the in-game theme follows the player race (Justice runs
/// the player as Orc) and the resolved MP3 name is in the event log.
#[test]
fn audio_player_race_music_resolves_on_justice() {
    let Some(root) = archive() else {
        return;
    };
    let trigger = std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt");
    let mut audio = GameAudio::new(
        headless(),
        SoundInfoDb::new(),
        UnitNameMap::default(),
        ArchiveProvider::new(&root),
    );
    audio.load_themes_from_trigger_data(&trigger);
    assert!(
        audio.music_for_player_race(Race::Orc).is_some(),
        "Orc in-game theme must start"
    );
    let line = audio.last_event_line().unwrap_or_default().to_string();
    println!("PLAYER_RACE_MUSIC {line}");
    assert!(line.contains("theme=Orc1"), "{line}");
    assert!(line.to_ascii_lowercase().contains("path=sound"), "{line}");
    assert!(line.contains("Orc1.mp3"), "{line}");
    assert!(line.contains(".mp3"), "{line}");
    assert!(
        audio.music_for_player_race(Race::Human).is_some(),
        "Human theme also resolves (host override)"
    );
    assert!(audio.unresolved().is_empty(), "{:?}", audio.unresolved());
}

/// `assets.paths` / `audio.map-sounds`: the shipped map's `war3map.w3s` custom
/// sounds and `war3map.w3r` regions parse, and a looping map ambient plays at
/// its region centre through the 3D hook layer.
#[test]
fn audio_map_sounds_load_and_play_at_region_centres() {
    let map_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_map/Human04");
    let (Ok(w3s), Ok(w3r)) = (
        std::fs::read(map_dir.join("war3map.w3s")),
        std::fs::read(map_dir.join("war3map.w3r")),
    ) else {
        return;
    };
    let map = MapSounds::load(&w3s, &w3r).expect("parse map sounds");
    assert_eq!(map.version, 1);
    assert_eq!(map.sounds.len(), 26, "Human04 w3s sound records");
    assert_eq!(map.regions.len(), 87, "Human04 w3r regions");

    let lake = map
        .sound("gg_snd_WaterLakeLoop1")
        .expect("lake loop definition");
    assert!(lake.file.ends_with("WaterLakeLoop1.wav"), "{}", lake.file);
    assert!(lake.is_looping_hint(), "flags={} marks the doodad loop", lake.flags);
    let stream = map
        .sound("gg_snd_WaterStreamLoop1")
        .expect("stream loop definition");
    assert!(stream.is_looping_hint());
    let dialogue = map
        .sound("gg_snd_H04Arthas02")
        .expect("dialogue definition");
    assert!(!dialogue.is_3d_hint() && !dialogue.is_looping_hint());
    let warcry = map
        .sound("gg_snd_BanditWarcry1")
        .expect("3D unit sound");
    assert!(warcry.is_3d_hint());

    let region = map.region("LakeLoop01").expect("LakeLoop01 region");
    let center = region.center();
    assert!((center.x - (-1568.0)).abs() < 1.0, "center x {}", center.x);
    assert!((center.z - (-1232.0)).abs() < 1.0, "center y {}", center.z);

    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let voice = audio
        .play_map_ambient(lake, region)
        .expect("map ambient starts at the region centre");
    assert!(voice.0 > 0);
    // Headless commands apply on render; pull a few blocks so the voice starts.
    let mut block = vec![0.0f32; 512 * 2];
    let mut peak = 0.0f32;
    for _ in 0..4 {
        audio.render(&mut block);
        peak = peak.max(block.iter().fold(0.0f32, |p, s| p.max(s.abs())));
    }
    let stats = audio.audio_stats();
    assert!(stats.voices_started >= 1, "voice started: {stats:?}");
    assert!(peak > 0.0, "map ambient produced samples (peak {peak})");
}

/// `audio.tileset-ambient` (Justice): the real melee arena's `war3map.w3e`
/// tileset byte selects the ambient theme, so the 1v1 match is not silent.
#[test]
fn audio_justice_tileset_selects_its_ambient_theme() {
    let map = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/\
         Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    let Some(root) = archive() else {
        return;
    };
    let Ok(base) = wc3_formats::map::map_mpq_base(&map) else {
        eprintln!("skipping: Justice map fixture missing at {}", map.display());
        return;
    };
    let archive = wc3_mpq::MpqArchive::open_at(&map, base).expect("open Justice mpq");
    let terrain = wc3_formats::w3e::Terrain::parse(
        &archive.read("war3map.w3e").expect("war3map.w3e"),
    )
    .expect("parse w3e");
    let tileset = terrain.tileset as char;
    println!("JUSTICE_TILESET {tileset}");
    assert_eq!(tileset, 'Y', "Justice is CityScape terrain");

    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let theme = audio
        .ambient_theme_for_tileset(tileset, false)
        .unwrap_or_else(|| panic!("tileset {tileset} has no day ambience"));
    println!("JUSTICE_AMBIENT_DAY {theme}");
    assert_eq!(theme, "CityScapeDay");
    assert_eq!(
        audio.ambient_theme_for_tileset(tileset, true).as_deref(),
        Some("CityScapeNight")
    );
    let voice = audio
        .ambient_for_tileset(tileset, false)
        .unwrap_or_else(|| panic!("tileset {tileset} day ambience did not start"));
    let line = audio.last_event_line().unwrap_or_default().to_string();
    println!("JUSTICE_AMBIENT_PLAY voice={} line={line}", voice.0);
    assert!(line.contains(&format!("tileset={tileset}")), "{line}");
    assert!(line.contains(".mid"), "{line}");
    assert!(line.contains(".dls"), "{line}");
    assert_eq!(audio.stats().ambient_missing, 0);
}

/// `audio.spatial` (retail distance fields): `MinDistance` is the near radius,
/// `MaxDistance` is the far end where the sound is inaudible. Measured rows:
/// `PeonDeath` 600/10000/3000, `FootmanDeath` 600/10000/3000,
/// `GruntWhat` 3000/10000/100000 (min/max/cutoff).
#[test]
fn audio_retail_distance_fields_map_to_near_and_far() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    for (name, expected) in [
        ("PeonDeath", (600.0f32, 10_000.0f32)),
        ("FootmanDeath", (600.0, 10_000.0)),
        ("GruntWhat", (3_000.0, 10_000.0)),
        ("PeonWhat", (3_000.0, 10_000.0)),
        ("PeonYesAttack", (3_000.0, 10_000.0)),
    ] {
        let def = sounds.get(name).unwrap_or_else(|| panic!("{name} missing"));
        let distances = def.playback_distances(600.0);
        println!(
            "VOICE_DISTANCES {name} min={} max={} cutoff={} -> near={} far={}",
            def.min_distance,
            def.max_distance,
            def.distance_cutoff,
            distances.0,
            distances.1
        );
        assert_eq!(distances, expected, "{name} near/far mapping");
    }
}

fn energy_with_curve(distance: f32, reference: f32, far: f32) -> f32 {
    let mut engine = AudioEngine::headless(AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 4,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    });
    engine.set_listener(Vec3::ZERO, 0.0);
    let sound = Arc::new(Sound {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.25; 48_000],
    });
    engine.play(
        sound,
        PlayParams {
            position: Some(Vec3::new(distance, 0.0, 0.0)),
            reference_distance: reference,
            max_distance: far,
            looping: true,
            ..Default::default()
        },
    );
    let mut block = vec![0.0f32; 512 * 2];
    let mut energy = 0.0f32;
    for _ in 0..8 {
        engine.render(&mut block);
        energy += block.iter().map(|sample| sample * sample).sum::<f32>();
    }
    energy
}

/// `audio.spatial`: at the shipped `MaxDistance` the curve is silence, so a
/// voice 12 000 units away (and 10x the 3 000-unit near radius) is inaudible.
/// The old mapping (near = MaxDistance, far = DistanceCutoff) kept it at
/// ~0.53 gain twelve thousand units away.
#[test]
fn audio_voice_curve_zeroes_at_retail_max_distance() {
    // PeonWhat / GruntWhat: near 3000, far 10000, cutoff 100000.
    let inside = energy_with_curve(1_000.0, 3_000.0, 10_000.0);
    let at_near = energy_with_curve(3_000.0, 3_000.0, 10_000.0);
    let far = energy_with_curve(12_000.0, 3_000.0, 10_000.0);
    let ten_x_min = energy_with_curve(30_000.0, 3_000.0, 10_000.0);
    assert!(inside > 0.0, "inside the near radius is audible: {inside}");
    assert!(
        (inside - at_near).abs() <= inside * 0.02,
        "unity at the near radius: inside={inside} at_near={at_near}"
    );
    assert_eq!(far, 0.0, "12 000 units is past MaxDistance 10 000: {far}");
    assert!(
        far < 1e-4,
        "gain at 12 000 units must be < 1e-4: energy={far}"
    );
    assert_eq!(
        ten_x_min, 0.0,
        "10x MinDistance is past MaxDistance: {ten_x_min}"
    );

    // PeonDeath curve: near 600, far 10000; the per-def DistanceCutoff 3000
    // culls it before that (checked in the GameAudio gate test).
    let death_far = energy_with_curve(12_000.0, 600.0, 10_000.0);
    assert_eq!(death_far, 0.0, "death line at 12 000: {death_far}");
}

/// `audio.audibility` (retail rule): a 3-D sound never starts when
/// `dist² > DistanceCutoff²`. Ack rows cut at 100 000, combat rows at 2100,
/// deaths at 3000; a row storing 0 uses the parser default 3000.
#[test]
fn audio_row_cutoff_gates_distant_voices() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.set_listener(Vec3::ZERO, 0.0);
    assert_eq!(audio.audible_radius(), 0.0, "no flat radius gate by default");

    // Ack rows: DistanceCutoff 100000 -> a 9 000-unit ack is still started.
    // Each probe steps the deterministic clock past the per-unit ack window.
    let far_ack = Vec3::new(9_000.0, 0.0, 0.0);
    audio.advance_ms(200);
    assert!(
        audio.unit_ack("hfoo", Some(far_ack)).is_some(),
        "ack at 9 000 units must not be culled (cutoff 100 000)"
    );
    // Boundary: 100 000 is audible, 100 001 is not.
    audio.advance_ms(200);
    assert!(
        audio.unit_ack("hfoo", Some(Vec3::new(100_000.0, 0.0, 0.0))).is_some(),
        "dist == cutoff is audible (squared compare uses >, not >=)"
    );
    audio.advance_ms(200);
    assert!(
        audio.unit_ack("hfoo", Some(Vec3::new(100_001.0, 0.0, 0.0))).is_none(),
        "dist > cutoff is culled"
    );

    // Death rows (AnimSounds): cutoff 3000.
    assert!(
        audio.unit_death("hpea", Some(Vec3::new(3_000.0, 0.0, 0.0))).is_some(),
        "death at exactly 3000 is audible"
    );
    assert!(
        audio.unit_death("hpea", Some(Vec3::new(3_001.0, 0.0, 0.0))).is_none(),
        "death at 3001 is culled (cutoff 3000)"
    );

    // Combat rows (UnitCombatSounds): cutoff 2100 -> culled at 3000.
    let combat = "AxeMediumChopWood";
    assert!(
        audio.play_sound(combat, Some(Vec3::new(2_099.0, 0.0, 0.0))).is_some(),
        "{combat} inside 2100"
    );
    assert!(
        audio.play_sound(combat, Some(Vec3::new(2_101.0, 0.0, 0.0))).is_none(),
        "{combat} past 2100"
    );
    assert!(
        audio.play_sound(combat, Some(Vec3::new(3_000.0, 0.0, 0.0))).is_none(),
        "{combat} culled at 3000 units"
    );

    // A gated event is counted as gated, not as a missing row.
    let gated_before = audio.stats().events_gated;
    assert!(
        audio
            .play_event(AudioEvent::Death, Some("hpea"), None, Some(Vec3::new(3_001.0, 0.0, 0.0)))
            .is_none()
    );
    assert_eq!(audio.stats().events_gated, gated_before + 1);
    assert!(
        audio.unresolved().is_empty(),
        "row cutoff is not a data gap: {:?}",
        audio.unresolved()
    );
    let line = audio.stats_line();
    println!("{line}");
    assert!(line.contains("gated="), "{line}");
}

/// `audio.audibility`: the squared compare equals the linear compare at the
/// boundary (+/-1 unit) and a row with cutoff 0 uses the parsed default 3000.
#[test]
fn audio_squared_cutoff_compare_matches_linear_at_boundary() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let combat = sounds.get("AxeMediumChopWood").expect("combat row");
    assert_eq!(combat.audible_cutoff(), 2_100.0);
    for distance in [2_099.0f32, 2_100.0, 2_101.0] {
        let squared = distance * distance <= combat.audible_cutoff() * combat.audible_cutoff();
        let linear = distance <= combat.audible_cutoff();
        assert_eq!(squared, linear, "distance {distance}");
        assert_eq!(combat.is_audible_distance(distance), linear);
    }

    // Row with the field at 0: parser default 3000.
    let zero = wc3_audio::SoundDef {
        name: "CutoffZero".into(),
        distance_cutoff: 0.0,
        ..Default::default()
    };
    assert_eq!(zero.audible_cutoff(), 3_000.0);
    assert!(zero.is_audible_distance(3_000.0));
    assert!(!zero.is_audible_distance(3_001.0));
}

/// `audio.audibility` (retail per-class cutoffs measured from the shipped SLKs):
/// ack 100000, combat 2100, ability 2000, ambience 900-2500, death 3000.
#[test]
fn audio_retail_cutoffs_per_class() {
    let Some(root) = archive() else {
        return;
    };
    let sounds = load_sounds(&root);
    let expected = [
        ("GruntWhat", 100_000.0),
        ("PeonWhat", 100_000.0),
        ("AxeMediumChopWood", 2_100.0),
        ("PeonDeath", 3_000.0),
        ("FootmanDeath", 3_000.0),
        ("BlizzardWave", 2_000.0),
        ("BuildingConstructionLoop", 2_500.0),
        ("AmbientSoundRain", 1_000.0),
    ];
    for (name, cutoff) in expected {
        let def = sounds.get(name).unwrap_or_else(|| panic!("{name} missing"));
        println!("CLASS_CUTOFF {name} cutoff={}", def.audible_cutoff());
        assert_eq!(def.audible_cutoff(), cutoff, "{name}");
    }
}

/// `audio.tileset-ambient` (region): the loop starts once per region key, keeps
/// playing without a restart inside the region, and stops when the player
/// leaves or the key changes.
/// `audio.ambient.region-switch`: changing region/tileset cross-fades to the new
/// theme, keeps the loop while the player stays, and stops on leaving.
#[test]
fn audio_ambient_region_switch_crossfades_and_stops() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let first = audio
        .ambient_for_region("City", 'Y', false)
        .expect("City tileset ambience starts");
    assert_eq!(audio.ambient_region(), Some("City"));
    assert_eq!(audio.stats().ambient_started, 1);
    let again = audio
        .ambient_for_region("City", 'Y', false)
        .expect("same region keeps the loop");
    assert_eq!(first, again, "the running loop is reused, not restarted");
    assert_eq!(audio.stats().ambient_started, 1, "no restart per frame");
    let mut block = vec![0.0f32; 512];
    for _ in 0..30 {
        audio.render(&mut block);
    }

    // Different region: old loop fades out under the new one (cross-fade).
    let second = audio
        .ambient_for_region("Cave", 'Y', true)
        .expect("night ambience in the next region");
    assert_ne!(first, second);
    assert_eq!(audio.ambient_region(), Some("Cave"));
    assert_eq!(audio.stats().ambient_started, 2);
    assert_eq!(audio.stats().ambient_crossfades, 1, "region change cross-fades");
    let line = audio.last_event_line().unwrap_or_default().to_string();
    println!("AMBIENT_REGION line={line}");
    assert!(line.contains("night=true"), "{line}");

    // Both loops are alive during the fade window, then the old one is gone.
    for _ in 0..4 {
        audio.render(&mut block);
    }
    assert_eq!(audio.audio_stats().active_voices, 2, "cross-fade overlaps two loops");
    for _ in 0..80 {
        audio.render(&mut block);
    }
    assert_eq!(audio.audio_stats().active_voices, 1, "old loop released");

    audio.leave_ambient_region();
    assert_eq!(audio.ambient_region(), None);
    for _ in 0..80 {
        audio.render(&mut block);
    }
    assert_eq!(audio.audio_stats().active_voices, 0, "ambient stopped");
}

/// `audio.audibility`: a culled set-based voice must not fall through to its
/// longer-range fallback row (`PeonDeath` cutoff 3000 -> `PeonWhat` 100000).
#[test]
fn audio_set_voice_does_not_fall_back_when_culled() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.set_listener(Vec3::ZERO, 0.0);
    assert!(
        audio.unit_death_set("Peon", Some(Vec3::new(3_001.0, 0.0, 0.0))).is_none(),
        "PeonDeath is culled at 3001 and must not degrade to PeonWhat"
    );
    assert!(
        audio.unit_death_set("Peon", Some(Vec3::new(2_000.0, 0.0, 0.0))).is_some(),
        "inside the death cutoff the death line still starts"
    );
}

// ---------------------------------------------------------------------------
// `audio.voices.per-def` / `audio.voices.distance-gate`
//
// The extracted `analysis/war3_full3` tree is RoC: TFT defs (headhunter,
// shaman, tauren, kodo, wind rider, spirit walker, ...) and their clips live in
// `War3x*.mpq`. These two mechanics therefore read the production layered
// install (`War3xlocal > War3x > War3`) instead of the loose extract.
// ---------------------------------------------------------------------------

/// The production layered install, opened read-only (skip when absent).
struct MpqLayers {
    /// Priority order: highest layer first (`War3xlocal > War3x > War3`).
    layers: Vec<wc3_mpq::MpqArchive>,
}

impl MpqLayers {
    fn open() -> Option<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-assets/wc3-install");
        let mut layers = Vec::new();
        for name in ["War3xlocal.mpq", "War3x.mpq", "War3.mpq"] {
            let path = root.join(name);
            if !path.is_file() {
                continue;
            }
            if let Ok(archive) = wc3_mpq::MpqArchive::open(&path) {
                layers.push(archive);
            }
        }
        if layers.len() < 3 {
            let present: Vec<&str> = ["War3xlocal.mpq", "War3x.mpq", "War3.mpq"]
                .into_iter()
                .filter(|name| root.join(name).is_file())
                .collect();
            println!(
                "SKIP layered-install audio tests: {} of 3 archives present {present:?}",
                layers.len()
            );
            return None;
        }
        Some(Self { layers })
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.layers
            .iter()
            .find_map(|archive| archive.read(path).ok())
    }

    fn contains(&self, path: &str) -> bool {
        self.layers.iter().any(|archive| archive.contains(path))
    }
}

/// Process-wide install: opening three archives is expensive and the two voice
/// mechanics read the same immutable data.
fn install_layers() -> Option<&'static MpqLayers> {
    static LAYERS: std::sync::OnceLock<Option<MpqLayers>> = std::sync::OnceLock::new();
    LAYERS.get_or_init(MpqLayers::open).as_ref()
}

/// Provider over the layered archives with **exact-name** resolution only.
///
/// No basename/directory fallback: a cross-race guess is exactly the defect
/// (`peasant` speaking an Orc file) these tests must catch, so a missing path
/// stays missing instead of silently resolving to another unit's clip.
struct LayeredProvider {
    layers: &'static MpqLayers,
    loaded: std::cell::RefCell<Vec<String>>,
}

impl SoundProvider for LayeredProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        if !self.exists(path) {
            return None;
        }
        self.loaded.borrow_mut().push(path.to_string());
        Some(Arc::new(Sound {
            sample_rate: 22_050,
            channels: 1,
            samples: vec![0.2; 128],
        }))
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        self.layers.read(path).map(Arc::from)
    }

    fn exists(&self, path: &str) -> bool {
        self.layers.contains(path)
    }

    fn resolve(&self, path: &str) -> Option<String> {
        self.exists(path).then(|| path.replace('/', "\\"))
    }
}

/// Retail `SoundInfo` tables the game loads, in load order.
const RETAIL_SOUND_TABLES: [&str; 8] = [
    "UI\\SoundInfo\\UnitAckSounds.slk",
    "UI\\SoundInfo\\UnitCombatSounds.slk",
    "UI\\SoundInfo\\UISounds.slk",
    "UI\\SoundInfo\\AnimSounds.slk",
    "UI\\SoundInfo\\AbilitySounds.slk",
    "UI\\SoundInfo\\AmbienceSounds.slk",
    "UI\\SoundInfo\\EnvironmentSounds.slk",
    "UI\\SoundInfo\\MIDISounds.slk",
];

fn load_retail_sounds(layers: &MpqLayers) -> SoundInfoDb {
    let mut sounds = SoundInfoDb::new();
    for name in RETAIL_SOUND_TABLES {
        let Some(bytes) = layers.read(name) else {
            continue;
        };
        let table = name.rsplit('\\').next().unwrap_or(name).trim_end_matches(".slk");
        sounds.add_table(table, &bytes).expect("parse sound table");
    }
    sounds
}

fn load_retail_units(layers: &MpqLayers) -> UnitNameMap {
    let bytes = layers
        .read("Units\\unitUI.slk")
        .expect("TFT Units/unitUI.slk in the layered install");
    UnitNameMap::parse(&bytes).expect("parse TFT unitUI")
}

/// Reference defs: the eight Orc melee defs from the operator bug plus their
/// eight Human counterparts.
const VOICE_REFERENCE: [(&str, &str); 16] = [
    ("opeo", "Peon"),
    ("ogru", "Grunt"),
    ("ohun", "HeadHunter"),
    ("oshm", "Shaman"),
    ("otau", "Tauren"),
    ("okod", "KotoBeast"),
    ("owyv", "WyvernRider"),
    ("ospw", "SpiritWalker"),
    ("hpea", "Peasant"),
    ("hfoo", "Footman"),
    ("hrif", "Rifleman"),
    ("hmpr", "Priest"),
    ("hsor", "Sorceress"),
    ("hkni", "Knight"),
    ("hmtm", "MortarTeam"),
    ("hgry", "GryphonRider"),
];

/// `audio.voices.per-def`: every reference def resolves its own
/// `UnitUI.unitSound` set and its own `UnitAckSounds` row, and the hook starts
/// a file from that row. Two different defs may share a file only when the
/// **data** shares the row; a fallback that gives the peasant an Orc clip
/// fails the test.
#[test]
fn audio_voices_per_def_each_def_resolves_its_own_ack_file() {
    let Some(layers) = install_layers() else {
        return;
    };
    let sounds = load_retail_sounds(layers);
    let units = load_retail_units(layers);
    let mut audio = GameAudio::new(
        headless(),
        sounds,
        units,
        LayeredProvider {
            layers,
            loaded: std::cell::RefCell::new(Vec::new()),
        },
    );

    let mut by_file: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    let mut rows = std::collections::BTreeSet::new();
    for (def, expected_set) in VOICE_REFERENCE {
        let (set, row) = audio
            .unit_ack_definition(def)
            .unwrap_or_else(|| panic!("{def}: no ack definition from the shipped object data"));
        assert_eq!(set, expected_set, "{def}: its own unitSound set");
        assert_eq!(
            row.source, "UnitAckSounds",
            "{def}: ack row {} must come from UnitAckSounds",
            row.name
        );
        assert!(
            row.name.eq_ignore_ascii_case(&format!("{set}What")),
            "{def}: row {} is not {{set}}What",
            row.name
        );
        assert!(row.audible_cutoff() > 0.0, "{def}: row keeps its cutoff");

        audio.unit_ack(def, None);
        let file = audio
            .provider()
            .loaded
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| panic!("{def}: hook did not start a file for row {}", row.name));
        let variants = row.paths();
        assert!(
            variants.iter().any(|path| path.eq_ignore_ascii_case(&file)),
            "{def}: started {file}, not one of the row's variants {variants:?}"
        );
        println!("VOICE_PATH def={def} set={set} row={} file={file}", row.name);

        let key = file.to_ascii_lowercase();
        if let Some((other_def, other_row)) = by_file.get(&key) {
            assert!(
                other_row.eq_ignore_ascii_case(&row.name),
                "per-def collision: {def} row {} and {other_def} row {other_row} both resolve {file}; \
                 a shared file is only legitimate when the data shares the row",
                row.name
            );
            println!(
                "VOICE_SHARED_ROW def={def} also_def={other_def} row={} (data shares the row)",
                row.name
            );
        } else {
            by_file.insert(key, (def.to_string(), row.name.clone()));
        }
        rows.insert(row.name.clone());
    }

    assert_eq!(
        rows.len(),
        VOICE_REFERENCE.len(),
        "every reference def has its own ack row: {rows:?}"
    );
    assert_eq!(
        by_file.len(),
        VOICE_REFERENCE.len(),
        "every reference def resolves a distinct ack file"
    );
}

/// `audio.voices.distance-gate`: the hard audibility rule is the row's own
/// squared `DistanceCutoff` (`StartSound` `0x6f102980`). A unit ack
/// `UnitAckSounds` row ships `100000`, so a 1200-unit ack still starts; a
/// `UnitCombatSounds` row ships `2100`, so the same clip is silent at 5000.
#[test]
fn audio_voices_distance_gate_ack_1200_combat_5000() {
    let Some(layers) = install_layers() else {
        return;
    };
    let sounds = load_retail_sounds(layers);
    let units = load_retail_units(layers);
    let peon = sounds.get("PeonWhat").expect("PeonWhat row");
    assert_eq!(peon.source, "UnitAckSounds");
    assert_eq!(peon.audible_cutoff(), 100_000.0, "retail ack cutoff");
    let combat = sounds
        .get("AxeMediumChopWood")
        .expect("UnitCombatSounds sample row");
    assert_eq!(combat.source, "UnitCombatSounds");
    assert_eq!(combat.audible_cutoff(), 2_100.0, "retail combat cutoff");

    let mut audio = GameAudio::new(
        headless(),
        sounds,
        units,
        LayeredProvider {
            layers,
            loaded: std::cell::RefCell::new(Vec::new()),
        },
    );
    audio.set_listener(Vec3::ZERO, 0.0);

    // Ack at 1200 units: 1200² is far below 100000².
    audio.advance_ms(200);
    assert!(
        audio
            .unit_ack("opeo", Some(Vec3::new(1_200.0, 0.0, 0.0)))
            .is_some(),
        "peon ack at 1200 units must be audible (row cutoff 100000)"
    );
    assert_eq!(audio.stats().acks_started, 1);
    assert_eq!(audio.stats().acks_gated, 0);

    // Combat inside its own cutoff: audible at 1200, silent at 5000.
    assert!(
        audio
            .play_sound("AxeMediumChopWood", Some(Vec3::new(1_200.0, 0.0, 0.0)))
            .is_some(),
        "combat row at 1200 is inside its 2100 cutoff"
    );
    assert!(
        audio
            .play_sound("AxeMediumChopWood", Some(Vec3::new(5_000.0, 0.0, 0.0)))
            .is_none(),
        "combat row at 5000 is outside its 2100 cutoff"
    );
    assert_eq!(audio.stats().voices_gated, 1);
    let line = audio.last_event_line().unwrap_or_default().to_string();
    println!("VOICE_GATE line={line}");
    assert!(line.starts_with("AUDIO_GATED"), "{line}");
    assert!(line.contains("cutoff=2100"), "{line}");

    // A unit ack keeps its own much larger range: 5000 ≪ 100000.
    audio.advance_ms(200);
    assert!(
        audio
            .unit_ack("hpea", Some(Vec3::new(5_000.0, 0.0, 0.0)))
            .is_some(),
        "peasant ack at 5000 units is still audible (row cutoff 100000)"
    );
    assert_eq!(audio.stats().acks_gated, 0);
}


// ---------------------------------------------------------------------------
// `audio.music.playlist-per-race` / `audio.mixer.ack-limiter` /
// `audio.mixer.pan` / `audio.budget.no-overflow`
// ---------------------------------------------------------------------------

/// `audio.music.playlist-per-race`: each race plays its own three shipped themes
/// in a stable hash order (no RNG in the sim path), advancing cross-fades, and a
/// race change starts the new rotation instead of stacking tracks.
#[test]
fn audio_music_playlist_per_race() {
    let Some(root) = archive() else {
        return;
    };
    let trigger = std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt");
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    let themes = audio.load_themes_from_trigger_data(&trigger);
    assert!(themes >= 30, "musictheme rows: {themes}");

    let mut block = vec![0.0f32; 512];
    // Playing a track for a real stretch of time before the next one starts is
    // what a match does; a fade-out has nothing to fade otherwise.
    let mut play_for = |audio: &mut GameAudio<ArchiveProvider>, frames: usize| {
        for _ in 0..(frames / 512) {
            audio.render(&mut block);
        }
    };
    for race in Race::ALL {
        let voice = audio
            .start_race_playlist(race)
            .unwrap_or_else(|| panic!("{race:?} playlist starts"));
        assert!(voice.0 > 0);
        play_for(&mut audio, 48_000);
        let expected = wc3_audio::hash_order(race.id(), race.theme_names(MusicState::Playing));
        let (playlist_race, playlist_tracks, first_track) = {
            let playlist = audio.playlist().expect("playlist is current");
            (
                playlist.race(),
                playlist.tracks().to_vec(),
                playlist.current().expect("first track").to_string(),
            )
        };
        assert_eq!(playlist_race, race);
        assert_eq!(playlist_tracks.len(), 3, "{race:?} plays three themes");
        assert_eq!(
            playlist_tracks.as_slice(),
            expected.as_slice(),
            "{race:?} order is the stable hash order"
        );
        // Deterministic: an independently built playlist agrees.
        let rebuilt = wc3_audio::MusicPlaylist::build(race, MusicState::Playing);
        assert_eq!(rebuilt.tracks(), playlist_tracks.as_slice());

        let crossfades_before = audio.stats().music_crossfades;
        let mut started = vec![first_track];
        for _ in 0..2 {
            play_for(&mut audio, 48_000);
            let next = audio
                .playlist_advance()
                .unwrap_or_else(|| panic!("{race:?} advances"));
            assert!(next.0 > 0);
            let current = audio
                .playlist()
                .and_then(|playlist| playlist.current())
                .expect("current track")
                .to_string();
            started.push(current);
        }
        started.sort();
        started.dedup();
        assert_eq!(started.len(), 3, "{race:?} rotates through all themes: {started:?}");
        assert!(
            audio.stats().music_crossfades >= crossfades_before + 2,
            "{race:?} theme changes cross-fade"
        );
        println!(
            "PLAYLIST race={} tracks={playlist_tracks:?} started={} music_started={}",
            race.id(),
            started.len(),
            audio.stats().music_started
        );
    }

    // Auto-rotation on the sim clock: a theme is replaced once its MP3 has
    // played out, and not before.
    audio.set_clock_ms(0);
    audio.start_race_playlist(Race::Human).expect("playlist restart");
    audio.set_clock_ms(10_000);
    assert!(audio.tick_music().is_none(), "a 10 s play is not a finished theme");
    audio.set_clock_ms(400_000);
    assert!(
        audio.tick_music().is_some(),
        "the playlist rotates after the theme length (streamed MP3 header)"
    );
    assert!(audio.tick_music().is_none(), "not twice in the same instant");

    // Race change: the Human track fades out while the Orc track starts.
    let human = audio.start_race_playlist(Race::Human).expect("human playlist");
    play_for(&mut audio, 48_000);
    let orc = audio.start_race_playlist(Race::Orc).expect("orc playlist");
    assert_ne!(human, orc);
    play_for(&mut audio, 10_240);
    let engine_stats = audio.audio_stats();
    println!(
        "MUSIC_CROSSFADE active={} started={} crossfades={}",
        engine_stats.active_voices,
        engine_stats.voices_started,
        audio.stats().music_crossfades
    );
    assert!(
        engine_stats.active_voices >= 2,
        "cross-fade overlaps the outgoing and incoming themes (active={} started={})",
        engine_stats.active_voices,
        engine_stats.voices_started
    );
    assert_eq!(audio.playlist().map(|playlist| playlist.race()), Some(Race::Orc));
    assert!(audio.stats().music_missing <= 3, "shipped themes resolve");
}

/// Explicit theme selection (victory/defeat stinger, host override) ends the
/// match rotation: `tick_music` must not replace the stinger with the playlist's
/// next track when the previous track length elapses.
#[test]
fn audio_music_explicit_theme_stops_playlist_rotation() {
    let Some(root) = archive() else {
        return;
    };
    let trigger = std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt");
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.load_themes_from_trigger_data(&trigger);

    audio.start_race_playlist(Race::Human).expect("human playlist");
    assert!(audio.playlist().is_some(), "rotation is current");
    audio
        .music_for_race(Race::Human, MusicState::Victory)
        .expect("victory stinger starts");
    assert!(
        audio.playlist().is_none(),
        "an explicit theme owns the score from now on"
    );
    audio.set_clock_ms(10_000_000);
    assert!(
        audio.tick_music().is_none(),
        "the stinger is never replaced by the old rotation"
    );
}

/// A corrupt music stream must not fade out the running theme: the probe runs
/// before the cross-fade, so a bad file leaves the score playing and reports the
/// decode gap once.
#[test]
fn audio_music_corrupt_track_keeps_the_score() {
    let Some(root) = archive() else {
        return;
    };
    let trigger = std::fs::read(root.join("UI/TriggerData.txt")).expect("TriggerData.txt");
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        CorruptMusicProvider {
            inner: ArchiveProvider::new(&root),
            corrupt: String::new(),
        },
    );
    audio.load_themes_from_trigger_data(&trigger);
    audio.music("Orc1").expect("orc theme starts");
    let started = audio.stats().music_started;
    // One block applies the queued play command: the theme is on the mixer.
    let mut block = vec![0.0f32; 512];
    audio.render(&mut block);
    let resolved = audio
        .themes()
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Orc1"))
        .and_then(|(_, path)| audio.provider().resolve(path))
        .expect("Orc1 resolves through the archive");
    audio.provider_mut().corrupt = resolved.clone();
    audio.add_theme("BrokenTheme", resolved);

    assert!(audio.music("BrokenTheme").is_none(), "corrupt track is rejected");
    assert_eq!(audio.stats().music_started, started, "no new theme started");
    assert!(
        audio.audio_stats().active_voices >= 1,
        "the running theme is still on the mixer"
    );
    let line = audio.last_event_line().unwrap_or_default();
    assert!(line.contains("probe=fail"), "decode gap reported: {line}");
}

/// `audio.mixer.ack-limiter`: one acknowledgement per unit per window; other
/// units are unaffected and the line resumes after the window. The clock is the
/// deterministic sim clock, so a burst at one instant yields exactly one line.
#[test]
fn audio_mixer_ack_limiter() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.set_clock_ms(1_000);
    assert!(audio.unit_ack("hpea", None).is_some(), "first ack plays");
    assert!(audio.unit_ack("hpea", None).is_none(), "second ack inside the window");
    assert_eq!(audio.stats().acks_limited, 1);
    let line = audio.last_event_line().unwrap_or_default().to_string();
    assert!(line.contains("limited=1"), "{line}");
    assert!(line.contains("gap_ms=0"), "{line}");
    println!("ACK_LIMIT line={line}");

    // A different unit is a different channel: not limited by this one.
    assert!(audio.unit_ack("hfoo", None).is_some());
    assert_eq!(audio.stats().acks_limited, 1);

    // After the window the same unit speaks again.
    audio.advance_ms(150);
    assert!(audio.unit_ack("hpea", None).is_some());
    assert_eq!(audio.stats().acks_limited, 1);

    // Attack acknowledgements share the limiter.
    audio.set_clock_ms(10_000);
    assert!(audio.unit_attack("hfoo", None).is_some());
    audio.advance_ms(20);
    assert!(audio.unit_attack("hfoo", None).is_none());
    assert_eq!(audio.stats().acks_limited, 2);

    // A ten-command burst at one instant plays exactly one line.
    audio.set_clock_ms(50_000);
    audio.selection_reset();
    let started_before = audio.stats().acks_started;
    let limited_before = audio.stats().acks_limited;
    let mut played = 0;
    for _ in 0..10 {
        if audio.unit_ack("ogru", None).is_some() {
            played += 1;
        }
    }
    assert_eq!(played, 1, "burst collapses to one ack");
    assert_eq!(audio.stats().acks_started, started_before + 1);
    assert_eq!(audio.stats().acks_limited, limited_before + 9);
    println!(
        "ACK_BURST played={played} limited={} started={}",
        audio.stats().acks_limited,
        audio.stats().acks_started
    );
}

/// `audio.mixer.pan`: a 3-D source is panned by its position relative to the
/// listener's camera yaw (right of the camera is louder in the right ear).
#[test]
fn audio_mixer_pan_by_camera() {
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 8,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 1.0,
    };
    let mut engine = AudioEngine::headless(config);
    let clip = Arc::new(Sound {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0.5f32; 48_000],
    });
    let play_at = |engine: &mut AudioEngine, position: Vec3, yaw: f32| {
        engine.stop_all();
        engine.set_listener(Vec3::ZERO, yaw);
        engine.play(
            clip.clone(),
            PlayParams {
                position: Some(position),
                reference_distance: 4_000.0,
                looping: true,
                priority: 5,
                ..Default::default()
            },
        );
    };
    let rms = |block: &[f32]| -> (f32, f32) {
        let mut left = 0.0f32;
        let mut right = 0.0f32;
        for frame in block.chunks_exact(2) {
            left += frame[0] * frame[0];
            right += frame[1] * frame[1];
        }
        let frames = (block.len() / 2).max(1) as f32;
        ((left / frames).sqrt(), (right / frames).sqrt())
    };
    let mut block = vec![0.0f32; 512 * 2];

    // Source to the world +X while the camera faces +Y: it is on the right.
    play_at(&mut engine, Vec3::new(1_500.0, 0.0, 0.0), 0.0);
    for _ in 0..6 {
        engine.render(&mut block);
    }
    let (left, right) = rms(&block);
    println!("PAN right-of-camera left={left:.4} right={right:.4}");
    assert!(right > left * 1.5, "source on the right: L={left} R={right}");

    // Same source, camera turned 180 degrees: it is now on the left.
    play_at(&mut engine, Vec3::new(1_500.0, 0.0, 0.0), std::f32::consts::PI);
    for _ in 0..6 {
        engine.render(&mut block);
    }
    let (left, right) = rms(&block);
    println!("PAN left-of-camera left={left:.4} right={right:.4}");
    assert!(left > right * 1.5, "source on the left: L={left} R={right}");

    // A UI sound has no position: constant-power centre pan, ears equal.
    engine.stop_all();
    engine.play(
        clip.clone(),
        PlayParams {
            category: wc3_audio::Category::Ui,
            looping: true,
            ..Default::default()
        },
    );
    for _ in 0..6 {
        engine.render(&mut block);
    }
    let (left, right) = rms(&block);
    println!("PAN centre left={left:.4} right={right:.4}");
    assert!((left - right).abs() < 0.02 * left.max(right), "2-D UI is centred");
}

/// `audio.budget.no-overflow`: a ten-minute session (region changes every 30 s,
/// eight gameplay events per second) never exceeds the 64 MB decoded-PCM
/// ceiling, and the cache evicts rather than growing without bound.
#[test]
fn audio_budget_no_overflow() {
    let Some(root) = archive() else {
        return;
    };
    let mut audio = GameAudio::new(
        headless(),
        load_sounds(&root),
        load_units(&root),
        ArchiveProvider::new(&root),
    );
    audio.load_weapon_table(
        &std::fs::read(root.join("Units/UnitWeapons.slk")).expect("UnitWeapons.slk"),
    );
    let budget = audio.ambient_pcm_budget();
    assert_eq!(budget, 64 * 1024 * 1024, "default ceiling is 64 MB");

    // Tileset letters that actually *start* on this install: a theme name can
    // resolve while its MIDI/DLS pair is missing, so the real path is probed on
    // a scratch instance (its misses must not pollute the measured run).
    let regions: Vec<(char, bool)> = {
        let mut probe = GameAudio::new(
            headless(),
            load_sounds(&root),
            load_units(&root),
            ArchiveProvider::new(&root),
        );
        let mut working = Vec::new();
        for letter in [
            'A', 'B', 'C', 'D', 'F', 'G', 'I', 'J', 'K', 'L', 'N', 'O', 'Q', 'V', 'W', 'X', 'Y',
            'Z',
        ] {
            if probe.ambient_for_region("probe", letter, false).is_some() {
                working.push((letter, false));
            }
            probe.leave_ambient_region();
        }
        probe.shutdown();
        working
    };
    assert!(
        regions.len() >= 6,
        "region themes available (need 6 to exceed the 64 MB cache): {regions:?}"
    );

    let events = [
        AudioEvent::MoveOrder,
        AudioEvent::Attack,
        AudioEvent::AttackImpact,
        AudioEvent::Death,
        AudioEvent::BuildComplete,
        AudioEvent::UiClick,
        AudioEvent::LowGold,
        AudioEvent::Waypoint,
    ];
    let block_frames = 512usize;
    let mut block = vec![0.0f32; block_frames];
    let mut peak_pcm = 0usize;
    let mut current_region: Option<String> = None;
    let mut event_cursor = 0usize;
    let mut started_events = 0u64;

    for second in 0..600u64 {
        // Region change every 30 s (20 switches over the run).
        if second % 30 == 0 {
            let (letter, night) = regions[(second / 30) as usize % regions.len()];
            let region = format!("Region{}", second / 30);
            audio.advance_ms(1_000);
            let voice = audio.ambient_for_region(&region, letter, night);
            assert!(voice.is_some(), "region {region} tileset {letter} ambience");
            current_region = Some(region);
        }
        // Eight events per second, a 125 ms cadence on the sim clock.
        for _ in 0..8 {
            audio.advance_ms(125);
            let event = events[event_cursor % events.len()];
            event_cursor += 1;
            let position = Some(Vec3::new(
                ((event_cursor * 137) % 2_000) as f32,
                0.0,
                ((event_cursor * 71) % 2_000) as f32,
            ));
            let voice = match event {
                AudioEvent::MoveOrder => audio.play_event(event, Some("hpea"), None, position),
                AudioEvent::Attack => audio.play_event(event, Some("hfoo"), None, position),
                AudioEvent::AttackImpact => audio.attack_impact(Some("hfoo"), Some("ogru"), position),
                AudioEvent::Death => audio.play_event(event, Some("ogru"), None, position),
                AudioEvent::BuildComplete | AudioEvent::LowGold => {
                    audio.play_event(event, None, Some(Race::Human), position)
                }
                other => audio.play_event(other, None, None, position),
            };
            if voice.is_some() {
                started_events += 1;
            }
            let mut out = vec![0.0f32; block_frames];
            audio.render(&mut out);
        }
        // A second of mixer output per simulated second.
        for _ in 0..(48_000 / block_frames) {
            audio.render(&mut block);
            audio.advance_ms(0);
        }
        let pcm = audio.pcm_bytes();
        assert!(
            pcm <= budget,
            "second {second}: PCM {} bytes exceeds the {} byte ceiling",
            pcm,
            budget
        );
        peak_pcm = peak_pcm.max(pcm);
    }

    let (entries, bytes, evictions, _) = audio.ambient_cache_stats();
    let stats = audio.stats();
    println!(
        "BUDGET seconds=600 regions={} events_started={} peak_pcm_mb={:.1} cache_entries={entries} cache_mb={:.1} evicted={evictions} ambient_started={} ambient_crossfades={} acks_limited={} missing={}",
        regions.len(),
        started_events,
        peak_pcm as f32 / (1024.0 * 1024.0),
        bytes as f32 / (1024.0 * 1024.0),
        stats.ambient_started,
        stats.ambient_crossfades,
        stats.acks_limited,
        stats.events_missing
    );
    println!("{}", audio.stats_line());
    assert!(peak_pcm <= budget, "peak {peak_pcm} <= {budget}");
    assert!(peak_pcm > 0, "the run actually kept ambience resident");
    assert!(entries > 0, "ambient cache holds themes");
    assert!(
        evictions > 0,
        "the cache evicts instead of growing past the ceiling (entries={entries})"
    );
    assert!(stats.ambient_started >= 20, "region switches started loops");
    assert!(stats.ambient_crossfades >= 19, "every switch cross-faded");
    assert!(started_events > 4_000, "events played: {started_events}");
    assert_eq!(audio.ambient_region(), current_region.as_deref());
}

// `artE_perdef_voices` (bugs #4/#13) lives in `tests/artE_perdef_voices.rs`:
// it independently parses the shipped SLK cells as the oracle and asserts the
// deterministic def -> set -> row -> file log, so the acceptance test does not
// share a parser with the code it verifies.
