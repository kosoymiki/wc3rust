//! Dump the decoded YUV planes of one frame (yuv420p layout).
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example dump_yuv -- <movie> <frame> <out.yuv>
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: dump_yuv <movie> <frame> <out.yuv>");
        std::process::exit(2);
    }
    let mut file = wc3_bink::BinkFile::open(&args[0]).expect("open movie");
    let index: usize = args[1].parse().expect("frame");
    file.decode_frame(index).expect("decode");
    let pic = file.picture();
    let (w, h) = (pic.width, pic.height);
    let mut out = Vec::with_capacity(w * h * 3 / 2);
    for y in 0..h {
        out.extend_from_slice(&pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * pic.stride_y..y * pic.stride_y + w]);
    }
    for plane in [wc3_bink::mpeg4::frame::Plane::U, wc3_bink::mpeg4::frame::Plane::V] {
        let p = pic.plane(plane);
        for y in 0..h / 2 {
            out.extend_from_slice(&p[y * pic.stride_c..y * pic.stride_c + w / 2]);
        }
    }
    std::fs::write(&args[2], &out).expect("write");
    println!("wrote {} bytes", out.len());
}
