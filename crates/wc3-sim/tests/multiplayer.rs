//! Mechanic tests for `net.lockstep`, `net.desync` and `net.replay` (P0-9).
//!
//! Policy: one test proves one named mechanic from `MECHANICS_GAP_MAP.md`;
//! the test name starts with the mechanic id.
//!
//! Contract under test: two peers running the same seeded simulation and the
//! same command frames produce an identical `state_hash` on every tick; a
//! diverging peer is detected at the first differing tick (`net.desync`); and a
//! recorded replay (setup + seed + frames + hashes) re-simulates to the same
//! final hash.

use wc3_sim::net::{
    CommandFrame, DesyncDetector, FrameClock, LockstepClient, LockstepPair, MatchSetup, NetError,
    Replay, LOCKSTEP_HZ,
};
use wc3_sim::{Command, Sim, UnitId, UnitKind, Vec2};

const SEED: u64 = 0xC0FFEE_1234;

fn setup() -> MatchSetup {
    MatchSetup::melee_1v1(SEED, 64, 64)
}

fn first_unit(sim: &Sim, owner: u8, kind: UnitKind) -> UnitId {
    sim.units
        .iter()
        .find(|unit| unit.owner == owner && unit.kind == kind)
        .map(|unit| unit.id)
        .expect("unit of kind for owner")
}

/// A command schedule identical for both peers: move/stop/train plus a harvest
/// order, spread over the first 300 ticks.
fn schedule(sim: &Sim) -> Vec<CommandFrame> {
    let hall = first_unit(sim, 0, UnitKind::TownHall);
    let worker = first_unit(sim, 0, UnitKind::Peasant);
    let mine = sim.gold_mines.first().map(|mine| mine.pos).unwrap_or_default();
    vec![
        CommandFrame::new(
            0,
            0,
            vec![Command::Harvest {
                units: vec![worker],
                to: mine,
            }],
        ),
        CommandFrame::new(
            0,
            1,
            vec![Command::Move {
                units: vec![first_unit(sim, 1, UnitKind::TownHall)],
                to: Vec2::new(5000.0, 5000.0),
            }],
        ),
        CommandFrame::new(
            45,
            0,
            vec![Command::Train {
                building: hall,
                kind: UnitKind::Peasant,
            }],
        ),
        CommandFrame::new(
            90,
            0,
            vec![Command::Move {
                units: vec![worker],
                to: Vec2::new(2200.0, 2200.0),
            }],
        ),
        CommandFrame::new(
            150,
            1,
            vec![Command::Stop {
                units: vec![first_unit(sim, 1, UnitKind::TownHall)],
            }],
        ),
        CommandFrame::new(
            210,
            0,
            vec![Command::Hold {
                units: vec![worker],
            }],
        ),
    ]
}

#[test]
fn net_lockstep_two_clients_identical_state_hash_every_tick() {
    let setup = setup();
    let mut pair = LockstepPair::new(&setup).expect("two peers from one setup");
    let frames = schedule(&pair.a.sim);
    pair.broadcast_all(&frames).expect("frames accepted");

    for tick in 0..300u64 {
        let (left, right) = pair.tick().expect("no desync");
        assert_eq!(left, right, "hash mismatch reported at tick {tick}");
        assert_eq!(
            pair.a.sim.tick, pair.b.sim.tick,
            "peers must advance in lockstep"
        );
    }
    assert_eq!(pair.a.sim.tick, 300);
    assert_eq!(pair.a.hashes(), pair.b.hashes(), "per-tick hash history");
    assert_eq!(pair.a.hash(), pair.b.hash());
    assert!(
        pair.a.log().len() >= frames.len(),
        "every broadcast frame was executed"
    );
}

