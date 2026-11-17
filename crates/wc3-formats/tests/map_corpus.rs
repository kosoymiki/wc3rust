//! Corpus gate: every War3Net test map must parse through every parser we own.
//!
//! The corpus is `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps`
//! (99 maps: 54 `.w3m` + 45 `.w3x`, spanning RoC beta v8, TFT, 1.33 and 2.0.3).
//!
//! A map passes when:
//! - its container opens with [`wc3_mpq::MpqArchive`] at the detected base;
//! - every target file present in the archive parses **and is consumed
//!   exactly** — leftover bytes mean a field-order/field-size mismatch even
//!   when the parser did not error;
//! - a file that is legitimately absent (early beta maps have no `wpm`) is
//!   reported, not failed.
//!
//! Expected bytes for the whole-corpus gate are pinned in the test so a
//! regression cannot quietly reduce coverage (for example by skipping maps).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wc3_formats::gamedata::{ObjectDataFile, ObjectKind};
use wc3_formats::map::{map_mpq_base, parse_doodads_ex, parse_units_ex};
use wc3_formats::w3e::Terrain;
use wc3_formats::w3i::MapInfo;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps")
}

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
                path.extension().and_then(|ext| ext.to_str()),
                Some("w3m") | Some("w3x")
            ) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[derive(Default)]
struct Coverage {
    maps: usize,
    opened: usize,
    w3e: usize,
    wpm: usize,
    w3i: usize,
    doo: usize,
    units: usize,
    object_files: BTreeMap<&'static str, usize>,
    absent: BTreeMap<&'static str, usize>,
    /// `(map, file, trailing bytes)` for upstream files with small padding.
    padded: Vec<(String, String, usize)>,
    failures: Vec<String>,
}

impl Coverage {
    fn fail(&mut self, map: &Path, what: &str, detail: String) {
        self.failures
            .push(format!("{} :: {what} :: {detail}", map.display()));
    }
}

/// Trailing bytes a well-formed upstream file may carry. Two donor fixtures do:
/// the Spazzler-protected map has one junk byte after `war3mapUnits.doo`, and
/// `PKCompressed.w3x` pads `war3map.w3d` with four zero bytes. Misalignment
/// shows up as a residual that grows with the record count, so a small
/// residual is recorded (and pinned below) while anything larger fails.
const MAX_TOLERATED_PADDING: usize = 8;

fn check_exact(map: &Path, what: &str, parsed: usize, total: usize, cov: &mut Coverage) {
    if parsed == total {
        return;
    }
    let trailing = total.saturating_sub(parsed);
    if trailing <= MAX_TOLERATED_PADDING {
        let name = map
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        cov.padded.push((name, what.to_string(), trailing));
        return;
    }
    cov.fail(
        map,
        what,
        format!("{parsed} of {total} bytes consumed ({trailing} trailing)"),
    );
}

