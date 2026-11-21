//! Decode every file listed in an archive and tally the codecs actually used.
//!
//! Evidence tool for the five-codec whitelist: if a shipped archive needs a
//! codec outside `0x01/0x02/0x08/0x40/0x80`, the read fails with
//! `UnsupportedCompression` and the run exits non-zero. Prints the observed
//! codec histogram (per file, from the decoded sectors), byte totals and every
//! failure.
//!
//! Usage: `cargo run -p wc3-mpq --example codec_census -- [ARCHIVE ...]`

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use wc3_mpq::MpqArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut archives: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if archives.is_empty() {
        archives.push(PathBuf::from(
            std::env::var_os("WC3_MPQ").unwrap_or_else(|| {
                "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()
            }),
        ));
    }
    let mut total_failures = 0usize;
    for path in archives {
        // `open_any` covers the install's bare `.mpq` archives and the shipped
        // `.w3m` campaign maps (`HM3W` header, archive at the first `MPQ\x1a`
        // in the first 4 KiB) in one call.
        let mut archive = MpqArchive::open_any(&path)?;
        if archive.listfile().is_empty() {
            archive.discover_known_names();
        }
        let names: Vec<String> = archive.listfile().to_vec();
        let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
        let mut bytes = 0u64;
        let mut failures = 0usize;
        for name in &names {
            let mut reader = match archive.stream(name) {
                Ok(reader) => reader,
                Err(err) => {
                    failures += 1;
                    if failures <= 20 {
                        println!("FAIL {name}: {err}");
                    }
                    continue;
                }
            };
            let mut buf = vec![0u8; 64 * 1024];
            let mut file_bytes = 0u64;
            let mut failed = None;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => file_bytes += n as u64,
                    Err(err) => {
                        failed = Some(err);
                        break;
                    }
                }
            }
            if let Some(err) = failed {
                failures += 1;
                if failures <= 20 {
                    println!("FAIL {name}: {err}");
                }
                continue;
            }
            bytes += file_bytes;
            *histogram.entry(reader.codecs().describe()).or_default() += 1;
        }
        total_failures += failures;
        println!(
            "{}: files={} failures={} decoded_bytes={bytes}",
            path.display(),
            names.len(),
            failures
        );
        for (codecs, count) in &histogram {
            println!("  {codecs}: {count}");
        }
    }
    if total_failures > 0 {
        return Err(format!("{total_failures} files failed to decode").into());
    }
    Ok(())
}
