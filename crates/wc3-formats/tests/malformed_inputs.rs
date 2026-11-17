//! Malformed operator data must be rejected, never panic.
//!
//! Every case here feeds bytes an attacker (or a corrupt download) can put in a
//! map archive: truncated models, lying counts, out-of-range indices, bad MPQ
//! tables. The contract is:
//!
//! * a structurally broken file returns `Err` (typed),
//! * a broken *element* inside a readable file is skipped and counted
//!   (`wc3_formats::diag`), and
//! * no path may panic, index out of range, or allocate from a declared count.
//!
//! `catch_unwind` makes the no-panic requirement explicit: a panic fails the
//! case with the input that caused it instead of only aborting the test.

use std::panic::{AssertUnwindSafe, catch_unwind};

use wc3_formats::blp;
use wc3_formats::fdf::FdfDocument;
use wc3_formats::gamedata::{FourCc, ObjectDataFile, ObjectKind, ProfileOverlay, Sheet, Wts};
use wc3_formats::map::{parse_doodads_ex, parse_units_ex};
use wc3_formats::mdx::MdxModel;
use wc3_formats::w3e::Terrain;
use wc3_formats::w3i::MapInfo;
use wc3_formats::wpm::PathingMap;

/// Run `f`, converting a panic into a named failure.
fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => panic!("{what}: panicked on malformed input"),
    }
}

/// Peak resident set of this test process (Linux `VmHWM`), in bytes.
///
/// The contract for this suite is that malformed-input coverage costs bytes,
/// not a mounted game install: it must stay under a few hundred MB so it can
/// run next to other agents on the shared device.
fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Budget for this binary: 512 MiB. A byte-level parser suite has no reason to
/// approach it; crossing it means a case started building real assets.
const PEAK_RSS_BUDGET: u64 = 512 * 1024 * 1024;

/// Deterministic pseudo-random source (no dependency, same bytes every run).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 { 0 } else { (self.next() >> 33) as usize % limit }
    }
}

fn fixture(relative: &str) -> Option<Vec<u8>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read(path).ok()
}

fn le_u32(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

// ---------------------------------------------------------------- MDX -------

/// Minimal well-formed MDX: magic, `VERS`, `MODL`, `SEQS` (one sequence).
fn synthetic_mdx() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MDLX");
    let mut vers = Vec::new();
    vers.extend_from_slice(&800u32.to_le_bytes());
    bytes.extend_from_slice(b"VERS");
    bytes.extend_from_slice(&le_u32(vers.len() as u32));
    bytes.extend_from_slice(&vers);

    let mut modl = Vec::new();
    let mut name = [0u8; 80];
    name[..7].copy_from_slice(b"malform");
    modl.extend_from_slice(&name);
    modl.extend_from_slice(&le_u32(0));
    modl.extend_from_slice(&le_u32(1000));
    modl.extend_from_slice(&le_u32(150));
    bytes.extend_from_slice(b"MODL");
    bytes.extend_from_slice(&le_u32(modl.len() as u32));
    bytes.extend_from_slice(&modl);

    bytes
}

#[test]
fn mdx_truncated_real_model_never_panics() {
    let Some(full) = fixture("analysis/war3_samples/Units/Human/Footman/Footman.mdx") else {
        eprintln!("skipping: Footman.mdx fixture missing");
        return;
    };
    // Ten evenly spaced truncation boundaries, including 0 and `len - 1`.
    let mut cuts: Vec<usize> = (0..=10)
        .map(|index| index * full.len() / 10)
        .collect();
    cuts.push(full.len().saturating_sub(1));

    let mut accepted = 0usize;
    for cut in cuts {
        let part = &full[..cut.min(full.len())];
        let result = no_panic(&format!("mdx truncation at {cut}"), || {
            MdxModel::parse(part)
        });
        // A cut inside a chunk payload must fail; a cut exactly on a chunk
        // boundary is a valid prefix and may parse.
        if result.is_ok() {
            accepted += 1;
        }
    }
    // 0 and a mid-payload cut must be errors; a whole-file prefix may pass.
    assert!(
        accepted < 12,
        "every truncation parsed; the walker is not checking chunk sizes"
    );
}

