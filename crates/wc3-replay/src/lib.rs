//! Replay container, command encoding, checksums and a lockstep scheduler.

use std::collections::BTreeMap;
use std::fmt;

use wc3_sim::{Ability, Command, Sim, TICKS_PER_SECOND, UnitId, UnitKind};

pub const MAGIC: &[u8; 4] = b"H2RP";
pub const VERSION: u32 = 1;

#[derive(Debug)]
pub enum ReplayError {
    Io(std::io::Error),
    BadMagic,
    BadVersion(u32),
    Corrupt(&'static str),
    ChecksumMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::BadMagic => f.write_str("bad replay magic"),
            Self::BadVersion(v) => write!(f, "unsupported replay version {v}"),
            Self::Corrupt(why) => write!(f, "corrupt replay: {why}"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(
                    f,
                    "state checksum mismatch: expected {expected:016x}, got {actual:016x}"
                )
            }
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<std::io::Error> for ReplayError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub tick: u64,
    pub player: u8,
    pub command: Command,
}

#[derive(Debug, Clone)]
pub struct Replay {
    pub seed: u64,
    pub map_hash: u64,
    pub final_hash: u64,
    pub ticks: u64,
    pub records: Vec<Record>,
}

impl Replay {
    pub fn from_sim(seed: u64, map_hash: u64, sim: &Sim, records: Vec<Record>) -> Self {
        Self {
            seed,
            map_hash,
            final_hash: sim.state_hash(),
            ticks: sim.tick,
            records,
        }
    }

    pub fn write(&self, mut writer: impl std::io::Write) -> std::io::Result<()> {
        writer.write_all(MAGIC)?;
        writer.write_all(&VERSION.to_le_bytes())?;
        writer.write_all(&self.seed.to_le_bytes())?;
        writer.write_all(&self.map_hash.to_le_bytes())?;
        writer.write_all(&self.final_hash.to_le_bytes())?;
        writer.write_all(&self.ticks.to_le_bytes())?;
        writer.write_all(&(self.records.len() as u32).to_le_bytes())?;
        for record in &self.records {
            writer.write_all(&record.tick.to_le_bytes())?;
            writer.write_all(&[record.player])?;
            let data = encode_command(&record.command);
            writer.write_all(&(data.len() as u32).to_le_bytes())?;
            writer.write_all(&data)?;
        }
        Ok(())
    }

    pub fn read(mut reader: impl std::io::Read) -> Result<Self, ReplayError> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(ReplayError::BadMagic);
        }
        let version = read_u32(&mut reader)?;
        if version != VERSION {
            return Err(ReplayError::BadVersion(version));
        }
        let seed = read_u64(&mut reader)?;
        let map_hash = read_u64(&mut reader)?;
        let final_hash = read_u64(&mut reader)?;
        let ticks = read_u64(&mut reader)?;
        let count = read_u32(&mut reader)? as usize;
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let tick = read_u64(&mut reader)?;
            let mut player = [0u8; 1];
            reader.read_exact(&mut player)?;
            let len = read_u32(&mut reader)? as usize;
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data)?;
            let command = decode_command(&data)?;
            records.push(Record {
                tick,
                player: player[0],
                command,
            });
        }
        Ok(Self {
            seed,
            map_hash,
            final_hash,
            ticks,
            records,
        })
    }

    /// Re-simulate and compare the final state hash. The caller supplies the
    /// initial scenario (unit spawns / map state) because the replay stores
    /// commands, not the scenario itself.
    pub fn verify_with<F: FnOnce(&mut Sim)>(
        &self,
        width: u32,
        height: u32,
        setup: F,
    ) -> Result<u64, ReplayError> {
        let mut sim = Sim::new(self.seed, width, height);
        setup(&mut sim);
        let mut index = 0usize;
        for tick in 0..self.ticks {
            let mut commands = Vec::new();
            while index < self.records.len() && self.records[index].tick == tick {
                commands.push(self.records[index].command.clone());
                index += 1;
            }
            sim.step(&commands);
        }
        let actual = sim.state_hash();
        if actual != self.final_hash {
            return Err(ReplayError::ChecksumMismatch {
                expected: self.final_hash,
                actual,
            });
        }
        Ok(actual)
    }

    pub fn verify(&self, width: u32, height: u32) -> Result<u64, ReplayError> {
        self.verify_with(width, height, |_| {})
    }
}

