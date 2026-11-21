//! Replay container robustness: truncated and forged files are typed errors,
//! the command codec round-trips, and the record census is deterministic.
//!
//! No fixture file is needed: the replay is produced by a short simulation.

use std::cell::Cell;

use glam::Vec2;
use wc3_replay::{
    MAX_TICKS, Replay, Record, ReplayError, decode_command, encode_command,
};
use wc3_sim::{Command, Sim, UnitId, UnitKind};

fn scenario() -> (Sim, Vec<Record>, u64, u64) {
    let mut sim = Sim::new(11, 32, 32);
    let a = sim.spawn(0, UnitKind::Footman, Vec2::new(200.0, 200.0));
    let b = sim.spawn(1, UnitKind::Archer, Vec2::new(1200.0, 1200.0));
    let mut records = Vec::new();
    for tick in 0..120u64 {
        let commands = if tick == 0 {
            vec![Command::Move {
                units: vec![a],
                to: Vec2::new(900.0, 900.0),
            }]
        } else if tick == 60 {
            vec![Command::Attack {
                units: vec![a],
                target: b,
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
        }
        sim.step(&commands);
    }
    let final_hash = sim.state_hash();
    (sim, records, final_hash, 0x1234_5678_9abc_def0)
}

fn encoded_replay() -> Vec<u8> {
    let (sim, records, _, map_hash) = scenario();
    let replay = Replay::from_sim(sim.seed, map_hash, &sim, records);
    let mut bytes = Vec::new();
    replay.write(&mut bytes).expect("write");
    bytes
}

fn sample_replay(ticks: u64, records: Vec<Record>) -> Replay {
    Replay {
        seed: 1,
        map_hash: 2,
        final_hash: 3,
        ticks,
        records,
    }
}

/// Deterministic xorshift so a failure is reproducible from the seed alone.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn byte(&mut self) -> u8 {
        self.next_u64() as u8
    }
}

/// The replay's record census and header survive a round-trip, and writing the
/// parsed replay again produces byte-identical output.
#[test]
fn replay_roundtrip_census_is_deterministic() {
    let (sim, records, final_hash, map_hash) = scenario();
    let record_count = records.len();
    let replay = Replay::from_sim(sim.seed, map_hash, &sim, records);
    let mut bytes = Vec::new();
    replay.write(&mut bytes).expect("write");

    let first = Replay::read(bytes.as_slice()).expect("read");
    let second = Replay::read(bytes.as_slice()).expect("read again");
    for parsed in [&first, &second] {
        assert_eq!(parsed.records.len(), record_count);
        assert_eq!(parsed.ticks, 120);
        assert_eq!(parsed.seed, sim.seed);
        assert_eq!(parsed.map_hash, map_hash);
        assert_eq!(parsed.final_hash, final_hash);
        assert_eq!(parsed.records.first().map(|r| r.tick), Some(0));
        assert_eq!(parsed.records.last().map(|r| r.tick), Some(60));
    }

    let mut rewritten = Vec::new();
    first.write(&mut rewritten).expect("rewrite");
    assert_eq!(rewritten, bytes, "write(read(bytes)) is byte-identical");
}

/// Every truncation point is a typed error — never a panic, and never a
/// silently accepted prefix.
#[test]
fn replay_truncation_is_typed_at_every_length() {
    let bytes = encoded_replay();
    for cut in 0..bytes.len() {
        match Replay::read(&bytes[..cut]) {
            Err(_) => {}
            Ok(replay) => panic!("truncation at {cut} parsed {} records", replay.records.len()),
        }
    }
    assert!(Replay::read(bytes.as_slice()).is_ok());
}

/// Forged header fields are bounded before they can size an allocation or a
/// loop: count, tick count, command length and trailing bytes.
#[test]
fn replay_forged_header_fields_are_bounded() {
    let bytes = encoded_replay();

    let mut forged = bytes.clone();
    forged[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Replay::read(forged.as_slice()),
        Err(ReplayError::Corrupt("record count"))
    ));

    let mut forged = bytes.clone();
    forged[32..40].copy_from_slice(&(MAX_TICKS + 1).to_le_bytes());
    assert!(matches!(
        Replay::read(forged.as_slice()),
        Err(ReplayError::Corrupt("tick count"))
    ));

    // First record: tick (8) + player (1) + command length (4).
    let mut forged = bytes.clone();
    forged[53..57].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Replay::read(forged.as_slice()),
        Err(ReplayError::Corrupt("command length"))
    ));

    let mut forged = bytes;
    forged.push(0);
    assert!(matches!(
        Replay::read(forged.as_slice()),
        Err(ReplayError::Corrupt("trailing bytes"))
    ));
}