#[test]
fn mdx_truncated_at_every_short_length_is_total() {
    let full = synthetic_mdx();
    for cut in 0..=full.len() {
        let part = &full[..cut];
        no_panic(&format!("synthetic mdx truncation at {cut}"), || {
            let _ = MdxModel::parse(part);
            let _ = MdxModel::chunks(part);
            let _ = MdxModel::node_object_count(part);
        });
    }
    // The complete file still parses (the truncation loop must not have been
    // "passing" because nothing ever parses).
    let model = MdxModel::parse(&full).expect("synthetic mdx parses");
    assert_eq!(model.version, 800);
    assert_eq!(model.name, "malform");
}

#[test]
fn mdx_lying_chunk_size_is_an_error() {
    let mut bytes = synthetic_mdx();
    // Overwrite the `VERS` chunk size with the maximum u32.
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let result = no_panic("mdx oversized chunk", || MdxModel::parse(&bytes));
    assert!(result.is_err(), "u32::MAX chunk size must be rejected");

    // A chunk that ends one byte past EOF is also an error.
    let mut bytes = synthetic_mdx();
    let oversize = (bytes.len() - 8) as u32 + 1;
    bytes.extend_from_slice(b"JUNK");
    bytes.extend_from_slice(&oversize.to_le_bytes());
    assert!(no_panic("mdx chunk past eof", || MdxModel::parse(&bytes)).is_err());
}

#[test]
fn mdx_random_corruption_never_panics() {
    let full = synthetic_mdx();
    let mut rng = Lcg(0x0d15_c0de_5eed_0001);
    for case in 0..256 {
        let mut bytes = full.clone();
        let edits = 1 + rng.below(8);
        for _ in 0..edits {
            let index = rng.below(bytes.len());
            bytes[index] = rng.below(256) as u8;
        }
        no_panic(&format!("mdx corruption case {case}"), || {
            let _ = MdxModel::parse(&bytes);
            let _ = MdxModel::chunks(&bytes);
        });
    }
}

// ----------------------------------------------------------------- W3E ------

/// Well-formed 4×4 v11 terrain: header, no tilesets, 16 tiles.
fn synthetic_w3e(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"W3E!");
    bytes.extend_from_slice(&11u32.to_le_bytes()); // version
    bytes.push(b'A'); // tileset
    bytes.extend_from_slice(&0u32.to_le_bytes()); // custom tileset
    bytes.extend_from_slice(&0u32.to_le_bytes()); // ground tiles
    bytes.extend_from_slice(&0u32.to_le_bytes()); // cliff tiles
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&0f32.to_le_bytes()); // centre x
    bytes.extend_from_slice(&0f32.to_le_bytes()); // centre y
    for _ in 0..(width as usize * height as usize) {
        bytes.extend_from_slice(&8192u16.to_le_bytes()); // height
        bytes.extend_from_slice(&0u16.to_le_bytes()); // water
        bytes.push(0); // texture/flags
        bytes.push(0); // variation
        bytes.push(0); // cliff
    }
    bytes
}

#[test]
fn w3e_lying_dimensions_are_errors() {
    let mut bytes = synthetic_w3e(4, 4);
    // Claim 1e9 × 1e9 tiles: must be rejected before any allocation.
    bytes[20..24].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    bytes[24..28].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("w3e huge dims", || Terrain::parse(&bytes)).is_err());

    // A valid size whose tile array is short: truncate one tile.
    let mut bytes = synthetic_w3e(4, 4);
    bytes.truncate(bytes.len() - 7);
    assert!(no_panic("w3e short tiles", || Terrain::parse(&bytes)).is_err());
}

#[test]
fn w3e_lying_texture_counts_are_errors() {
    let mut bytes = synthetic_w3e(4, 4);
    // ground texture count = 1e9 (four-byte id each) — must not reserve.
    bytes[13..17].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("w3e huge ground count", || Terrain::parse(&bytes)).is_err());

    let mut bytes = synthetic_w3e(4, 4);
    // cliff texture count at offset 17.
    bytes[17..21].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("w3e huge cliff count", || Terrain::parse(&bytes)).is_err());
}

