//! Cover-to-cover extraction audit: every MPQ entry against the extracted tree.
//!
//! For each listfile entry this re-reads the payload from the archive (through
//! the streaming reader), reads the extracted file from disk, and compares
//! size + SHA-256 byte for byte. It also records the codec tags actually used
//! per file, zero-length/zero-content files, header signatures and per-extension
//! totals, and writes:
//!
//! * `extraction-audit.json`   — machine summary (counts, codecs, failures)
//! * `extraction-audit.md`     — human report
//! * `extraction-audit-files.tsv` — one row per file (provenance)
//! * `war3.index`              — binary archive index for the fast path
//!
//! Usage: `cargo run -p wc3-mpq --example audit_extract -- [--mpq PATH]
//!         [--out DIR] [--index PATH] [--report DIR] [--limit N]`

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sha2::{Digest, Sha256};
use wc3_mpq::{CodecUsage, MpqArchive, MpqIndex};

const DEFAULT_MPQ: &str = "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq";
const DEFAULT_OUT: &str = "analysis/war3_full3";
const DEFAULT_REPORT: &str = "analysis/agent-assets";

#[derive(Default)]
struct Stats {
    entries: usize,
    ok: usize,
    missing_on_disk: usize,
    size_mismatch: usize,
    hash_mismatch: usize,
    read_fail: usize,
    disk_read_fail: usize,
    zero_length: usize,
    zero_content: usize,
    empty_files: usize,
    mpq_bytes: u64,
    disk_bytes: u64,
    stream_bytes: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq_path = PathBuf::from(std::env::var_os("WC3_MPQ").unwrap_or_else(|| DEFAULT_MPQ.into()));
    let mut out_dir = PathBuf::from(DEFAULT_OUT);
    let mut report_dir = PathBuf::from(DEFAULT_REPORT);
    let mut index_path = PathBuf::from("analysis/agent-assets/war3.index");
    let mut limit = usize::MAX;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq_path = PathBuf::from(&args[i]);
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            "--report" => {
                i += 1;
                report_dir = PathBuf::from(&args[i]);
            }
            "--index" => {
                i += 1;
                index_path = PathBuf::from(&args[i]);
            }
            "--limit" => {
                i += 1;
                limit = args[i].parse()?;
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }

    let open_start = Instant::now();
    let archive = MpqArchive::open(&mpq_path)?;
    let open_ms = open_start.elapsed().as_secs_f64() * 1000.0;

    let index_start = Instant::now();
    let index = MpqIndex::build(&archive);
    let index_ms = index_start.elapsed().as_secs_f64() * 1000.0;
    let index_bytes = index.save(&index_path)?;

    println!(
        "archive={} size={} blocks={} listfile={} open_ms={open_ms:.2} index_ms={index_ms:.2} index_saved={}B",
        mpq_path.display(),
        archive.fingerprint().archive_size,
        archive.block_count(),
        index.len(),
        index_bytes
    );
    println!("fingerprint={:?}", archive.fingerprint());

    let mut stats = Stats::default();
    let mut codec_hist: BTreeMap<u16, usize> = BTreeMap::new();
    let mut ext_hist: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut failures: Vec<serde_like::Failure> = Vec::new();
    let mut rows = String::with_capacity(index.len() * 96);
    rows.push_str("name\tsize\tcodec\tsha256\tstatus\n");