/// A record tick that would never be simulated (at or past `end_tick`) or that
/// breaks file order is refused by the reader and by the writer.
#[test]
fn replay_record_order_is_enforced_both_ways() {
    let record = |tick: u64, player: u8, units: u32| Record {
        tick,
        player,
        command: Command::Stop {
            units: vec![UnitId(units)],
        },
    };

    let ordered = sample_replay(10, vec![record(5, 0, 1), record(5, 1, 2)]);
    let mut bytes = Vec::new();
    ordered.write(&mut bytes).expect("equal ticks are ordered");
    assert_eq!(Replay::read(bytes.as_slice()).expect("read").records.len(), 2);

    let unordered = sample_replay(10, vec![record(5, 0, 1), record(3, 1, 2)]);
    assert!(unordered.write(&mut Vec::new()).is_err());
    // The reader sees the same defect when the order is forged in the bytes.
    let mut forged = Vec::new();
    sample_replay(10, vec![record(1, 0, 1), record(2, 1, 2)])
        .write(&mut forged)
        .expect("write well-ordered");
    forged[44..52].copy_from_slice(&9u64.to_le_bytes());
    assert!(matches!(
        Replay::read(forged.as_slice()),
        Err(ReplayError::Corrupt("record ticks are not ordered"))
    ));

    let outside = sample_replay(10, vec![record(10, 0, 1)]);
    assert!(outside.write(&mut Vec::new()).is_err());
}

/// The duration limit is checked before the setup closure or the simulation
/// are touched, and it reports both numbers.
#[test]
fn replay_duration_limit_refuses_before_any_work() {
    let replay = sample_replay(1_000, Vec::new());
    let setup_ran = Cell::new(false);
    match replay.verify_with_limit(32, 32, 10, |_| setup_ran.set(true)) {
        Err(ReplayError::Duration { ticks, limit }) => {
            assert_eq!((ticks, limit), (1_000, 10));
        }
        other => panic!("expected a duration error, got {other:?}"),
    }
    assert!(!setup_ran.get(), "setup closure must not run");

    // The external bound cannot widen the crate's own ceiling.
    let replay = sample_replay(MAX_TICKS + 1, Vec::new());
    assert!(matches!(
        replay.verify_with_limit(32, 32, u64::MAX, |_| {}),
        Err(ReplayError::Duration { .. })
    ));
}

/// The command codec rejects unknown tags and trailing bytes, and arbitrary
/// bytes are a typed error rather than a panic.
#[test]
fn replay_command_codec_is_total() {
    assert!(matches!(
        decode_command(&[255]),
        Err(ReplayError::Corrupt("command tag"))
    ));

    let mut encoded = encode_command(&Command::Stop {
        units: vec![UnitId(1), UnitId(2)],
    });
    encoded.push(0xAA);
    assert!(matches!(
        decode_command(&encoded),
        Err(ReplayError::Corrupt("trailing command bytes"))
    ));

    let mut rng = Rng(0xA5A5_5A5A_0123_4567);
    for len in 0..64usize {
        for _ in 0..32 {
            let data: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
            let _ = decode_command(&data);
        }
    }
    for tag in 17u8..=255 {
        assert!(matches!(
            decode_command(&[tag]),
            Err(ReplayError::Corrupt("command tag"))
        ));
    }

    // A forged unit-id count is bounded by the bytes present, not by the
    // announced 65 535.
    assert!(matches!(
        decode_command(&[2, 0xFF, 0xFF]),
        Err(ReplayError::Corrupt("unit id count"))
    ));
    let empty = encode_command(&Command::Stop { units: Vec::new() });
    assert!(matches!(
        decode_command(&empty),
        Ok(Command::Stop { units }) if units.is_empty()
    ));
}