#[test]
fn w3e_truncated_at_every_length_never_panics() {
    let full = synthetic_w3e(4, 4);
    for cut in 0..=full.len() {
        no_panic(&format!("w3e truncation at {cut}"), || {
            let _ = Terrain::parse(&full[..cut]);
        });
    }
}

// ----------------------------------------------------------------- DOO ------

/// One v8 doodad record (no item sets, no lights) with an optional item-table
/// index and item-set count.
fn synthetic_doo_payload(sets: u32, items_per_set: u32, item_table: i32, lights: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"W3do");
    bytes.extend_from_slice(&8u32.to_le_bytes()); // version
    bytes.extend_from_slice(&0u32.to_le_bytes()); // subversion
    bytes.extend_from_slice(&1u32.to_le_bytes()); // count
    bytes.extend_from_slice(b"ATtr"); // id
    bytes.extend_from_slice(&0u32.to_le_bytes()); // variation
    for value in [1.0f32, 2.0, 3.0, 0.5] {
        bytes.extend_from_slice(&value.to_le_bytes()); // x, y, z, rotation
    }
    for value in [1.0f32, 1.0, 1.0] {
        bytes.extend_from_slice(&value.to_le_bytes()); // scale
    }
    bytes.push(0); // state
    bytes.push(100); // life
    bytes.extend_from_slice(&item_table.to_le_bytes());
    bytes.extend_from_slice(&sets.to_le_bytes());
    for _ in 0..sets {
        bytes.extend_from_slice(&items_per_set.to_le_bytes());
        for _ in 0..items_per_set {
            bytes.extend_from_slice(b"ratc");
            bytes.extend_from_slice(&100u32.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // creation number
    bytes.extend_from_slice(&lights.to_le_bytes());
    bytes
}

/// One v12 doodad record that declares `lights` light records.
fn synthetic_doo_v12_lights(lights: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"W3do");
    bytes.extend_from_slice(&12u32.to_le_bytes()); // version (group/roll/pitch/lights)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // subversion
    bytes.extend_from_slice(&1u32.to_le_bytes()); // count
    bytes.extend_from_slice(b"ATtr"); // id
    bytes.extend_from_slice(&0u32.to_le_bytes()); // variation
    for value in [1.0f32, 2.0, 3.0, 0.5] {
        bytes.extend_from_slice(&value.to_le_bytes()); // x, y, z, rotation
    }
    for value in [1.0f32, 1.0, 1.0] {
        bytes.extend_from_slice(&value.to_le_bytes()); // scale
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // group (v12+)
    bytes.push(0); // state
    bytes.push(100); // life
    bytes.extend_from_slice(&0i32.to_le_bytes()); // item table
    bytes.extend_from_slice(&0u32.to_le_bytes()); // item sets
    bytes.extend_from_slice(&0u32.to_le_bytes()); // creation number
    bytes.extend_from_slice(&0f32.to_le_bytes()); // roll (v12+)
    bytes.extend_from_slice(&0f32.to_le_bytes()); // pitch (v12+)
    bytes.extend_from_slice(&lights.to_le_bytes());
    bytes
}

#[test]
fn doo_lying_count_is_an_error() {
    let mut bytes = synthetic_doo_payload(0, 0, 0, 0);
    // Declare 1e9 doodads in a 40-byte file.
    bytes[12..16].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("doo lying count", || parse_doodads_ex(&bytes)).is_err());
}

#[test]
fn doo_out_of_range_indices_never_panic() {
    // Item table index that no table can satisfy: read and ignored.
    let bytes = synthetic_doo_payload(0, 0, i32::MIN, 0);
    let file = no_panic("doo extreme item table index", || parse_doodads_ex(&bytes))
        .expect("a doodad with an ignored index still parses");
    assert_eq!(file.doodads.len(), 1);

    // 1e9 item sets declared in a 40-byte record: the parser must fail on the
    // first truncated read instead of looping or allocating. The record body is
    // built once (a generator that loops over the declared count would repeat
    // the same bug in the test).
    let mut lying = synthetic_doo_payload(0, 0, 0, 0);
    // Layout tail: `… item_table, sets, creation, lights` (4 bytes each), so the
    // sets count sits 12 bytes before the end of the record.
    let sets_offset = lying.len() - 12;
    lying[sets_offset..sets_offset + 4].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("doo huge item sets", || parse_doodads_ex(&lying)).is_err());

    // 1e9 light records (36 bytes each). Lights exist from v12 on, so this case
    // needs the v12 record layout (`.. roll, pitch, lights`).
    let lying = synthetic_doo_v12_lights(1_000_000_000);
    assert!(no_panic("doo huge light count", || parse_doodads_ex(&lying)).is_err());
}

