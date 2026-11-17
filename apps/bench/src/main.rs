//! Baseline benchmark for the headless engine pieces.
//!
//! Reports timings for MPQ access, MDX/BLP parsing, terrain loading, A* pathing
//! and simulation ticks, plus process RSS. W3AndroidEngine comparison is external:
//! install it on the device and use its own FPS/memory counters (see docs/RELEASE.md).

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec2;
use wc3_re::mapfiles::MapBundle;
use wc3_mpq::MpqArchive;
use wc3_path::Grid;
use wc3_sim::{Command, Sim, UnitKind};

fn rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().trim_end_matches(" kB").parse().unwrap_or(0);
        }
    }
    0
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mpq_path = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
        });
    let map_path = root.join("analysis/war3_map/Maps/Campaign/Human04.w3m");

    println!("BENCH rss_start_kb={}", rss_kb());

    let start = Instant::now();
    let archive = MpqArchive::open(&mpq_path).expect("open mpq");
    println!(
        "BENCH mpq_open_ms={} blocks={} listfile={}",
        start.elapsed().as_millis(),
        archive.block_count(),
        archive.listfile().len()
    );

    let start = Instant::now();
    let mdx = archive
        .read("Units/Human/Footman/Footman.mdx")
        .expect("mdx");
    let model = wc3_formats::mdx::MdxModel::parse(&mdx).expect("parse mdx");
    println!(
        "BENCH mdx_read_parse_ms={} bytes={} geosets={} bones={} faces={}",
        start.elapsed().as_millis(),
        mdx.len(),
        model.geosets.len(),
        model.bones.len(),
        model.total_faces()
    );

    let start = Instant::now();
    let blp = archive.read("Textures/Footman.blp").expect("blp");
    let image = wc3_formats::blp::decode(&blp).expect("decode blp");
    println!(
        "BENCH blp_decode_ms={} bytes={} rgba={} mips={}",
        start.elapsed().as_millis(),
        blp.len(),
        image.rgba.len(),
        image.mipmaps.len()
    );

    let start = Instant::now();
    let map = MapBundle::open(&map_path).expect("open map");
    let terrain = map.terrain.as_ref().expect("map terrain");
    println!(
        "BENCH map_open_ms={} terrain={}x{} units={}",
        start.elapsed().as_millis(),
        terrain.width,
        terrain.height,
        map.units.as_ref().map_or(0, |units| units.len())
    );

    let mut grid = Grid::new(128, 128, 128.0);
    for y in 40..80 {
        grid.set_walkable(64, y, false);
    }
    let start = Instant::now();
    let path = grid.astar((0, 0), (127, 127)).expect("path");
    println!(
        "BENCH astar_ms={} path_cells={}",
        start.elapsed().as_millis(),
        path.len()
    );

    let mut sim = Sim::new(42, 64, 64);
    for i in 0..200 {
        sim.spawn(
            0,
            UnitKind::Footman,
            Vec2::new((i % 20) as f32 * 100.0, (i / 20) as f32 * 100.0),
        );
        sim.spawn(
            1,
            UnitKind::Archer,
            Vec2::new(3000.0 + (i % 20) as f32 * 50.0, (i / 20) as f32 * 50.0),
        );
    }
    let ids: Vec<_> = sim
        .units
        .iter()
        .filter(|u| u.owner == 0)
        .map(|u| u.id)
        .collect();
    sim.step(&[Command::Move {
        units: ids,
        to: Vec2::new(4000.0, 4000.0),
    }]);
    let start = Instant::now();
    for _ in 0..1000 {
        sim.step(&[]);
    }
    let elapsed = start.elapsed();
    println!(
        "BENCH sim_1000_ticks_ms={} units={} tick_us={:.2} hash={:016x}",
        elapsed.as_millis(),
        sim.units.len(),
        elapsed.as_micros() as f64 / 1000.0,
        sim.state_hash()
    );
    println!("BENCH rss_end_kb={}", rss_kb());
}
