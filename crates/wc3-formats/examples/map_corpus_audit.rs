//! Audit the War3Net test-map corpus through the real reader.
//!
//! Prints one line per map with the MPQ base offset, whether the archive has a
//! `(listfile)`, and the version fields of every format we parse. Used as the
//! evidence tool behind `tests/map_corpus.rs`; exits non-zero when a map that
//! contains a format fails to parse it.
//!
//! Usage: `cargo run -p wc3-formats --example map_corpus_audit [ROOT]`

use std::path::{Path, PathBuf};

use wc3_formats::gamedata::{ObjectDataFile, ObjectKind};
use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::w3e::Terrain;
use wc3_formats::w3i::MapInfo;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;

fn collect_maps(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("w3m") | Some("w3x")
            ) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn fourcc(bytes: &[u8]) -> String {
    bytes
        .get(..4)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

fn main() {
    let root = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps")
    });
    let maps = collect_maps(&root);
    let mut failures = 0usize;
    let mut opened = 0usize;
    for path in &maps {
        let rel = path.strip_prefix(&root).unwrap_or(path);
        let base = match map_mpq_base(path) {
            Ok(base) => base,
            Err(err) => {
                println!("FAIL {} base: {err}", rel.display());
                failures += 1;
                continue;
            }
        };
        let archive = match MpqArchive::open_at(path, base) {
            Ok(archive) => archive,
            Err(err) => {
                println!("FAIL {} open@{base}: {err}", rel.display());
                failures += 1;
                continue;
            }
        };
        opened += 1;
        let mut notes = Vec::new();
        notes.push(format!("base={base}"));
        notes.push(format!(
            "listfile={} blocks={}",
            archive.listfile().len(),
            archive.block_count()
        ));

        if let Ok(bytes) = archive.read("war3map.w3e") {
            match Terrain::parse(&bytes) {
                Ok(t) => {
                    // Second profile: the classic engine gates on version
                    // 3..=11 (`0x6f3eb1fe` / `0x6f3eb207`), so a Reforged v12
                    // pack must be reported as engine-incompatible instead of
                    // silently accepted by the multi-era parser.
                    let engine = match Terrain::parse_classic_engine(&bytes) {
                        Ok(_) => "ok".to_string(),
                        Err(err) => format!("reject({err})"),
                    };
                    notes.push(format!(
                        "w3e=v{} {}x{} tiles={} engine={engine}",
                        t.version,
                        t.width,
                        t.height,
                        t.tiles.len()
                    ))
                }
                Err(err) => {
                    notes.push(format!("w3e=ERR({err})"));
                    failures += 1;
                }
            }
        } else {
            notes.push("w3e=absent".into());
        }

        if let Ok(bytes) = archive.read("war3map.wpm") {
            match PathingMap::parse(&bytes) {
                Ok(p) => notes.push(format!("wpm=v0 {}x{}", p.width, p.height)),
                Err(err) => {
                    notes.push(format!("wpm=ERR({err})"));
                    failures += 1;
                }
            }
        } else {
            notes.push("wpm=absent".into());
        }

        if let Ok(bytes) = archive.read("war3map.w3i") {
            match MapInfo::parse(&bytes) {
                Ok(info) => notes.push(format!(
                    "w3i=v{} name={:?} author={:?}",
                    info.version, info.name, info.author
                )),
                Err(err) => {
                    notes.push(format!("w3i=ERR({err})"));
                    failures += 1;
                }
            }
        } else {
            notes.push("w3i=absent".into());
        }

        if let Ok(bytes) = archive.read("war3map.doo") {
            let version = bytes.get(4..8).map(|b| {
                u32::from_le_bytes(b.try_into().expect("4"))
            });
            let sub = bytes.get(8..12).map(|b| {
                u32::from_le_bytes(b.try_into().expect("4"))
            });
            let head = format!(
                "{}{}/{}",
                fourcc(&bytes),
                version.map(|v| v.to_string()).unwrap_or_default(),
                sub.map(|v| v.to_string()).unwrap_or_default()
            );
            match parse_doodads(&bytes) {
                Ok(d) => notes.push(format!("doo={head} n={}", d.len())),
                Err(err) => {
                    notes.push(format!("doo={head} ERR({err})"));
                    failures += 1;
                }
            }
        } else {
            notes.push("doo=absent".into());
        }

        if let Ok(bytes) = archive.read("war3mapUnits.doo") {
            let version = bytes.get(4..8).map(|b| {
                u32::from_le_bytes(b.try_into().expect("4"))
            });
            let sub = bytes.get(8..12).map(|b| {
                u32::from_le_bytes(b.try_into().expect("4"))
            });
            let head = format!(
                "{}{}/{}",
                fourcc(&bytes),
                version.map(|v| v.to_string()).unwrap_or_default(),
                sub.map(|v| v.to_string()).unwrap_or_default()
            );
            match parse_units(&bytes) {
                Ok(u) => notes.push(format!("units={head} n={}", u.len())),
                Err(err) => {
                    notes.push(format!("units={head} ERR({err})"));
                    failures += 1;
                }
            }
        } else {
            notes.push("units=absent".into());
        }

        for kind in ObjectKind::ALL {
            let name = format!("war3map.{}", kind.extension());
            if let Ok(bytes) = archive.read(&name) {
                match ObjectDataFile::parse(kind, &bytes) {
                    Ok(file) => notes.push(format!(
                        "{}=v{} base={} custom={}",
                        kind.extension(),
                        file.version,
                        file.base.len(),
                        file.custom.len()
                    )),
                    Err(err) => {
                        notes.push(format!("{}=ERR({err})", kind.extension()));
                        failures += 1;
                    }
                }
            }
        }

        println!("OK {} {}", rel.display(), notes.join(" "));
    }
    println!(
        "SUMMARY maps={} opened={} failures={}",
        maps.len(),
        opened,
        failures
    );
    if failures != 0 {
        std::process::exit(1);
    }
}
