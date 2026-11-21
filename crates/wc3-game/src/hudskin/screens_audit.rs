//! Completeness audit for the headless screen matrix in
//! `analysis/hudskin/screens`.
//!
//! The QA sweep found one 0-byte artifact (`portrait-1080x2340-tooltip-
//! 1080x2340.png`, 2026-11-21 12:58): the render was killed between
//! `File::create` and the write, and nothing in the pipeline rejected the
//! resulting empty file. [`soft_raster::write_png`](super::soft_raster::write_png)
//! now completes-then-renames, and this audit fails a named test if any screen
//! PNG in the matrix is empty, is not a PNG, or disagrees with the `<w>x<h>`
//! suffix the generator embeds in its file name.
//!
//! The matrix is untracked local evidence (the pack and the frames are not in
//! git), so a checkout without `analysis/hudskin/screens` skips the artifact
//! scan instead of failing; the audit logic itself is always tested against a
//! synthetic directory.

use std::path::{Path, PathBuf};

/// One rejected artifact and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defect {
    pub file: String,
    pub reason: String,
}

/// Dimensions stated by a PNG's IHDR chunk.
fn png_size(bytes: &[u8]) -> Result<(u32, u32), &'static str> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return Err("not a PNG (signature)");
    }
    if bytes.len() < 33 {
        return Err("truncated before IHDR");
    }
    // The IHDR chunk is first: length(4) type(4) width(4) height(4).
    if &bytes[12..16] != b"IHDR" {
        return Err("first chunk is not IHDR");
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("4 bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("4 bytes"));
    if width == 0 || height == 0 {
        return Err("IHDR has a zero dimension");
    }
    if !bytes.windows(4).any(|window| window == b"IEND") {
        return Err("no IEND chunk (truncated frame)");
    }
    Ok((width, height))
}

/// `<state>-<w>x<h>.png` → `(w, h)` parsed from the trailing suffix.
fn expected_size(file_name: &str) -> Option<(u32, u32)> {
    let stem = file_name.strip_suffix(".png")?;
    let (w, h) = stem.rsplit('-').next()?.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Audit every `*.png` directly inside `dir` (subdirectories are not artifacts
/// of the matrix: `preview/` holds review sheets).
pub fn audit_dir(dir: &Path) -> (usize, Vec<Defect>) {
    let mut scanned = 0usize;
    let mut defects = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, defects);
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".png"))
        .collect();
    names.sort();
    for name in names {
        scanned += 1;
        let path = dir.join(&name);
        let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if size == 0 {
            defects.push(Defect {
                file: name,
                reason: "0 bytes".to_string(),
            });
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                defects.push(Defect {
                    file: name,
                    reason: format!("unreadable: {error}"),
                });
                continue;
            }
        };
        let actual = match png_size(&bytes) {
            Ok(size) => size,
            Err(reason) => {
                defects.push(Defect {
                    file: name,
                    reason: reason.to_string(),
                });
                continue;
            }
        };
        match expected_size(&name) {
            Some(expected) if expected == actual => {}
            Some(expected) => defects.push(Defect {
                file: name,
                reason: format!("IHDR {}x{} != name {}x{}", actual.0, actual.1, expected.0, expected.1),
            }),
            None => defects.push(Defect {
                file: name,
                reason: "name has no trailing <w>x<h>".to_string(),
            }),
        }
    }
    (scanned, defects)
}

fn screens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("analysis/hudskin/screens")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "h2-screens-audit-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn rgba(width: u32, height: u32) -> Vec<u8> {
        vec![40u8; (width * height * 4) as usize]
    }

    #[test]
    fn hudskin_screens_audit_rejects_empty_truncated_and_misnamed_frames() {
        let dir = scratch_dir("synthetic");
        let good = super::super::soft_raster::encode_png(5, 3, &rgba(5, 3));
        std::fs::write(dir.join("state-5x3.png"), &good).expect("good");
        std::fs::write(dir.join("empty-5x3.png"), b"").expect("empty");
        let wrong = super::super::soft_raster::encode_png(4, 3, &rgba(4, 3));
        std::fs::write(dir.join("wrong-5x3.png"), &wrong).expect("wrong size");
        std::fs::write(dir.join("junk-5x3.png"), b"not a png at all").expect("junk");
        std::fs::create_dir_all(dir.join("preview")).expect("preview dir");
        std::fs::write(dir.join("preview/sheet.png"), b"").expect("preview ignored");

        let (scanned, defects) = audit_dir(&dir);
        assert_eq!(scanned, 4, "preview/ is not scanned");
        let reasons: Vec<(String, String)> = defects
            .iter()
            .map(|defect| (defect.file.clone(), defect.reason.clone()))
            .collect();
        assert_eq!(reasons.len(), 3, "three rejected frames: {reasons:?}");
        assert!(reasons
            .iter()
            .any(|(file, reason)| file == "empty-5x3.png" && reason == "0 bytes"));
        assert!(reasons
            .iter()
            .any(|(file, reason)| file == "wrong-5x3.png"
                && reason == "IHDR 4x3 != name 5x3"));
        assert!(reasons
            .iter()
            .any(|(file, reason)| file == "junk-5x3.png" && reason.contains("not a PNG")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hudskin_screens_matrix_has_no_zero_byte_or_mislabelled_png() {
        let dir = screens_dir();
        if !dir.is_dir() {
            eprintln!(
                "hudskin screens matrix absent at {} (untracked headless evidence); \
                 artifact scan skipped",
                dir.display()
            );
            return;
        }
        let (scanned, defects) = audit_dir(&dir);
        assert!(
            scanned >= 10,
            "{} contains only {scanned} PNGs; the matrix should hold the generated frames",
            dir.display()
        );
        assert!(
            defects.is_empty(),
            "{} defective screen artifacts in {}: {defects:#?}",
            defects.len(),
            dir.display()
        );
    }
}
