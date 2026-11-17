//! Shared negative corpus for every external-input parser (AUD-025).
//!
//! One hostile input per format, fed through the same public entry point the
//! game uses. The contract under test is the unified input-limit policy in
//! `docs/audit/INPUT_LIMITS.md`: a declared size, count, dimension or depth is
//! rejected *before* the allocation it would drive, with a typed error and no
//! panic (the release profile is `panic = "abort"`, so a panic is a process
//! crash, not a recoverable error).
//!
//! Each case names the cap it exercises. Caps enforced inside a crate by a
//! fixture that needs that crate's private test helpers (MPQ block-table
//! encryption) also have a crate-local test; the reference is in the case.
//!
//! Run: `scripts/h2-cargo test -p wc3-game --test input_limits --features movie_decoder`

use std::io::Write as _;
use std::path::PathBuf;

/// Write a hostile fixture to the platform temp dir and return its path.
fn write_temp(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wc3-input-limits-{tag}.bin"));
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(bytes).expect("write fixture");
    path
}

/// Minimal RIFF chunk: id, declared size, body, word padding.
fn riff_chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + body.len() + 1);
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() & 1 == 1 {
        out.push(0);
    }
    out
}

// ---------------------------------------------------------------- MPQ

/// MPQ: the hash-table count is untrusted and drives a `count * 16` read
/// (`crates/wc3-mpq/src/lib.rs:517`). The header must be refused before the
/// table read, not after allocating for it.
///
/// The block-entry byte cap (`MAX_FILE_SIZE`) needs an encrypted block table,
/// so its fixture lives in `wc3-mpq`:
/// `aud003_block_entry_above_the_file_size_cap_is_rejected_before_allocating`.
#[test]
fn mpq_rejects_an_oversized_table_count_before_reading_it() {
    let mut header = vec![0u8; 32];
    header[0..4].copy_from_slice(b"MPQ\x1a");
    header[4..8].copy_from_slice(&32u32.to_le_bytes()); // header size
    header[8..12].copy_from_slice(&32u32.to_le_bytes()); // archive size
    // bytes 12..14: version 0; bytes 14..16: sector shift 3.
    header[14..16].copy_from_slice(&3u16.to_le_bytes());
    header[16..20].copy_from_slice(&32u32.to_le_bytes()); // hash table pos
    header[20..24].copy_from_slice(&32u32.to_le_bytes()); // block table pos
    // 1 << 24 entries is a power of two, so it passes the shape check and can
    // only be stopped by the entry-count ceiling (8 << 20).
    header[24..28].copy_from_slice(&(1u32 << 24).to_le_bytes());
    header[28..32].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(header.len(), 32, "the fixture is a bare header");

    let path = write_temp("mpq-table-cap", &header);
    let err = wc3_mpq::MpqArchive::open(&path).expect_err("hostile table count is rejected");
    assert!(
        matches!(err, wc3_mpq::MpqError::Corrupt(_)),
        "expected a typed corrupt-archive error, got {err:?}"
    );
    std::fs::remove_file(&path).ok();
}

// ---------------------------------------------------------------- AVI