#[test]
fn every_donor_map_parses_through_the_formats_crate() {
    let root = corpus_root();
    if !root.is_dir() {
        eprintln!("skipping: donor map corpus missing at {}", root.display());
        return;
    }
    let maps = collect_maps(&root);
    assert_eq!(maps.len(), 99, "donor corpus size changed");

    let mut cov = Coverage::default();
    for path in &maps {
        cov.maps += 1;
        let base = match map_mpq_base(path) {
            Ok(base) => base,
            Err(err) => {
                cov.fail(path, "container", format!("{err}"));
                continue;
            }
        };
        let archive = match MpqArchive::open_at(path, base) {
            Ok(archive) => archive,
            Err(err) => {
                cov.fail(path, "container", format!("open@{base}: {err}"));
                continue;
            }
        };
        cov.opened += 1;

        match archive.read("war3map.w3e") {
            Ok(bytes) => {
                cov.w3e += 1;
                match Terrain::parse(&bytes) {
                    Ok(terrain) => {
                        check_exact(path, "w3e", bytes.len() - terrain.trailing_bytes, bytes.len(), &mut cov);
                        // Engine-profile parity: the classic ceiling is v11
                        // (`0x6f3eb207`), so every classic map must load under
                        // it and every Reforged map must be refused.
                        if terrain.version <= 11 {
                            Terrain::parse_classic_engine(&bytes).unwrap_or_else(|err| {
                                panic!(
                                    "{}: engine profile rejected v{}: {err}",
                                    path.display(),
                                    terrain.version
                                )
                            });
                        } else {
                            assert!(
                                Terrain::parse_classic_engine(&bytes).is_err(),
                                "{}: engine profile accepted v{}",
                                path.display(),
                                terrain.version
                            );
                        }
                        if terrain.version >= 12 {
                            assert_eq!(
                                terrain.texture_indices.len(),
                                terrain.tiles.len(),
                                "{}: v12 texture table",
                                path.display()
                            );
                        }
                        if terrain.tiles.len() != terrain.width as usize * terrain.height as usize {
                            cov.fail(path, "w3e", "tile count != width*height".into());
                        }
                    }
                    Err(err) => cov.fail(path, "w3e", format!("{err}")),
                }
            }
            Err(_) => {
                *cov.absent.entry("w3e").or_default() += 1;
            }
        }

        match archive.read("war3map.wpm") {
            Ok(bytes) => {
                cov.wpm += 1;
                match PathingMap::parse(&bytes) {
                    Ok(wpm) => check_exact(path, "wpm", bytes.len() - wpm.trailing_bytes, bytes.len(), &mut cov),
                    Err(err) => cov.fail(path, "wpm", format!("{err}")),
                }
            }
            Err(_) => {
                *cov.absent.entry("wpm").or_default() += 1;
            }
        }

        match archive.read("war3map.w3i") {
            Ok(bytes) => {
                cov.w3i += 1;
                match MapInfo::parse(&bytes) {
                    Ok(info) => {
                        if info.version >= 18 && info.name.is_empty() {
                            cov.fail(path, "w3i", "empty map name on v18+ file".into());
                        }
                        // v8 files have no map/editor version header; a
                        // parser that reads those bytes truncates the name
                        // ("Gnoll Wood" -> "od").
                        if info.version == 8 && info.name.is_empty() {
                            cov.fail(path, "w3i", "empty map name on v8 file".into());
                        }
                        if info.raw_tail.len() > bytes.len() {
                            cov.fail(path, "w3i", "raw tail larger than input".into());
                        }
                    }
                    Err(err) => cov.fail(path, "w3i", format!("{err}")),
                }
            }
            Err(_) => {
                *cov.absent.entry("w3i").or_default() += 1;
            }
        }

        match archive.read("war3map.doo") {
            Ok(bytes) => {
                cov.doo += 1;
                match parse_doodads_ex(&bytes) {
                    Ok(file) => check_exact(path, "doo", file.consumed, bytes.len(), &mut cov),
                    Err(err) => cov.fail(path, "doo", format!("{err}")),
                }
            }
            Err(_) => {
                *cov.absent.entry("doo").or_default() += 1;
            }
        }

        match archive.read("war3mapUnits.doo") {
            Ok(bytes) => {
                cov.units += 1;
                match parse_units_ex(&bytes) {
                    Ok((_, consumed)) => check_exact(path, "units", consumed, bytes.len(), &mut cov),
                    Err(err) => cov.fail(path, "units", format!("{err}")),
                }
            }
            Err(_) => {
                *cov.absent.entry("units").or_default() += 1;
            }
        }

        for kind in ObjectKind::ALL {
            let name = format!("war3map.{}", kind.extension());
            let Ok(bytes) = archive.read(&name) else {
                continue;
            };
            *cov.object_files.entry(kind.extension()).or_default() += 1;
            match ObjectDataFile::parse_ex(kind, &bytes) {
                Ok((_, consumed)) => {
                    check_exact(path, kind.extension(), consumed, bytes.len(), &mut cov)
                }
                Err(err) => cov.fail(path, kind.extension(), format!("{err}")),
            }
        }
    }

    eprintln!(
        "coverage: maps={} opened={} w3e={} wpm={} w3i={} doo={} units={} objects={:?} absent={:?}",
        cov.maps,
        cov.opened,
        cov.w3e,
        cov.wpm,
        cov.w3i,
        cov.doo,
        cov.units,
        cov.object_files,
        cov.absent
    );
    eprintln!("padded: {:?}", cov.padded);
    // Pinned padding set: exactly these two upstream artifacts, nothing else.
    assert_eq!(
        cov.padded,
        vec![
            (
                "Jurassic Park Survival EE v6.4.w3x".to_string(),
                "units".to_string(),
                1
            ),
            ("PKCompressed.w3x".to_string(), "w3d".to_string(), 4),
        ],
        "unexpected trailing bytes (possible field-order drift)"
    );
    if !cov.failures.is_empty() {
        for failure in &cov.failures {
            eprintln!("FAIL {failure}");
        }
        panic!(
            "{} of {} maps failed; first failure: {}",
            cov.failures.len().div_ceil(1),
            cov.maps,
            cov.failures[0]
        );
    }
    assert_eq!(cov.opened, 99, "every donor map container must open");
    assert_eq!(cov.w3e, 99, "terrain present in every map");
    assert_eq!(cov.wpm, 64, "pathing map present in 64 maps");
    assert_eq!(cov.w3i, 99);
    assert_eq!(cov.doo, 99);
    assert_eq!(cov.units, 98, "one map ships no war3mapUnits.doo");
    assert_eq!(cov.absent.get("wpm"), Some(&35));
    assert_eq!(cov.absent.get("units"), Some(&1));
}
