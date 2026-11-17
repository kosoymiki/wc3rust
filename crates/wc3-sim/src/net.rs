//! Deterministic lockstep transport and replay for the simulation
//! (mechanics `net.lockstep`, `net.desync`, `net.replay`).
//!
//! The simulation is already deterministic given `(seed, command sequence)`,
//! so multiplayer needs no state replication: every peer runs the same `Sim`,
//! and the network carries only *command frames*. A frame is
//!
//! ```text
//! CommandFrame { tick, player, commands }
//! ```
//!
//! where `tick` is the simulation tick the commands apply to. Frames are
//! executed in a total order that every peer computes identically: by `tick`,
//! then by `player`, then by arrival sequence. A peer never steps a tick while a
//! frame for that tick is still outstanding, which is what makes the lockstep:
//! tick *N* is simulated by everyone before tick *N+1* is released.
//!
//! Divergence is detected by exchanging the post-tick [`Sim::state_hash`]: the
//! first tick whose hashes differ is reported as [`NetError::Desync`] with both
//! hashes, and the match stops instead of silently drifting.
//!
//! Time base: 30 Hz (`LOCKSTEP_HZ`). [`FrameClock`] converts elapsed wall-clock
//! milliseconds into the number of ticks due — the simulation itself never
//! reads a clock, so replay and lockstep share one deterministic path.
//!
//! Replays are the same command frames plus the match setup, the seed, and the
//! start/end hashes. [`Replay::verify`] re-simulates the match from the setup
//! and fails if the reproduced hash differs, so a replay is proof rather than a
//! recording.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use glam::Vec2;

use crate::data::Race;
use crate::save::SaveError;
use crate::{Command, Ruleset, Sim};

/// Lockstep tick rate (WC3 melee timing, 30 ticks per second).
pub const LOCKSTEP_HZ: u32 = 30;
/// Milliseconds in one lockstep tick, as an exact rational helper.
pub const MICROS_PER_TICK: u64 = 1_000_000 / LOCKSTEP_HZ as u64;
/// Replay file magic.
pub const REPLAY_MAGIC: [u8; 4] = *b"W3RP";
/// Replay layout version written by this build.
pub const REPLAY_VERSION: u16 = 1;
const ENVELOPE_HEADER: usize = 12;
const ENVELOPE_CHECKSUM: usize = 8;
/// Guard against a corrupted replay allocating without bound.
const MAX_FRAMES: u32 = 2_000_000;
/// Guard against a corrupted match setup.
const MAX_MAP_CELLS: u32 = 1024;
/// Guard against a corrupted scene entry count.
const MAX_SCENE_ENTRIES: u32 = 100_000;

/// Every failure mode of the lockstep/replay transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// Replay magic is wrong.
    BadMagic([u8; 4]),
    /// Replay version is not this build's version.
    UnknownVersion(u16),
    /// Fewer bytes than the structure needs.
    Truncated { needed: usize, available: usize },
    /// Declared payload length disagrees with the file size.
    LengthMismatch { declared: usize, actual: usize },
    /// Payload checksum mismatch.
    Checksum { expected: u64, actual: u64 },
    /// Unknown tag in a tag-prefixed structure.
    BadTag { section: &'static str, tag: u32 },
    /// A stored id is not a known data id.
    UnknownName { section: &'static str, name: String },
    /// A scalar is outside its legal domain.
    InvalidValue { field: &'static str, detail: String },
    /// Bytes remained after the end of a structure.
    TrailingBytes(usize),
    /// A frame was submitted for a tick that has already been simulated.
    LateFrame { tick: u64, current: u64 },
    /// Two frames from the same player for the same tick.
    DuplicateFrame { tick: u64, player: u8 },
    /// Player index outside the simulation's two seats.
    PlayerOutOfRange(u8),
    /// Peers produced different state hashes on the same tick.
    Desync { tick: u64, local: u64, remote: u64 },
    /// A recorded hash does not match the re-simulated hash.
    HashMismatch {
        tick: u64,
        expected: u64,
        actual: u64,
    },
    /// A recorded frame was never consumed by the replay window.
    UnconsumedFrame { tick: u64 },
    /// The match setup cannot be instantiated.
    Setup(String),
    /// Command bytes inside a frame are malformed.
    Command(String),
    /// Filesystem failure.
    Io(String),
    /// Socket failure (connect, read, write, reset).
    Transport(String),
    /// The peer closed the connection mid-match.
    Closed,
    /// Handshake rejected (wrong magic/version, setup digest mismatch).
    Handshake(String),
    /// Both sides claimed the same seat.
    SeatConflict { local: u8, remote: u8 },
    /// A message arrived out of order or with an impossible field.
    Protocol(String),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic(found) => write!(f, "not a W3RP replay (magic {found:?})"),
            Self::UnknownVersion(v) => write!(
                f,
                "unsupported replay version {v} (this build reads {REPLAY_VERSION})"
            ),
            Self::Truncated { needed, available } => {
                write!(
                    f,
                    "truncated payload: needed {needed} bytes, have {available}"
                )
            }
            Self::LengthMismatch { declared, actual } => write!(
                f,
                "payload length mismatch: declared {declared}, file size implies {actual}"
            ),
            Self::Checksum { expected, actual } => write!(
                f,
                "payload checksum mismatch: stored {expected:#x}, computed {actual:#x}"
            ),
            Self::BadTag { section, tag } => {
                write!(f, "unknown tag {tag:#x} in {section}")
            }
            Self::UnknownName { section, name } => {
                write!(f, "unknown id {name:?} in {section}")
            }
            Self::InvalidValue { field, detail } => write!(f, "invalid {field}: {detail}"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes"),
            Self::LateFrame { tick, current } => write!(
                f,
                "frame for tick {tick} arrived after tick {current} was simulated"
            ),
            Self::DuplicateFrame { tick, player } => {
                write!(f, "duplicate frame for tick {tick} from player {player}")
            }
            Self::PlayerOutOfRange(player) => write!(f, "player {player} is not a seat"),
            Self::Desync {
                tick,
                local,
                remote,
            } => write!(
                f,
                "desync at tick {tick}: local hash {local:#018x}, remote hash {remote:#018x}"
            ),
            Self::HashMismatch {
                tick,
                expected,
                actual,
            } => write!(
                f,
                "hash mismatch after tick {tick}: recorded {expected:#018x}, reproduced {actual:#018x}"
            ),
            Self::UnconsumedFrame { tick } => {
                write!(f, "frame for tick {tick} was never consumed by the replay")
            }
            Self::Setup(detail) => write!(f, "match setup: {detail}"),
            Self::Command(detail) => write!(f, "command frame: {detail}"),
            Self::Io(detail) => write!(f, "io error: {detail}"),
            Self::Transport(detail) => write!(f, "transport: {detail}"),
            Self::Closed => write!(f, "peer closed the connection"),
            Self::Handshake(detail) => write!(f, "handshake rejected: {detail}"),
            Self::SeatConflict { local, remote } => {
                write!(f, "seat conflict: local {local}, remote {remote}")
            }
            Self::Protocol(detail) => write!(f, "protocol error: {detail}"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<SaveError> for NetError {
    fn from(error: SaveError) -> Self {
        Self::Command(error.to_string())
    }
}

impl From<std::io::Error> for NetError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Result alias for lockstep operations.
pub type NetResult<T> = Result<T, NetError>;

// ---------------------------------------------------------------------------
// Frame-local byte writer / reader
// ---------------------------------------------------------------------------

struct Fw {
    buf: Vec<u8>,
}

impl Fw {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.u32(value as u32);
    }

    fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn len(&mut self, value: usize) {
        self.u32(u32::try_from(value).expect("section length fits in u32"));
    }

    fn str(&mut self, value: &str) {
        self.len(value.len());
        self.buf.extend_from_slice(value.as_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.buf.extend_from_slice(value);
    }

    fn v2(&mut self, value: Vec2) {
        self.f32(value.x);
        self.f32(value.y);
    }
}

struct Fr<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Fr<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, count: usize) -> NetResult<&'a [u8]> {
        if count > self.remaining() {
            return Err(NetError::Truncated {
                needed: self.pos + count,
                available: self.buf.len(),
            });
        }
        let slice = &self.buf[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn u8(&mut self) -> NetResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> NetResult<u16> {
        let slice = self.take(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn u32(&mut self) -> NetResult<u32> {
        let slice = self.take(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    fn u64(&mut self) -> NetResult<u64> {
        let slice = self.take(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(slice);
        Ok(u64::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> NetResult<i32> {
        Ok(self.u32()? as i32)
    }

    fn bool(&mut self, field: &'static str) -> NetResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(NetError::InvalidValue {
                field,
                detail: format!("boolean tag {other}"),
            }),
        }
    }

    fn len(&mut self, limit: u32) -> NetResult<usize> {
        let count = self.u32()?;
        if count > limit {
            return Err(NetError::InvalidValue {
                field: "length",
                detail: format!("{count} exceeds {limit}"),
            });
        }
        let count = count as usize;
        if count > self.remaining() {
            return Err(NetError::Truncated {
                needed: self.pos + count,
                available: self.buf.len(),
            });
        }
        Ok(count)
    }

    fn f32(&mut self, field: &'static str) -> NetResult<f32> {
        let bits = self.u32()?;
        let value = f32::from_bits(bits);
        if !value.is_finite() {
            return Err(NetError::InvalidValue {
                field,
                detail: format!("non-finite float {value}"),
            });
        }
        Ok(value)
    }

    fn string(&mut self) -> NetResult<String> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| NetError::InvalidValue {
            field: "string",
            detail: "not valid UTF-8".to_string(),
        })
    }

    fn v2(&mut self) -> NetResult<Vec2> {
        Ok(Vec2::new(self.f32("vec2.x")?, self.f32("vec2.y")?))
    }

    fn bytes(&mut self) -> NetResult<&'a [u8]> {
        let count = self.len(u32::MAX)?;
        self.take(count)
    }
}

// ---------------------------------------------------------------------------
// Command frames
// ---------------------------------------------------------------------------

/// One player's commands for one simulation tick.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandFrame {
    /// Tick the commands apply to (`Sim::tick` before stepping).
    pub tick: u64,
    /// Seat that issued the commands (0 or 1).
    pub player: u8,
    /// Commands in issue order.
    pub commands: Vec<Command>,
}

impl CommandFrame {
    /// Builds a frame.
    pub fn new(tick: u64, player: u8, commands: Vec<Command>) -> Self {
        Self {
            tick,
            player,
            commands,
        }
    }

    /// Number of commands in the frame.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// True when the frame carries no commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Serializes the frame: `tick u64, player u8, count u32, [len u32 + bytes]*`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Fw::new();
        w.u64(self.tick);
        w.u8(self.player);
        w.len(self.commands.len());
        for command in &self.commands {
            w.bytes(&crate::save::command_encode(command));
        }
        w.buf
    }

    /// Parses a frame produced by [`CommandFrame::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> NetResult<Self> {
        let mut r = Fr::new(bytes);
        let tick = r.u64()?;
        let player = r.u8()?;
        let count = r.len(u32::MAX)?;
        let mut commands = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let raw = r.bytes()?;
            commands.push(crate::save::command_decode(raw)?);
        }
        if r.remaining() != 0 {
            return Err(NetError::TrailingBytes(r.remaining()));
        }
        if player > 1 {
            return Err(NetError::PlayerOutOfRange(player));
        }
        Ok(Self {
            tick,
            player,
            commands,
        })
    }
}

// ---------------------------------------------------------------------------
// Match setup
// ---------------------------------------------------------------------------

/// Plain-data scene extracted from a real map: `sloc` starts, gold mines,
/// placed units, destructables and the native WPM pathing grid.
///
/// Carried inside [`MatchSetup`] so a replay is self-contained and two peers can
/// prove they play the same scene ([`MapScene::digest`]) without transferring the
/// map archive over the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct MapScene {
    pub width: u32,
    pub height: u32,
    pub cell_size: f32,
    /// Row-major walkability, `width * height` cells (WPM at 32-unit cells).
    pub walkable: Vec<bool>,
    /// `sloc` start locations in local map space, in placement order.
    pub starts: Vec<Vec2>,
    /// `ngol`/`ugol` gold mines.
    pub mines: Vec<Vec2>,
    /// `(def id, owner, position)` for every placed unit the data layer knows.
    pub units: Vec<(String, u8, Vec2)>,
    /// `(destructable id, position)` from `war3map.doo`.
    pub destructables: Vec<(String, Vec2)>,
    /// Starting force each seat gets at its own `sloc` marker.
    pub seat_force: [SeatForce; 2],
}