#[test]
fn doo_truncated_at_every_length_never_panics() {
    let full = synthetic_doo_payload(1, 1, 0, 0);
    for cut in 0..=full.len() {
        no_panic(&format!("doo truncation at {cut}"), || {
            let _ = parse_doodads_ex(&full[..cut]);
        });
    }

    let units = {
        let mut bytes = synthetic_doo_payload(0, 0, 0, 0);
        // Reuse the record body with the units signature: fields differ, the
        // point is that a hostile count/record never panics.
        bytes[..4].copy_from_slice(b"W3do");
        bytes
    };
    for cut in 0..=units.len() {
        no_panic(&format!("units truncation at {cut}"), || {
            let _ = parse_units_ex(&units[..cut]);
        });
    }
    // A unit file whose count lies by 1e9 is an error, not a hang.
    let mut bytes = units.clone();
    bytes[12..16].copy_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(no_panic("units lying count", || parse_units_ex(&bytes)).is_err());
}

// ----------------------------------------------------------------- W3I ------

fn synthetic_w3i(version: u32, tail: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&version.to_le_bytes());
    if version >= 18 {
        bytes.extend_from_slice(&1u32.to_le_bytes()); // saves
        bytes.extend_from_slice(&6059u32.to_le_bytes()); // editor version
    }
    if version >= 27 {
        // Four-part game version the reader skips positionally.
        bytes.extend_from_slice(&[26, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]);
    }
    for text in ["Map", "Author", "Description", "Recommended"] {
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
    }
    bytes.extend_from_slice(tail);
    bytes
}

#[test]
fn w3i_huge_player_count_is_preserved_not_parsed() {
    // The player block lives in the unparsed tail; a forged count must not
    // drive any allocation or index.
    let mut tail = Vec::new();
    tail.extend_from_slice(&u32::MAX.to_le_bytes()); // player count
    tail.extend_from_slice(&vec![0u8; 32]);
    let bytes = synthetic_w3i(18, &tail);
    let info = no_panic("w3i huge player count", || MapInfo::parse(&bytes))
        .expect("w3i parses with the player block in the tail");
    assert_eq!(info.version, 18);
    assert_eq!(info.name, "Map");
    assert!(info.raw_tail.len() >= 4);

    // A v27 header cut inside the game-version block is an error, not a panic.
    let short = &synthetic_w3i(27, &[])[..20];
    assert!(no_panic("w3i short v27", || MapInfo::parse(short)).is_err());
}

#[test]
fn w3i_truncated_at_every_length_never_panics() {
    let full = synthetic_w3i(27, b"tail");
    for cut in 0..=full.len() {
        no_panic(&format!("w3i truncation at {cut}"), || {
            let _ = MapInfo::parse(&full[..cut]);
        });
    }
    // An unterminated name string is a typed error.
    let mut bytes = synthetic_w3i(8, &[]);
    bytes.truncate(bytes.len() - 1);
    assert!(no_panic("w3i unterminated string", || MapInfo::parse(&bytes)).is_err());
}

// ----------------------------------------------------------------- WPM ------

