//! Scripted audio scenario on the real device: select worker, move, build,
//! train, fight, die, hero death, deny, quest, victory — plus a map ambient
//! loop at its region centre. Prints one `GAME_AUDIO_*` line per step for
//! logcat evidence and a summary with underruns/backend errors.
//!
//! Usage: `audio_scenario_probe [--install DIR] [--map DIR]`

use std::path::PathBuf;

use wc3_assetdb::audio_provider::{start_game_audio, SharedAssetDb, WiredGameAudio};
use wc3_assetdb::AssetDb;
use wc3_audio::events::{event_defs, AudioEvent, MusicState, Race};
use wc3_audio::map_sounds::MapSounds;

fn settle(audio: &mut WiredGameAudio, block: &mut [f32]) {
    for _ in 0..6 {
        audio.render(block);
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
}

fn set_of(audio: &WiredGameAudio, id: &str) -> Option<String> {
    audio.unit_names().sound_set(id).map(str::to_string)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut install = PathBuf::from("analysis/agent-assets/wc3-install");
    let mut map_dir = PathBuf::from("analysis/war3_map/Human04");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--install" => {
                i += 1;
                install = PathBuf::from(args.get(i).ok_or("--install needs a path")?);
            }
            "--map" => {
                i += 1;
                map_dir = PathBuf::from(args.get(i).ok_or("--map needs a path")?);
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
    let (mut audio, wiring) = start_game_audio(SharedAssetDb::new(db));
    println!("GAME_AUDIO_WIRING {}", wiring.summary());

    let mut block = vec![0.0f32; 512 * 2];
    let mut started = 0usize;
    let mut expected = 0usize;

    // 1. Select worker (own set ack).
    expected += 1;
    if let Some(set) = set_of(&audio, "hpea") {
        if let Some((def, voice)) = audio.unit_ack_set(&set, None) {
            println!("GAME_AUDIO_SELECT unit=hpea set={set} def={def} voice={}", voice.0);
            started += 1;
        }
    }
    settle(&mut audio, &mut block);

    // 2. Move order (own set Yes/What).
    expected += 1;
    if let Some(set) = set_of(&audio, "hpea") {
        let names = [format!("{set}Yes"), format!("{set}What")];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some((def, voice)) = audio.play_first_named(&refs, None) {
            println!("GAME_AUDIO_MOVE unit=hpea set={set} def={def} voice={}", voice.0);
            started += 1;
        }
    }
    settle(&mut audio, &mut block);

    // 3-5. Build start, loop, complete (Human rows).
    for event in [
        AudioEvent::BuildStart,
        AudioEvent::BuildLoop,
        AudioEvent::BuildComplete,
    ] {
        expected += 1;
        let label = event.id().to_ascii_uppercase();
        let defs = event_defs(event, Some(Race::Human));
        if let Some((def, voice)) = audio.play_first_named(defs, None) {
            println!("GAME_AUDIO_{label} race=human def={def} voice={}", voice.0);
            started += 1;
        }
        settle(&mut audio, &mut block);
    }

    // 6. Training complete (own set Ready).
    expected += 1;
    if let Some(set) = set_of(&audio, "hfoo") {
        if let Some((def, voice)) = audio.unit_ready_set(&set, None) {
            println!("GAME_AUDIO_TRAIN_COMPLETE unit=hfoo set={set} def={def} voice={}", voice.0);
            started += 1;
        }
    }
    settle(&mut audio, &mut block);

    // 7. Fight: attacker's own YesAttack, defender's own Death.
    expected += 2;
    if let Some(set) = set_of(&audio, "hfoo") {
        if let Some((def, voice)) = audio.unit_attack_set(&set, None) {
            println!("GAME_AUDIO_FIGHT_ATTACK unit=hfoo set={set} def={def} voice={}", voice.0);
            started += 1;
        }
    }
    settle(&mut audio, &mut block);
    if let Some(set) = set_of(&audio, "ogru") {
        if let Some((def, voice)) = audio.unit_death_set(&set, None) {
            println!("GAME_AUDIO_FIGHT_DEATH unit=ogru set={set} def={def} voice={}", voice.0);
            started += 1;
        }
    }
    settle(&mut audio, &mut block);

    // 8. Hero death (race row).
    expected += 1;
    if let Some((def, voice)) = audio.play_first_named(
        event_defs(AudioEvent::HeroDeath, Some(Race::Human)),
        None,
    ) {
        println!("GAME_AUDIO_HERO_DEATH race=human def={def} voice={}", voice.0);
        started += 1;
    }
    settle(&mut audio, &mut block);

    // 9. Deny (not enough gold).
    expected += 1;
    if let Some((def, voice)) = audio.play_first_named(
        event_defs(AudioEvent::DenyNoGold, Some(Race::Human)),
        None,
    ) {
        println!("GAME_AUDIO_DENY race=human def={def} voice={}", voice.0);
        started += 1;
    }
    settle(&mut audio, &mut block);

    // 10. Quest completed.
    expected += 1;
    if let Some((def, voice)) =
        audio.play_first_named(event_defs(AudioEvent::QuestCompleted, None), None)
    {
        println!("GAME_AUDIO_QUEST def={def} voice={}", voice.0);
        started += 1;
    }
    settle(&mut audio, &mut block);

    // 11. Map ambient loop at its region centre (Human04 w3s/w3r).
    expected += 1;
    if let (Ok(w3s), Ok(w3r)) = (
        std::fs::read(map_dir.join("war3map.w3s")),
        std::fs::read(map_dir.join("war3map.w3r")),
    ) {
        if let Ok(map) = MapSounds::load(&w3s, &w3r) {
            if let (Some(sound), Some(region)) =
                (map.sound("gg_snd_WaterLakeLoop1"), map.region("LakeLoop01"))
            {
                if let Some(voice) = audio.play_map_ambient(sound, region) {
                    let center = region.center();
                    println!(
                        "GAME_AUDIO_MAP_AMBIENT sound={} region={} pos=({:.0},{:.0}) voice={}",
                        sound.name, region.name, center.x, center.z, voice.0
                    );
                    started += 1;
                }
            }
        }
    }
    settle(&mut audio, &mut block);

    // 12. Victory stinger (race music).
    expected += 1;
    if let Some(voice) = audio.music_for_race(Race::Human, MusicState::Victory) {
        println!("GAME_AUDIO_VICTORY race=human voice={}", voice.0);
        started += 1;
    }
    settle(&mut audio, &mut block);

    let stats = audio.audio_stats();
    println!(
        "GAME_AUDIO_SUMMARY started={started}/{expected} voices={} underruns={} backend_errors={}",
        stats.voices_started, stats.buffer_underruns, stats.backend_errors
    );
    if started != expected {
        return Err(format!("{started}/{expected} scenario sounds started").into());
    }
    if stats.buffer_underruns != 0 {
        return Err(format!("{} underruns during the scenario", stats.buffer_underruns).into());
    }
    Ok(())
}
