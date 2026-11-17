//! Diagnostic (tech-lead): distance fields of the shipped unit sound defs.
use std::path::Path;
use wc3_game::assets::Assets;

fn main() {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.herofolends.p0/files/War3.mpq",
    ]
    .iter()
    .map(Path::new)
    .find(|p| p.is_file());
    let Some(mpq) = mpq else { println!("no install"); return };
    let assets = Assets::load(Some(mpq), None);
    let Some(audio) = assets.audio.as_ref() else {
        println!("no audio");
        return;
    };
    let db = audio.sound_table();
    println!("sound defs = {}", db.len());
    for name in [
        "PeonWhat", "PeonYes", "PeonDeath", "PeonYesAttack",
        "FootmanWhat", "FootmanDeath", "GruntWhat", "GruntDeath",
        "PeasantDeath", "CityScapeDay",
    ] {
        match db.get(name) {
            Some(def) => println!(
                "{name:<14} min={:<7} max={:<7} cutoff={:<7} vol={:.2} prio={} ch={} files={}",
                def.min_distance, def.max_distance, def.distance_cutoff, def.volume, def.priority, def.channel, def.files.len()
            ),
            None => println!("{name:<14} MISSING"),
        }
    }
}
