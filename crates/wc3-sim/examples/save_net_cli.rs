//! Headless CLI smoke for the save/load and replay paths (P0-8/P0-9).
//!
//! ```text
//! cargo run -p wc3-sim --example save_net_cli -- smoke  <dir>
//! cargo run -p wc3-sim --example save_net_cli -- save   <file> [ticks]
//! cargo run -p wc3-sim --example save_net_cli -- load   <file>
//! cargo run -p wc3-sim --example save_net_cli -- replay <file>
//! ```
//!
//! Exit code 0 on success, 2 on any failure (bad arguments, unreadable save,
//! hash mismatch). Every command prints one JSON object so the smoke can be
//! asserted mechanically.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wc3_sim::net::{
    CommandFrame, LockstepPair, MapScene, MatchSetup, NetSession, Replay, SeatForce,
    TransportConfig,
};
use wc3_sim::{Command, Sim, UnitKind, Vec2};

const SEED: u64 = 0x5EED_2026;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{{\"ok\":false,\"error\":{}}}", json_string(&message));
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args.first().map(String::as_str) {
        Some("smoke") => {
            let dir = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| "smoke needs an output directory".to_string())?,
            );
            smoke(&dir)?;
        }
        Some("save") => {
            let path = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| "save needs an output file".to_string())?,
            );
            let ticks = args
                .get(2)
                .map(|raw| raw.parse::<u32>())
                .transpose()
                .map_err(|error| format!("bad tick count: {error}"))?
                .unwrap_or(600);
            let sim = play(ticks)?;
            sim.save_to_path(&path).map_err(|error| error.to_string())?;
            println!(
                "{{\"ok\":true,\"command\":\"save\",\"path\":{},\"bytes\":{},\"tick\":{},\"hash\":{}}}",
                json_string(&path.display().to_string()),
                sim.to_save_bytes().len(),
                sim.tick,
                sim.state_hash()
            );
        }
        Some("load") => {
            let path = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| "load needs a save file".to_string())?,
            );
            let sim = Sim::from_save_path(&path).map_err(|error| error.to_string())?;
            println!(
                "{{\"ok\":true,\"command\":\"load\",\"path\":{},\"tick\":{},\"hash\":{},\"digest\":{},\"units\":{}}}",
                json_string(&path.display().to_string()),
                sim.tick,
                sim.state_hash(),
                sim.save_digest(),
                sim.units.len()
            );
        }
        Some("match") => {
            match_commandline(&args[1..])?;
        }
        Some("replay") => {
            let path = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| "replay needs an output file".to_string())?,
            );
            let report = record_and_verify(&path)?;
            println!(
                "{{\"ok\":true,\"command\":\"replay\",\"path\":{},\"frames\":{},\"ticks\":{},\"end_hash\":{}}}",
                json_string(&path.display().to_string()),
                report.0,
                report.1,
                report.2
            );
        }
        _ => {
            return Err(
                "usage: save_net_cli <smoke <dir> | save <file> [ticks] | load <file> | replay <file> | match --port N --seat 0|1 [--map PATH] [--ticks N] [--fast] [--save PATH] [--replay PATH]>"
                    .to_string(),
            )
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Deterministic scripted match used by `save` and `smoke`.
fn play(ticks: u32) -> Result<Sim, String> {
    let setup = MatchSetup::melee_1v1(SEED, 64, 64);
    let mut sim = setup.build().map_err(|error| error.to_string())?;
    let hall = sim
        .units
        .iter()
        .find(|unit| unit.owner == 0 && unit.kind == UnitKind::TownHall)
        .map(|unit| unit.id)
        .ok_or_else(|| "no town hall in the melee setup".to_string())?;
    let worker = sim
        .units
        .iter()
        .find(|unit| unit.owner == 0 && unit.kind == UnitKind::Peasant)
        .map(|unit| unit.id)
        .ok_or_else(|| "no worker in the melee setup".to_string())?;
    let mine = sim.gold_mines.first().map(|mine| mine.pos).unwrap_or_default();
    for tick in 0..u64::from(ticks) {
        let commands = match tick {
            30 => vec![Command::Harvest {
                units: vec![worker],
                to: mine,
            }],
            90 => vec![Command::Train {
                building: hall,
                kind: UnitKind::Peasant,
            }],
            150 => vec![Command::Move {
                units: vec![worker],
                to: Vec2::new(2400.0, 2400.0),
            }],
            _ => Vec::new(),
        };
        sim.step(&commands);
    }
    Ok(sim)
}

/// Records a 180-tick match, writes the replay, reads it back and verifies it.
/// Returns `(frames, ticks, end_hash)`.
fn record_and_verify(path: &Path) -> Result<(usize, u64, u64), String> {
    let setup = MatchSetup::melee_1v1(SEED, 64, 64);
    let mut pair = LockstepPair::new(&setup).map_err(|error| error.to_string())?;
    let start_hash = pair.a.sim.state_hash();
    let hall = pair
        .a
        .sim
        .units
        .iter()
        .find(|unit| unit.owner == 0 && unit.kind == UnitKind::TownHall)
        .map(|unit| unit.id)
        .ok_or_else(|| "no town hall in the melee setup".to_string())?;
    let frames = vec![
        CommandFrame::new(
            0,
            0,
            vec![Command::Train {
                building: hall,
                kind: UnitKind::Peasant,
            }],
        ),
        CommandFrame::new(
            60,
            0,
            vec![Command::Move {
                units: vec![hall],
                to: Vec2::new(4096.0, 4096.0),
            }],
        ),
        CommandFrame::new(
            120,
            1,
            vec![Command::Hold {
                units: vec![hall],
            }],
        ),
    ];
    pair.broadcast_all(&frames).map_err(|error| error.to_string())?;
    pair.run(180).map_err(|error| error.to_string())?;
    if pair.a.hash() != pair.b.hash() {
        return Err("peers diverged during recording".to_string());
    }
    let end_hash = pair.a.hash();
    let replay = Replay::from_client(&setup, start_hash, &pair.a);
    replay.to_path(path).map_err(|error| error.to_string())?;
    let from_disk = Replay::from_path(path).map_err(|error| error.to_string())?;
    let report = from_disk.verify().map_err(|error| error.to_string())?;
    if report.end_hash != end_hash {
        return Err(format!(
            "replay hash mismatch: recorded {end_hash:#x}, reproduced {:#x}",
            report.end_hash
        ));
    }
    Ok((report.frames, report.ticks, report.end_hash))
}

/// Full smoke: save -> load -> 60 more ticks on both -> record -> replay.
fn smoke(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let save_path = dir.join("match.w3sv");
    let replay_path = dir.join("match.w3rp");

    let mut original = play(600)?;
    original
        .save_to_path(&save_path)
        .map_err(|error| error.to_string())?;
    let mut loaded = Sim::from_save_path(&save_path).map_err(|error| error.to_string())?;
    let save_tick = original.tick;
    if loaded.state_hash() != original.state_hash() {
        return Err("save/load hash mismatch".to_string());
    }
    for _ in 0..60 {
        original.step(&[]);
        loaded.step(&[]);
    }
    if loaded.state_hash() != original.state_hash() {
        return Err("loaded match diverged after 60 ticks".to_string());
    }

    let (frames, ticks, end_hash) = record_and_verify(&replay_path)?;
    println!(
        "{{\"ok\":true,\"command\":\"smoke\",\"dir\":{},\"save_bytes\":{},\"save_tick\":{},\"load_hash\":{},\"checkpoint_tick\":{},\"checkpoint_hash\":{},\"replay_frames\":{},\"replay_ticks\":{},\"replay_end_hash\":{}}}",
        json_string(&dir.display().to_string()),
        std::fs::metadata(&save_path).map(|meta| meta.len()).unwrap_or(0),
        save_tick,
        loaded.state_hash(),
        original.tick,
        original.state_hash(),
        frames,
        ticks,
        end_hash
    );
    Ok(())
}

/// Minimal JSON string encoder (no serde dependency in this crate).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// `match` — two headless processes playing one 1v1 over TCP
// ---------------------------------------------------------------------------

struct MatchArgs {
    port: u16,
    seat: u8,
    map: Option<PathBuf>,
    ticks: u32,
    fast: bool,
    save: Option<PathBuf>,
    replay: Option<PathBuf>,
    seed: u64,
}

fn parse_match_args(args: &[String]) -> Result<MatchArgs, String> {
    let mut parsed = MatchArgs {
        port: 0,
        seat: 0,
        map: None,
        ticks: 8_000,
        fast: false,
        save: None,
        replay: None,
        seed: SEED,
    };
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].clone();
        let value = |index: &mut usize, name: &str| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--port" => {
                parsed.port = value(&mut index, "--port")?
                    .parse()
                    .map_err(|error| format!("bad --port: {error}"))?
            }
            "--seat" => {
                parsed.seat = value(&mut index, "--seat")?
                    .parse()
                    .map_err(|error| format!("bad --seat: {error}"))?
            }
            "--map" => parsed.map = Some(PathBuf::from(value(&mut index, "--map")?)),
            "--ticks" => {
                parsed.ticks = value(&mut index, "--ticks")?
                    .parse()
                    .map_err(|error| format!("bad --ticks: {error}"))?
            }
            "--seed" => {
                parsed.seed = u64::from_str_radix(
                    value(&mut index, "--seed")?.trim_start_matches("0x"),
                    16,
                )
                .map_err(|error| format!("bad --seed: {error}"))?
            }
            "--save" => parsed.save = Some(PathBuf::from(value(&mut index, "--save")?)),
            "--replay" => parsed.replay = Some(PathBuf::from(value(&mut index, "--replay")?)),
            "--fast" => parsed.fast = true,
            other => return Err(format!("unknown match flag {other}")),
        }
        index += 1;
    }
    if parsed.port == 0 {
        return Err("match needs --port".to_string());
    }
    if parsed.seat > 1 {
        return Err("--seat must be 0 or 1".to_string());
    }
    Ok(parsed)
}

