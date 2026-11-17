//! BLP header + alpha histogram + PPM dump: `blp_info <file.blp> [out.ppm]`.
use wc3_formats::blp;

fn main() {
    let path = std::env::args().nth(1).expect("blp path");
    let bytes = std::fs::read(&path).expect("read blp");
    println!("magic={:?} len={}", &bytes[..4.min(bytes.len())], bytes.len());
    let image = blp::decode(&bytes).expect("decode blp");
    let mut buckets = [0u64; 8];
    for px in image.rgba.chunks_exact(4) {
        buckets[(px[3] / 32) as usize] += 1;
    }
    let n = (image.width * image.height).max(1) as u64;
    println!(
        "{}x{} alpha_hist={:?} (buckets of 32)",
        image.width, image.height, buckets
    );
    let mut min = [255u8; 4];
    let mut max = [0u8; 4];
    let mut sum = [0u64; 4];
    for px in image.rgba.chunks_exact(4) {
        for c in 0..4 {
            min[c] = min[c].min(px[c]);
            max[c] = max[c].max(px[c]);
            sum[c] += px[c] as u64;
        }
    }
    println!(
        "rgba min={:?} max={:?} avg=({},{},{},{})",
        min, max, sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n
    );
    // Raw JPEG diagnostics: WC3 BLP1 JPEGs are 4-component; print the
    // component order the `jpeg-decoder` crate returns so the BGR(A) mapping
    // can be verified against the donor implementations.
    if bytes.len() > 160 && &bytes[4..8] == &[0, 0, 0, 0] {
        let header_size = u32::from_le_bytes(bytes[156..160].try_into().unwrap()) as usize;
        let mip_off = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
        let mip_size = u32::from_le_bytes(bytes[92..96].try_into().unwrap()) as usize;
        let mut stream = Vec::new();
        stream.extend_from_slice(&bytes[160..160 + header_size]);
        stream.extend_from_slice(&bytes[mip_off..mip_off + mip_size]);
        let mut decoder = jpeg_decoder::Decoder::new(&stream[..]);
        if let Ok(pixels) = decoder.decode() {
            let info = decoder.info().unwrap();
            println!("jpeg pixel_format={:?}", info.pixel_format);
            let n = match info.pixel_format {
                jpeg_decoder::PixelFormat::CMYK32 => 4,
                jpeg_decoder::PixelFormat::RGB24 => 3,
                _ => 1,
            };
            for i in 0..6 {
                let base = i * n;
                if base + n <= pixels.len() {
                    println!("raw[{i}]={:?}", &pixels[base..base + n]);
                }
            }
            if n == 4 {
                let mut raw = [0u64; 3];
                let mut swapped = [0u64; 3];
                let mut inverted = [0u64; 3];
                let mut count = 0u64;
                let mut by_k = [0u64; 2];
                for px in pixels.chunks_exact(4) {
                    by_k[(px[3] > 128) as usize] += 1;
                    if px[3] > 128 {
                        raw[0] += px[0] as u64;
                        raw[1] += px[1] as u64;
                        raw[2] += px[2] as u64;
                        swapped[0] += px[2] as u64;
                        swapped[1] += px[1] as u64;
                        swapped[2] += px[0] as u64;
                        inverted[0] += (255 - px[0]) as u64;
                        inverted[1] += (255 - px[1]) as u64;
                        inverted[2] += (255 - px[2]) as u64;
                        count += 1;
                    }
                }
                if count > 0 {
                    println!(
                        "raw_avg=({},{},{}) swapped_avg=({},{},{}) inverted_avg=({},{},{}) k_hi={} k_lo={}",
                        raw[0]/count, raw[1]/count, raw[2]/count,
                        swapped[0]/count, swapped[1]/count, swapped[2]/count,
                        inverted[0]/count, inverted[1]/count, inverted[2]/count,
                        by_k[1], by_k[0]
                    );
                }
            }
        }
    }
    if let Some(out) = std::env::args().nth(2) {
        let mut ppm = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
        for px in image.rgba.chunks_exact(4) {
            if px[3] < 8 {
                ppm.extend_from_slice(&[255, 0, 255]);
            } else {
                ppm.extend_from_slice(&px[..3]);
            }
        }
        std::fs::write(&out, ppm).expect("write ppm");
        println!("wrote {out}");
    }
}
