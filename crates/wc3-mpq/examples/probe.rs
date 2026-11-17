//! Probe known WC3 binary names inside an MPQ without a listfile.
use std::env;
use wc3_mpq::MpqArchive;

fn main() {
    let path = env::args().nth(1).expect("mpq path");
    let mut archive = MpqArchive::open(&path).expect("open archive");
    let bases = [
        "War3.exe", "war3.exe", "Warcraft III.exe", "Warcraft_III.exe",
        "Game.dll", "game.dll", "Storm.dll", "storm.dll",
        "WorldEdit.exe", "worldedit.exe", "WorldEdit.exe",
        "War3x.exe", "war3x.exe", "FrozenThrone.exe", "TheFrozenThrone.exe",
        "ijl15.dll", "mss32.dll", "binkw32.dll", "BlizzardError.exe",
        "BNUpdate.exe", "bnupdate.exe", "install.exe", "Setup.exe",
        "War3Patch.mpq", "war3patch.mpq", "War3x.mpq", "War3xLocal.mpq",
        "war3xlocal.mpq", "War3xLocal.mpq", "War3.mpq",
        "game\\War3.exe", "game\\Game.dll", "game\\Storm.dll",
        "_setup\\War3.exe", "War3\\War3.exe",
    ];
    let mut found = 0;
    for base in bases {
        match archive.read(base) {
            Ok(bytes) => {
                found += 1;
                let head = &bytes[..4.min(bytes.len())];
                println!("FOUND {base} len={} head={:02x?}", bytes.len(), head);
                let out = format!("/data/data/com.termux/files/home/src/herofolends-rs/analysis/mpq-{}", base.replace('\\', "_"));
                std::fs::write(&out, &bytes).ok();
                println!("  wrote {out}");
            }
            Err(_) => {}
        }
    }
    println!("TOTAL_FOUND {found}");
}
