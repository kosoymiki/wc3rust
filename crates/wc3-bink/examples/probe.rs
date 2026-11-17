//! Decoder diagnostics: plane statistics for the first N frames.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example probe -- <movie> [frames]
//! ```

use wc3_bink::avi::Avi;
use wc3_bink::mpeg4::Mpeg4Decoder;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: probe <movie> [frames]");
        std::process::exit(2);
    }
    let frames: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let avi = match Avi::open(&args[0]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {e}", args[0]);
            std::process::exit(1);
        }
    };
    // Optional `--at <index>`: decode frame 0 to acquire the VOL, then decode
    // only that payload (valid for I-frames, which are self-contained).
    if let Some(pos) = args.iter().position(|a| a == "--at") {
        let target: usize = args[pos + 1].parse().expect("index");
        let mut dec = Mpeg4Decoder::new();
        let first = avi.video_frame(0).expect("frame 0");
        let _ = dec.decode(first);
        let payload = avi.video_frame(target).expect("target frame");
        let result = dec.decode(payload);
        if let Some(v) = dec.last_vop() {
            println!(
                "  vop: type={:?} qscale={} f_code={} dc_thr={} rounding={} coded={} tincr={} tinc={}",
                v.coding_type, v.qscale, v.f_code, v.intra_dc_threshold, v.rounding, v.coded, v.time_incr, v.time_increment
            );
        }
        if let Some(vol) = dec.vol() {
            println!(
                "  vol: {}x{} tinc_res={} progressive={} quant_type={} qpel={} verid={}",
                vol.width, vol.height, vol.time_increment_resolution, vol.progressive, vol.quant_type, vol.quarter_sample, vol.verid
            );
        }
        let st = dec.stats();
        println!(
            "  stats: mb={} p_intra={} four_mv={} dquant={} esc={}/{}/{}",
            st.macroblocks, st.p_intra, st.four_mv, st.dquant, st.escapes1, st.escapes2, st.escapes3
        );
        match result {
            Ok(t) => {
                let p = dec.picture();
                if let Some(pos) = args.iter().position(|a| a == "--dump-planes") {
                    let prefix = &args[pos + 1];
                    let (w, h) = (p.width, p.height);
                    let cw = (w + 1) / 2;
                    let ch = (h + 1) / 2;
                    let mut y = Vec::with_capacity(w * h);
                    let mut u = Vec::with_capacity(cw * ch);
                    let mut v = Vec::with_capacity(cw * ch);
                    for row in 0..h {
                        y.extend_from_slice(&p.y[row * p.stride_y..row * p.stride_y + w]);
                    }
                    for row in 0..ch {
                        u.extend_from_slice(&p.u[row * p.stride_c..row * p.stride_c + cw]);
                        v.extend_from_slice(&p.v[row * p.stride_c..row * p.stride_c + cw]);
                    }
                    std::fs::write(format!("{prefix}.y"), &y).expect("y");
                    std::fs::write(format!("{prefix}.u"), &u).expect("u");
                    std::fs::write(format!("{prefix}.v"), &v).expect("v");
                    println!("wrote {prefix}.{{y,u,v}}");
                }
                let mean = |plane: &[u8], stride: usize, w: usize, h: usize| {
                    let mut sum = 0u64;
                    for y in 0..h {
                        for x in 0..w {
                            sum += u64::from(plane[y * stride + x]);
                        }
                    }
                    sum as f64 / (w * h) as f64
                };
                let cw = (p.width + 1) / 2;
                let ch = (p.height + 1) / 2;
                println!(
                    "isolated frame {target}: {t:?} Y={:.2} U={:.2} V={:.2}",
                    mean(&p.y, p.stride_y, p.width, p.height),
                    mean(&p.u, p.stride_c, cw, ch),
                    mean(&p.v, p.stride_c, cw, ch)
                );
            }
            Err(e) => println!("isolated frame {target}: error {e}"),
        }
        return;
    }
    let mut dec = Mpeg4Decoder::new();
    for (i, payload) in avi.video_frames().enumerate().take(frames) {
        match dec.decode(payload) {
            Ok(t) => {
                let p = dec.picture();
                if let Some(pos) = args.iter().position(|a| a == "--dump-planes") {
                    let prefix = &args[pos + 1];
                    let (w, h) = (p.width, p.height);
                    let cw = (w + 1) / 2;
                    let ch = (h + 1) / 2;
                    let mut y = Vec::with_capacity(w * h);
                    let mut u = Vec::with_capacity(cw * ch);
                    let mut v = Vec::with_capacity(cw * ch);
                    for row in 0..h {
                        y.extend_from_slice(&p.y[row * p.stride_y..row * p.stride_y + w]);
                    }
                    for row in 0..ch {
                        u.extend_from_slice(&p.u[row * p.stride_c..row * p.stride_c + cw]);
                        v.extend_from_slice(&p.v[row * p.stride_c..row * p.stride_c + cw]);
                    }
                    std::fs::write(format!("{prefix}.y"), &y).expect("y");
                    std::fs::write(format!("{prefix}.u"), &u).expect("u");
                    std::fs::write(format!("{prefix}.v"), &v).expect("v");
                    std::fs::write(format!("{prefix}.mbmap"), p.mb_kind_map()).expect("mbmap");
                    println!("wrote {prefix}.{{y,u,v,mbmap}}");
                }
                let mean = |plane: &[u8], stride: usize, w: usize, h: usize| {
                    let mut sum = 0u64;
                    for y in 0..h {
                        for x in 0..w {
                            sum += u64::from(plane[y * stride + x]);
                        }
                    }
                    sum as f64 / (w * h) as f64
                };
                let cw = (p.width + 1) / 2;
                let ch = (p.height + 1) / 2;
                let vq = dec.last_vop().map(|v| v.qscale).unwrap_or(0);
                println!(
                    "frame {i:4} {t:?} q={vq} {}x{} Y={:.2} U={:.2} V={:.2} px0={:?} Y0..8={:?}",
                    p.width,
                    p.height,
                    mean(&p.y, p.stride_y, p.width, p.height),
                    mean(&p.u, p.stride_c, cw, ch),
                    mean(&p.v, p.stride_c, cw, ch),
                    [p.y[0], p.u[0], p.v[0]],
                    &p.y[..8]
                );
                let st = dec.stats();
                if std::env::var_os("WC3_BINK_STATS").is_some() {
                    println!(
                        "        mb={} p_intra={} four_mv={} dquant={} esc1={} esc2={} esc3={}",
                        st.macroblocks, st.p_intra, st.four_mv, st.dquant, st.escapes1, st.escapes2, st.escapes3
                    );
                }
            }
            Err(e) => {
                println!("frame {i:4} error: {e}");
                break;
            }
        }
    }
}