/// Workers and fighters a seat starts with at its own start marker.
///
/// A library melee map carries no player-owned units (every placed unit is a
/// creep), so the 1v1 scenario spawns the standard WC3 opening at each `sloc`:
/// one main hall plus this force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatForce {
    pub workers: usize,
    pub fighters: usize,
}

impl Default for SeatForce {
    fn default() -> Self {
        Self {
            workers: 3,
            fighters: 2,
        }
    }
}

impl MapScene {
    /// Extracts the scene from a `.w3m`/`.w3x` through the canonical
    /// `wc3-re::mapfiles::MapBundle`.
    ///
    /// Mirrors `wc3_game::sim_bridge::spawn_map_scene`: pathing comes from
    /// `war3map.wpm` at its native 32-unit cells, placed positions are
    /// translated by the terrain origin (`.w3e` centre), `sloc` markers become
    /// starts, `ngol`/`ugol` become gold mines, and only def-covered unit ids
    /// are kept (custom object-data units are the object database's job).
    pub fn load(path: impl AsRef<Path>) -> NetResult<Self> {
        let path = path.as_ref();
        let bundle = wc3_re::mapfiles::MapBundle::open(path)
            .map_err(|error| NetError::Setup(format!("map {}: {error}", path.display())))?;
        let pathing = bundle
            .pathing
            .as_ref()
            .ok_or_else(|| NetError::Setup(format!("map {} has no war3map.wpm", path.display())))?;
        let origin = bundle
            .terrain
            .as_ref()
            .map(|terrain| (terrain.center.0, terrain.center.1))
            .unwrap_or((0.0, 0.0));
        let to_local = |x: f32, y: f32| Vec2::new(x - origin.0, y - origin.1);

        let mut walkable = Vec::with_capacity(pathing.width as usize * pathing.height as usize);
        for y in 0..pathing.height {
            for x in 0..pathing.width {
                walkable.push(pathing.walkable(x, y));
            }
        }

        let mut starts = Vec::new();
        let mut mines = Vec::new();
        let mut units = Vec::new();
        for unit in bundle.units.iter().flatten() {
            let def_id = String::from_utf8_lossy(&unit.type_id).to_string();
            let pos = to_local(unit.position.x, unit.position.y);
            match def_id.as_str() {
                "sloc" => starts.push(pos),
                "ngol" | "ugol" => mines.push(pos),
                _ => {
                    if crate::data::def_of(&def_id).is_some() {
                        let owner = if unit.player <= 1 {
                            unit.player as u8
                        } else {
                            2
                        };
                        units.push((def_id, owner, pos));
                    }
                }
            }
        }
        if starts.len() < 2 {
            return Err(NetError::Setup(format!(
                "map {} has {} start locations, 1v1 needs 2",
                path.display(),
                starts.len()
            )));
        }

        let mut destructables = Vec::new();
        for doodad in bundle.doodads.iter().flatten() {
            let def_id = String::from_utf8_lossy(&doodad.type_id).to_string();
            if crate::data::destructable_def(&def_id).is_some() {
                destructables.push((def_id, to_local(doodad.position.x, doodad.position.y)));
            }
        }

        Ok(Self {
            width: pathing.width,
            height: pathing.height,
            cell_size: crate::PATHING_CELL_SIZE,
            walkable,
            starts,
            mines,
            units,
            destructables,
            seat_force: [SeatForce::default(), SeatForce::default()],
        })
    }

