//! Cross-check the MPQ reader against an independent implementation.
//!
//! Walks a tree produced by StormLib (`tools/storm_probe --extract "" DIR`) and
//! compares every file byte-for-byte (SHA-256) with what this crate reads from
//! the archive. This is the external oracle for the extraction audit: without
//! it, a reader bug would validate against its own output.
//!
//! `cargo run -p wc3-mpq --example compare_oracle -- --mpq PATH --oracle DIR
//!  [--report FILE]`

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wc3_mpq::MpqArchive;

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = PathBuf::from(
        std::env::var_os("WC3_MPQ")
            .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()),
    );
    let mut oracle: Option<PathBuf> = None;
    let mut report_path: Option<PathBuf> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = PathBuf::from(&args[i]);
            }
            "--oracle" => {
                i += 1;
                oracle = Some(PathBuf::from(&args[i]));
            }
            "--report" => {
                i += 1;
                report_path = Some(PathBuf::from(&args[i]));
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }
    let oracle = oracle.ok_or("--oracle DIR is required")?;
    let archive = MpqArchive::open(&mpq)?;
    println!(
        "archive={} blocks={} listfile={}",
        mpq.display(),
        archive.block_count(),
        archive.listfile().len()
    );

    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![oracle.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => files.push(path),
                _ => {}
            }
        }
    }
    files.sort();
    println!("oracle files={}", files.len());

    let mut matched = 0usize;
    let mut mismatched = 0usize;
    let mut read_fail = 0usize;
    let mut bytes = 0u64;
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut by_ext: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for path in &files {
        let relative = path
            .strip_prefix(&oracle)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('/', "\\");
        let disk = std::fs::read(path)?;
        bytes += disk.len() as u64;
        let ext = relative
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_else(|| "(none)".into());
        let slot = by_ext.entry(ext).or_default();
        slot.0 += 1;
        match archive.read(&relative) {
            Ok(bytes) => {
                if sha256(&bytes) == sha256(&disk) {
                    matched += 1;
                } else {
                    mismatched += 1;
                    slot.1 += 1;
                    failures.push((
                        relative.clone(),
                        format!("sha256 differs (ours {} bytes)", bytes.len()),
                    ));
                }
            }
            Err(err) => {
                read_fail += 1;
                slot.1 += 1;
                failures.push((relative.clone(), err.to_string()));
            }
        }
    }

    // Archive entries the oracle tree does not contain (listfile vs StormLib
    // enumeration mismatch, e.g. hidden files).
    let mut extra = 0usize;
    for name in archive.listfile() {
        let disk_path = oracle.join(name.replace('\\', "/"));
        if !disk_path.is_file() {
            extra += 1;
        }
    }

    println!(
        "RESULT files={} matched={} mismatched={} read_fail={} extra_in_archive={} bytes={}",
        files.len(),
        matched,
        mismatched,
        read_fail,
        extra,
        bytes
    );
    for (ext, (count, bad)) in &by_ext {
        println!("  ext {ext}: files={count} mismatches={bad}");
    }
    for (name, detail) in failures.iter().take(40) {
        println!("  MISMATCH {name}: {detail}");
    }

    if let Some(path) = report_path {
        let mut json = String::new();
        json.push_str("{\n");
        json.push_str(&format!("  \"mpq\": \"{}\",\n", mpq.display()));
        json.push_str(&format!("  \"oracle\": \"{}\",\n", oracle.display()));
        json.push_str(&format!("  \"files\": {},\n", files.len()));
        json.push_str(&format!("  \"bytes\": {},\n", bytes));
        json.push_str(&format!("  \"matched\": {},\n", matched));
        json.push_str(&format!("  \"mismatched\": {},\n", mismatched));
        json.push_str(&format!("  \"read_fail\": {},\n", read_fail));
        json.push_str(&format!("  \"extra_in_archive\": {},\n", extra));
        json.push_str("  \"failures\": [\n");
        for (n, (name, detail)) in failures.iter().enumerate() {
            let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            json.push_str(&format!(
                "    {{\"name\": \"{}\", \"detail\": \"{}\"}}{}\n",
                escape(name),
                escape(detail),
                if n + 1 == failures.len() { "" } else { "," }
            ));
        }
        json.push_str("  ]\n}\n");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&path)?;
        file.write_all(json.as_bytes())?;
        println!("report={}", path.display());
    }

    if mismatched + read_fail > 0 {
        return Err(format!("{mismatched} mismatches, {read_fail} read failures").into());
    }
    Ok(())
}