#[test]
fn net_lockstep_seeded_rng_identical_on_both_seats() {
    // Seeded RNG: the same setup must give the same initial hash and the same
    // random stream, so combat rolls replay identically on every peer.
    let sim_a = setup().build().expect("peer A");
    let sim_b = setup().build().expect("peer B");
    assert_eq!(sim_a.state_hash(), sim_b.state_hash(), "start hash");
    let mut a = LockstepClient::new(sim_a, 0).expect("client A");
    let mut b = LockstepClient::new(sim_b, 1).expect("client B");
    for draw in 0..32 {
        assert_eq!(
            a.sim.random(10_000),
            b.sim.random(10_000),
            "random draw {draw} diverged between seats"
        );
    }
    a.run(60);
    b.run(60);
    assert_eq!(a.hash(), b.hash(), "60 ticks of AI without commands");
}

#[test]
fn net_desync_detected_on_diverging_client() {
    let mut pair = LockstepPair::new(&setup()).expect("pair");
    pair.run(30).expect("clean 30 ticks");

    // A deliberately diverging peer: one resource point of local state drift,
    // which is exactly what a buggy patch or a lost frame looks like.
    pair.b.sim.resources[0] += 1;

    let error = pair.tick().expect_err("divergence must be detected");
    match error {
        NetError::Desync {
            tick,
            local,
            remote,
        } => {
            assert_eq!(tick, 31, "detected on the first diverging tick");
            assert_ne!(local, remote, "hashes must differ");
        }
        other => panic!("expected Desync, got {other:?}"),
    }
}

#[test]
fn net_desync_detector_flags_remote_mismatch() {
    let mut detector = DesyncDetector::new();
    detector.expect(10, 0xAAAA);
    detector.expect(11, 0xBBBB);

    detector.check(10, 0xAAAA).expect("matching hash");
    detector.check(12, 0x1234).expect("unknown tick is not a mismatch");
    let error = detector
        .check(11, 0xBBBC)
        .expect_err("reported hash differs");
    match error {
        NetError::Desync {
            tick,
            local,
            remote,
        } => {
            assert_eq!(tick, 11);
            assert_eq!(local, 0xBBBB);
            assert_eq!(remote, 0xBBBC);
        }
        other => panic!("expected Desync, got {other:?}"),
    }
}