    /// Drops neutral creeps, turning the map into a creep-free melee variant
    /// while keeping its starts, pathing, mines and destructables.
    pub fn clear_neutrals(&mut self) {
        self.units.retain(|(_, owner, _)| *owner < 2);
    }

    /// Start location for a seat (seat 0 = first `sloc`, seat 1 = second).
    pub fn start(&self, seat: u8) -> Option<Vec2> {
        self.starts.get(seat as usize).copied()
    }

    /// Stable digest of every scene field; two peers compare it in the
    /// handshake so a mismatched map fails before tick 0.
    pub fn digest(&self) -> u64 {
        let mut w = Fw::new();
        write_scene(&mut w, self);
        checksum(&w.buf)
    }
}

/// How a fresh match is laid out. Every peer and every replay uses the same
/// setup, so the starting `state_hash` is identical by construction.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchLayout {
    /// Empty map; tests place units explicitly.
    Empty { width: u32, height: u32 },
    /// Standard 1v1 melee opening at 25%/75% of the map.
    Melee1v1 {
        width: u32,
        height: u32,
        player_race: Race,
        enemy_race: Race,
    },
    /// 1v1 on a real map: both seats are anchored at the map's own `sloc`
    /// starts and the map's units/mines/destructables/pathing are loaded.
    Map(MapScene),
}

/// Everything needed to instantiate a deterministic match.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchSetup {
    pub seed: u64,
    pub ruleset: Ruleset,
    pub passive_income: bool,
    pub layout: MatchLayout,
}

impl MatchSetup {
    /// Standard 1v1 melee setup on a `width x height` cell map.
    pub fn melee_1v1(seed: u64, width: u32, height: u32) -> Self {
        Self {
            seed,
            ruleset: Ruleset::Tft,
            passive_income: false,
            layout: MatchLayout::Melee1v1 {
                width,
                height,
                player_race: Race::Human,
                enemy_race: Race::Human,
            },
        }
    }

    /// Empty map for tests that build their own scene.
    pub fn empty(seed: u64, width: u32, height: u32) -> Self {
        Self {
            seed,
            ruleset: Ruleset::Tft,
            passive_income: false,
            layout: MatchLayout::Empty { width, height },
        }
    }

    /// 1v1 on a real map scene (starts, mines, units, pathing).
    pub fn on_map(seed: u64, scene: MapScene) -> Self {
        Self {
            seed,
            ruleset: Ruleset::Tft,
            passive_income: false,
            layout: MatchLayout::Map(scene),
        }
    }

    /// Convenience: extract the scene from a map file and build the setup.
    pub fn from_map_file(seed: u64, path: impl AsRef<Path>) -> NetResult<Self> {
        Ok(Self::on_map(seed, MapScene::load(path)?))
    }

    /// Map width in pathing cells.
    pub fn width(&self) -> u32 {
        match &self.layout {
            MatchLayout::Empty { width, .. } => *width,
            MatchLayout::Melee1v1 { width, .. } => *width,
            MatchLayout::Map(scene) => scene.width,
        }
    }

    /// Map height in pathing cells.
    pub fn height(&self) -> u32 {
        match &self.layout {
            MatchLayout::Empty { height, .. } => *height,
            MatchLayout::Melee1v1 { height, .. } => *height,
            MatchLayout::Map(scene) => scene.height,
        }
    }

    /// The map scene, when the layout is a real map.
    pub fn scene(&self) -> Option<&MapScene> {
        match &self.layout {
            MatchLayout::Map(scene) => Some(scene),
            _ => None,
        }
    }