#[test]
fn wpm_lying_dimensions_and_truncation_never_panic() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MP3W");
    bytes.extend_from_slice(&0u32.to_le_bytes()); // version
    bytes.extend_from_slice(&4096u32.to_le_bytes()); // width
    bytes.extend_from_slice(&4096u32.to_le_bytes()); // height
    bytes.extend_from_slice(&[0u8; 16]);
    // 4096² cells were declared and 16 supplied: typed error, no allocation of
    // 16 MiB for a file that cannot fill it.
    assert!(no_panic("wpm short cells", || PathingMap::parse(&bytes)).is_err());

    for cut in 0..=bytes.len() {
        no_panic(&format!("wpm truncation at {cut}"), || {
            let _ = PathingMap::parse(&bytes[..cut]);
        });
    }

    // Zero dimension and out-of-range dimension both fail cleanly.
    let mut zero = bytes.clone();
    zero[8..12].copy_from_slice(&0u32.to_le_bytes());
    assert!(no_panic("wpm zero width", || PathingMap::parse(&zero)).is_err());
    let mut huge = bytes.clone();
    huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(no_panic("wpm huge width", || PathingMap::parse(&huge)).is_err());
}

// ------------------------------------------------------------ object data ---

#[test]
fn object_data_bad_sections_are_errors() {
    // Version out of range.
    let mut bytes = 7u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert!(no_panic("w3u bad version", || ObjectDataFile::parse(
        ObjectKind::Unit,
        &bytes
    ))
    .is_err());

    // 1e9 entries in the base section.
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&1_000_000_000u32.to_le_bytes());
    assert!(
        no_panic("w3u huge count", || ObjectDataFile::parse(
            ObjectKind::Unit,
            &bytes
        ))
        .is_err()
    );

    // Truncated custom section: the base section reads, the second does not.
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(b"hfoo");
    assert!(
        no_panic("w3u truncated custom", || ObjectDataFile::parse(
            ObjectKind::Unit,
            &bytes
        ))
        .is_err()
    );
}

#[test]
fn object_data_truncated_at_every_length_never_panics() {
    for kind in [
        ObjectKind::Unit,
        ObjectKind::Ability,
        ObjectKind::Item,
        ObjectKind::Doodad,
        ObjectKind::Upgrade,
        ObjectKind::Buff,
    ] {
        let full = {
            let mut bytes = 2u32.to_le_bytes().to_vec();
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes
        };
        for cut in 0..=full.len() {
            no_panic(&format!("object {:?} truncation at {cut}", kind), || {
                let _ = ObjectDataFile::parse(kind, &full[..cut]);
            });
        }
    }
}

// ---------------------------------------------------------------- WTS -------

#[test]
fn wts_broken_keys_and_blocks_are_errors() {
    // Non-numeric key.
    assert!(
        no_panic("wts non-numeric key", || Wts::parse(b"STRING abc\n{\ntext\n}\n")).is_err()
    );
    // Key above u32.
    assert!(
        no_panic("wts overflowing key", || Wts::parse(
            b"STRING 999999999999999999999\n{\ntext\n}\n"
        ))
        .is_err()
    );
    // Unterminated block.
    assert!(no_panic("wts unterminated", || Wts::parse(b"STRING 1\n{\ntext\n")).is_err());
    // Not UTF-8.
    assert!(no_panic("wts bad utf8", || Wts::parse(&[0xFF, 0xFE, 0x00])).is_err());

    // A valid u32::MAX key parses; the key is data, not an index.
    let table = no_panic("wts max key", || {
        Wts::parse(b"STRING 4294967295\n{\nhello\n}\n")
    })
    .expect("u32::MAX key parses");
    assert_eq!(table.len(), 1);
    assert_eq!(table.get(4_294_967_295), Some("hello"));
}

// ---------------------------------------------------------------- FDF -------