/// Default ready melee map (`Melee.w3x` = library `(2)Ponds_v1.4.w3x`).
fn default_map() -> PathBuf {
    if let Ok(path) = std::env::var("WC3_MELEE_MAP") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x",
    )
}

fn match_commandline(args: &[String]) -> Result<(), String> {
    let parsed = parse_match_args(args)?;
    let map = parsed.map.clone().unwrap_or_else(default_map);
    let mut scene = MapScene::load(&map).map_err(|error| error.to_string())?;
    // Decisive 1v1 variant: the map's own starts/pathing/mines, creeps cleared,
    // seat 0 raids with six fighters while seat 1 holds with one.
    scene.clear_neutrals();
    scene.seat_force = [
        SeatForce { workers: 3, fighters: 6 },
        SeatForce { workers: 2, fighters: 1 },
    ];
    let setup = MatchSetup::on_map(parsed.seed, scene);
    let addr = format!("127.0.0.1:{}", parsed.port);
    let config = TransportConfig {
        seat: parsed.seat,
        pace: !parsed.fast,
        timeout_ms: 60_000,
    };

    let mut session = if parsed.seat == 0 {
        NetSession::host_with(&addr, &setup, config).map_err(|error| error.to_string())?
    } else {
        connect_with_retry(&addr, &setup, config)?
    };
    let start_hash = session.start_hash;

    for _ in 0..parsed.ticks {
        let tick = session.sim.tick;
        let commands = seat_commands(&session.sim, session.local_seat, tick);
        let step = session.tick(commands).map_err(|error| error.to_string())?;
        if step.victory.is_some() {
            break;
        }
    }

    if let Some(path) = &parsed.save {
        session
            .sim
            .save_to_path(path)
            .map_err(|error| error.to_string())?;
    }
    let mut replay_ok = false;
    if let Some(path) = &parsed.replay {
        let replay = Replay::from_session(&setup, start_hash, &session);
        replay.to_path(path).map_err(|error| error.to_string())?;
        let report = Replay::from_path(path)
            .map_err(|error| error.to_string())?
            .verify()
            .map_err(|error| error.to_string())?;
        replay_ok = report.end_hash == session.last_hash;
    }

    println!(
        "{{\"ok\":true,\"command\":\"match\",\"seat\":{},\"map\":{},\"width\":{},\"ticks\":{},\"start_hash\":{},\"end_hash\":{},\"victory\":{},\"frames\":{},\"replay_verified\":{}}}",
        parsed.seat,
        json_string(&map.display().to_string()),
        setup.width(),
        session.sim.tick,
        start_hash,
        session.last_hash,
        match session.victory {
            Some(owner) => owner.to_string(),
            None => "null".to_string(),
        },
        session.frames().len(),
        replay_ok
    );
    Ok(())
}