/// AVI: `LIST rec ` nesting inside `movi` is capped by `MAX_MOVI_DEPTH`
/// (`crates/wc3-bink/src/avi.rs:512`); the walk is iterative, and the depth
/// check must fire before another frame is pushed.
#[cfg(feature = "movie_decoder")]
#[test]
fn avi_rejects_nesting_beyond_the_movi_depth_limit() {
    use wc3_bink::avi::{Avi, AviError, MAX_MOVI_DEPTH};

    let mut nested = riff_chunk(b"00dc", &[]);
    for _ in 0..=MAX_MOVI_DEPTH {
        let mut body = b"rec ".to_vec();
        body.extend_from_slice(&nested);
        nested = riff_chunk(b"LIST", &body);
    }
    let mut movi = b"movi".to_vec();
    movi.extend_from_slice(&nested);
    let list = riff_chunk(b"LIST", &movi);
    let mut file = Vec::with_capacity(12 + list.len());
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&((4 + list.len()) as u32).to_le_bytes());
    file.extend_from_slice(b"AVI ");
    file.extend_from_slice(&list);

    let err = Avi::from_bytes(file).expect_err("33 nested rec levels are rejected");
    assert!(
        matches!(err, AviError::Limit { limit, .. } if limit == MAX_MOVI_DEPTH as usize),
        "expected the movi depth limit, got {err:?}"
    );

    // One level less is still a valid walk (the limit is inclusive, not off by
    // one), so a legitimate cinematic nesting is not rejected.
    let mut nested = riff_chunk(b"00dc", &[]);
    for _ in 0..MAX_MOVI_DEPTH - 1 {
        let mut body = b"rec ".to_vec();
        body.extend_from_slice(&nested);
        nested = riff_chunk(b"LIST", &body);
    }
    let mut movi = b"movi".to_vec();
    movi.extend_from_slice(&nested);
    let list = riff_chunk(b"LIST", &movi);
    let mut legit = Vec::with_capacity(12 + list.len());
    legit.extend_from_slice(b"RIFF");
    legit.extend_from_slice(&((4 + list.len()) as u32).to_le_bytes());
    legit.extend_from_slice(b"AVI ");
    legit.extend_from_slice(&list);
    // The walk itself succeeds; the file still has no `hdrl`, which is a
    // different (documented) rejection.
    let err = Avi::from_bytes(legit).expect_err("no vids stream");
    assert!(
        matches!(err, AviError::Missing("a vids stream")),
        "nesting below the limit must not hit the depth cap, got {err:?}"
    );
}

// ---------------------------------------------------------------- ASTC

/// ASTC: the u24 dimensions are capped at `astc::MAX_TEXTURE_SIDE` before the
/// block arithmetic (`crates/wc3-render/src/astc.rs:61`); 16M texels per side
/// must not become a wrapped or impossible payload length.
#[test]
fn astc_rejects_dimensions_beyond_the_texture_side_cap() {
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&0x5CA1AB13u32.to_le_bytes());
    header.extend_from_slice(&[6, 6, 1]); // block 6x6x1
    header.extend_from_slice(&[0xFF; 3]); // size_x = 16_777_215
    header.extend_from_slice(&[0xFF; 3]); // size_y = 16_777_215
    header.extend_from_slice(&[1, 0, 0]); // size_z = 1
    assert_eq!(header.len(), 16);

    let err = wc3_render::astc::decode(&header).expect_err("16M-texel side is capped");
    assert!(err.contains("side limit"), "unexpected error: {err}");

    // A block-aligned level at the cap is still legal: the cap rejects
    // impossible sizes, not large retail textures.
    let legal = wc3_render::astc::payload_len((6, 6, 1), (16_384, 16_384, 1))
        .expect("16384 is the documented maximum side");
    assert_eq!(legal, 2731 * 2731 * 16);
}

// ---------------------------------------------------------------- save

/// Save: the declared payload length must match the file byte for byte
/// (`crates/wc3-sim/src/save.rs:5334`) before the payload is sliced or any
/// section allocates. 4 GiB declared in a 12-byte file is a typed error.
#[test]
fn save_rejects_a_declared_payload_above_the_file_size() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"W3SV");
    bytes.extend_from_slice(&1u16.to_le_bytes()); // oldest accepted version
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // declared payload
    assert_eq!(bytes.len(), 12);

    let err = match wc3_sim::Sim::from_save(&bytes) {
        Ok(_) => panic!("a 4 GiB declared payload in a 12-byte file must be rejected"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            wc3_sim::save::SaveError::LengthMismatch { declared, actual }
                if declared == u32::MAX as usize && actual == 0
        ),
        "expected LengthMismatch before allocating, got {err:?}"
    );

    // A real save still round-trips: the envelope rules above must not reject
    // legitimate files.
    let sim = wc3_sim::Sim::new(7, 16, 16);
    let saved = sim.to_save_bytes();
    let loaded = wc3_sim::Sim::from_save(&saved).expect("own save round-trips");
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