pub fn encode_command(command: &Command) -> Vec<u8> {
    let mut out = Vec::new();
    match command {
        Command::Move { units, to } => {
            out.push(0);
            write_ids(&mut out, units);
            out.extend_from_slice(&to.x.to_le_bytes());
            out.extend_from_slice(&to.y.to_le_bytes());
        }
        Command::Attack { units, target } => {
            out.push(1);
            write_ids(&mut out, units);
            out.extend_from_slice(&target.0.to_le_bytes());
        }
        Command::Stop { units } => {
            out.push(2);
            write_ids(&mut out, units);
        }
        Command::Train { building, kind } => {
            out.push(3);
            out.extend_from_slice(&building.0.to_le_bytes());
            out.push(kind_tag(*kind));
        }
        Command::Build { builder, kind, at } => {
            out.push(4);
            out.extend_from_slice(&builder.0.to_le_bytes());
            out.push(kind_tag(*kind));
            out.extend_from_slice(&at.x.to_le_bytes());
            out.extend_from_slice(&at.y.to_le_bytes());
        }
        Command::Cast {
            unit,
            ability,
            target,
        } => {
            out.push(5);
            out.extend_from_slice(&unit.0.to_le_bytes());
            match ability {
                Ability::Heal(amount) => {
                    out.push(0);
                    out.extend_from_slice(&amount.to_le_bytes());
                }
                Ability::Fireball(amount) => {
                    out.push(1);
                    out.extend_from_slice(&amount.to_le_bytes());
                }
            }
            match target {
                Some(target) => {
                    out.push(1);
                    out.extend_from_slice(&target.0.to_le_bytes());
                }
                None => out.push(0),
            }
        }
        // Commands added to `wc3-sim` after the first replay format; each keeps
        // the same shape as its closest sibling so decode is lossless.
        Command::MoveAttack { units, to } => {
            out.push(6);
            write_ids(&mut out, units);
            out.extend_from_slice(&to.x.to_le_bytes());
            out.extend_from_slice(&to.y.to_le_bytes());
        }
        Command::Hold { units } => {
            out.push(7);
            write_ids(&mut out, units);
        }
        Command::Harvest { units, to } => {
            out.push(8);
            write_ids(&mut out, units);
            out.extend_from_slice(&to.x.to_le_bytes());
            out.extend_from_slice(&to.y.to_le_bytes());
        }
        Command::CancelTrain { building, index } => {
            out.push(9);
            out.extend_from_slice(&building.0.to_le_bytes());
            out.extend_from_slice(&(*index as u32).to_le_bytes());
        }
        // Commands 10-13 (AUD-030-N1): `LearnAbility` writes the ability id,
        // the other three reuse their sibling shapes. Decode interns the id back
        // to the canonical `&'static str`.
        Command::LearnAbility { unit, ability } => {
            out.push(10);
            out.extend_from_slice(&unit.0.to_le_bytes());
            let bytes = ability.as_bytes();
            out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        Command::Repair { units, target } => {
            out.push(11);
            write_ids(&mut out, units);
            out.extend_from_slice(&target.0.to_le_bytes());
        }
        Command::HarvestLumber { units, to } => {
            out.push(12);
            write_ids(&mut out, units);
            out.extend_from_slice(&to.x.to_le_bytes());
            out.extend_from_slice(&to.y.to_le_bytes());
        }
        Command::ReviveHero { hero, altar } => {
            out.push(13);
            out.extend_from_slice(&hero.0.to_le_bytes());
            out.extend_from_slice(&altar.0.to_le_bytes());
        }
    }
    out
}

pub fn decode_command(data: &[u8]) -> Result<Command, ReplayError> {
    let mut r = Reader { data, pos: 0 };
    let tag = r.u8()?;
    let command = match tag {
        0 => {
            let units = r.ids()?;
            let to = glam::Vec2::new(r.f32()?, r.f32()?);
            Command::Move { units, to }
        }
        1 => {
            let units = r.ids()?;
            Command::Attack {
                units,
                target: UnitId(r.u32()?),
            }
        }
        2 => Command::Stop { units: r.ids()? },
        3 => Command::Train {
            building: UnitId(r.u32()?),
            kind: kind_from(r.u8()?)?,
        },
        4 => {
            let builder = UnitId(r.u32()?);
            let kind = kind_from(r.u8()?)?;
            Command::Build {
                builder,
                kind,
                at: glam::Vec2::new(r.f32()?, r.f32()?),
            }
        }
        5 => {
            let unit = UnitId(r.u32()?);
            let ability = match r.u8()? {
                0 => Ability::Heal(r.i32()?),
                1 => Ability::Fireball(r.i32()?),
                _ => return Err(ReplayError::Corrupt("ability tag")),
            };
            let target = if r.u8()? == 1 {
                Some(UnitId(r.u32()?))
            } else {
                None
            };
            Command::Cast {
                unit,
                ability,
                target,
            }
        }
        6 => {
            let units = r.ids()?;
            let to = glam::Vec2::new(r.f32()?, r.f32()?);
            Command::MoveAttack { units, to }
        }
        7 => Command::Hold { units: r.ids()? },
        8 => {
            let units = r.ids()?;
            let to = glam::Vec2::new(r.f32()?, r.f32()?);
            Command::Harvest { units, to }
        }
        9 => Command::CancelTrain {
            building: UnitId(r.u32()?),
            index: r.u32()? as usize,
        },
        10 => {
            let unit = UnitId(r.u32()?);
            let raw = r.string()?;
            let ability = wc3_sim::save::intern_static_name(&raw)
                .ok_or(ReplayError::Corrupt("ability id"))?;
            Command::LearnAbility { unit, ability }
        }
        11 => Command::Repair {
            units: r.ids()?,
            target: UnitId(r.u32()?),
        },
        12 => {
            let units = r.ids()?;
            let to = glam::Vec2::new(r.f32()?, r.f32()?);
            Command::HarvestLumber { units, to }
        }
        13 => Command::ReviveHero {
            hero: UnitId(r.u32()?),
            altar: UnitId(r.u32()?),
        },
        _ => return Err(ReplayError::Corrupt("command tag")),
    };
    Ok(command)
}

fn kind_tag(kind: UnitKind) -> u8 {
    match kind {
        UnitKind::Footman => 0,
        UnitKind::Archer => 1,
        UnitKind::Peasant => 2,
        UnitKind::TownHall => 3,
    }
}

fn kind_from(tag: u8) -> Result<UnitKind, ReplayError> {
    Ok(match tag {
        0 => UnitKind::Footman,
        1 => UnitKind::Archer,
        2 => UnitKind::Peasant,
        3 => UnitKind::TownHall,
        _ => return Err(ReplayError::Corrupt("unit kind tag")),
    })
}

fn write_ids(out: &mut Vec<u8>, units: &[UnitId]) {
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.0.to_le_bytes());
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(ReplayError::Corrupt("offset"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(ReplayError::Corrupt("truncated"))?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, ReplayError> {
        Ok(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, ReplayError> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))
    }
    fn i32(&mut self) -> Result<i32, ReplayError> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))
    }
    fn f32(&mut self) -> Result<f32, ReplayError> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))
    }
    /// Length-prefixed UTF-8 (ability ids written by `encode_command`).
    fn string(&mut self) -> Result<String, ReplayError> {
        let len = u16::from_le_bytes(self.bytes(2)?.try_into().expect("2")) as usize;
        let bytes = self.bytes(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| ReplayError::Corrupt("ability id encoding"))
    }
    fn ids(&mut self) -> Result<Vec<UnitId>, ReplayError> {
        let count = u16::from_le_bytes(self.bytes(2)?.try_into().expect("2")) as usize;
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(UnitId(self.u32()?));
        }
        Ok(ids)
    }
}