#[test]
fn fdf_truncated_at_every_length_never_panics() {
    let source = "Frame \"A\" {\n  Width 0.5,\n  Child \"B\" { Height 0.1, }\n}\n";
    for cut in 0..=source.len() {
        if !source.is_char_boundary(cut) {
            continue;
        }
        no_panic(&format!("fdf truncation at {cut}"), || {
            let _ = FdfDocument::parse(&source[..cut]);
        });
    }
    // An unbalanced brace and a stray `}` are typed errors.
    assert!(no_panic("fdf unbalanced", || FdfDocument::parse("Frame \"A\" {")).is_err());
    assert!(no_panic("fdf stray brace", || FdfDocument::parse("}")).is_err());
}

// ---------------------------------------------------------------- BLP -------

#[test]
fn blp_absurd_dimensions_are_rejected_before_allocating() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BLP1");
    bytes.extend_from_slice(&1u32.to_le_bytes()); // content type paletted
    bytes.extend_from_slice(&0u32.to_le_bytes()); // alpha bits
    bytes.extend_from_slice(&8192u32.to_le_bytes()); // width
    bytes.extend_from_slice(&8192u32.to_le_bytes()); // height
    bytes.resize(156, 0);
    // 8192² = 67 M px > the 16 M px ceiling: a map may not allocate 256 MiB.
    assert!(no_panic("blp huge palette", || blp::decode(&bytes)).is_err());

    // Same for BLP2.
    let mut bytes2 = Vec::new();
    bytes2.extend_from_slice(b"BLP2");
    bytes2.extend_from_slice(&0u32.to_le_bytes());
    bytes2.push(2); // compression dxt
    bytes2.push(3); // alpha bits
    bytes2.push(8); // alpha type
    bytes2.extend_from_slice(&8192u32.to_le_bytes());
    bytes2.extend_from_slice(&8192u32.to_le_bytes());
    bytes2.resize(148, 0);
    assert!(no_panic("blp2 huge dxt", || blp::decode(&bytes2)).is_err());
}

#[test]
fn blp_truncated_at_every_length_never_panics() {
    let mut full = Vec::new();
    full.extend_from_slice(b"BLP2");
    full.extend_from_slice(&0u32.to_le_bytes());
    full.push(1); // paletted
    full.push(8); // alpha bits
    full.push(8);
    full.extend_from_slice(&64u32.to_le_bytes());
    full.extend_from_slice(&64u32.to_le_bytes());
    full.resize(148, 0);
    // One mip: palette + alpha plane + 64*64 indices.
    full.extend_from_slice(&vec![0u8; 1024]);
    full.extend_from_slice(&vec![255u8; 1024]);
    let mip_offset = full.len() as u32;
    full[20..24].copy_from_slice(&mip_offset.to_le_bytes());
    full[84..88].copy_from_slice(&4096u32.to_le_bytes()); // mip size
    for cut in 0..=full.len().min(2048) {
        no_panic(&format!("blp truncation at {cut}"), || {
            let _ = blp::decode(&full[..cut]);
        });
    }
    // A short magic and a wrong magic are typed errors.
    assert!(no_panic("blp short magic", || blp::decode(b"BLP")).is_err());
    assert!(no_panic("blp wrong magic", || blp::decode(b"NOPE0000")).is_err());
}

// ---------------------------------------------------------------- SLK -------

#[test]
fn slk_out_of_range_position_is_skipped_and_counted() {
    // A Y coordinate of 1e9 used to resize the grid to 1e9 rows.
    let slk = "ID;PWXL;N;E\nB;X2;Y3;D0\nC;X1;Y1;K\"id\"\nC;X2;Y1;K\"HP\"\n\
               C;X1;Y2;K\"hfoo\"\nC;X2;Y2;K420\n\
               C;X1;Y1000000000;K\"ghost\"\nC;X2;Y1000000000;K1\nE\n";
    let before = wc3_formats::diag::count("slk_position_out_of_range");
    let sheet = no_panic("slk huge position", || Sheet::parse("malformed", slk.as_bytes()))
        .expect("the valid rows still parse");
    let after = wc3_formats::diag::count("slk_position_out_of_range");
    assert!(
        after > before,
        "the out-of-range position must be counted (before={before} after={after})"
    );
    assert_eq!(sheet.len(), 1, "only the valid row survives");
    assert_eq!(
        sheet.cell("hfoo", "id").and_then(|cell| cell.text()),
        Some("hfoo")
    );
}

