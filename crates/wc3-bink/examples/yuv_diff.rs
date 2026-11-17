//! Compare the decoded YUV planes against `ffmpeg -pix_fmt yuv420p` (oracle).
//!
//! ```text
//! scripts/h2-cargo run -p wc3-bink --example yuv_diff -- <movie> <frame> [frame...]
//! ```
//!
//! Working in YUV removes the BT.601 conversion from the comparison, so a
//! difference of one level in a plane is visible as exactly that.

use std::io::Read;
use std::process::{Command, Stdio};

use wc3_bink::BinkFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: yuv_diff <movie> <frame> [frame...]");
        std::process::exit(2);
    }
    let path = &args[0];
    let scan = args.get(1).map(|s| s.as_str()) == Some("--first");
    let frames: Vec<usize> = if scan {
        vec![]
    } else {
        args[1..].iter().filter_map(|s| s.parse().ok()).collect()
    };
    let scan_limit: usize = if scan {
        args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600)
    } else {
        0
    };
    let sum_mode = args.get(1).map(|s| s.as_str()) == Some("--sum");
    let sum_from: usize = if sum_mode {
        args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0)
    } else {
        0
    };
    let sum_to: usize = if sum_mode {
        args.get(3).and_then(|s| s.parse().ok()).unwrap_or(500)
    } else {
        0
    };
    let mut total = [0usize; 3];
    let mut worst = [0i32; 3];
    let mut signed = [0i64; 3];
    let last = if scan {
        scan_limit
    } else if sum_mode {
        sum_to
    } else {
        *frames.iter().max().expect("frames")
    };

    let mut file = BinkFile::open(path).expect("open movie");
    let (w, h) = file.size();
    let (w, h) = (w as usize, h as usize);
    let y_len = w * h;
    let c_len = (w / 2) * (h / 2);
    let frame_bytes = y_len + 2 * c_len;

    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            path,
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ffmpeg");
    let mut out = child.stdout.take().expect("stdout");
    let mut oracle = vec![0u8; frame_bytes];

    let mut want = frames.clone();
    want.sort_unstable();

    for i in 0..=last {
        if out.read_exact(&mut oracle).is_err() {
            eprintln!("oracle ended at {i}");
            break;
        }
        if file.decode_frame(i).is_err() {
            eprintln!("decode error at {i}");
            break;
        }
        if sum_mode {
            if i < sum_from {
                continue;
            }
            let pic = file.picture();
            let stride_y = pic.stride_y;
            let stride_c = pic.stride_c;
            for y in 0..h {
                for x in 0..w {
                    let d = i32::from(pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * stride_y + x])
                        - i32::from(oracle[y * w + x]);
                    signed[0] += i64::from(d);
                    if d != 0 {
                        total[0] += 1;
                        worst[0] = worst[0].max(d.abs());
                    }
                }
            }
            for (pi, plane) in [
                (0usize, wc3_bink::mpeg4::frame::Plane::U),
                (1usize, wc3_bink::mpeg4::frame::Plane::V),
            ] {
                let p = pic.plane(plane);
                for y in 0..h / 2 {
                    for x in 0..w / 2 {
                        let d = i32::from(p[y * stride_c + x]) - i32::from(oracle[y_len + pi * c_len + y * (w / 2) + x]);
                        signed[pi + 1] += i64::from(d);
                        if d != 0 {
                            total[pi + 1] += 1;
                            worst[pi + 1] = worst[pi + 1].max(d.abs());
                        }
                    }
                }
            }
            continue;
        }
        if scan {
            let pic = file.picture();
            let mut diffs = 0usize;
            for y in 0..h {
                for x in 0..w {
                    if pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * pic.stride_y + x] != oracle[y * w + x] {
                        diffs += 1;
                    }
                }
            }
            if diffs > 0 {
                println!("first Y difference at frame {i}: {diffs} samples");
                return;
            }
            continue;
        }
        if !want.contains(&i) {
            continue;
        }
        let pic = file.picture();
        let mut report = |name: &str, ours: &[u8], stride: usize, pw: usize, ph: usize, off: usize| {
            let mut diffs = 0usize;
            let mut max = 0i32;
            let mut first = None;
            for y in 0..ph {
                for x in 0..pw {
                    let a = i32::from(ours[y * stride + x]);
                    let b = i32::from(oracle[off + y * pw + x]);
                    let d = a - b;
                    if d != 0 {
                        diffs += 1;
                        max = max.max(d.abs());
                        if first.is_none() {
                            first = Some((x, y, a, b));
                        }
                    }
                }
            }
            println!(
                "  {name}: {diffs} differing samples, max |d| = {max}, first = {first:?}"
            );
        };
        report("Y", pic.plane(wc3_bink::mpeg4::frame::Plane::Y), pic.stride_y, w, h, 0);
        report(
            "U",
            pic.plane(wc3_bink::mpeg4::frame::Plane::U),
            pic.stride_c,
            w / 2,
            h / 2,
            y_len,
        );
        report(
            "V",
            pic.plane(wc3_bink::mpeg4::frame::Plane::V),
            pic.stride_c,
            w / 2,
            h / 2,
            y_len + c_len,
        );
        if std::env::var_os("H2_YUV_COLHIST").is_some() {
            let pic = file.picture();
            let stride_y = pic.stride_y;
            let mut cols = vec![0usize; (w + 15) / 16];
            let mut rows = vec![0usize; (h + 15) / 16];
            for y in 0..h {
                for x in 0..w {
                    if pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * stride_y + x] != oracle[y * w + x] {
                        cols[x / 16] += 1;
                        rows[y / 16] += 1;
                    }
                }
            }
            println!("  col hist: {:?}", cols);
            println!("  row hist: {:?}", rows);
        }
        if std::env::var_os("H2_YUV_DETAIL").is_some() {
            let pic = file.picture();
            let stride_y = pic.stride_y;
            let stride_c = pic.stride_c;
            let mut shown = 0;
            for y in 0..h {
                for x in 0..w {
                    let a = pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * stride_y + x];
                    let b = oracle[y * w + x];
                    if a != b && shown < 40 {
                        let mv = pic.mv_at(x / 8, y / 8);
                        println!("  Y ({x},{y}) mb({},{}) ours={a} oracle={b} mv={mv:?}", x / 16, y / 16);
                        shown += 1;
                    }
                }
            }
            for (pi, name) in [(0usize, "U"), (1usize, "V")] {
                let plane = if pi == 0 {
                    pic.plane(wc3_bink::mpeg4::frame::Plane::U)
                } else {
                    pic.plane(wc3_bink::mpeg4::frame::Plane::V)
                };
                for y in 0..h / 2 {
                    for x in 0..w / 2 {
                        let a = plane[y * stride_c + x];
                        let b = oracle[y_len + pi * c_len + y * (w / 2) + x];
                        if a != b && shown < 80 {
                            let mv = pic.mv_at(x / 8, y / 8);
                            let kinds = pic.mb_kind_map();
                            let mbw = (w + 15) / 16;
                            let kind = kinds[(y / 8) * mbw + (x / 8)] as char;
                            println!("  {name} ({x},{y}) mb({},{}) kind={kind} ours={a} oracle={b} mv={mv:?}", x / 8, y / 8);
                            shown += 1;
                        }
                    }
                }
            }
        }
        if std::env::var_os("H2_YUV_MB").is_some() {
            let pic = file.picture();
            let kinds = pic.mb_kind_map();
            let mb_w = (w + 15) / 16;
            let stride_y = pic.stride_y;
            let stride_c = pic.stride_c;
            println!("  per-MB differing samples (Y/U/V) and kind:");
            for mby in 0..h.div_ceil(16) {
                let mut line = String::new();
                for mbx in 0..mb_w {
                    let mut dy = 0;
                    for y in mby * 16..((mby + 1) * 16).min(h) {
                        for x in mbx * 16..((mbx + 1) * 16).min(w) {
                            if pic.plane(wc3_bink::mpeg4::frame::Plane::Y)[y * stride_y + x] != oracle[y * w + x] {
                                dy += 1;
                            }
                        }
                    }
                    let mut du = 0;
                    let mut dv = 0;
                    for y in mby * 8..((mby + 1) * 8).min(h / 2) {
                        for x in mbx * 8..((mbx + 1) * 8).min(w / 2) {
                            if pic.plane(wc3_bink::mpeg4::frame::Plane::U)[y * stride_c + x] != oracle[y_len + y * (w / 2) + x] {
                                du += 1;
                            }
                            if pic.plane(wc3_bink::mpeg4::frame::Plane::V)[y * stride_c + x] != oracle[y_len + c_len + y * (w / 2) + x] {
                                dv += 1;
                            }
                        }
                    }
                    let kind = kinds[mby * mb_w + mbx] as char;
                    if dy + du + dv == 0 {
                        line.push_str(" .    ");
                    } else {
                        line.push_str(&format!("{kind}{dy:2}/{du:2}/{dv:2} "));
                    }
                }
                println!("  y{mby:2} {line}");
            }
        }
        println!("frame {i} done");
    }
    let _ = child.kill();
    let _ = child.wait();
    if sum_mode {
        println!(
            "sum frames {sum_from}..{sum_to}: Y={} (max {}, signed {}) U={} (max {}, signed {}) V={} (max {}, signed {})",
            total[0], worst[0], signed[0], total[1], worst[1], signed[1], total[2], worst[2], signed[2]
        );
    }
}