fn read_u32(r: &mut impl std::io::Read) -> Result<u32, ReplayError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl std::io::Read) -> Result<u64, ReplayError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Lockstep scheduler: a command submitted for tick T is delivered at
/// T + delay, giving every peer the same fixed schedule.
#[derive(Debug, Clone)]
pub struct Lockstep {
    pub local_slot: u8,
    pub slots: u8,
    pub delay_ticks: u32,
    pub tick: u64,
    pending: BTreeMap<u64, Vec<(u8, Command)>>,
    pub checksums: BTreeMap<u64, Vec<(u8, u64)>>,
}

impl Lockstep {
    pub fn new(local_slot: u8, slots: u8, delay_ticks: u32) -> Self {
        Self {
            local_slot,
            slots,
            delay_ticks,
            tick: 0,
            pending: BTreeMap::new(),
            checksums: BTreeMap::new(),
        }
    }

    pub fn submit(&mut self, command: Command) {
        let target = self.tick + self.delay_ticks as u64;
        self.pending
            .entry(target)
            .or_default()
            .push((self.local_slot, command));
    }

    pub fn submit_remote(&mut self, slot: u8, target_tick: u64, command: Command) {
        self.pending
            .entry(target_tick)
            .or_default()
            .push((slot, command));
    }

    /// Commands scheduled for the current tick, sorted by player slot.
    pub fn commands(&mut self) -> Vec<Command> {
        let mut commands = self.pending.remove(&self.tick).unwrap_or_default();
        commands.sort_by_key(|(slot, _)| *slot);
        commands.into_iter().map(|(_, command)| command).collect()
    }

    pub fn record_checksum(&mut self, slot: u8, checksum: u64) {
        self.checksums
            .entry(self.tick)
            .or_default()
            .push((slot, checksum));
    }