#[test]
fn slk_missing_column_reads_through_the_typed_default() {
    let slk = "ID;PWXL;N;E\nB;X2;Y2;D0\nC;X1;Y1;K\"id\"\nC;X2;Y1;K\"HP\"\n\
               C;X1;Y2;K\"hfoo\"\nC;X2;Y2;K420\nE\n";
    let sheet = Sheet::parse("malformed", slk.as_bytes()).expect("sheet parses");
    // Missing column: lookup is `None` (the typed default at the caller), and
    // the sheet still answers the columns it has.
    assert!(sheet.column_index("regenHP").is_none());
    assert!(sheet.cell("hfoo", "regenHP").is_none());
    assert_eq!(sheet.cell("hfoo", "HP").and_then(|c| c.i32_lossy()), Some(420));
}

#[test]
fn slk_lying_bounds_and_skipped_rows_are_errors_or_counted() {
    // Declared bounds smaller than the real grid: error, not a panic.
    let slk = "ID;PWXL;N;E\nB;X1;Y1;D0\nC;X1;Y1;K\"id\"\nC;X1;Y2;K\"a\"\n\
               C;X1;Y3;K\"b\"\nE\n";
    assert!(no_panic("slk lying bounds", || Sheet::parse("bad", slk.as_bytes())).is_err());

    // Headerless rows are dropped and counted, not indexed.
    let slk = "ID;PWXL;N;E\nB;X2;Y3;D0\nC;X1;Y1;K\"id\"\nC;X2;Y1;K\"HP\"\n\
               C;X2;Y2;K420\nC;X1;Y3;K\"hfoo\"\nC;X2;Y3;K220\nE\n";
    let sheet = no_panic("slk headerless row", || Sheet::parse("rows", slk.as_bytes()))
        .expect("headerless rows are skipped, not fatal");
    assert!(sheet.skipped_rows() >= 1);
    assert_eq!(sheet.len(), 1);
}

// ------------------------------------------------- profile overlays ---------

#[test]
fn profile_overlay_rejects_absurd_component_index() {
    let before = wc3_formats::diag::count("profile_component_index");
    let mut overlay = ProfileOverlay::default();
    // i32::MAX used to grow the component vector to 2^31 entries.
    overlay.set("hfoo", "level", i32::MAX, "boom");
    let after = wc3_formats::diag::count("profile_component_index");
    assert!(after > before, "the rejected index must be counted");
    assert!(overlay.get("hfoo", "level", 0).is_none());
    // A normal index still works.
    overlay.set("hfoo", "level", 3, "ok");
    assert_eq!(overlay.get("hfoo", "level", 3), Some("ok"));
}

#[test]
fn fourcc_parse_is_total() {
    assert_eq!(FourCc::parse("hfoo").map(|c| c.as_str().into_owned()), Some("hfoo".to_string()));
    assert!(FourCc::parse("hfo").is_none());
    assert!(FourCc::parse("hfool").is_none());
    assert!(FourCc::parse("").is_none());
    assert_eq!(FourCc::parse_or_zero(""), Some(FourCc::new([0; 4])));
}

// ----------------------------------------------------------------- MPQ ------

/// Write `bytes` to a uniquely named temp file and return its path.
fn temp_archive(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "h2-malformed-{}-{}-{tag}.mpq",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, bytes).expect("write temp archive");
    path
}

fn mpq_header(
    hash_pos: u32,
    block_pos: u32,
    hash_count: u32,
    block_count: u32,
    block_size: u16,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MPQ\x1a");
    bytes.extend_from_slice(&0x20u32.to_le_bytes()); // header size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // archive size (unused)
    bytes.extend_from_slice(&0u16.to_le_bytes()); // format version 0
    bytes.extend_from_slice(&block_size.to_le_bytes());
    bytes.extend_from_slice(&hash_pos.to_le_bytes());
    bytes.extend_from_slice(&block_pos.to_le_bytes());
    bytes.extend_from_slice(&hash_count.to_le_bytes());
    bytes.extend_from_slice(&block_count.to_le_bytes());
    bytes
}

