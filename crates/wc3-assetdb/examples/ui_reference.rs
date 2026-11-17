//! Retail HUD reference render: assemble the shipped console art the way
//! `UI\FrameDef\UI\ConsoleUI.fdf` anchors it, at the device resolution.
//!
//! Why: the operator asked for the *original* HUD. The strongest source is not
//! a screenshot but the game's own art plus its FDF geometry:
//!
//! ```text
//! bottom console : Height 0.176 units (0.6 units = screen height) -> 317 px @1080
//! top strip      : Height 0.032 units                             ->  58 px @1080
//! tiles          : 0.256 wide each, anchored bottom-left / bottom-right
//! ```
//!
//! Output: `analysis/hudskin/reference-retail-<w>x<h>.ppm` (P6) plus the measured
//! tile sizes, so the composer can be compared against Blizzard's own layout.

use std::path::PathBuf;
use wc3_formats::blp;
use wc3_mpq::MpqArchive;

const UI_UNITS_PER_SCREEN: f32 = 0.6;

fn read_blp(archive: &MpqArchive, name: &str) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = archive.read(name).ok()?;
    let image = blp::decode(&bytes).ok()?;
    Some((image.width, image.height, image.rgba))
}

struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    fn blit(&mut self, src: &[u8], sw: u32, sh: u32, dx: i32, dy: i32, dw: u32, dh: u32) {
        if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
            return;
        }
        for y in 0..dh {
            let ty = dy + y as i32;
            if ty < 0 || ty >= self.height as i32 {
                continue;
            }
            let sy = (y as u64 * sh as u64 / dh as u64) as u32;
            for x in 0..dw {
                let tx = dx + x as i32;
                if tx < 0 || tx >= self.width as i32 {
                    continue;
                }
                let sx = (x as u64 * sw as u64 / dw as u64) as u32;
                let si = ((sy.min(sh - 1) * sw + sx.min(sw - 1)) * 4) as usize;
                let a = src[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let di = ((ty as u32 * self.width + tx as u32) * 4) as usize;
                for c in 0..3 {
                    let s = src[si + c] as u32;
                    let d = self.rgba[di + c] as u32;
                    self.rgba[di + c] = ((s * a + d * (255 - a)) / 255) as u8;
                }
                self.rgba[di + 3] = 255;
            }
        }
    }

    fn write_ppm(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = Vec::with_capacity((self.width * self.height * 3) as usize + 32);
        out.extend_from_slice(format!("P6\n{} {}\n255\n", self.width, self.height).as_bytes());
        for pixel in self.rgba.chunks_exact(4) {
            out.extend_from_slice(&pixel[..3]);
        }
        std::fs::File::create(path)?.write_all(&out)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "/data/data/org.herofolends.p0/files/War3.mpq".to_string());
    let race = args.get(2).cloned().unwrap_or_else(|| "Human".to_string());
    let archive = match MpqArchive::open(&archive_path) {
        Ok(archive) => archive,
        Err(e) => {
            println!("UI_REFERENCE skip: cannot open {archive_path}: {e}");
            return;
        }
    };

    let (width, height) = (2414u32, 1080u32);
    let unit = height as f32 / UI_UNITS_PER_SCREEN;
    let console_h = (0.176 * unit).round() as u32;
    let top_h = (0.032 * unit).round() as u32;
    println!(
        "UI_REFERENCE {width}x{height} unit={unit:.0}px console={console_h}px ({:.1}%) top={top_h}px ({:.1}%)",
        console_h as f32 / height as f32 * 100.0,
        top_h as f32 / height as f32 * 100.0
    );

    let mut canvas = Canvas::new(width, height);
    for pixel in canvas.rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[40, 52, 34, 255]);
    }

    let tile_w = (0.256 * unit).round() as u32;
    let mid_w = (0.256 * unit).round() as u32;
    let right_w = (0.256 * unit).round() as u32;
    let edge = (0.032 * unit).round() as u32;
    let bottom_y = (height - console_h) as i32;
    let mut drawn = 0usize;
    let mut report: Vec<String> = Vec::new();
    for index in 1..=4u32 {
        let name = format!("UI\\Console\\{race}\\{race}UITile{index:02}.blp");
        match read_blp(&archive, &name) {
            Some((w, h, rgba)) => {
                report.push(format!("{name}: {w}x{h}"));
                let (dx, dw) = match index {
                    1 => (0i32, tile_w),
                    2 => (tile_w as i32, mid_w),
                    3 => (width as i32 - edge as i32 - right_w as i32, right_w),
                    _ => (width as i32 - edge as i32, edge),
                };
                canvas.blit(&rgba, w, h, dx, bottom_y, dw, console_h);
                drawn += 1;
            }
            None => report.push(format!("{name}: missing")),
        }
    }

    let top_pieces = [
        (1u32, 0i32, (0.256 * unit).round() as u32),
        (2u32, (0.256 * unit).round() as i32, (0.087 * unit).round() as u32),
        (2u32, (width as f32 - 0.288 * unit) as i32, (0.053 * unit).round() as u32),
        (3u32, (width as f32 - 0.288 * unit) as i32, (0.256 * unit).round() as u32),
        (4u32, (width as f32 - 0.032 * unit) as i32, (0.032 * unit).round() as u32),
    ];
    for (index, dx, dw) in top_pieces {
        let name = format!("UI\\Console\\{race}\\{race}UITile{index:02}.blp");
        if let Some((w, h, rgba)) = read_blp(&archive, &name) {
            canvas.blit(&rgba, w, h, dx, 0, dw, top_h);
            drawn += 1;
        }
    }

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/hudskin");
    let _ = std::fs::create_dir_all(&out_dir);
    let out = out_dir.join(format!("reference-retail-{width}x{height}.ppm"));
    match canvas.write_ppm(&out) {
        Ok(()) => println!(
            "UI_REFERENCE wrote {} pieces={drawn} {}",
            out.display(),
            report.join("; ")
        ),
        Err(e) => println!("UI_REFERENCE write failed: {e}"),
    }
}