/// Save: the file size itself is the first allocation a hostile path can drive
/// (`from_save_path` used a bare `std::fs::read`). Both the in-memory envelope
/// and the file loader must refuse above [`wc3_sim::save::MAX_SAVE_BYTES`]
/// before reading/parsing.
#[test]
fn save_rejects_files_above_the_loader_byte_cap() {
    let limit = wc3_sim::save::MAX_SAVE_BYTES;

    // In-memory: a caller that read the file itself still cannot bypass the cap.
    let mut oversized = vec![0u8; (limit + 1) as usize];
    oversized[0..4].copy_from_slice(b"W3SV");
    oversized[4..6].copy_from_slice(&1u16.to_le_bytes());
    let err = match wc3_sim::Sim::from_save(&oversized) {
        Ok(_) => panic!("an in-memory save above the cap must be rejected"),
        Err(err) => err,
    };
    assert!(
        matches!(err, wc3_sim::save::SaveError::Oversize { len, limit: l } if len == limit + 1 && l == limit),
        "expected Oversize, got {err:?}"
    );

    // File: a sparse file of the same length is refused from metadata alone, so
    // the loader never allocates the buffer.
    let path = std::env::temp_dir().join("wc3-input-limits-save-oversize.w3sv");
    let file = std::fs::File::create(&path).expect("create sparse save");
    file.set_len(limit + 1).expect("size the sparse save");
    drop(file);
    let err = match wc3_sim::Sim::from_save_path(&path) {
        Ok(_) => panic!("a save file above the cap must be rejected"),
        Err(err) => err,
    };
    assert!(
        matches!(err, wc3_sim::save::SaveError::Oversize { len, .. } if len == limit + 1),
        "expected Oversize from the file path, got {err:?}"
    );
    std::fs::remove_file(&path).ok();
}

// ---------------------------------------------------------------- net

/// Network/replay: the replay envelope's declared payload is compared with the
/// file size before the slice is taken (`crates/wc3-sim/src/net.rs:1452`), and
/// the socket envelope caps a message at 64 MiB
/// (`crates/wc3-sim/src/net.rs:1974`).
#[test]
fn replay_rejects_a_declared_payload_above_the_file_size() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&wc3_sim::net::REPLAY_MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes()); // version
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(bytes.len(), 12);

    let err = wc3_sim::net::Replay::decode(&bytes).expect_err("declared length above the file");
    assert!(
        matches!(
            err,
            wc3_sim::net::NetError::LengthMismatch { declared, actual }
                if declared == u32::MAX as usize && actual == 0
        ),
        "expected LengthMismatch before allocating, got {err:?}"
    );

    // An empty-but-valid replay decodes: the envelope rules do not reject
    // legitimate files.
    let setup = wc3_sim::net::MatchSetup::empty(7, 16, 16);
    let replay = wc3_sim::net::Replay::record(&setup, 0, 0, 0, Vec::new());
    let decoded = wc3_sim::net::Replay::decode(&replay.encode()).expect("own replay decodes");
    assert_eq!(decoded.setup, setup);
    assert!(decoded.frames.is_empty());
}

// ---------------------------------------------------------------- retail

/// Retail regression for AUD-025: the caps must not reject shipped Warcraft III
/// archives or change what the reader returns. Skips (with a note) on a machine
/// without the fixture, exactly like `wc3-mpq`'s
/// `largest_shipped_entry_is_below_the_cap`.
#[test]
fn retail_archives_still_decode_within_the_caps() {
    let candidates = [
        PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq"),
        PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3x.mpq"),
        PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3xlocal.mpq"),
    ];
    let mut checked = 0usize;
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let archive = wc3_mpq::MpqArchive::open(&path).expect("open shipped archive");
        assert!(
            archive.block_count() > 0,
            "{} has no blocks",
            path.display()
        );
        for index in 0..archive.block_count() as u32 {
            let Some((_, _, size, _flags)) = archive.block_entry(index) else {
                continue;
            };
            assert!(
                size as u64 <= wc3_mpq::MAX_FILE_SIZE,
                "{} declares a {size}-byte entry above the cap",
                path.display()
            );
        }
        // Decode one real entry through the capped read path; `(listfile)` is
        // present in every shipped archive and is byte-stable.
        let listfile = archive
            .read("(listfile)")
            .unwrap_or_else(|err| panic!("{}: read (listfile): {err}", path.display()));
        assert!(
            !listfile.is_empty(),
            "{}: (listfile) decoded to zero bytes",
            path.display()
        );
        checked += 1;
    }
    if checked == 0 {
        eprintln!("skipping: no shipped archive fixture available");
    }
}
