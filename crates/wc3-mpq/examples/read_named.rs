fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("archive");
    let name = args.next().expect("inner name");
    let archive = wc3_mpq::MpqArchive::open(&path).expect("open");
    let data = archive.read(&name).expect("read");
    use std::io::Write;
    std::io::stdout().write_all(&data).expect("write");
    eprintln!("read {name}: {} bytes", data.len());
}