    /// Validates dimensions before allocating a grid.
    pub fn validate(&self) -> NetResult<()> {
        let (width, height) = (self.width(), self.height());
        if width < 8 || height < 8 {
            return Err(NetError::Setup(format!(
                "map {width}x{height} is smaller than 8x8"
            )));
        }
        if width > MAX_MAP_CELLS || height > MAX_MAP_CELLS {
            return Err(NetError::Setup(format!(
                "map {width}x{height} exceeds {MAX_MAP_CELLS} cells per axis"
            )));
        }
        if let Some(scene) = self.scene() {
            if scene.walkable.len() != width as usize * height as usize {
                return Err(NetError::Setup(format!(
                    "scene walkable is {} cells, {width}x{height} needs {}",
                    scene.walkable.len(),
                    width as usize * height as usize
                )));
            }
            if scene.starts.len() < 2 {
                return Err(NetError::Setup(
                    "scene has fewer than two start locations".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Instantiates the match.
    pub fn build(&self) -> NetResult<Sim> {
        self.validate()?;
        let mut sim = Sim::new(self.seed, self.width(), self.height());
        sim.ruleset = self.ruleset;
        sim.passive_income = self.passive_income;
        match &self.layout {
            MatchLayout::Empty { .. } => {}
            MatchLayout::Melee1v1 {
                player_race,
                enemy_race,
                ..
            } => {
                let world_width = sim.grid.width as f32 * sim.grid.cell_size;
                let world_height = sim.grid.height as f32 * sim.grid.cell_size;
                sim.setup_melee_1v1(
                    *player_race,
                    *enemy_race,
                    Vec2::new(world_width * 0.25, world_height * 0.5),
                    Vec2::new(world_width * 0.75, world_height * 0.5),
                )
                .map_err(|error| NetError::Setup(error.to_string()))?;
            }
            MatchLayout::Map(scene) => {
                sim.set_pathing(
                    scene.width,
                    scene.height,
                    scene.cell_size,
                    scene.walkable.clone(),
                );
                for mine in &scene.mines {
                    sim.add_gold_mine(*mine, crate::GOLD_MINE_GOLD);
                }
                for (def_id, owner, pos) in &scene.units {
                    sim.spawn_def_exact(*owner, def_id, *pos);
                }
                for (def_id, pos) in &scene.destructables {
                    sim.spawn_destructable(def_id, *pos);
                }
                // Both seats fight from the map's own start markers: a seat that
                // the map left without a main hall or workers gets the minimum
                // base *at its own sloc*, never at an invented position.
                for seat in 0..2u8 {
                    let start = scene.starts[seat as usize];
                    ensure_seat_base(&mut sim, seat, start, scene.seat_force[seat as usize]);
                }
            }
        }
        Ok(sim)
    }

    /// Encoded setup (used by the handshake, the digest and replays).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Fw::new();
        write_setup(&mut w, self);
        w.buf
    }

    /// Stable digest of the whole setup; the transport compares it at
    /// handshake so a mismatched map/config fails before tick 0.
    pub fn digest(&self) -> u64 {
        checksum(&self.to_bytes())
    }

    fn read(r: &mut Fr<'_>) -> NetResult<Self> {
        let seed = r.u64()?;
        let ruleset = match r.u8()? {
            0 => Ruleset::Roc,
            1 => Ruleset::Tft,
            other => {
                return Err(NetError::BadTag {
                    section: "setup.ruleset",
                    tag: u32::from(other),
                });
            }
        };
        let passive_income = r.bool("setup.passive_income")?;
        let layout = match r.u8()? {
            0 => MatchLayout::Empty {
                width: r.u32()?,
                height: r.u32()?,
            },
            1 => MatchLayout::Melee1v1 {
                width: r.u32()?,
                height: r.u32()?,
                player_race: race_from(r.u8()?)?,
                enemy_race: race_from(r.u8()?)?,
            },
            2 => MatchLayout::Map(read_scene(r)?),
            other => {
                return Err(NetError::BadTag {
                    section: "setup.layout",
                    tag: u32::from(other),
                });
            }
        };
        let setup = Self {
            seed,
            ruleset,
            passive_income,
            layout,
        };
        setup.validate()?;
        Ok(setup)
    }
}

fn write_setup(w: &mut Fw, setup: &MatchSetup) {
    w.u64(setup.seed);
    w.u8(match setup.ruleset {
        Ruleset::Roc => 0,
        Ruleset::Tft => 1,
    });
    w.bool(setup.passive_income);
    match &setup.layout {
        MatchLayout::Empty { width, height } => {
            w.u8(0);
            w.u32(*width);
            w.u32(*height);
        }
        MatchLayout::Melee1v1 {
            width,
            height,
            player_race,
            enemy_race,
        } => {
            w.u8(1);
            w.u32(*width);
            w.u32(*height);
            w.u8(race_tag(*player_race));
            w.u8(race_tag(*enemy_race));
        }
        MatchLayout::Map(scene) => {
            w.u8(2);
            write_scene(w, scene);
        }
    }
}

fn write_scene(w: &mut Fw, scene: &MapScene) {
    w.u32(scene.width);
    w.u32(scene.height);
    w.f32(scene.cell_size);
    write_bitmap(w, &scene.walkable);
    w.len(scene.starts.len());
    for start in &scene.starts {
        w.v2(*start);
    }
    w.len(scene.mines.len());
    for mine in &scene.mines {
        w.v2(*mine);
    }
    w.len(scene.units.len());
    for (def_id, owner, pos) in &scene.units {
        w.str(def_id);
        w.u8(*owner);
        w.v2(*pos);
    }
    w.len(scene.destructables.len());
    for (def_id, pos) in &scene.destructables {
        w.str(def_id);
        w.v2(*pos);
    }
    for force in &scene.seat_force {
        w.u32(force.workers as u32);
        w.u32(force.fighters as u32);
    }
}

fn read_scene(r: &mut Fr<'_>) -> NetResult<MapScene> {
    let width = r.u32()?;
    let height = r.u32()?;
    if width == 0 || height == 0 || width > MAX_MAP_CELLS || height > MAX_MAP_CELLS {
        return Err(NetError::Setup(format!(
            "scene {width}x{height} is invalid"
        )));
    }
    let cell_size = r.f32("scene.cell_size")?;
    let walkable = read_bitmap(r, width as usize * height as usize)?;
    let start_count = r.len(MAX_SCENE_ENTRIES)?;
    let mut starts = Vec::with_capacity(start_count);
    for _ in 0..start_count {
        starts.push(r.v2()?);
    }
    let mine_count = r.len(MAX_SCENE_ENTRIES)?;
    let mut mines = Vec::with_capacity(mine_count);
    for _ in 0..mine_count {
        mines.push(r.v2()?);
    }
    let unit_count = r.len(MAX_SCENE_ENTRIES)?;
    let mut units = Vec::with_capacity(unit_count);
    for _ in 0..unit_count {
        units.push((r.string()?, r.u8()?, r.v2()?));
    }
    let doodad_count = r.len(MAX_SCENE_ENTRIES)?;
    let mut destructables = Vec::with_capacity(doodad_count);
    for _ in 0..doodad_count {
        destructables.push((r.string()?, r.v2()?));
    }
    let mut seat_force = [SeatForce::default(), SeatForce::default()];
    for force in seat_force.iter_mut() {
        force.workers = r.u32()? as usize;
        force.fighters = r.u32()? as usize;
        if force.workers > 32 || force.fighters > 32 {
            return Err(NetError::InvalidValue {
                field: "scene.seat_force",
                detail: "a seat force above 32 is not a melee opening".to_string(),
            });
        }
    }
    Ok(MapScene {
        width,
        height,
        cell_size,
        walkable,
        starts,
        mines,
        units,
        destructables,
        seat_force,
    })
}

/// Workers and fighters a seat gets at its own start marker.
///
/// Melee maps carry no player-owned units — every placed unit is a creep — so a
/// 1v1 needs the standard WC3 opening, spawned *at the map's `sloc` marker*:
/// one main hall, three workers and two basic fighters. Units the map already
/// placed for the seat count toward those totals.
fn ensure_seat_base(sim: &mut Sim, seat: u8, start: Vec2, force: SeatForce) {
    let race = sim
        .units_of(seat)
        .next()
        .map(|unit| unit.def().race)
        .unwrap_or(Race::Human);
    let has_hall = sim
        .units_of(seat)
        .any(|unit| unit.alive && unit.def().main_hall);
    if !has_hall {
        if let Some(hall) = hall_def_for(race) {
            if let Some(pos) = sim.walkable_pos_near(start, 12) {
                sim.spawn_def(seat, hall, pos);
            }
        }
    }
    let workers = sim
        .units_of(seat)
        .filter(|unit| unit.alive && unit.def().stats.is_worker)
        .count();
    if let Some(worker) = worker_def_for(race) {
        for index in workers..force.workers {
            let offset = Vec2::new(96.0 + index as f32 * 48.0, 96.0);
            if let Some(pos) = sim.walkable_pos_near(start + offset, 8) {
                sim.spawn_def(seat, worker, pos);
            }
        }
    }
    let fighters = sim
        .units_of(seat)
        .filter(|unit| unit.alive && !unit.is_building() && !unit.def().stats.is_worker)
        .count();
    let fighter = crate::basic_unit_for(race);
    for index in fighters..force.fighters {
        let offset = Vec2::new(-96.0 - index as f32 * 48.0, 128.0);
        if let Some(pos) = sim.walkable_pos_near(start + offset, 8) {
            sim.spawn_def(seat, fighter, pos);
        }
    }
}

fn hall_def_for(race: Race) -> Option<&'static str> {
    crate::data::ROSTER
        .iter()
        .find(|def| def.race == race && def.main_hall)
        .map(|def| def.id)
}

fn worker_def_for(race: Race) -> Option<&'static str> {
    crate::data::ROSTER
        .iter()
        .find(|def| def.race == race && def.stats.is_worker)
        .map(|def| def.id)
}

fn race_tag(race: Race) -> u8 {
    match race {
        Race::Human => 0,
        Race::Orc => 1,
        Race::Undead => 2,
        Race::NightElf => 3,
        Race::Neutral => 4,
    }
}

fn race_from(tag: u8) -> NetResult<Race> {
    match tag {
        0 => Ok(Race::Human),
        1 => Ok(Race::Orc),
        2 => Ok(Race::Undead),
        3 => Ok(Race::NightElf),
        4 => Ok(Race::Neutral),
        other => Err(NetError::BadTag {
            section: "setup.race",
            tag: u32::from(other),
        }),
    }
}

// ---------------------------------------------------------------------------
// Lockstep client
// ---------------------------------------------------------------------------

/// One peer: its own `Sim`, its seat, and the frames it has accepted but not
/// yet simulated.
pub struct LockstepClient {
    pub sim: Sim,
    pub player: u8,
    queue: BTreeMap<(u64, u8, u64), CommandFrame>,
    sequence: u64,
    log: Vec<CommandFrame>,
    hashes: Vec<(u64, u64)>,
}

impl LockstepClient {
    /// Wraps a simulation as a lockstep peer.
    pub fn new(sim: Sim, player: u8) -> NetResult<Self> {
        if player > 1 {
            return Err(NetError::PlayerOutOfRange(player));
        }
        Ok(Self {
            sim,
            player,
            queue: BTreeMap::new(),
            sequence: 0,
            log: Vec::new(),
            hashes: Vec::new(),
        })
    }

    /// Accepts a frame for a future (or the current) tick. Frames for the
    /// current tick are applied by the next [`LockstepClient::tick`].
    pub fn schedule(&mut self, frame: CommandFrame) -> NetResult<()> {
        if frame.player > 1 {
            return Err(NetError::PlayerOutOfRange(frame.player));
        }
        if frame.tick < self.sim.tick {
            return Err(NetError::LateFrame {
                tick: frame.tick,
                current: self.sim.tick,
            });
        }
        let key = (frame.tick, frame.player, self.sequence);
        if self
            .queue
            .keys()
            .any(|(tick, player, _)| *tick == frame.tick && *player == frame.player)
        {
            return Err(NetError::DuplicateFrame {
                tick: frame.tick,
                player: frame.player,
            });
        }
        self.sequence += 1;
        self.queue.insert(key, frame);
        Ok(())
    }

    /// Frames waiting for a future tick.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Frames already simulated, in execution order.
    pub fn log(&self) -> &[CommandFrame] {
        &self.log
    }

    /// `(tick, hash)` after every simulated tick, in order. The tick is the
    /// post-step value (`sim.tick` after `step`).
    pub fn hashes(&self) -> &[(u64, u64)] {
        &self.hashes
    }

    /// Current state hash.
    pub fn hash(&self) -> u64 {
        self.sim.state_hash()
    }

    /// Simulates exactly one tick: applies every frame queued for the current
    /// tick in `(player, sequence)` order, steps the simulation, records the
    /// post-tick hash, and returns it.
    pub fn tick(&mut self) -> u64 {
        let tick = self.sim.tick;
        let due: Vec<(u64, u8, u64)> = self
            .queue
            .range((tick, 0, 0)..=(tick, u8::MAX, u64::MAX))
            .map(|(key, _)| *key)
            .collect();
        let mut commands = Vec::new();
        for key in due {
            if let Some(frame) = self.queue.remove(&key) {
                commands.extend(frame.commands.iter().cloned());
                self.log.push(frame);
            }
        }
        self.sim.step(&commands);
        let hash = self.sim.state_hash();
        self.hashes.push((self.sim.tick, hash));
        hash
    }

    /// Simulates `ticks` ticks and returns the final hash.
    pub fn run(&mut self, ticks: u32) -> u64 {
        let mut hash = self.hash();
        for _ in 0..ticks {
            hash = self.tick();
        }
        hash
    }
}

/// Two peers fed the same frames. This is the lockstep harness: each
/// [`LockstepPair::tick`] steps both simulations and compares their post-tick
/// hashes, returning [`NetError::Desync`] on the first divergence.
pub struct LockstepPair {
    pub a: LockstepClient,
    pub b: LockstepClient,
}

impl LockstepPair {
    /// Creates two peers from one setup. Fails if the two instantiations do not
    /// agree on the starting hash — that would make every later comparison
    /// meaningless.
    pub fn new(setup: &MatchSetup) -> NetResult<Self> {
        let sim_a = setup.build()?;
        let sim_b = setup.build()?;
        let start = sim_a.state_hash();
        if sim_b.state_hash() != start {
            return Err(NetError::Desync {
                tick: 0,
                local: start,
                remote: sim_b.state_hash(),
            });
        }
        Ok(Self {
            a: LockstepClient::new(sim_a, 0)?,
            b: LockstepClient::new(sim_b, 1)?,
        })
    }

    /// Resumes a pair from two already-built simulations (mid-match save/load):
    /// both must agree on the current hash, otherwise the resume is refused.
    pub fn from_sims(sim_a: Sim, sim_b: Sim) -> NetResult<Self> {
        let left = sim_a.state_hash();
        let right = sim_b.state_hash();
        if left != right {
            return Err(NetError::Desync {
                tick: sim_a.tick,
                local: left,
                remote: right,
            });
        }
        Ok(Self {
            a: LockstepClient::new(sim_a, 0)?,
            b: LockstepClient::new(sim_b, 1)?,
        })
    }

    /// Sends one frame to both peers.
    pub fn broadcast(&mut self, frame: CommandFrame) -> NetResult<()> {
        let clone = frame.clone();
        self.a.schedule(frame)?;
        self.b.schedule(clone)
    }

    /// Sends a batch of frames to both peers in order.
    pub fn broadcast_all(&mut self, frames: &[CommandFrame]) -> NetResult<()> {
        for frame in frames {
            self.broadcast(frame.clone())?;
        }
        Ok(())
    }

    /// Steps both peers one tick and compares their hashes.
    pub fn tick(&mut self) -> NetResult<(u64, u64)> {
        let left = self.a.tick();
        let right = self.b.tick();
        if left != right {
            return Err(NetError::Desync {
                tick: self.a.sim.tick,
                local: left,
                remote: right,
            });
        }
        Ok((left, right))
    }

    /// Steps `ticks` ticks, failing on the first desync.
    pub fn run(&mut self, ticks: u32) -> NetResult<()> {
        for _ in 0..ticks {
            self.tick()?;
        }
        Ok(())
    }

    /// Runs one tick, then verifies that the hash is the one both peers
    /// recorded. Used by tests that compare against a pre-recorded trace.
    pub fn tick_checked(&mut self, expected: u64, tick: u64) -> NetResult<()> {
        let (left, _) = self.tick()?;
        if left != expected {
            return Err(NetError::HashMismatch {
                tick,
                expected,
                actual: left,
            });
        }
        Ok(())
    }
}

/// Compares one tick hash between a local peer and a remote report.
pub fn verify_remote_hash(tick: u64, local: u64, remote: u64) -> NetResult<()> {
    if local == remote {
        Ok(())
    } else {
        Err(NetError::Desync {
            tick,
            local,
            remote,
        })
    }
}

/// Accumulates per-tick hashes reported by remote peers and flags the first
/// mismatch. Production callers feed it every peer hash; the lockstep pair
/// above is the in-process equivalent.
#[derive(Default)]
pub struct DesyncDetector {
    expected: BTreeMap<u64, u64>,
}

impl DesyncDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the authoritative hash for a tick.
    pub fn expect(&mut self, tick: u64, hash: u64) {
        self.expected.insert(tick, hash);
    }

    /// Checks a reported hash against the recorded one.
    pub fn check(&self, tick: u64, hash: u64) -> NetResult<()> {
        match self.expected.get(&tick) {
            Some(expected) => verify_remote_hash(tick, *expected, hash),
            None => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// 30 Hz frame clock
// ---------------------------------------------------------------------------

/// Converts elapsed wall-clock time into lockstep ticks at exactly 30 Hz.
///
/// The clock never touches the simulation: the caller runs the returned number
/// of ticks. Because the conversion is exact rational arithmetic on cumulative
/// time (microseconds * 30 / 1_000_000), peers that receive the same elapsed
/// time series derive the same tick counts.
pub struct FrameClock {
    elapsed_micros: u64,
    released_ticks: u64,
    max_catchup: u32,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new(5)
    }
}

impl FrameClock {
    /// Creates a clock that releases at most `max_catchup` ticks per advance,
    /// so a stalled peer never bursts through the whole backlog in one frame.
    pub fn new(max_catchup: u32) -> Self {
        Self {
            elapsed_micros: 0,
            released_ticks: 0,
            max_catchup: max_catchup.max(1),
        }
    }

    /// Adds elapsed time and returns how many ticks are now due (0..=max_catchup).
    pub fn advance(&mut self, delta_millis: u64) -> u32 {
        self.elapsed_micros = self
            .elapsed_micros
            .saturating_add(delta_millis.saturating_mul(1_000));
        let due_total =
            (u128::from(self.elapsed_micros) * u128::from(LOCKSTEP_HZ) / 1_000_000) as u64;
        let due = due_total.saturating_sub(self.released_ticks);
        let release = due.min(u64::from(self.max_catchup)) as u32;
        // Ticks beyond the cap are dropped, not accumulated: the match keeps
        // its 30 Hz cadence instead of telescoping after a stall.
        self.released_ticks = due_total;
        release
    }

    /// Ticks released so far.
    pub fn released(&self) -> u64 {
        self.released_ticks
    }
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

/// A recorded match: setup + seed + command frames + start/end hashes.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    pub version: u16,
    pub setup: MatchSetup,
    pub start_hash: u64,
    pub end_tick: u64,
    pub end_hash: u64,
    pub frames: Vec<CommandFrame>,
}

/// Outcome of re-simulating a replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayReport {
    pub start_hash: u64,
    pub end_tick: u64,
    pub end_hash: u64,
    pub frames: usize,
    pub ticks: u64,
}

impl Replay {
    /// Records a replay from a peer that already ran the match.
    pub fn from_client(setup: &MatchSetup, start_hash: u64, client: &LockstepClient) -> Self {
        let (end_tick, end_hash) = client.hashes().last().copied().unwrap_or((0, start_hash));
        Self {
            version: REPLAY_VERSION,
            setup: setup.clone(),
            start_hash,
            end_tick,
            end_hash,
            frames: client.log().to_vec(),
        }
    }

    /// Records a replay from a finished (or in-progress) transport session.
    pub fn from_session(setup: &MatchSetup, start_hash: u64, session: &NetSession) -> Self {
        let (end_tick, end_hash) = session
            .hashes()
            .last()
            .copied()
            .unwrap_or((session.sim.tick, session.last_hash));
        Self {
            version: REPLAY_VERSION,
            setup: setup.clone(),
            start_hash,
            end_tick,
            end_hash,
            frames: session.frames().to_vec(),
        }
    }

    /// Records a replay from an explicit frame list.
    pub fn record(
        setup: &MatchSetup,
        start_hash: u64,
        end_tick: u64,
        end_hash: u64,
        frames: Vec<CommandFrame>,
    ) -> Self {
        Self {
            version: REPLAY_VERSION,
            setup: setup.clone(),
            start_hash,
            end_tick,
            end_hash,
            frames,
        }
    }

    /// Serializes the replay (same envelope discipline as the save format).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Fw::new();
        write_setup(&mut w, &self.setup);
        w.u64(self.start_hash);
        w.u64(self.end_tick);
        w.u64(self.end_hash);
        w.len(self.frames.len());
        for frame in &self.frames {
            w.bytes(&frame.to_bytes());
        }
        let payload = w.buf;
        let mut out = Vec::with_capacity(ENVELOPE_HEADER + payload.len() + ENVELOPE_CHECKSUM);
        out.extend_from_slice(&REPLAY_MAGIC);
        out.extend_from_slice(&REPLAY_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&checksum(&payload).to_le_bytes());
        out
    }

    /// Parses a replay produced by [`Replay::encode`].
    pub fn decode(bytes: &[u8]) -> NetResult<Self> {
        let mut r = Fr::new(bytes);
        let magic = r.take(4)?;
        if magic != REPLAY_MAGIC {
            let mut found = [0u8; 4];
            found.copy_from_slice(magic);
            return Err(NetError::BadMagic(found));
        }
        let version = r.u16()?;
        if version != REPLAY_VERSION {
            return Err(NetError::UnknownVersion(version));
        }
        let flags = r.u16()?;
        if flags != 0 {
            return Err(NetError::InvalidValue {
                field: "replay.flags",
                detail: format!("reserved flags are {flags}"),
            });
        }
        let declared = r.u32()? as usize;
        let actual = bytes
            .len()
            .saturating_sub(ENVELOPE_HEADER + ENVELOPE_CHECKSUM);
        if declared != actual {
            return Err(NetError::LengthMismatch { declared, actual });
        }
        let payload = r.take(declared)?;
        let stored = r.u64()?;
        let computed = checksum(payload);
        if stored != computed {
            return Err(NetError::Checksum {
                expected: stored,
                actual: computed,
            });
        }
        if r.remaining() != 0 {
            return Err(NetError::TrailingBytes(r.remaining()));
        }

        let mut p = Fr::new(payload);
        let setup = MatchSetup::read(&mut p)?;
        let start_hash = p.u64()?;
        let end_tick = p.u64()?;
        let end_hash = p.u64()?;
        let count = p.len(MAX_FRAMES)?;
        let frame_bytes = p.remaining();
        let mut frames = Vec::with_capacity(count.min(frame_bytes));
        for _ in 0..count {
            let raw = p.bytes()?;
            frames.push(CommandFrame::from_bytes(raw)?);
        }
        if p.remaining() != 0 {
            return Err(NetError::TrailingBytes(p.remaining()));
        }
        Ok(Self {
            version,
            setup,
            start_hash,
            end_tick,
            end_hash,
            frames,
        })
    }

    /// Writes the replay file.
    pub fn to_path(&self, path: impl AsRef<Path>) -> NetResult<()> {
        std::fs::write(path.as_ref(), self.encode())
            .map_err(|error| NetError::Io(format!("{}: {error}", path.as_ref().display())))
    }

    /// Reads a replay file.
    pub fn from_path(path: impl AsRef<Path>) -> NetResult<Self> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|error| NetError::Io(format!("{}: {error}", path.as_ref().display())))?;
        Self::decode(&bytes)
    }

    /// Re-simulates the match and verifies both the starting hash and the final
    /// hash. Returns the report on success, a typed error on any mismatch.
    pub fn verify(&self) -> NetResult<ReplayReport> {
        let mut sim = self.setup.build()?;
        let start = sim.state_hash();
        if start != self.start_hash {
            return Err(NetError::HashMismatch {
                tick: sim.tick,
                expected: self.start_hash,
                actual: start,
            });
        }
        let mut frames = self.frames.iter().peekable();
        while sim.tick < self.end_tick {
            let tick = sim.tick;
            let mut commands = Vec::new();
            while let Some(frame) = frames.peek() {
                match frame.tick.cmp(&tick) {
                    std::cmp::Ordering::Equal => {
                        commands.extend(frame.commands.iter().cloned());
                        frames.next();
                    }
                    std::cmp::Ordering::Less => {
                        return Err(NetError::LateFrame {
                            tick: frame.tick,
                            current: tick,
                        });
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }
            sim.step(&commands);
        }
        if let Some(frame) = frames.next() {
            return Err(NetError::UnconsumedFrame { tick: frame.tick });
        }
        let end = sim.state_hash();
        if end != self.end_hash {
            return Err(NetError::HashMismatch {
                tick: sim.tick,
                expected: self.end_hash,
                actual: end,
            });
        }
        Ok(ReplayReport {
            start_hash: start,
            end_tick: sim.tick,
            end_hash: end,
            frames: self.frames.len(),
            ticks: sim.tick,
        })
    }
}

fn write_bitmap(w: &mut Fw, bits: &[bool]) {
    let mut bytes = vec![0u8; bits.len().div_ceil(8)];
    for (index, bit) in bits.iter().enumerate() {
        if *bit {
            bytes[index / 8] |= 1 << (index % 8);
        }
    }
    w.bytes(&bytes);
}

fn read_bitmap(r: &mut Fr<'_>, cells: usize) -> NetResult<Vec<bool>> {
    let bytes = r.bytes()?;
    if bytes.len() != cells.div_ceil(8) {
        return Err(NetError::InvalidValue {
            field: "scene.walkable",
            detail: format!(
                "bitmap is {} bytes, {cells} cells need {}",
                bytes.len(),
                cells.div_ceil(8)
            ),
        });
    }
    let mut out = Vec::with_capacity(cells);
    for index in 0..cells {
        out.push(bytes[index / 8] & (1 << (index % 8)) != 0);
    }
    Ok(out)
}

fn checksum(payload: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in payload {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// TCP transport (protocol `W3LS` v1)
// ---------------------------------------------------------------------------
//
// Wire protocol, one length-prefixed message per record:
//
//   `u32 len | u8 kind | payload`   (little-endian; len counts kind+payload)
//
// kinds: `1 Hello`, `2 Batch`, `3 Hash`, `4 End`.
//
// * `Hello` (once): `u8 seat, u64 setup_digest, u64 start_hash, u32 setup_len,
//   setup bytes`. The host is seat 0 (binds + accepts), the connector is seat 1
//   (connects). Both validate that the seats differ, the setup digests match and
//   the starting `state_hash` matches; any mismatch aborts before tick 0.
// * `Batch` (every tick, both directions): `u64 tick, u8 player, u32 count,
//   [u32 len + frame bytes]*` — the sender's commands for that tick.
// * `Hash` (every tick, both directions after stepping): `u64 tick, u64 hash`.
// * `End` (optional): `u8 winner_or_255, u64 end_tick, u64 end_hash`.
//
// Tick loop (strict barrier, no rollback, no reconnect — v1):
//   1. send local Batch(tick T)
//   2. receive remote Batch(tick T)
//   3. apply frames in `(player)` order, step, hash
//   4. send local Hash, receive remote Hash, compare -> `Desync { tick, local, remote }`
//
// Both peers therefore apply exactly the same command set to tick T before
// either is allowed to run T+1; a missing/duplicated tick is a `Protocol` error
// and a differing hash stops the match at the first bad tick.
//
// Pacing: `pace = true` holds the strict 30 Hz cadence
// (`started + ticks * MICROS_PER_TICK`), `pace = false` runs as fast as the
// sockets allow (used by tests). Pacing never affects simulation state.

/// Wire magic for the transport handshake.
pub const PROTOCOL_MAGIC: [u8; 4] = *b"W3LS";
/// Transport protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

const MSG_HELLO: u8 = 1;
const MSG_BATCH: u8 = 2;
const MSG_HASH: u8 = 3;
#[allow(dead_code)]
const MSG_END: u8 = 4;

/// Default socket read/write timeout: a lockstep peer that goes silent aborts
/// the match instead of hanging forever.
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// One step of a networked match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickStep {
    /// Post-step tick (`Sim::tick`).
    pub tick: u64,
    /// Post-step state hash (agreed by both peers).
    pub hash: u64,
    /// Winner, once `Event::Victory` fired on this tick.
    pub victory: Option<u8>,
}

/// Configuration for [`NetSession`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportConfig {
    /// Seat this process controls. `0` hosts (binds/accepts), `1` connects.
    pub seat: u8,
    /// Hold the 30 Hz cadence (CLI) or run as fast as possible (tests).
    pub pace: bool,
    /// Read/write timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            seat: 0,
            pace: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

/// A networked lockstep peer: one `Sim`, one socket, one seat.
pub struct NetSession {
    stream: std::net::TcpStream,
    pub local_seat: u8,
    pub remote_seat: u8,
    pub setup: MatchSetup,
    pub sim: Sim,
    pub start_hash: u64,
    pub last_hash: u64,
    pub victory: Option<u8>,
    hashes: Vec<(u64, u64)>,
    frames: Vec<CommandFrame>,
    started: std::time::Instant,
    pace: bool,
}

impl NetSession {
    /// Hosts a match: binds `addr`, accepts exactly one peer, completes the
    /// handshake, and returns the session as seat 0.
    pub fn host(addr: &str, setup: &MatchSetup) -> NetResult<Self> {
        let listener = std::net::TcpListener::bind(addr)
            .map_err(|error| NetError::Transport(format!("bind {addr}: {error}")))?;
        let (stream, _peer) = listener
            .accept()
            .map_err(|error| NetError::Transport(format!("accept on {addr}: {error}")))?;
        Self::establish(stream, setup, TransportConfig::default(), 0)
    }

    /// Connects to a host as seat 1 and completes the handshake.
    pub fn connect(addr: &str, setup: &MatchSetup) -> NetResult<Self> {
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|error| NetError::Transport(format!("connect {addr}: {error}")))?;
        Self::establish(stream, setup, TransportConfig::default(), 1)
    }

    /// Host with explicit transport configuration (seat must be 0).
    pub fn host_with(addr: &str, setup: &MatchSetup, config: TransportConfig) -> NetResult<Self> {
        if config.seat != 0 {
            return Err(NetError::SeatConflict {
                local: config.seat,
                remote: 0,
            });
        }
        let listener = std::net::TcpListener::bind(addr)
            .map_err(|error| NetError::Transport(format!("bind {addr}: {error}")))?;
        let (stream, _peer) = listener
            .accept()
            .map_err(|error| NetError::Transport(format!("accept on {addr}: {error}")))?;
        Self::establish(stream, setup, config, 0)
    }

    /// Hosts a match on an already-bound listener. Lets a caller bind port 0,
    /// learn the port, then accept exactly one peer without a bind race.
    pub fn host_on(
        listener: std::net::TcpListener,
        setup: &MatchSetup,
        config: TransportConfig,
    ) -> NetResult<Self> {
        if config.seat != 0 {
            return Err(NetError::SeatConflict {
                local: config.seat,
                remote: 0,
            });
        }
        let (stream, _peer) = listener
            .accept()
            .map_err(|error| NetError::Transport(format!("accept: {error}")))?;
        Self::establish(stream, setup, config, 0)
    }

    /// Connect with explicit transport configuration (seat must be 1).
    pub fn connect_with(
        addr: &str,
        setup: &MatchSetup,
        config: TransportConfig,
    ) -> NetResult<Self> {
        if config.seat != 1 {
            return Err(NetError::SeatConflict {
                local: config.seat,
                remote: 1,
            });
        }
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|error| NetError::Transport(format!("connect {addr}: {error}")))?;
        Self::establish(stream, setup, config, 1)
    }

    fn establish(
        stream: std::net::TcpStream,
        setup: &MatchSetup,
        config: TransportConfig,
        local_seat: u8,
    ) -> NetResult<Self> {
        let timeout = std::time::Duration::from_millis(config.timeout_ms.max(1));
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| NetError::Transport(format!("set_read_timeout: {error}")))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| NetError::Transport(format!("set_write_timeout: {error}")))?;
        let _ = stream.set_nodelay(true);

        let sim = setup.build()?;
        let start_hash = sim.state_hash();
        let setup_bytes = setup.to_bytes();
        let mut hello = Fw::new();
        hello.u8(local_seat);
        hello.u64(setup.digest());
        hello.u64(start_hash);
        hello.bytes(&setup_bytes);

        let mut session = Self {
            stream,
            local_seat,
            remote_seat: 1 - local_seat,
            setup: setup.clone(),
            sim,
            start_hash,
            last_hash: start_hash,
            victory: None,
            hashes: Vec::new(),
            frames: Vec::new(),
            started: std::time::Instant::now(),
            pace: config.pace,
        };

        session.send_message(MSG_HELLO, &hello.buf)?;
        let (kind, payload) = session.recv_message()?;
        if kind != MSG_HELLO {
            return Err(NetError::Handshake(format!(
                "expected hello, got message kind {kind}"
            )));
        }
        let mut r = Fr::new(&payload);
        let remote_seat = r.u8()?;
        let remote_digest = r.u64()?;
        let remote_start = r.u64()?;
        let remote_setup_bytes = r.bytes()?;
        if remote_seat == local_seat {
            return Err(NetError::SeatConflict {
                local: local_seat,
                remote: remote_seat,
            });
        }
        if remote_seat > 1 {
            return Err(NetError::Handshake(format!("bad seat {remote_seat}")));
        }
        if remote_digest != setup.digest() {
            let remote_setup = MatchSetup::read(&mut Fr::new(remote_setup_bytes))?;
            return Err(NetError::Handshake(format!(
                "setup mismatch: local digest {:#x}, remote {:#x} (local {}x{}, remote {}x{})",
                setup.digest(),
                remote_digest,
                setup.width(),
                setup.height(),
                remote_setup.width(),
                remote_setup.height()
            )));
        }
        if remote_start != start_hash {
            return Err(NetError::Desync {
                tick: 0,
                local: start_hash,
                remote: remote_start,
            });
        }
        session.remote_seat = remote_seat;
        Ok(session)
    }

    /// `(tick, hash)` after every completed tick.
    pub fn hashes(&self) -> &[(u64, u64)] {
        &self.hashes
    }

    /// Frames applied so far, in execution order (both seats).
    pub fn frames(&self) -> &[CommandFrame] {
        &self.frames
    }

    /// Runs one tick: send batch, receive batch, step, exchange hashes.
    pub fn tick(&mut self, commands: Vec<Command>) -> NetResult<TickStep> {
        self.pace_wait();
        let tick = self.sim.tick;
        let local = CommandFrame::new(tick, self.local_seat, commands);
        self.send_batch(&local)?;
        let remote = self.recv_batch(tick)?;

        // Deterministic application order: lower seat first, independent of
        // which batch arrived first.
        let mut frames = [local, remote];
        frames.sort_by_key(|frame| frame.player);
        let mut all = Vec::new();
        for frame in &frames {
            all.extend(frame.commands.iter().cloned());
            // Only non-empty frames belong in the replay log; an empty tick
            // contributes nothing to the command stream.
            if !frame.commands.is_empty() {
                self.frames.push(frame.clone());
            }
        }
        self.sim.step(&all);
        let hash = self.sim.state_hash();

        self.send_hash(self.sim.tick, hash)?;
        let (remote_tick, remote_hash) = self.recv_hash(self.sim.tick)?;
        if remote_tick != self.sim.tick {
            return Err(NetError::Protocol(format!(
                "hash tick {} does not match local {}",
                remote_tick, self.sim.tick
            )));
        }
        if remote_hash != hash {
            return Err(NetError::Desync {
                tick: self.sim.tick,
                local: hash,
                remote: remote_hash,
            });
        }

        self.last_hash = hash;
        self.hashes.push((self.sim.tick, hash));
        if let Some(winner) = self.tick_victory() {
            self.victory = Some(winner);
        }
        Ok(TickStep {
            tick: self.sim.tick,
            hash,
            victory: self.victory,
        })
    }

    /// Runs `ticks` ticks, generating this seat's commands with `commands`.
    /// Stops early when victory fires and returns the number of ticks run.
    pub fn run<F>(&mut self, ticks: u32, mut commands: F) -> NetResult<u64>
    where
        F: FnMut(&Sim, u8, u64) -> Vec<Command>,
    {
        let mut remaining = ticks;
        while remaining > 0 {
            let generated = commands(&self.sim, self.local_seat, self.sim.tick);
            let step = self.tick(generated)?;
            remaining -= 1;
            if step.victory.is_some() {
                break;
            }
        }
        Ok(self.sim.tick)
    }

    fn tick_victory(&self) -> Option<u8> {
        self.sim.events.iter().find_map(|event| match event {
            crate::Event::Victory(owner) => Some(*owner),
            _ => None,
        })
    }

    fn pace_wait(&self) {
        if !self.pace {
            return;
        }
        let target =
            self.started + std::time::Duration::from_micros(self.sim.tick * MICROS_PER_TICK);
        let now = std::time::Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
    }

    // ---- wire helpers ----

    fn send_message(&mut self, kind: u8, payload: &[u8]) -> NetResult<()> {
        use std::io::Write;
        let len = 1 + payload.len();
        let len = u32::try_from(len)
            .map_err(|_| NetError::Protocol("message larger than 4 GiB".to_string()))?;
        self.stream
            .write_all(&len.to_le_bytes())
            .map_err(|error| NetError::Transport(format!("write length: {error}")))?;
        self.stream
            .write_all(&[kind])
            .map_err(|error| NetError::Transport(format!("write kind: {error}")))?;
        self.stream
            .write_all(payload)
            .map_err(|error| NetError::Transport(format!("write payload: {error}")))?;
        self.stream
            .flush()
            .map_err(|error| NetError::Transport(format!("flush: {error}")))?;
        Ok(())
    }

    fn recv_message(&mut self) -> NetResult<(u8, Vec<u8>)> {
        use std::io::Read;
        let mut len_bytes = [0u8; 4];
        self.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        if len == 0 || len > 64 * 1024 * 1024 {
            return Err(NetError::Protocol(format!("bad message length {len}")));
        }
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        let kind = buf[0];
        buf.remove(0);
        Ok((kind, buf))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> NetResult<()> {
        use std::io::Read;
        self.stream
            .read_exact(buf)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::UnexpectedEof => NetError::Closed,
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                    NetError::Transport("socket timeout".to_string())
                }
                _ => NetError::Transport(format!("read: {error}")),
            })
    }

    fn send_batch(&mut self, frame: &CommandFrame) -> NetResult<()> {
        let mut w = Fw::new();
        w.u64(frame.tick);
        w.u8(frame.player);
        w.len(frame.commands.len());
        for command in &frame.commands {
            w.bytes(&crate::save::command_encode(command));
        }
        self.send_message(MSG_BATCH, &w.buf)
    }

    fn recv_batch(&mut self, tick: u64) -> NetResult<CommandFrame> {
        for _ in 0..8 {
            let (kind, payload) = self.recv_message()?;
            match kind {
                MSG_BATCH => {
                    let mut r = Fr::new(&payload);
                    let frame_tick = r.u64()?;
                    let player = r.u8()?;
                    let count = r.len(4096)?;
                    let mut commands = Vec::with_capacity(count);
                    for _ in 0..count {
                        let raw = r.bytes()?;
                        commands.push(crate::save::command_decode(raw)?);
                    }
                    if frame_tick != tick {
                        return Err(NetError::Protocol(format!(
                            "batch for tick {frame_tick}, expected {tick}"
                        )));
                    }
                    if player != self.remote_seat {
                        return Err(NetError::Protocol(format!(
                            "batch from player {player}, expected {}",
                            self.remote_seat
                        )));
                    }
                    return Ok(CommandFrame::new(frame_tick, player, commands));
                }
                MSG_END => return Err(NetError::Closed),
                other => {
                    return Err(NetError::Protocol(format!(
                        "expected batch, got message kind {other}"
                    )));
                }
            }
        }
        Err(NetError::Protocol(
            "too many out-of-order messages".to_string(),
        ))
    }

    fn send_hash(&mut self, tick: u64, hash: u64) -> NetResult<()> {
        let mut w = Fw::new();
        w.u64(tick);
        w.u64(hash);
        self.send_message(MSG_HASH, &w.buf)
    }

    fn recv_hash(&mut self, tick: u64) -> NetResult<(u64, u64)> {
        let (kind, payload) = self.recv_message()?;
        if kind != MSG_HASH {
            return Err(NetError::Protocol(format!(
                "expected hash, got message kind {kind}"
            )));
        }
        let mut r = Fr::new(&payload);
        let remote_tick = r.u64()?;
        let hash = r.u64()?;
        if remote_tick != tick {
            return Err(NetError::Protocol(format!(
                "hash for tick {remote_tick}, expected {tick}"
            )));
        }
        Ok((remote_tick, hash))
    }
}

/// Runs a full 1v1 match over TCP in two processes: `seat 0` hosts on `port`,
/// `seat 1` connects to `127.0.0.1:port`.
pub fn run_tcp_match(
    port: u16,
    config: TransportConfig,
    setup: &MatchSetup,
    mut commands: impl FnMut(&Sim, u8, u64) -> Vec<Command>,
    max_ticks: u32,
) -> NetResult<(NetSession, u64)> {
    let addr = format!("127.0.0.1:{port}");
    let mut session = if config.seat == 0 {
        NetSession::host_with(&addr, setup, config)?
    } else {
        NetSession::connect_with(&addr, setup, config)?
    };
    session.run(max_ticks, &mut commands)?;
    let ticks = session.sim.tick;
    Ok((session, ticks))
}
