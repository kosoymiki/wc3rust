//! Device probe for per-unit voices: plays the selection ack, attack ack and
//! death line for the reference units on the real SDL3 device, each from the
//! unit's own `UnitUI.unitSound` set. Prints one `GAME_AUDIO_*` line per voice
//! so the same lines can be written to logcat for device QA.
//!
//! Usage: `unit_voice_probe [--install DIR]`

use std::path::PathBuf;

use wc3_assetdb::audio_provider::{start_game_audio, SharedAssetDb};
use wc3_assetdb::AssetDb;

fn render(audio: &mut wc3_assetdb::audio_provider::WiredGameAudio, block: &mut [f32], blocks: usize) {
    for _ in 0..blocks {
        audio.render(block);
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut install = PathBuf::from("analysis/agent-assets/wc3-install");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--install" => {
                i += 1;
                install = PathBuf::from(args.get(i).ok_or("--install needs a path")?);
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

    let defs = ["hpea", "opeo", "uaco", "ewsp", "hfoo", "ogru", "ugho", "earc"];
    let mut block = vec![0.0f32; 512 * 2];
    let mut failures = 0usize;
    for def in defs {
        let Some(set) = audio.unit_names().sound_set(def).map(str::to_string) else {
            println!("GAME_AUDIO_MISSING unit={def} set=none");
            failures += 1;
            continue;
        };
        let ack = audio.unit_ack_set(&set, None);
        render(&mut audio, &mut block, 8);
        let attack = audio.unit_attack_set(&set, None);
        render(&mut audio, &mut block, 8);
        let death = audio.unit_death_set(&set, None);
        render(&mut audio, &mut block, 8);
        match (ack, attack, death) {
            (Some((ack, ack_voice)), Some((att, att_voice)), Some((death, death_voice))) => {
                println!("GAME_AUDIO_ACK unit={def} set={set} def={ack} voice={}", ack_voice.0);
                println!(
                    "GAME_AUDIO_ATTACK unit={def} set={set} def={att} voice={}",
                    att_voice.0
                );
                println!(
                    "GAME_AUDIO_DEATH unit={def} set={set} def={death} voice={}",
                    death_voice.0
                );
            }
            _ => {
                println!("GAME_AUDIO_MISSING unit={def} set={set}");
                failures += 1;
            }
        }
    }

    let stats = audio.audio_stats();
    println!(
        "GAME_AUDIO_SUMMARY voices_started={} underruns={} backend_errors={}",
        stats.voices_started, stats.buffer_underruns, stats.backend_errors
    );
    if failures > 0 {
        return Err(format!("{failures} unit voice(s) did not start").into());
    }
    Ok(())
}