    let read_start = Instant::now();
    for (done, entry) in index.entries().iter().enumerate() {
        if done >= limit {
            break;
        }
        stats.entries += 1;
        let name = &entry.name;
        let mut mpq_bytes = Vec::new();
        let mut reader = match archive.stream(name) {
            Ok(reader) => reader,
            Err(err) => {
                stats.read_fail += 1;
                failures.push(serde_like::Failure {
                    name: name.clone(),
                    kind: "open".into(),
                    detail: err.to_string(),
                });
                rows.push_str(&format!("{name}\t0\tnone\t-\tOPEN_FAIL\n"));
                continue;
            }
        };
        let expected_len = reader.len() as usize;
        if let Err(err) = reader.read_to_end(&mut mpq_bytes) {
            stats.read_fail += 1;
            failures.push(serde_like::Failure {
                name: name.clone(),
                kind: "read".into(),
                detail: err.to_string(),
            });
            rows.push_str(&format!("{name}\t0\tnone\t-\tREAD_FAIL\n"));
            continue;
        }
        let codecs = reader.codecs();
        *codec_hist.entry(codecs.0).or_default() += 1;
        stats.stream_bytes += mpq_bytes.len() as u64;
        stats.mpq_bytes += mpq_bytes.len() as u64;
        let digest = sha256(&mpq_bytes);
        if codecs.is_empty() {
            stats.empty_files += 1;
        }
        if mpq_bytes.is_empty() {
            stats.zero_length += 1;
        }
        if !mpq_bytes.is_empty() && mpq_bytes.iter().all(|b| *b == 0) {
            stats.zero_content += 1;
        }

        let ext = extension_of(name);
        let slot = ext_hist.entry(ext).or_insert((0, 0));
        slot.0 += 1;
        slot.1 += mpq_bytes.len() as u64;

        let disk_path = out_dir.join(name.replace('\\', "/"));
        let disk = std::fs::read(&disk_path);
        let status = match disk {
            Err(err) => {
                stats.missing_on_disk += 1;
                failures.push(serde_like::Failure {
                    name: name.clone(),
                    kind: "missing".into(),
                    detail: format!("{}: {err}", disk_path.display()),
                });
                "MISSING"
            }
            Ok(disk_bytes) => {
                stats.disk_bytes += disk_bytes.len() as u64;
                if disk_bytes.len() != expected_len || disk_bytes.len() != mpq_bytes.len() {
                    stats.size_mismatch += 1;
                    failures.push(serde_like::Failure {
                        name: name.clone(),
                        kind: "size".into(),
                        detail: format!(
                            "mpq={} stream={} disk={}",
                            entry.size,
                            mpq_bytes.len(),
                            disk_bytes.len()
                        ),
                    });
                    "SIZE_MISMATCH"
                } else if sha256(&disk_bytes) != digest {
                    stats.hash_mismatch += 1;
                    failures.push(serde_like::Failure {
                        name: name.clone(),
                        kind: "sha256".into(),
                        detail: format!("mpq={} disk differs", expected_len),
                    });
                    "SHA_MISMATCH"
                } else if mpq_bytes.is_empty() && entry.size != 0 {
                    stats.size_mismatch += 1;
                    failures.push(serde_like::Failure {
                        name: name.clone(),
                        kind: "empty".into(),
                        detail: format!("declared {} bytes, decoded 0", entry.size),
                    });
                    "EMPTY"
                } else {
                    stats.ok += 1;
                    rows.push_str(&format!(
                        "{name}\t{}\t{}\t{}\tOK\n",
                        mpq_bytes.len(),
                        codecs.describe(),
                        hex8(&digest)
                    ));
                    continue;
                }
            }
        };
        rows.push_str(&format!(
            "{name}\t{}\t{}\t{}\t{status}\n",
            mpq_bytes.len(),
            codecs.describe(),
            hex8(&digest)
        ));
    }
    let read_secs = read_start.elapsed().as_secs_f64();

    // Files on disk that the listfile does not mention (hidden assets).
    let disk_files = count_files(&out_dir);
    let extra = disk_files.saturating_sub(stats.entries);

    std::fs::create_dir_all(&report_dir)?;
    std::fs::write(report_dir.join("extraction-audit-files.tsv"), &rows)?;

    let codec_rows: Vec<(String, usize)> = codec_hist
        .iter()
        .map(|(bits, count)| (CodecUsage(*bits).describe(), *count))
        .collect();
    let ext_rows: Vec<(String, usize, u64)> = ext_hist
        .iter()
        .map(|(ext, (count, bytes))| (ext.clone(), *count, *bytes))
        .collect();

    let json = render_json(
        &mpq_path,
        &out_dir,
        &index_path,
        index_bytes,
        open_ms,
        index_ms,
        read_secs,
        &stats,
        &codec_rows,
        &ext_rows,
        extra,
        &failures,
    );
    std::fs::write(report_dir.join("extraction-audit.json"), &json)?;
    let md = render_markdown(
        &mpq_path,
        &out_dir,
        open_ms,
        index_ms,
        read_secs,
        &stats,
        &codec_rows,
        &ext_rows,
        extra,
        &failures,
    );
    std::fs::write(report_dir.join("extraction-audit.md"), &md)?;
    println!("{md}");
    println!("report_dir={}", report_dir.display());
    println!(
        "AUDIT ok={} entries={} read_fail={} missing={} size_mismatch={} sha_mismatch={}",
        stats.ok,
        stats.entries,
        stats.read_fail,
        stats.missing_on_disk,
        stats.size_mismatch,
        stats.hash_mismatch
    );
    if !failures.is_empty() {
        return Err(format!("{} audit failures", failures.len()).into());
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn hex8(digest: &[u8; 32]) -> String {
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "(none)".to_string())
}

fn count_files(root: &Path) -> usize {
    let mut count = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => count += 1,
                _ => {}
            }
        }
    }
    count
}

