fn main() {
    let path = std::env::args().nth(1).expect("mpq path");
    let archive = wc3_mpq::MpqArchive::open(&path).expect("open mpq");
    let mut total = 0usize;
    for name in archive.listfile() {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".w3m") || lower.ends_with(".w3x") {
            println!("{name}");
            total += 1;
        }
    }
    eprintln!("map_entries={total}");
}