    /// Returns the first divergent slot pair at any tick, if checksums disagree.
    pub fn desync(&self) -> Option<(u64, u8, u8)> {
        for (&tick, checksums) in &self.checksums {
            if checksums.len() < 2 {
                continue;
            }
            let &(slot_a, value_a) = &checksums[0];
            for &(slot_b, value_b) in &checksums[1..] {
                if value_a != value_b {
                    return Some((tick, slot_a, slot_b));
                }
            }
        }
        None
    }

    pub fn advance(&mut self) {
        self.tick += 1;
    }
}

pub fn ticks_for(seconds: f32) -> u64 {
    (seconds * TICKS_PER_SECOND as f32).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn scenario() -> (Sim, Vec<Record>, u64, u64) {
        let mut sim = Sim::new(11, 32, 32);
        let a = sim.spawn(0, wc3_sim::UnitKind::Footman, Vec2::new(200.0, 200.0));
        let b = sim.spawn(1, wc3_sim::UnitKind::Archer, Vec2::new(1200.0, 1200.0));
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
        let map_hash = 0x1234_5678_9abc_def0;
        (sim, records, final_hash, map_hash)
    }

    #[test]
    fn replay_roundtrip_and_verify() {
        let (sim, records, final_hash, map_hash) = scenario();
        let replay = Replay::from_sim(sim.seed, map_hash, &sim, records);
        let mut buffer = Vec::new();
        replay.write(&mut buffer).unwrap();
        let read = Replay::read(buffer.as_slice()).unwrap();
        assert_eq!(read.final_hash, final_hash);
        let verified = read
            .verify_with(32, 32, |sim| {
                sim.spawn(0, wc3_sim::UnitKind::Footman, Vec2::new(200.0, 200.0));
                sim.spawn(1, wc3_sim::UnitKind::Archer, Vec2::new(1200.0, 1200.0));
            })
            .unwrap();
        assert_eq!(verified, final_hash);
    }

    #[test]
    fn command_encoding_roundtrip() {
        let commands = vec![
            Command::Move {
                units: vec![UnitId(1), UnitId(2)],
                to: Vec2::new(1.5, -2.25),
            },
            Command::Attack {
                units: vec![UnitId(3)],
                target: UnitId(9),
            },
            Command::Stop {
                units: vec![UnitId(4)],
            },
            Command::Train {
                building: UnitId(5),
                kind: UnitKind::Archer,
            },
            Command::Build {
                builder: UnitId(6),
                kind: UnitKind::TownHall,
                at: Vec2::new(10.0, 20.0),
            },
            Command::Cast {
                unit: UnitId(7),
                ability: Ability::Fireball(25),
                target: Some(UnitId(8)),
            },
            // Tags 6-13: added to `wc3-sim` after the first replay format; each
            // must survive the round-trip with its exact shape (AUD-030-N1).
            Command::MoveAttack {
                units: vec![UnitId(11)],
                to: Vec2::new(-3.0, 4.5),
            },
            Command::Hold {
                units: vec![UnitId(12), UnitId(13)],
            },
            Command::Harvest {
                units: vec![UnitId(14)],
                to: Vec2::new(512.0, 512.0),
            },
            Command::CancelTrain {
                building: UnitId(15),
                index: 2,
            },
            Command::LearnAbility {
                unit: UnitId(16),
                ability: "AHhb",
            },
            Command::Repair {
                units: vec![UnitId(17)],
                target: UnitId(18),
            },
            Command::HarvestLumber {
                units: vec![UnitId(19)],
                to: Vec2::new(64.0, 128.0),
            },
            Command::ReviveHero {
                hero: UnitId(20),
                altar: UnitId(21),
            },
        ];
        for command in commands {
            let encoded = encode_command(&command);
            assert_eq!(decode_command(&encoded).unwrap(), command);
        }
    }

    #[test]
    fn unknown_ability_id_is_a_typed_error() {
        let mut encoded = vec![10u8];
        encoded.extend_from_slice(&1u32.to_le_bytes());
        let raw = b"NOTANABILITY";
        encoded.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        encoded.extend_from_slice(raw);
        assert!(matches!(
            decode_command(&encoded),
            Err(ReplayError::Corrupt("ability id"))
        ));
    }

    #[test]
    fn lockstep_delivers_after_delay() {
        let mut lockstep = Lockstep::new(0, 2, 3);
        lockstep.submit(Command::Stop {
            units: vec![UnitId(1)],
        });
        assert!(lockstep.commands().is_empty());
        lockstep.advance();
        lockstep.advance();
        assert!(lockstep.commands().is_empty());
        lockstep.advance();
        assert_eq!(lockstep.commands().len(), 1);
    }

    #[test]
    fn detects_desync() {
        let mut lockstep = Lockstep::new(0, 2, 1);
        lockstep.record_checksum(0, 10);
        lockstep.record_checksum(1, 11);
        assert_eq!(lockstep.desync(), Some((0, 0, 1)));
    }
}