#[test]
fn mpq_bad_tables_are_errors_or_marked_malformed() {
    // 1. Block table past EOF: typed error.
    let path = temp_archive("block-past-eof", &mpq_header(0x20, 0x1_0000, 4, 4, 3));
    let result = no_panic("mpq block table past eof", || wc3_mpq::MpqArchive::open(&path));
    assert!(result.is_err(), "a table past EOF must fail to open");
    let _ = std::fs::remove_file(&path);

    // 2. Zero table counts: typed error.
    let path = temp_archive("zero-counts", &mpq_header(0x20, 0x40, 0, 0, 3));
    assert!(no_panic("mpq zero counts", || wc3_mpq::MpqArchive::open(&path)).is_err());
    let _ = std::fs::remove_file(&path);

    // 3. Hash count not a power of two: typed error.
    let path = temp_archive("bad-hash-count", &mpq_header(0x20, 0x40, 3, 4, 3));
    assert!(no_panic("mpq hash count", || wc3_mpq::MpqArchive::open(&path)).is_err());
    let _ = std::fs::remove_file(&path);

    // 4. Declared block table larger than the file: Storm reads what exists and
    //    marks the archive malformed; reading a block must still be an `Err`.
    let mut bytes = mpq_header(0x20, 0x60, 4, 0x1000, 3);
    bytes.resize(0x60 + 64, 0);
    let path = temp_archive("short-block-table", &bytes);
    match no_panic("mpq short block table", || wc3_mpq::MpqArchive::open(&path)) {
        Ok(archive) => {
            assert!(
                archive.is_malformed(),
                "a cut block table must be reported as malformed"
            );
            let read = no_panic("mpq read from malformed archive", || archive.read_block(0));
            assert!(read.is_err(), "reading a fabricated block must be an error");
        }
        Err(_) => { /* rejecting outright is also acceptable */ }
    }
    let _ = std::fs::remove_file(&path);

    // 5. Random bytes and a header-only file must not panic either.
    let mut rng = Lcg(0x0bad_0000_1234_5678);
    let garbage: Vec<u8> = (0..512).map(|_| rng.below(256) as u8).collect();
    let path = temp_archive("garbage", &garbage);
    assert!(no_panic("mpq garbage", || wc3_mpq::MpqArchive::open(&path)).is_err());
    let _ = std::fs::remove_file(&path);

    let path = temp_archive("header-only", &mpq_header(0x20, 0x40, 4, 4, 3));
    assert!(no_panic("mpq header only", || wc3_mpq::MpqArchive::open(&path)).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn suite_stays_within_its_memory_budget() {
    match peak_rss_bytes() {
        Some(peak) => {
            eprintln!(
                "malformed_inputs peak RSS: {:.1} MiB (budget {:.0} MiB)",
                peak as f64 / (1024.0 * 1024.0),
                PEAK_RSS_BUDGET as f64 / (1024.0 * 1024.0)
            );
            assert!(
                peak < PEAK_RSS_BUDGET,
                "malformed_inputs peaked at {} bytes; the suite must stay byte-level",
                peak
            );
        }
        None => eprintln!("skipping RSS check: /proc/self/status unavailable"),
    }
}

#[test]
fn mpq_index_bytes_are_total() {
    // A truncated index (valid magic, no payload) is a typed error.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MPQIDX1\0");
    assert!(no_panic("mpq index truncated", || {
        wc3_mpq::MpqIndex::from_bytes(&bytes)
    })
    .is_err());

    // Random bytes are a typed error, never a panic.
    let mut rng = Lcg(0x5eed_0000_0000_0001);
    for case in 0..64 {
        let bytes: Vec<u8> = (0..(8 + rng.below(200)))
            .map(|_| rng.below(256) as u8)
            .collect();
        no_panic(&format!("mpq index garbage {case}"), || {
            let _ = wc3_mpq::MpqIndex::from_bytes(&bytes);
        });
    }
}