#[test]
fn net_replay_record_and_replay_identical_hash() {
    let setup = setup();
    let mut pair = LockstepPair::new(&setup).expect("pair");
    let start_hash = pair.a.sim.state_hash();
    let frames = schedule(&pair.a.sim);
    pair.broadcast_all(&frames).expect("frames accepted");
    pair.run(240).expect("clean match");
    let end_hash = pair.a.hash();
    assert_eq!(end_hash, pair.b.hash());

    let replay = Replay::from_client(&setup, start_hash, &pair.a);
    assert_eq!(replay.start_hash, start_hash);
    assert_eq!(replay.end_tick, 240);
    assert_eq!(replay.end_hash, end_hash);

    // Encode -> decode -> re-simulate.
    let encoded = replay.encode();
    let decoded = Replay::decode(&encoded).expect("replay must decode");
    assert_eq!(decoded, replay, "replay round trip");
    let report = decoded.verify().expect("replay must reproduce the match");
    assert_eq!(report.start_hash, start_hash);
    assert_eq!(report.end_tick, 240);
    assert_eq!(report.ticks, 240);
    assert_eq!(report.end_hash, end_hash);
    assert_eq!(report.frames, frames.len());

    // The same replay through a file, exactly as the CLI does it.
    let dir = std::env::temp_dir().join(format!("wc3-sim-replay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("match.w3rp");
    replay.to_path(&path).expect("write replay");
    let from_disk = Replay::from_path(&path).expect("read replay");
    assert_eq!(from_disk, replay);
    assert_eq!(
        from_disk.verify().expect("disk replay").end_hash,
        end_hash
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn net_replay_tampered_stream_detected() {
    let setup = setup();
    let mut pair = LockstepPair::new(&setup).expect("pair");
    let start_hash = pair.a.sim.state_hash();
    pair.broadcast_all(&schedule(&pair.a.sim))
        .expect("frames");
    pair.run(120).expect("clean match");
    let replay = Replay::from_client(&setup, start_hash, &pair.a);

    // A frame that claims different commands must not reproduce the hash.
    let mut tampered = replay.clone();
    let worker_move = tampered
        .frames
        .iter_mut()
        .find(|frame| frame.tick == 90)
        .expect("move frame");
    worker_move.commands[0] = Command::Move {
        units: vec![first_unit(&pair.a.sim, 0, UnitKind::Peasant)],
        to: Vec2::new(999.0, 999.0),
    };
    let error = tampered.verify().expect_err("tampered replay must fail");
    assert!(
        matches!(error, NetError::HashMismatch { .. }),
        "expected HashMismatch, got {error:?}"
    );

    // A dropped frame changes the command stream and must also fail.
    let mut dropped = replay.clone();
    dropped.frames.retain(|frame| frame.tick != 90);
    assert!(dropped.verify().is_err(), "dropped frame must fail");

    // A bit flip in the envelope is caught by the checksum before parsing.
    let mut corrupt = replay.encode();
    let middle = corrupt.len() / 2;
    corrupt[middle] ^= 0x20;
    let error = Replay::decode(&corrupt).expect_err("corrupt replay must fail");
    assert!(
        matches!(error, NetError::Checksum { .. }),
        "expected Checksum, got {error:?}"
    );

    // A truncated replay is a typed error, never a panic.
    let encoded = replay.encode();
    for cut in [0usize, 1, 3, 4, 7, 11, 12, encoded.len() / 2, encoded.len() - 1] {
        assert!(
            Replay::decode(&encoded[..cut]).is_err(),
            "cut {cut} must fail"
        );
    }
}

#[test]
fn net_replay_short_file_is_typed_error() {
    assert!(matches!(
        Replay::decode(&[]),
        Err(NetError::Truncated { .. })
    ));
    assert!(matches!(
        Replay::decode(b"W3RP"),
        Err(NetError::Truncated { .. })
    ));
    assert!(matches!(
        Replay::decode(b"XXXX\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
        Err(NetError::BadMagic(_))
    ));

    let mut wrong_version = Vec::new();
    wrong_version.extend_from_slice(b"W3RP");
    wrong_version.extend_from_slice(&9u16.to_le_bytes());
    wrong_version.extend_from_slice(&0u16.to_le_bytes());
    wrong_version.extend_from_slice(&0u32.to_le_bytes());
    wrong_version.extend_from_slice(&0u64.to_le_bytes());
    assert!(matches!(
        Replay::decode(&wrong_version),
        Err(NetError::UnknownVersion(9))
    ));
}

#[test]
fn net_lockstep_late_frame_rejected() {
    let mut client = LockstepClient::new(setup().build().expect("sim"), 0).expect("client");
    client.run(5);
    let error = client
        .schedule(CommandFrame::new(
            2,
            0,
            vec![Command::Stop { units: vec![] }],
        ))
        .expect_err("late frame");
    match error {
        NetError::LateFrame { tick, current } => {
            assert_eq!(tick, 2);
            assert_eq!(current, 5);
        }
        other => panic!("expected LateFrame, got {other:?}"),
    }

    // A duplicate frame for the same tick and player is also rejected.
    let frame = CommandFrame::new(7, 0, vec![Command::Stop { units: vec![] }]);
    client.schedule(frame.clone()).expect("first frame");
    assert!(matches!(
        client.schedule(frame),
        Err(NetError::DuplicateFrame { tick: 7, player: 0 })
    ));

    // An out-of-range seat is rejected.
    assert!(matches!(
        LockstepClient::new(setup().build().expect("sim"), 4),
        Err(NetError::PlayerOutOfRange(4))
    ));
}

#[test]
fn net_lockstep_frame_clock_30hz() {
    assert_eq!(LOCKSTEP_HZ, 30);
    // A clock with headroom proves the 30 Hz arithmetic exactly.
    let mut clock = FrameClock::new(60);
    assert_eq!(clock.advance(0), 0, "no time, no tick");
    assert_eq!(clock.advance(500), 15, "half a second is 15 ticks");
    assert_eq!(clock.advance(500), 15, "one second total is 30 ticks");
    assert_eq!(clock.released(), 30);
    assert_eq!(clock.advance(33), 0, "33 ms is one tick short of 30.99");
    assert_eq!(clock.advance(1), 1, "34 ms crosses the 31st tick");

    // Catch-up is capped: a long stall releases at most `max_catchup` ticks per
    // advance instead of telescoping the whole backlog into one frame.
    let mut stalled = FrameClock::new(5);
    assert_eq!(stalled.advance(10_000), 5, "10 s backlog capped at 5 ticks");
    assert_eq!(stalled.advance(10_000), 5, "cap applies per advance");
}

#[test]
fn net_lockstep_command_frames_roundtrip_all_variants() {
    // Every command the transport can carry must survive the frame codec,
    // otherwise an order silently disappears in multiplayer.
    let ability = wc3_sim::abilities::SPELL_DEFS[0].id;
    let commands = vec![
        Command::Move {
            units: vec![UnitId(1), UnitId(2)],
            to: Vec2::new(100.5, -200.25),
        },
        Command::Attack {
            units: vec![UnitId(3)],
            target: UnitId(4),
        },
        Command::Stop {
            units: vec![UnitId(5)],
        },
        Command::MoveAttack {
            units: vec![UnitId(6)],
            to: Vec2::new(1.0, 2.0),
        },
        Command::Hold {
            units: vec![UnitId(7)],
        },
        Command::Harvest {
            units: vec![UnitId(8)],
            to: Vec2::new(3.0, 4.0),
        },
        Command::CancelTrain {
            building: UnitId(9),
            index: 2,
        },
        Command::Train {
            building: UnitId(10),
            kind: UnitKind::Footman,
        },
        Command::Build {
            builder: UnitId(11),
            kind: UnitKind::TownHall,
            at: Vec2::new(5.0, 6.0),
        },
        Command::Cast {
            unit: UnitId(12),
            ability: wc3_sim::Ability::Heal(200),
            target: Some(UnitId(13)),
        },
        Command::LearnAbility {
            unit: UnitId(14),
            ability,
        },
        Command::Repair {
            units: vec![UnitId(15)],
            target: UnitId(16),
        },
    ];
    let frame = CommandFrame::new(1234, 1, commands.clone());
    let bytes = frame.to_bytes();
    let decoded = CommandFrame::from_bytes(&bytes).expect("frame must decode");
    assert_eq!(decoded, frame, "frame round trip");
    assert_eq!(decoded.commands.len(), commands.len());
    assert_eq!(decoded.tick, 1234);
    assert_eq!(decoded.player, 1);

    // Trailing bytes are rejected rather than silently ignored.
    let mut extended = bytes.clone();
    extended.push(0);
    assert!(matches!(
        CommandFrame::from_bytes(&extended),
        Err(NetError::TrailingBytes(1))
    ));
}

// ---------------------------------------------------------------------------
// P0-9 full goal: TCP transport, 1v1 on the ready melee map, save/load resume
// ---------------------------------------------------------------------------

use std::path::PathBuf;
use wc3_sim::net::{MapScene, NetResult, NetSession, SeatForce, TransportConfig};
use wc3_sim::{Event, Sim as SimType};

/// The library's ready melee map (`(2)Ponds_v1.4.w3x`), the same file the game
/// boots as `Melee.w3x`.
fn melee_map() -> PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x",
    );
    assert!(path.is_file(), "ready melee map missing at {}", path.display());
    path
}

/// Binds a real listener on an ephemeral port and returns it with its port.
/// Handing the listener to the host thread removes the bind race entirely.
fn bind_ephemeral() -> (std::net::TcpListener, u16) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    (listener, port)
}

fn connect_retry(addr: &str, setup: &MatchSetup, config: TransportConfig) -> NetResult<NetSession> {
    let mut last = None;
    // Generous window: the suite runs heavy map matches in parallel, so the
    // host thread can take seconds to reach `accept`.
    for _ in 0..6000 {
        match NetSession::connect_with(addr, setup, config) {
            Ok(session) => return Ok(session),
            // A completed connect that fails the handshake is final: retrying
            // would mask it as "connection refused" once the host exits.
            Err(error @ (NetError::Handshake(_) | NetError::SeatConflict { .. } | NetError::Desync { .. })) => {
                return Err(error)
            }
            Err(error) => {
                last = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
    Err(last.expect("at least one connect attempt"))
}

fn victory_of(sim: &SimType) -> Option<u8> {
    sim.events.iter().find_map(|event| match event {
        Event::Victory(owner) => Some(*owner),
        _ => None,
    })
}

fn own_base(sim: &SimType, seat: u8) -> Option<Vec2> {
    sim.units_of(seat)
        .find(|unit| unit.alive && unit.def().main_hall)
        .map(|unit| unit.pos)
}

/// Every living *mobile* unit of `seat` in id order (deterministic). Buildings
/// are excluded: `Command::Attack` is for units that can actually fight.
fn seat_units(sim: &SimType, seat: u8) -> Vec<UnitId> {
    sim.units
        .iter()
        .filter(|unit| unit.alive && unit.owner == seat && !unit.is_building())
        .map(|unit| unit.id)
        .collect()
}

/// Nearest living enemy unit to our own main hall — a stable, deterministic
/// assault target that advances as the enemy line collapses.
fn assault_target(sim: &SimType, seat: u8) -> Option<UnitId> {
    let base = own_base(sim, seat)?;
    let enemy = 1 - seat;
    let mut best: Option<(f32, UnitId)> = None;
    for unit in sim.units.iter().filter(|unit| unit.alive && unit.owner == enemy) {
        let distance = unit.pos.distance(base);
        if best.map(|(value, _)| distance < value).unwrap_or(true) {
            best = Some((distance, unit.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Seat 0 assaults, seat 1 idles: the deterministic 1v1 scenario used by both
/// the in-process and the TCP match tests.
fn match_command(sim: &SimType, seat: u8, tick: u64) -> Vec<Command> {
    if seat != 0 || tick % 60 != 0 {
        return Vec::new();
    }
    let Some(target) = assault_target(sim, seat) else {
        return Vec::new();
    };
    let units = seat_units(sim, seat);
    if units.is_empty() {
        return Vec::new();
    }
    let to = sim.unit(target).map(|unit| unit.pos).unwrap_or_default();
    vec![Command::MoveAttack { units, to }]
}


/// The 1v1 scenario: the ready melee map's own starts, pathing, mines and
/// destructables, with neutral creeps cleared and a decisive force split —
/// seat 0 raids with six fighters, seat 1 holds with one. Both seats are still
/// anchored at their own `sloc` marker; the split only makes the outcome a
/// definite victory instead of a creeps-decided coin flip.
fn melee_scenario() -> MatchSetup {
    let mut scene = MapScene::load(melee_map()).expect("map scene");
    scene.clear_neutrals();
    scene.seat_force = [
        SeatForce { workers: 3, fighters: 6 },
        SeatForce { workers: 2, fighters: 1 },
    ];
    MatchSetup::on_map(SEED, scene)
}

#[test]
fn net_match_1v1_map_starts_deterministic_victory() {
    let setup = melee_scenario();
    let scene = setup.scene().expect("map scene").clone();
    assert!(scene.starts.len() >= 2, "map exposes at least two sloc starts");
    let start_hash = setup.build().expect("match builds").state_hash();
    let mut pair = LockstepPair::new(&setup).expect("two peers from one setup");

    // Both seats fight from the map's own start markers.
    for seat in 0..2u8 {
        let start = scene.starts[seat as usize];
        let hall = pair
            .a
            .sim
            .units_of(seat)
            .find(|unit| unit.alive && unit.def().main_hall)
            .unwrap_or_else(|| panic!("seat {seat} has a main hall"));
        assert!(
            hall.pos.distance(start) < 700.0,
            "seat {seat} hall {:?} is not at its map start {start:?}",
            hall.pos
        );
    }

    let mut frames = Vec::new();
    let mut victory = None;
    for tick in 0..12_000u64 {
        let commands = match_command(&pair.a.sim, 0, tick);
        if !commands.is_empty() {
            let frame = CommandFrame::new(tick, 0, commands);
            pair.broadcast(frame.clone()).expect("frame accepted");
            frames.push(frame);
        }
        let (left, right) = pair.tick().expect("no desync");
        assert_eq!(left, right, "tick {tick} hash mismatch");
        if let Some(winner) = victory_of(&pair.a.sim) {
            victory = Some(winner);
            break;
        }
    }
    assert_eq!(victory, Some(0), "seat 0 wins the scripted 1v1");
    assert_eq!(pair.a.hash(), pair.b.hash());
    assert_eq!(
        victory_of(&pair.b.sim),
        Some(0),
        "both peers report the same victory"
    );

    // The match records and replays to the identical hash.
    let replay = Replay::record(
        &setup,
        start_hash,
        pair.a.sim.tick,
        pair.a.hash(),
        frames,
    );
    let report = replay.verify().expect("map replay verifies");
    assert_eq!(report.end_hash, pair.a.hash());
    assert_eq!(report.frames, replay.frames.len());
    eprintln!(
        "MAP_MATCH starts={} units={} ticks={} end_hash={:#x} replay_frames={}",
        scene.starts.len(),
        scene.units.len(),
        report.ticks,
        report.end_hash,
        report.frames
    );
}

#[test]
fn net_match_1v1_tcp_deterministic_victory() {
    let mut setup = melee_scenario();
    setup.seed ^= 0x77;
    let (listener, port) = bind_ephemeral();
    let addr = format!("127.0.0.1:{port}");
    let host_setup = setup.clone();
    let host = std::thread::spawn(move || -> Result<(u64, u64, Option<u8>, usize, u64, Vec<(u64, u64)>), String> {
        let config = TransportConfig { seat: 0, pace: false, timeout_ms: 60_000 };
        let mut session = NetSession::host_on(listener, &host_setup, config)
            .map_err(|error| error.to_string())?;
        let start_hash = session.start_hash;
        for _ in 0..12_000u64 {
            let tick = session.sim.tick;
            let commands = match_command(&session.sim, session.local_seat, tick);
            let step = session.tick(commands).map_err(|error| error.to_string())?;
            if step.victory.is_some() {
                break;
            }
        }
        Ok((
            start_hash,
            session.last_hash,
            session.victory,
            session.frames().len(),
            session.sim.tick,
            session.hashes().to_vec(),
        ))
    });

    let config = TransportConfig { seat: 1, pace: false, timeout_ms: 60_000 };
    let mut client = connect_retry(&addr, &setup, config).expect("client handshake");
    for _ in 0..12_000u64 {
        let tick = client.sim.tick;
        let commands = match_command(&client.sim, client.local_seat, tick);
        let step = client.tick(commands).expect("client tick");
        if step.victory.is_some() {
            break;
        }
    }
    let (start_hash, host_hash, host_victory, host_frames, host_ticks, host_hashes) =
        host.join().expect("host thread").expect("host session");

    assert_eq!(client.start_hash, start_hash, "agreed start hash");
    assert_eq!(client.hashes(), &host_hashes[..], "per-tick hash history");
    assert_eq!(client.last_hash, host_hash, "final hash");
    assert_eq!(host_victory, Some(0), "host reported seat 0 victory");
    assert_eq!(client.victory, Some(0), "client reported seat 0 victory");
    assert_eq!(client.sim.tick, host_ticks);

    // Replay from the client session: same frames, same final hash.
    let replay = Replay::from_session(&setup, start_hash, &client);
    let report = replay.verify().expect("tcp match replay verifies");
    assert_eq!(report.end_hash, client.last_hash);
    assert!(host_frames > 0, "the match carried command frames");
    eprintln!(
        "TCP_MATCH ticks={} end_hash={:#x} frames={} victory={:?}",
        report.ticks, report.end_hash, report.frames, client.victory
    );
}

#[test]
fn net_match_1v1_save_load_resume_identical_hash() {
    let mut setup = melee_scenario();
    setup.seed ^= 0x99;
    let mut pair = LockstepPair::new(&setup).expect("pair");
    let mut frames = Vec::new();
    for tick in 0..180u64 {
        let commands = match_command(&pair.a.sim, 0, tick);
        if !commands.is_empty() {
            let frame = CommandFrame::new(tick, 0, commands);
            pair.broadcast(frame.clone()).expect("frame");
            frames.push(frame);
        }
        pair.tick().expect("no desync");
    }

    // "Save and quit": the continuation is built only from the save bytes.
    let save_a = pair.a.sim.to_save_bytes();
    let save_b = pair.b.sim.to_save_bytes();
    let mut resumed = LockstepPair::from_sims(
        Sim::from_save(&save_a).expect("save A loads"),
        Sim::from_save(&save_b).expect("save B loads"),
    )
    .expect("resume pair");

    assert_eq!(resumed.a.hash(), pair.a.hash(), "resume hash");
    for tick in 180..480u64 {
        let commands = match_command(&pair.a.sim, 0, tick);
        if !commands.is_empty() {
            let frame = CommandFrame::new(tick, 0, commands);
            pair.broadcast(frame.clone()).expect("frame");
            resumed.broadcast(frame).expect("resume frame");
            frames.push(CommandFrame::new(tick, 0, Vec::new()));
        }
        pair.tick().expect("no desync");
        resumed.tick().expect("no desync after load");
        if tick % 60 == 0 {
            assert_eq!(
                resumed.a.hash(),
                pair.a.hash(),
                "resumed hashes diverged at tick {tick}"
            );
            assert_eq!(resumed.b.hash(), pair.b.hash());
        }
    }
    assert_eq!(resumed.a.sim.tick, 480);
    assert_eq!(
        resumed.a.sim.to_save_bytes(),
        pair.a.sim.to_save_bytes(),
        "resumed serialized state is identical"
    );
}

#[test]
fn net_lockstep_tcp_two_sessions_identical_hash_every_tick() {
    let setup = MatchSetup::melee_1v1(SEED, 48, 48);
    let (listener, port) = bind_ephemeral();
    let addr = format!("127.0.0.1:{port}");
    let host_setup = setup.clone();
    let host = std::thread::spawn(move || -> Result<Vec<(u64, u64)>, String> {
        let config = TransportConfig { seat: 0, pace: false, timeout_ms: 30_000 };
        let mut session =
            NetSession::host_on(listener, &host_setup, config).map_err(|e| e.to_string())?;
        let worker = session.sim.units.iter()
            .find(|unit| unit.owner == 0 && unit.def().stats.is_worker)
            .map(|unit| unit.id);
        for tick in 0..120u64 {
            let commands = if tick == 10 {
                worker
                    .map(|id| vec![Command::Move { units: vec![id], to: Vec2::new(1500.0, 1500.0) }])
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            session.tick(commands).map_err(|e| e.to_string())?;
        }
        Ok(session.hashes().to_vec())
    });

    let config = TransportConfig { seat: 1, pace: false, timeout_ms: 30_000 };
    let mut client = connect_retry(&addr, &setup, config).expect("client handshake");
    let worker = client
        .sim
        .units
        .iter()
        .find(|unit| unit.owner == 1 && unit.def().stats.is_worker)
        .map(|unit| unit.id);
    for tick in 0..120u64 {
        let commands = if tick == 10 {
            worker
                .map(|id| vec![Command::Move { units: vec![id], to: Vec2::new(2500.0, 2500.0) }])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        client.tick(commands).expect("client tick");
    }
    let host_hashes = host.join().expect("host thread").expect("host session");
    assert_eq!(client.hashes().len(), 120);
    assert_eq!(client.hashes(), &host_hashes[..], "per-tick hashes over TCP");
}

#[test]
fn net_lockstep_tcp_seat_assignment_rejects_conflicts_and_setup_mismatch() {
    let setup = MatchSetup::melee_1v1(SEED, 32, 32);
    // Hosting as seat 1 is a configuration error, rejected before binding.
    let error = NetSession::host_with(
        "127.0.0.1:0",
        &setup,
        TransportConfig { seat: 1, pace: false, timeout_ms: 1_000 },
    )
    .err()
    .expect("seat 1 cannot host");
    assert!(matches!(error, NetError::SeatConflict { local: 1, remote: 0 }));

    // Two peers with different setups must fail the handshake, not start.
    let other = MatchSetup::melee_1v1(SEED ^ 0xDEAD, 32, 32);
    let (listener, port) = bind_ephemeral();
    let addr = format!("127.0.0.1:{port}");
    let host = std::thread::spawn(move || -> Result<(), NetError> {
        let config = TransportConfig { seat: 0, pace: false, timeout_ms: 10_000 };
        NetSession::host_on(listener, &setup, config).map(|_| ())
    });
    let config = TransportConfig { seat: 1, pace: false, timeout_ms: 10_000 };
    let client = connect_retry(&addr, &other, config);
    let host_result = host.join().expect("host thread");
    let client_detail = match &client {
        Err(error) => error.to_string(),
        Ok(_) => "unexpected success".to_string(),
    };
    assert!(
        matches!(client, Err(NetError::Handshake(_))),
        "client must reject the mismatched setup: {client_detail}"
    );
    assert!(
        matches!(host_result, Err(NetError::Handshake(_))),
        "host must reject the mismatched setup, got {host_result:?}"
    );
}

#[test]
fn net_desync_detected_over_tcp() {
    let setup = MatchSetup::melee_1v1(SEED ^ 0xAB, 32, 32);
    let (listener, port) = bind_ephemeral();
    let addr = format!("127.0.0.1:{port}");
    let host_setup = setup.clone();
    let host = std::thread::spawn(move || -> Result<(), NetError> {
        let config = TransportConfig { seat: 0, pace: false, timeout_ms: 10_000 };
        let mut session = NetSession::host_on(listener, &host_setup, config)?;
        session.tick(Vec::new())?;
        session.tick(Vec::new()).map(|_| ())
    });

    let config = TransportConfig { seat: 1, pace: false, timeout_ms: 10_000 };
    let mut client = connect_retry(&addr, &setup, config).expect("client handshake");
    client.tick(Vec::new()).expect("clean first tick");
    // Deliberate local divergence: one point of state drift.
    client.sim.resources[0] += 1;
    let client_error = client.tick(Vec::new()).expect_err("client sees the desync");
    let host_error = host.join().expect("host thread").expect_err("host sees the desync");

    for error in [client_error, host_error] {
        match error {
            NetError::Desync { tick, local, remote } => {
                assert_eq!(tick, 2, "desync reported on the diverging tick");
                assert_ne!(local, remote, "both hashes are reported");
            }
            other => panic!("expected Desync, got {other:?}"),
        }
    }
}
