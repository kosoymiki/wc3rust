//! Inventory the nine Warcraft III cinematic files from the installer tree.
//!
//! `cargo run -p wc3-formats --example movie_inventory -- [DIR...]`
//!
//! Default roots are the operator's extracted ISO tree; `HEROFOLENDS_MOVIES`
//! (colon-separated) is honoured first. The table lists name, presence, bytes
//! and path, plus `MOVIE_MISSING <name>` lines for absent titles — the exact
//! contract the runtime resolver uses.

use std::path::PathBuf;

use wc3_formats::media::{KNOWN_MOVIES, scan_movie_dirs};

const DEFAULT_ROOTS: [&str; 2] = [
    "/storage/emulated/0/Download/wr3/iso/movies/Movies",
    "/storage/emulated/0/Download/wr3/iso/trailers/trailers",
];

fn main() {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(list) = std::env::var_os("HEROFOLENDS_MOVIES") {
        roots.extend(std::env::split_paths(&list));
    }
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        roots.extend(DEFAULT_ROOTS.iter().map(PathBuf::from));
    } else {
        roots.extend(args);
    }
    for root in &roots {
        println!("ROOT {} exists={}", root.display(), root.is_dir());
    }
    let (found, missing, unreadable) = scan_movie_dirs(&roots);
    let mut total_bytes = 0u64;
    for name in KNOWN_MOVIES {
        match found.iter().find(|movie| movie.name.eq_ignore_ascii_case(name)) {
            Some(movie) => {
                total_bytes += movie.bytes;
                println!("MOVIE {name} bytes={} path={}", movie.bytes, movie.path.display());
            }
            None => println!("MOVIE_MISSING {name}"),
        }
    }
    for error in &unreadable {
        println!("UNREADABLE {error}");
    }
    println!(
        "SUMMARY present={} missing={} bytes={total_bytes} roots={}",
        found.len(),
        missing.len(),
        roots.len()
    );
}
