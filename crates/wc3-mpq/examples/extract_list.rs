//! Extract every file from an MPQ listfile matching selected extensions.
//! usage: extract_list <archive> <listfile.txt> <outdir> [ext,ext,...]
use std::env;
use std::path::Path;
use wc3_mpq::MpqArchive;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: extract_list <archive> <listfile> <outdir> [ext,...]");
        return;
    }
    let exts: Vec<String> = args
        .get(4)
        .map(|s| s.split(',').map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase()).collect())
        .unwrap_or_default();
    let mut archive = MpqArchive::open(&args[1]).expect("open archive");
    let list = std::fs::read_to_string(&args[2]).expect("listfile");
    let outdir = Path::new(&args[3]);
    std::fs::create_dir_all(outdir).ok();
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut total_bytes = 0u64;
    for line in list.lines() {
        let name = line.trim().trim_end_matches('\r');
        if name.is_empty() {
            continue;
        }
        if !exts.is_empty() {
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if !exts.iter().any(|e| e == &ext) {
                continue;
            }
        }
        let seen = ok + failed;
        if seen > 0 && seen % 500 == 0 {
            println!("PROGRESS seen={seen} ok={ok} failed={failed} bytes={total_bytes}");
        }
        match archive.read(name) {
            Ok(bytes) => {
                let target = outdir.join(name.replace('\\', "/"));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                if std::fs::write(&target, &bytes).is_ok() {
                    ok += 1;
                    total_bytes += bytes.len() as u64;
                } else {
                    failed += 1;
                    eprintln!("WRITE_FAIL {name}");
                }
            }
            Err(error) => {
                failed += 1;
                eprintln!("READ_FAIL {name}: {error}");
            }
        }
    }
    println!("EXTRACTED ok={ok} failed={failed} bytes={total_bytes} out={}", outdir.display());
}
