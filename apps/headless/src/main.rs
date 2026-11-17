//! Headless vertical-slice runner: map -> simulation -> replay -> JASS.
//!
//! Usage: `herofolends-headless [map.w3m|w3x] [ticks]`

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec2;
use wc3_re::mapfiles::MapBundle;
use wc3_replay::{Record, Replay};
use wc3_sim::{Command, Sim, UnitKind};

fn main() {
    let mut args = std::env::args().skip(1);
    let map_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m")
    });
    let ticks: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(300);

    let started = Instant::now();
    let map = match MapBundle::open(&map_path) {
        Ok(map) => map,
        Err(error) => {
            eprintln!("MAP_ERROR {error}");
            std::process::exit(2);
        }
    };
    println!("MAP path={}", map.path.display());
    let terrain = map.terrain.as_ref().expect("map terrain");
    println!(
        "MAP terrain={}x{} tileset={} tiles={}",
        terrain.width,
        terrain.height,
        terrain.tileset as char,
        terrain.tiles.len()
    );
    if let Some(info) = &map.info {
        println!(
            "MAP info_version={} name={:?} author={:?}",
            info.version,
            map.localized_info().map(|info| info.name).unwrap_or(info.name.clone()),
            info.author
        );
    }
    if let Some(pathing) = &map.pathing {
        println!("MAP pathing={}x{}", pathing.width, pathing.height);
    }
    println!(
        "MAP script_bytes={}",
        map.script.as_ref().map(String::len).unwrap_or(0)
    );
    println!(
        "MAP object_files={:?}",
        map.objects.keys().collect::<Vec<_>>()
    );

    let mut sim = Sim::new(0xC0FFEE, terrain.width, terrain.height);
    let centre = Vec2::new(terrain.width as f32 * 64.0, terrain.height as f32 * 64.0);
    let footman = sim.spawn(0, UnitKind::Footman, centre);
    let archer = sim.spawn(1, UnitKind::Archer, centre + Vec2::new(600.0, 0.0));
    let hall = sim.spawn(0, UnitKind::TownHall, centre - Vec2::new(300.0, 0.0));
    println!("SIM units=3 tick_rate={}", wc3_sim::TICKS_PER_SECOND);

    let move_command = Command::Move {
        units: vec![footman],
        to: centre + Vec2::new(800.0, 800.0),
    };
    let mut records = Vec::new();
    let mut command_log = Vec::new();
    for tick in 0..ticks {
        let commands = if tick == 0 {
            vec![move_command.clone()]
        } else if tick == 60 {
            vec![Command::Attack {
                units: vec![footman],
                target: archer,
            }]
        } else if tick == 90 {
            vec![Command::Train {
                building: hall,
                kind: UnitKind::Peasant,
            }]
        } else if tick == 120 {
            vec![Command::Build {
                builder: footman,
                kind: UnitKind::Archer,
                at: centre + Vec2::new(200.0, 200.0),
            }]
        } else {
            Vec::new()
        };
        for command in &commands {
            records.push(Record {
                tick,
                player: 0,
                command: command.clone(),
            });
            command_log.push((tick, command.clone()));
        }
        sim.step(&commands);
    }
    let final_hash = sim.state_hash();
    println!(
        "SIM tick={} hash={final_hash:016x} alive={} gold0={} gold1={}",
        sim.tick,
        sim.alive_units().count(),
        sim.resources[0],
        sim.resources[1]
    );

    let mut sim_verify = Sim::new(0xC0FFEE, terrain.width, terrain.height);
    let footman2 = sim_verify.spawn(0, UnitKind::Footman, centre);
    let archer2 = sim_verify.spawn(1, UnitKind::Archer, centre + Vec2::new(600.0, 0.0));
    let hall2 = sim_verify.spawn(0, UnitKind::TownHall, centre - Vec2::new(300.0, 0.0));
    let _ = (footman2, archer2, hall2);
    let mut index = 0usize;
    let mut verified_hash = 0u64;
    for tick in 0..ticks {
        let mut commands = Vec::new();
        while index < command_log.len() && command_log[index].0 == tick {
            commands.push(command_log[index].1.clone());
            index += 1;
        }
        sim_verify.step(&commands);
        if tick + 1 == ticks {
            verified_hash = sim_verify.state_hash();
        }
    }
    println!(
        "SIM verify_hash={verified_hash:016x} match={}",
        verified_hash == final_hash
    );

    let replay = Replay::from_sim(0xC0FFEE, 0, &sim, records);
    let mut buffer = Vec::new();
    match replay.write(&mut buffer) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("REPLAY_WRITE_ERROR {error}");
            std::process::exit(3);
        }
    }
    let verify = Replay::read(buffer.as_slice()).and_then(|r| {
        r.verify_with(terrain.width, terrain.height, |sim| {
            sim.spawn(0, UnitKind::Footman, centre);
            sim.spawn(1, UnitKind::Archer, centre + Vec2::new(600.0, 0.0));
            sim.spawn(0, UnitKind::TownHall, centre - Vec2::new(300.0, 0.0));
        })
    });
    match verify {
        Ok(hash) => println!(
            "REPLAY bytes={} verified_hash={hash:016x} match={}",
            buffer.len(),
            hash == final_hash
        ),
        Err(error) => {
            eprintln!("REPLAY_ERROR {error}");
            std::process::exit(4);
        }
    }

    if let Some(script) = &map.script {
        match wc3_jass::parse(script) {
            Ok(program) => {
                let functions = program.functions.len();
                let globals = program.globals.len();
                println!("JASS parse=ok functions={functions} globals={globals}");
                let mut runtime = wc3_jass::JassRuntime::new(program, 1);
                if runtime.functions.contains_key("main") {
                    match runtime.run_function("main") {
                        Ok(value) => {
                            println!("JASS main=ok value={value} output={}", runtime.output.len())
                        }
                        Err(error) => println!("JASS main=error {error}"),
                    }
                } else {
                    println!("JASS main=absent");
                }
            }
            Err(error) => println!("JASS parse=error {error}"),
        }
    }

    println!("ELAPSED_MS {}", started.elapsed().as_millis());
}