#[allow(clippy::too_many_arguments)]
fn render_json(
    mpq: &Path,
    out: &Path,
    index_path: &Path,
    index_bytes: usize,
    open_ms: f64,
    index_ms: f64,
    read_secs: f64,
    stats: &Stats,
    codecs: &[(String, usize)],
    exts: &[(String, usize, u64)],
    extra_on_disk: usize,
    failures: &[serde_like::Failure],
) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"mpq\": \"{}\",\n", escape(&mpq.display().to_string())));
    s.push_str(&format!("  \"extracted_dir\": \"{}\",\n", escape(&out.display().to_string())));
    s.push_str(&format!("  \"index\": \"{}\",\n", escape(&index_path.display().to_string())));
    s.push_str(&format!("  \"index_bytes\": {index_bytes},\n"));
    s.push_str(&format!("  \"open_ms\": {open_ms:.3},\n"));
    s.push_str(&format!("  \"index_build_ms\": {index_ms:.3},\n"));
    s.push_str(&format!("  \"audit_seconds\": {read_secs:.3},\n"));
    s.push_str(&format!("  \"entries\": {},\n", stats.entries));
    s.push_str(&format!("  \"ok\": {},\n", stats.ok));
    s.push_str(&format!("  \"read_fail\": {},\n", stats.read_fail));
    s.push_str(&format!("  \"missing_on_disk\": {},\n", stats.missing_on_disk));
    s.push_str(&format!("  \"size_mismatch\": {},\n", stats.size_mismatch));
    s.push_str(&format!("  \"hash_mismatch\": {},\n", stats.hash_mismatch));
    s.push_str(&format!("  \"disk_read_fail\": {},\n", stats.disk_read_fail));
    s.push_str(&format!("  \"zero_length\": {},\n", stats.zero_length));
    s.push_str(&format!("  \"zero_content\": {},\n", stats.zero_content));
    s.push_str(&format!("  \"empty_codec\": {},\n", stats.empty_files));
    s.push_str(&format!("  \"mpq_bytes\": {},\n", stats.mpq_bytes));
    s.push_str(&format!("  \"disk_bytes\": {},\n", stats.disk_bytes));
    s.push_str(&format!("  \"files_on_disk\": {},\n", stats.entries + extra_on_disk));
    s.push_str(&format!("  \"extra_on_disk\": {},\n", extra_on_disk));
    s.push_str("  \"codecs\": [");
    for (n, (name, count)) in codecs.iter().enumerate() {
        if n > 0 {
            s.push(',');
        }
        s.push_str(&format!("{{\"codec\":\"{}\",\"files\":{}}}", escape(name), count));
    }
    s.push_str("],\n  \"extensions\": [");
    for (n, (ext, count, bytes)) in exts.iter().enumerate() {
        if n > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"ext\":\"{}\",\"files\":{},\"bytes\":{}}}",
            escape(ext),
            count,
            bytes
        ));
    }
    s.push_str("],\n  \"failures\": [");
    for (n, failure) in failures.iter().enumerate() {
        if n > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"detail\":\"{}\"}}",
            escape(&failure.name),
            escape(&failure.kind),
            escape(&failure.detail)
        ));
    }
    s.push_str("]\n}\n");
    s
}

#[allow(clippy::too_many_arguments)]
fn render_markdown(
    mpq: &Path,
    out: &Path,
    open_ms: f64,
    index_ms: f64,
    read_secs: f64,
    stats: &Stats,
    codecs: &[(String, usize)],
    exts: &[(String, usize, u64)],
    extra: usize,
    failures: &[serde_like::Failure],
) -> String {
    let mut s = String::new();
    s.push_str("# Extraction audit\n\n");
    s.push_str(&format!("- archive: `{}`\n", mpq.display()));
    s.push_str(&format!("- extracted: `{}`\n", out.display()));
    s.push_str(&format!("- entries: **{}**\n", stats.entries));
    s.push_str(&format!(
        "- verdict: **{} ok** / {} read_fail / {} missing / {} size_mismatch / {} sha_mismatch\n",
        stats.ok, stats.read_fail, stats.missing_on_disk, stats.size_mismatch, stats.hash_mismatch
    ));
    s.push_str(&format!(
        "- bytes: mpq={} disk={} ({} zero-length, {} zero-content)\n",
        stats.mpq_bytes, stats.disk_bytes, stats.zero_length, stats.zero_content
    ));
    s.push_str(&format!(
        "- timing: open={open_ms:.1} ms, index={index_ms:.1} ms, full re-read+sha256={read_secs:.2} s\n",
    ));
    s.push_str(&format!("- files on disk not in listfile: {extra}\n\n"));
    s.push_str("## Codecs actually used\n\n| codec | files |\n|---|---:|\n");
    for (name, count) in codecs {
        s.push_str(&format!("| {name} | {count} |\n"));
    }
    s.push_str("\n## Extensions\n\n| ext | files | bytes |\n|---|---:|---:|\n");
    for (ext, count, bytes) in exts {
        s.push_str(&format!("| {ext} | {count} | {bytes} |\n"));
    }
    if failures.is_empty() {
        s.push_str("\nNo failures.\n");
    } else {
        s.push_str(&format!("\n## Failures ({})\n\n", failures.len()));
        for failure in failures.iter().take(100) {
            s.push_str(&format!(
                "- `{}` {}: {}\n",
                failure.name, failure.kind, failure.detail
            ));
        }
        if failures.len() > 100 {
            s.push_str(&format!("- ... {} more in the JSON\n", failures.len() - 100));
        }
    }
    s
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

mod serde_like {
    #[derive(Clone)]
    pub struct Failure {
        pub name: String,
        pub kind: String,
        pub detail: String,
    }
}
