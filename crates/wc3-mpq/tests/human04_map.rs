//! Regression gate for encrypted multi-sector sector padding.
//!
//! Human04's `war3map.wts` declares a zlib codec mask but stores PKWARE-tagged
//! sectors whose compressed physical length is not dword-aligned
//! (`0x9a5` bytes). Before the StormLib-style rounding fix the final word was
//! never decrypted and the PKWARE stream reported EOF. This test proves the
//! single `wc3-mpq` reader now returns the exact payload that `wc3-re::mpqex`
//! used to obtain through its separate correcting reader.

use std::path::PathBuf;
use wc3_mpq::MpqArchive;

fn map_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_full3/Maps/Campaign/Human04.w3m")
}

fn mpq_base(bytes: &[u8]) -> u64 {
    if bytes.starts_with(b"MPQ\x1a") {
        return 0;
    }
    bytes
        .windows(4)
        .position(|window| window == b"MPQ\x1a")
        .expect("MPQ header in HM3W container") as u64
}

#[test]
fn reads_encrypted_pkware_wts_and_script() {
    let path = map_path();
    if !path.is_file() {
        eprintln!("skip: Human04.w3m missing");
        return;
    }
    let head = std::fs::read(&path).unwrap();
    let archive = MpqArchive::open_at(&path, mpq_base(&head)).expect("open map");
    let wts = archive.read("war3map.wts").expect("read wts");
    let text = String::from_utf8_lossy(&wts);
    assert!(text.contains("STRING 0"), "wts head: {:?}", &text[..80.min(text.len())]);
    assert!(text.contains("Андорал"), "localized string present");
    let script = archive.read("war3map.j").expect("read script");
    assert_eq!(script.len(), 221_222);
}
