//! Decode BLP1/BLP2 to tightly packed RGBA8 without a third-party image crate.
//!
//! Usage: cargo run -p wc3-formats --example blp_dump_rgba -- input.blp output.rgba
use std::path::PathBuf;
use wc3_formats::blp;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().expect("input BLP path"));
    let output = PathBuf::from(args.next().expect("output raw RGBA path"));
    if args.next().is_some() {
        panic!("usage: blp_dump_rgba <input.blp> <output.rgba>");
    }
    let bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("read {}: {e}", input.display()));
    let image = blp::decode(&bytes).unwrap_or_else(|e| panic!("decode {}: {e}", input.display()));
    let expected = image.width as usize * image.height as usize * 4;
    if image.rgba.len() != expected {
        panic!("decoder returned {} bytes; expected {expected}", image.rgba.len());
    }
    std::fs::write(&output, &image.rgba)
        .unwrap_or_else(|e| panic!("write {}: {e}", output.display()));
    println!("BLP_RGBA_OK {}x{} bytes={}", image.width, image.height, expected);
}