fn connect_with_retry(
    addr: &str,
    setup: &MatchSetup,
    config: TransportConfig,
) -> Result<NetSession, String> {
    let mut last = String::new();
    for _ in 0..600 {
        match NetSession::connect_with(addr, setup, config) {
            Ok(session) => return Ok(session),
            Err(error) => {
                last = error.to_string();
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
    Err(format!("connect {addr}: {last}"))
}

/// Seat 0 assaults the enemy every 60 ticks; seat 1 holds. Identical on every
/// peer because it only reads the replicated sim state.
fn seat_commands(sim: &Sim, seat: u8, tick: u64) -> Vec<Command> {
    if seat != 0 || tick % 60 != 0 {
        return Vec::new();
    }
    let Some(base) = sim
        .units_of(seat)
        .find(|unit| unit.alive && unit.def().main_hall)
        .map(|unit| unit.pos)
    else {
        return Vec::new();
    };
    let mut best: Option<(f32, wc3_sim::UnitId)> = None;
    for unit in sim.units.iter().filter(|unit| unit.alive && unit.owner == 1) {
        let distance = unit.pos.distance(base);
        if best.map(|(value, _)| distance < value).unwrap_or(true) {
            best = Some((distance, unit.id));
        }
    }
    let Some((_, target)) = best else {
        return Vec::new();
    };
    let units: Vec<wc3_sim::UnitId> = sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == seat && !unit.is_building())
        .map(|unit| unit.id)
        .collect();
    if units.is_empty() {
        return Vec::new();
    }
    let to = sim.unit(target).map(|unit| unit.pos).unwrap_or_default();
    vec![Command::MoveAttack { units, to }]
}
