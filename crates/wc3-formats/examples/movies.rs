//! Movie discovery/extraction CLI over the shipped archives.
//!
//! `cargo run -p wc3-formats --example movies -- [--extract DIR] ARCHIVE...`
//!
//! Prints one row per movie: `archive|name|bytes|source|magic`. With
//! `--extract DIR` every movie is also written to `DIR/<name>.bik` and the row
//! gains `|written=<bytes>` or `|error=<reason>`.

use std::path::PathBuf;

use wc3_formats::media::{discover_movies, extract_movie};
use wc3_mpq::MpqArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut extract: Option<PathBuf> = None;
    let mut verbose = false;
    let mut archives: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--extract" => {
                extract = Some(PathBuf::from(args.next().expect("--extract needs a directory")));
            }
            "--verbose" => verbose = true,
            "-h" | "--help" => {
                eprintln!("usage: movies [--extract DIR] [--verbose] ARCHIVE...");
                return Ok(());
            }
            other => archives.push(other.to_string()),
        }
    }
    if archives.is_empty() {
        return Err("usage: movies [--extract DIR] ARCHIVE...".into());
    }
    let mut total_files = 0usize;
    let mut total_bytes = 0u64;
    let mut written_bytes = 0u64;
    for path in &archives {
        let mut archive = MpqArchive::open(path)?;
        if verbose {
            // Diagnostic: every name that mentions "movie" and every unnamed
            // block, so an install that keeps the CGI in an unexpected path or
            // under a name the listfile lacks is still visible.
            for (name, meta) in archive.files() {
                if name.to_ascii_lowercase().contains("movie") {
                    println!(
                        "NAME {path}|{name}|{}|block={}",
                        meta.size, meta.block_index
                    );
                }
            }
            let named: std::collections::BTreeSet<u32> =
                archive.files().map(|(_, meta)| meta.block_index).collect();
            for block in 0..archive.block_count() as u32 {
                if named.contains(&block) {
                    continue;
                }
                let Some((_off, _csize, size, _flags)) = archive.block_entry(block) else {
                    continue;
                };
                let magic = archive
                    .read_block(block)
                    .map(|bytes| {
                        let mut head = [0u8; 4];
                        head.copy_from_slice(&bytes[..4.min(bytes.len())]);
                        let mut repr = String::new();
                        for b in head {
                            if b.is_ascii_graphic() {
                                repr.push(b as char);
                            } else {
                                repr.push('.');
                            }
                        }
                        repr
                    })
                    .unwrap_or_else(|err| format!("err:{err}"));
                println!("UNNAMED {path}|block={block}|size={size}|magic={magic}");
            }
        }
        let movies = discover_movies(&mut archive);
        println!("ARCHIVE {path} movies={}", movies.len());
        for entry in &movies {
            total_files += 1;
            total_bytes += entry.size as u64;
            let magic = String::from_utf8_lossy(&entry.magic).into_owned();
            print!(
                "MOVIE {path}|{}|{}|{}|{}",
                entry.name,
                entry.size,
                entry.source.as_str(),
                magic.escape_default()
            );
            if let Some(dir) = &extract {
                match extract_movie(&archive, entry, dir) {
                    Ok((out, bytes)) => {
                        written_bytes += bytes;
                        print!("|written={bytes}|path={}", out.display());
                    }
                    Err(err) => print!("|error={err}"),
                }
            }
            println!();
        }
    }
    println!(
        "SUMMARY movies={total_files} declared_bytes={total_bytes} written_bytes={written_bytes} archives={}",
        archives.len()
    );
    Ok(())
}
