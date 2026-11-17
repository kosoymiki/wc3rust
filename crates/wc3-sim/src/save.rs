//! Versioned binary save of the complete simulation state (mechanic `save.load`).
//!
//! The save is a hand-rolled, dependency-free little-endian format. It exists so
//! a match can be interrupted and resumed bit-identically: `Sim::from_save`
//! reconstructs every gameplay field the simulation reads, rebuilds the derived
//! accelerators (id index, pathfinder scratch, spatial hash, fog stamp), and the
//! restored `Sim` continues to produce the same [`Sim::state_hash`] as the
//! original on every following tick.
//!
//! # Format (`W3SV`, version 4)
//!
//! ```text
//! offset  size  field
//! 0       4     magic            b"W3SV"
//! 4       2     format version   u16 LE (1..=7)
//! 6       2     flags            u16 LE (must be 0)
//! 8       4     payload length   u32 LE
//! 12      N     payload          sections below, in this exact order
//! 12+N    8     checksum         u64 LE, FNV-1a over the payload
//! ```
//!
//! Payload sections, each prefixed with a `u32` tag so a hexdump is
//! self-describing and a mis-ordered reader fails with [`SaveError::BadTag`]:
//!
//! | tag | section | contents |
//! |-----|---------|----------|
//! | `0x4831` | header | tick, seed, rng state, ruleset, passive income |
//! | `0x4731` | grid | width, height, cell size, bit-packed walkable bitmap |
//! | `0x4931` | ids | next unit/item/destructable/camp/projectile id |
//! | `0x5231` | resources | gold and lumber per player |
//! | `0x4631` | fog | visibility + explored bitmaps per player |
//! | `0x5031` | gameplay | every `GameplayConfig` field |
//! | `0x4D31` | mines | gold mines |
//! | `0x4431` | destructables | trees/crates |
//! | `0x4331` | camps | neutral creep camps |
//! | `0x4932` | items | ground/carried items |
//! | `0x5331` | shops | shop stock slots |
//! | `0x5531` | units | every `Unit` field, including orders and queues |
//! | `0x5032` | projectiles | missiles in flight |
//! | `0x4332` | casts | pending area-effect casts |
//! | `0x4131` | ai | AI controllers |
//! | `0x5532` | upgrades | researched upgrade levels per player |
//! | `0x5332` | script | JASS VM: compiled code, globals, threads, timers, triggers, host world |
//! | `0x454E` | end | end marker (proves the writer reached the end) |
//!
//! ## Script section (version 4)
//!
//! A WC3 save restores the script world, not just the map entities: triggers
//! registered by `war3map.j`, timers counting down, and threads parked on
//! `TriggerSleepAction` continue after loading. Version 4 appends one `script`
//! section that carries:
//!
//! * the compiled program ([`crate::jass::Code`]) — stored with the save so
//!   `Sim::from_save` restores the script from bytes alone; a save can never be
//!   silently re-bound to a different `war3map.j`. Every function index, local
//!   slot, string/global slot, native slot and jump target is validated on read;
//! * every VM global (scalars and sparse arrays) and the script clock, tick, RNG,
//!   step budget and thread cap;
//! * every live thread: frame list (`func`, `pc`, `locals`, `stack_base`), the
//!   value stack, `wake_tick`/`wake_at`/`wait_started` for parked threads, and
//!   the full `EventContext` the thread carries;
//! * the whole script host: unit handle table (handle → unit id), every object
//!   handle (groups, locations, rects, regions, timers, triggers with their
//!   registered events/actions/conditions, sounds, dialogs, hashtables, …),
//!   item/destructable bindings, per-unit script metadata, player slots and the
//!   remaining script-visible world state, plus region/range occupancy.
//!
//! Not restorable by design (diagnostics, not gameplay state): the native/func
//! call counters, `unresolved` census, `errors`, `output`/`text_log` logs and
//! `current_thread` (transient; zero while idle). A restore that drops them
//! prints `GAME_SCRIPT_RESTORE_PARTIAL missing=…` on stderr instead of hiding
//! the loss. The VM's scratch `Sim` is derived (swapped out between ticks) and
//! is not persisted.
//!
//! A version-1..3 file has no script section and restores `script = None`: those
//! writers never persisted script state, so there is nothing to restore and the
//! load is complete for the format that produced it.
//!
//! # Forward-compatibility rule (hard)
//!
//! The reader is **strict about the version and strict about the tail**:
//!
//! 1. A reader accepts every version from 1 through [`SAVE_FORMAT_VERSION`]
//!    (the layout is append-only: each newer version only adds bytes at the end
//!    of a record, gated on the version). Anything else returns
//!    [`SaveError::UnknownVersion`] before a single payload byte is parsed.
//! 2. A writer that changes the layout **must** bump [`SAVE_FORMAT_VERSION`] and
//!    add a matching decode arm (`decode_vN`). It must never append a field to an
//!    existing version: readers reject trailing bytes after the end marker, so
//!    "append silently" fails loudly instead of mis-parsing.
//! 3. Sleeping-beauty compatibility (read v1 with a v2 reader) is expressed as
//!    extra `decode_v1` arms kept alive by a test; new versions never mutate the
//!    meaning of an old tag.
//!
//! Consequences: an old save opened by a newer build either decodes through an
//! explicit migration arm or fails with a typed error. It never panics and never
//! produces a half-restored simulation.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use glam::Vec2;

use crate::abilities::{PendingCast, SPELL_COUNT, Spell};
use crate::ai::{AiConfig, AiMode, AiState};
use crate::data;
use crate::jass::host::{
    CameraSetup, CineFilter, Object, PlayerState, RegEvent, SoundState, TimerState, TriggerState,
    UnitMeta, UnitTypes,
};
use crate::jass::value::{Handle, HandleKind, Value};
use crate::jass::vm::{
    BinTag, Code, CodeFunc, EventContext, Frame, GlobalKind, GlobalSlot, Instr, JassVm,
    ThreadState, UnTag,
};
use crate::jass::scheduler::EventIds;
use crate::nav;
use crate::spatial;
use crate::{
    Camp, Construction, Destructable, DestructableId, GoldMine, Item, ItemId, Order, OrderSpec,
    PATH_BUDGET, Production, Projectile, ProjectileId, Research, Ruleset, Shop, ShopSlot, Sim,
    Unit, UnitId,
};

/// Save file magic (`W3` save, versioned).
pub const SAVE_MAGIC: [u8; 4] = *b"W3SV";
/// Layout version written by this build.  Version 2 appends the buff, squad and
/// order-stamp sections; version 3 appends the per-unit `ethereal` byte; version
/// 4 appends the JASS script section (compiled code, globals, threads, host
/// objects, timers and triggers); version 5 appends the per-unit life-regen
/// accumulator and rate (`hp_milli`/`hp_regen_milli`); version 6 appends the
/// headless UI state of `dialog`/`button`/`timerdialog` objects (message,
/// display target, quit flag, timer-dialog speed/colours/remaining override) and
/// the two trigger registrations that used to be discarded
/// (`TriggerRegisterGameStateEvent`, `TriggerRegisterGameEvent`); version 7
/// appends the per-unit runtime `UNIT_TYPE_*` delta of
/// `UnitAddType`/`UnitRemoveType`.  Older files are still readable through their
/// decode arms; those versions never stored the newer state, so it restores to
/// its neutral value.
pub const SAVE_FORMAT_VERSION: u16 = 7;

/// Hard ceiling for every ID space a save can carry (`next_id`, `next_item_id`,
/// `next_destructable_id`, `next_camp_id`, `next_projectile_id` and the script
/// handle counter).
///
/// Justification, from the format and the entity counts the simulation supports:
/// a save belongs to one map, and `Sim::new`/`decode_payload` reject any grid
/// larger than 1024×1024 = 2^20 cells, so no legitimate save can hold more than
/// one entity per cell. The only save-derived dense structure is the unit id
/// index (`4` bytes per slot), so the cap bounds it at 4 MiB no matter what the
/// file claims. In practice the simulation runs thousands of units and tens of
/// thousands of destructables, which is two orders of magnitude below the cap:
/// this is a hostile-input bound, not a gameplay limit.
pub const MAX_ID_SPACE: u32 = 1 << 20;
/// Bytes before the payload: magic + version + flags + payload length.
const HEADER_LEN: usize = 12;
/// Checksum appended after the payload.
const CHECKSUM_LEN: usize = 8;
/// Spatial hash cell size used by `Sim::new` / `Sim::set_pathing`.
const SPATIAL_CELL: f32 = 256.0;

const SEC_HEADER: u32 = 0x4831;
const SEC_GRID: u32 = 0x4731;
const SEC_IDS: u32 = 0x4931;
const SEC_RESOURCES: u32 = 0x5231;
const SEC_FOG: u32 = 0x4631;
const SEC_GAMEPLAY: u32 = 0x5031;
const SEC_MINES: u32 = 0x4D31;
const SEC_DESTRUCTABLES: u32 = 0x4431;
const SEC_CAMPS: u32 = 0x4331;
const SEC_ITEMS: u32 = 0x4932;
const SEC_SHOPS: u32 = 0x5331;
const SEC_UNITS: u32 = 0x5531;
const SEC_PROJECTILES: u32 = 0x5032;
const SEC_CASTS: u32 = 0x4332;
const SEC_AI: u32 = 0x4131;
const SEC_UPGRADES: u32 = 0x5532;
/// Version 2: live buffs.
const SEC_BUFFS: u32 = 0x4231;
/// Version 2: squad controllers.
const SEC_SQUADS: u32 = 0x5131;
/// Version 2: per-tick order spam stamps.
const SEC_STAMPS: u32 = 0x4F31;
/// Version 4: the JASS script world.
const SEC_SCRIPT: u32 = 0x5332;
const SEC_END: u32 = 0x454E;

/// Restore report prefix for script state that is intentionally not persisted
/// (diagnostic counters/logs, transient execution slot). Printed once per load
/// that carries a script section, so the loss is observed, never silent.
const SCRIPT_RESTORE_PARTIAL_PREFIX: &str = "GAME_SCRIPT_RESTORE_PARTIAL missing=";

/// Every failure mode of reading a save. Reading never panics on malformed
/// input; each broken invariant maps to one of these variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// First four bytes are not [`SAVE_MAGIC`].
    BadMagic([u8; 4]),
    /// Version is not the one this build understands.
    UnknownVersion(u16),
    /// Reserved header flags are non-zero.
    Flags(u16),
    /// The file ended before `needed` bytes were available.
    Truncated { needed: usize, available: usize },
    /// The declared payload length disagrees with the file size.
    LengthMismatch { declared: usize, actual: usize },
    /// Payload checksum does not match.
    Checksum { expected: u64, actual: u64 },
    /// A section tag (or enum tag) is outside the documented set.
    BadTag { section: &'static str, tag: u32 },
    /// A stored index does not address the table it points into.
    OutOfRange {
        section: &'static str,
        index: u32,
        limit: usize,
    },
    /// An ID counter or ID is outside the save's documented ID space, or two
    /// entities claim the same slot. `field` names the counter (`ids.next_id`,
    /// `script.next_object`, …), `value` is the offending number and `detail`
    /// states which rule was broken. Rejected before any allocation is sized
    /// from it, so a crafted file can never turn into an OOM or a panic.
    InvalidIdSpace {
        field: &'static str,
        value: u32,
        detail: String,
    },
    /// A stored string id is not a known roster/item/upgrade/ability id.
    UnknownName { section: &'static str, name: String },
    /// An AI composition table is not one of the shipped static tables.
    UnknownComposition,
    /// Grid/fog dimensions are inconsistent.
    InvalidGrid { width: u32, height: u32 },
    /// The save's compiled script targets a different native registry than this
    /// build (native slots would dispatch to the wrong handlers).
    ScriptMismatch { detail: String },
    /// A scalar is outside its legal domain.
    InvalidValue { field: &'static str, detail: String },
    /// Bytes remained after the end marker.
    TrailingBytes(usize),
    /// Filesystem failure while loading/saving a path.
    Io(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic(found) => write!(f, "not a W3SV save (magic {found:?})"),
            Self::UnknownVersion(v) => write!(
                f,
                "unsupported save version {v} (this build reads {SAVE_FORMAT_VERSION})"
            ),
            Self::Flags(v) => write!(f, "reserved header flags must be 0, found {v}"),
            Self::Truncated { needed, available } => {
                write!(
                    f,
                    "truncated save: needed {needed} bytes, file has {available}"
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
                write!(f, "unknown tag {tag:#x} in section {section}")
            }
            Self::OutOfRange {
                section,
                index,
                limit,
            } => write!(f, "index {index} out of range in {section} (limit {limit})"),
            Self::InvalidIdSpace {
                field,
                value,
                detail,
            } => write!(f, "invalid {field} {value}: {detail}"),
            Self::UnknownName { section, name } => {
                write!(f, "unknown id {name:?} in section {section}")
            }
            Self::UnknownComposition => write!(f, "unknown AI composition table"),
            Self::InvalidGrid { width, height } => {
                write!(f, "invalid grid {width}x{height}")
            }
            Self::ScriptMismatch { detail } => write!(f, "script restore refused: {detail}"),
            Self::InvalidValue { field, detail } => write!(f, "invalid {field}: {detail}"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes after the end marker"),
            Self::Io(message) => write!(f, "io error: {message}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Result alias for save/restore operations.
pub type SaveResult<T> = Result<T, SaveError>;

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

struct W {
    buf: Vec<u8>,
}

impl W {
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
        let value = u32::try_from(value).expect("save section length fits in u32");
        self.u32(value);
    }

    fn str(&mut self, value: &str) {
        self.len(value.len());
        self.buf.extend_from_slice(value.as_bytes());
    }

    fn v2(&mut self, value: Vec2) {
        self.f32(value.x);
        self.f32(value.y);
    }

    fn opt_v2(&mut self, value: Option<Vec2>) {
        match value {
            Some(point) => {
                self.u8(1);
                self.v2(point);
            }
            None => self.u8(0),
        }
    }

    fn opt_id(&mut self, value: Option<UnitId>) {
        match value {
            Some(id) => {
                self.u8(1);
                self.u32(id.0);
            }
            None => self.u8(0),
        }
    }

    fn opt_u8(&mut self, value: Option<u8>) {
        match value {
            Some(byte) => {
                self.u8(1);
                self.u8(byte);
            }
            None => self.u8(0),
        }
    }

    fn opt_f32(&mut self, value: Option<f32>) {
        match value {
            Some(real) => {
                self.u8(1);
                self.f32(real);
            }
            None => self.u8(0),
        }
    }

    fn opt_u32(&mut self, value: Option<u32>) {
        match value {
            Some(word) => {
                self.u8(1);
                self.u32(word);
            }
            None => self.u8(0),
        }
    }

    fn opt_str(&mut self, value: Option<&str>) {
        match value {
            Some(text) => {
                self.u8(1);
                self.str(text);
            }
            None => self.u8(0),
        }
    }

    fn opt_cell(&mut self, value: Option<(u32, u32)>) {
        match value {
            Some((x, y)) => {
                self.u8(1);
                self.u32(x);
                self.u32(y);
            }
            None => self.u8(0),
        }
    }

    fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.buf.extend_from_slice(value);
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct R<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, count: usize) -> SaveResult<&'a [u8]> {
        if count > self.remaining() {
            return Err(SaveError::Truncated {
                needed: self.pos + count,
                available: self.buf.len(),
            });
        }
        let slice = &self.buf[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn u8(&mut self) -> SaveResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> SaveResult<u16> {
        let slice = self.take(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn u32(&mut self) -> SaveResult<u32> {
        let slice = self.take(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    fn u64(&mut self) -> SaveResult<u64> {
        let slice = self.take(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(slice);
        Ok(u64::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> SaveResult<i32> {
        Ok(self.u32()? as i32)
    }

    fn f32(&mut self) -> SaveResult<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn bool(&mut self, field: &'static str) -> SaveResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(SaveError::InvalidValue {
                field,
                detail: format!("boolean tag {other}"),
            }),
        }
    }

    /// Element count for a following sequence. Rejects counts that cannot fit in
    /// the remaining bytes so a corrupt header can never trigger a huge
    /// allocation; every element of every sequence costs at least one byte.
    fn len(&mut self) -> SaveResult<usize> {
        let count = self.u32()? as usize;
        if count > self.remaining() {
            return Err(SaveError::Truncated {
                needed: self.pos + count,
                available: self.buf.len(),
            });
        }
        Ok(count)
    }

    /// Plain `u32 -> usize` scalar (an index, a tick count, a queue position).
    /// Unlike [`R::len`] this is not a byte bound and must not be range-checked
    /// against the remaining payload.
    fn count(&mut self) -> SaveResult<usize> {
        Ok(self.u32()? as usize)
    }

    fn str(&mut self) -> SaveResult<&'a str> {
        let count = self.u32()? as usize;
        let bytes = self.take(count)?;
        std::str::from_utf8(bytes).map_err(|_| SaveError::InvalidValue {
            field: "string",
            detail: "not valid UTF-8".to_string(),
        })
    }

    fn v2(&mut self) -> SaveResult<Vec2> {
        let x = self.f32()?;
        let y = self.f32()?;
        Ok(Vec2::new(x, y))
    }

    fn opt_v2(&mut self) -> SaveResult<Option<Vec2>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.v2()?)),
            other => Err(SaveError::InvalidValue {
                field: "Option<Vec2>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_id(&mut self) -> SaveResult<Option<UnitId>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(UnitId(self.u32()?))),
            other => Err(SaveError::InvalidValue {
                field: "Option<UnitId>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_u8(&mut self) -> SaveResult<Option<u8>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u8()?)),
            other => Err(SaveError::InvalidValue {
                field: "Option<u8>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_u32(&mut self) -> SaveResult<Option<u32>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u32()?)),
            other => Err(SaveError::InvalidValue {
                field: "Option<u32>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_f32(&mut self) -> SaveResult<Option<f32>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.f32()?)),
            other => Err(SaveError::InvalidValue {
                field: "Option<f32>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_str(&mut self) -> SaveResult<Option<&'a str>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.str()?)),
            other => Err(SaveError::InvalidValue {
                field: "Option<str>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn opt_cell(&mut self) -> SaveResult<Option<(u32, u32)>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some((self.u32()?, self.u32()?))),
            other => Err(SaveError::InvalidValue {
                field: "Option<(u32, u32)>",
                detail: format!("tag {other}"),
            }),
        }
    }

    fn bytes(&mut self) -> SaveResult<&'a [u8]> {
        let count = self.u32()? as usize;
        self.take(count)
    }

    fn section(&mut self, expected: u32, name: &'static str) -> SaveResult<()> {
        let tag = self.u32()?;
        if tag != expected {
            return Err(SaveError::BadTag { section: name, tag });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Static id interning
// ---------------------------------------------------------------------------

/// Every `&'static str` the simulation stores is a data-table id (roster unit,
/// item, upgrade, ability, missile art). Interner maps a saved string back to
/// the canonical static so a restored `Sim` is pointer-comparable with a fresh
/// one and no allocation outlives the load.
static ID_POOL: OnceLock<BTreeMap<&'static str, ()>> = OnceLock::new();

fn pool() -> &'static BTreeMap<&'static str, ()> {
    ID_POOL.get_or_init(|| {
        let mut set = BTreeMap::new();
        set.insert("", ());
        // Roster plus the three appended tables (creeps, shops, extras); a
        // unit's `def_index` spans all four through `data::def_at`.
        let defs = data::ROSTER
            .iter()
            .chain(data::CREEP_DEFS)
            .chain(data::SHOP_DEFS)
            .chain(data::EXTRA_DEFS);
        for def in defs {
            set.insert(def.id, ());
            if !def.missile_art.is_empty() {
                set.insert(def.missile_art, ());
            }
            for id in def.ability_ids {
                set.insert(*id, ());
            }
        }
        for def in data::ITEMS {
            set.insert(def.id, ());
        }
        for def in data::UPGRADES {
            set.insert(def.id, ());
        }
        for def in &crate::abilities::SPELL_DEFS {
            set.insert(def.id, ());
        }
        for def in crate::abilities::GENERIC_ABILITIES {
            set.insert(def.id, ());
        }
        set
    })
}

/// Number of valid `def_index` values (roster + creeps + shops + extras).
fn def_count() -> usize {
    static COUNT: OnceLock<usize> = OnceLock::new();
    *COUNT.get_or_init(|| {
        let mut count = 0usize;
        while count <= u16::MAX as usize && data::def_at(count as u16).is_some() {
            count += 1;
        }
        count
    })
}

fn intern(section: &'static str, name: &str) -> SaveResult<&'static str> {
    pool()
        .get_key_value(name)
        .map(|(key, _)| *key)
        .ok_or_else(|| SaveError::UnknownName {
            section,
            name: name.to_string(),
        })
}

/// Resolves a saved AI composition list back to one of the shipped static
/// tables (`ai_composition_for`). Compositions are static per race, so a
/// restored `AiState` must borrow the identical slice.
fn intern_composition(
    entries: &[(&'static str, u32)],
) -> SaveResult<&'static [(&'static str, u32)]> {
    use crate::data::Race;
    const RACES: [Race; 5] = [
        Race::Human,
        Race::Orc,
        Race::Undead,
        Race::NightElf,
        Race::Neutral,
    ];
    for race in RACES {
        // Two tables exist: the live runtime config (`AiConfig::for_race`, which
        // `Sim::enable_ai_for_race` installs and the save writes) and the older
        // two-unit `ai_composition_for`. Interning only the latter made every
        // resume fail with `UnknownComposition` even on a save written by the
        // same build (device: `GAME_RESUME_FAILED ... err=UnknownComposition`).
        for table in [
            AiConfig::for_race(race).composition,
            crate::ai_composition_for(race),
        ] {
            if table.len() == entries.len()
                && table
                    .iter()
                    .zip(entries)
                    .all(|(left, right)| left.0 == right.0 && left.1 == right.1)
            {
                return Ok(table);
            }
        }
    }
    Err(SaveError::UnknownComposition)
}

// ---------------------------------------------------------------------------
// Small enum / scalar codecs
// ---------------------------------------------------------------------------

fn ruleset_tag(ruleset: Ruleset) -> u8 {
    match ruleset {
        Ruleset::Roc => 0,
        Ruleset::Tft => 1,
    }
}

fn ruleset_from(tag: u8) -> SaveResult<Ruleset> {
    match tag {
        0 => Ok(Ruleset::Roc),
        1 => Ok(Ruleset::Tft),
        other => Err(SaveError::BadTag {
            section: "header.ruleset",
            tag: u32::from(other),
        }),
    }
}

fn unit_kind_tag(kind: crate::UnitKind) -> u8 {
    use crate::UnitKind;
    match kind {
        UnitKind::Footman => 0,
        UnitKind::Archer => 1,
        UnitKind::Peasant => 2,
        UnitKind::TownHall => 3,
    }
}

fn unit_kind_from(tag: u8) -> SaveResult<crate::UnitKind> {
    use crate::UnitKind;
    match tag {
        0 => Ok(UnitKind::Footman),
        1 => Ok(UnitKind::Archer),
        2 => Ok(UnitKind::Peasant),
        3 => Ok(UnitKind::TownHall),
        other => Err(SaveError::BadTag {
            section: "unit.kind",
            tag: u32::from(other),
        }),
    }
}

fn attack_type_tag(attack: data::AttackType) -> u8 {
    use data::AttackType;
    match attack {
        AttackType::Normal => 0,
        AttackType::Pierce => 1,
        AttackType::Siege => 2,
        AttackType::Magic => 3,
        AttackType::Chaos => 4,
        AttackType::Spells => 5,
        AttackType::Hero => 6,
    }
}

fn attack_type_from(tag: u8) -> SaveResult<data::AttackType> {
    use data::AttackType;
    match tag {
        0 => Ok(AttackType::Normal),
        1 => Ok(AttackType::Pierce),
        2 => Ok(AttackType::Siege),
        3 => Ok(AttackType::Magic),
        4 => Ok(AttackType::Chaos),
        5 => Ok(AttackType::Spells),
        6 => Ok(AttackType::Hero),
        other => Err(SaveError::BadTag {
            section: "projectile.attack",
            tag: u32::from(other),
        }),
    }
}

fn phase_tag(phase: crate::HarvestPhase) -> u8 {
    use crate::HarvestPhase;
    match phase {
        HarvestPhase::ToMine => 0,
        HarvestPhase::Gathering => 1,
        HarvestPhase::ToHall => 2,
    }
}

fn phase_from(tag: u8) -> SaveResult<crate::HarvestPhase> {
    use crate::HarvestPhase;
    match tag {
        0 => Ok(HarvestPhase::ToMine),
        1 => Ok(HarvestPhase::Gathering),
        2 => Ok(HarvestPhase::ToHall),
        other => Err(SaveError::BadTag {
            section: "unit.harvest_phase",
            tag: u32::from(other),
        }),
    }
}

fn spell_tag(spell: Spell) -> u8 {
    match spell {
        Spell::HolyLight => 0,
        Spell::DivineShield => 1,
        Spell::Blizzard => 2,
        Spell::Heal => 3,
        Spell::EntanglingRoots => 4,
        Spell::ChainLightning => 5,
        Spell::StormBolt => 6,
        Spell::ThunderClap => 7,
        Spell::DeathCoil => 8,
        Spell::FrostNova => 9,
        Spell::CarrionSwarm => 10,
        // Data-mapped hero abilities are cast by id through the generic path,
        // never queued as a typed Spell; u8::MAX is the save-file reject tag.
        Spell::DataDamage { .. }
        | Spell::DataHeal { .. }
        | Spell::DataBuff { .. }
        | Spell::DataSummon { .. }
        | Spell::DataAura { .. }
        | Spell::DataAuraPassive { .. }
        | Spell::DataTransformation { .. }
        | Spell::DataOnly { .. } => u8::MAX,
    }
}

fn spell_from(tag: u8) -> SaveResult<Spell> {
    match tag {
        0 => Ok(Spell::HolyLight),
        1 => Ok(Spell::DivineShield),
        2 => Ok(Spell::Blizzard),
        3 => Ok(Spell::Heal),
        4 => Ok(Spell::EntanglingRoots),
        5 => Ok(Spell::ChainLightning),
        6 => Ok(Spell::StormBolt),
        7 => Ok(Spell::ThunderClap),
        8 => Ok(Spell::DeathCoil),
        9 => Ok(Spell::FrostNova),
        10 => Ok(Spell::CarrionSwarm),
        other => Err(SaveError::BadTag {
            section: "cast.spell",
            tag: u32::from(other),
        }),
    }
}

fn ai_mode_tag(mode: AiMode) -> u8 {
    match mode {
        AiMode::Build => 0,
        AiMode::Attack => 1,
        AiMode::Retreat => 2,
    }
}

fn ai_mode_from(tag: u8) -> SaveResult<AiMode> {
    match tag {
        0 => Ok(AiMode::Build),
        1 => Ok(AiMode::Attack),
        2 => Ok(AiMode::Retreat),
        other => Err(SaveError::BadTag {
            section: "ai.mode",
            tag: u32::from(other),
        }),
    }
}

// ---------------------------------------------------------------------------
// Order codecs
// ---------------------------------------------------------------------------

fn write_order(w: &mut W, order: &Order) {
    match order {
        Order::Idle => w.u8(0),
        Order::Move(to) => {
            w.u8(1);
            w.v2(*to);
        }
        Order::Attack(target) => {
            w.u8(2);
            w.u32(target.0);
        }
        Order::MoveAttack(to) => {
            w.u8(3);
            w.v2(*to);
        }
        Order::Hold => w.u8(4),
        Order::Patrol { a, b, forward } => {
            w.u8(5);
            w.v2(*a);
            w.v2(*b);
            w.bool(*forward);
        }
        Order::Harvest { mine, phase } => {
            w.u8(6);
            w.v2(*mine);
            w.u8(phase_tag(*phase));
        }
        Order::HarvestLumber { tree, phase } => {
            w.u8(8);
            w.v2(*tree);
            w.u8(phase_tag(*phase));
        }
        Order::Build(site) => {
            w.u8(7);
            w.u32(site.0);
        }
        Order::Repair(target) => {
            w.u8(9);
            w.u32(target.0);
        }
    }
}

fn read_order(r: &mut R<'_>) -> SaveResult<Order> {
    let tag = r.u8()?;
    Ok(match tag {
        0 => Order::Idle,
        1 => Order::Move(r.v2()?),
        2 => Order::Attack(UnitId(r.u32()?)),
        3 => Order::MoveAttack(r.v2()?),
        4 => Order::Hold,
        5 => Order::Patrol {
            a: r.v2()?,
            b: r.v2()?,
            forward: r.bool("patrol.forward")?,
        },
        6 => Order::Harvest {
            mine: r.v2()?,
            phase: phase_from(r.u8()?)?,
        },
        8 => Order::HarvestLumber {
            tree: r.v2()?,
            phase: phase_from(r.u8()?)?,
        },
        7 => Order::Build(UnitId(r.u32()?)),
        9 => Order::Repair(UnitId(r.u32()?)),
        other => {
            return Err(SaveError::BadTag {
                section: "unit.order",
                tag: u32::from(other),
            });
        }
    })
}

fn write_order_spec(w: &mut W, spec: &OrderSpec) {
    match spec {
        OrderSpec::Move(to) => {
            w.u8(0);
            w.v2(*to);
        }
        OrderSpec::Attack(target) => {
            w.u8(1);
            w.u32(target.0);
        }
        OrderSpec::AttackMove(to) => {
            w.u8(2);
            w.v2(*to);
        }
        OrderSpec::Patrol(a, b) => {
            w.u8(3);
            w.v2(*a);
            w.v2(*b);
        }
        OrderSpec::Hold => w.u8(4),
        OrderSpec::Stop => w.u8(5),
        OrderSpec::Harvest(to) => {
            w.u8(6);
            w.v2(*to);
        }
        OrderSpec::HarvestLumber(at) => {
            w.u8(7);
            w.v2(*at);
        }
        OrderSpec::Build(site) => {
            w.u8(8);
            w.u32(site.0);
        }
        OrderSpec::Repair(target) => {
            w.u8(9);
            w.u32(target.0);
        }
    }
}

fn read_order_spec(r: &mut R<'_>) -> SaveResult<OrderSpec> {
    let tag = r.u8()?;
    Ok(match tag {
        0 => OrderSpec::Move(r.v2()?),
        1 => OrderSpec::Attack(UnitId(r.u32()?)),
        2 => OrderSpec::AttackMove(r.v2()?),
        3 => OrderSpec::Patrol(r.v2()?, r.v2()?),
        4 => OrderSpec::Hold,
        5 => OrderSpec::Stop,
        6 => OrderSpec::Harvest(r.v2()?),
        7 => OrderSpec::HarvestLumber(r.v2()?),
        8 => OrderSpec::Build(UnitId(r.u32()?)),
        9 => OrderSpec::Repair(UnitId(r.u32()?)),
        other => {
            return Err(SaveError::BadTag {
                section: "unit.order_queue",
                tag: u32::from(other),
            });
        }
    })
}

// ---------------------------------------------------------------------------
// Gameplay config
// ---------------------------------------------------------------------------

fn write_gameplay(w: &mut W, cfg: &crate::GameplayConfig) {
    w.u8(cfg.max_hero_level);
    w.u8(cfg.max_unit_level);
    write_u32_list(w, &cfg.need_hero_xp);
    write_u32_list(w, &cfg.grant_hero_xp);
    w.u32(cfg.grant_normal_xp);
    write_u32_list(w, &cfg.hero_factor_xp);
    w.f32(cfg.hero_exp_range);
    w.f32(cfg.str_attack_bonus);
    w.i32(cfg.str_hit_point_bonus);
    w.f32(cfg.str_regen_bonus);
    w.i32(cfg.int_mana_bonus);
    w.f32(cfg.int_regen_bonus);
    w.f32(cfg.agi_defense_bonus);
    w.f32(cfg.agi_defense_base);
    w.f32(cfg.agi_move_bonus);
    w.f32(cfg.min_unit_speed);
    w.f32(cfg.max_unit_speed);
    w.f32(cfg.min_bldg_speed);
    w.f32(cfg.max_bldg_speed);
    w.f32(cfg.frost_move_speed_decrease);
    w.f32(cfg.frost_attack_speed_decrease);
    w.f32(cfg.revive_base_factor);
    w.f32(cfg.revive_level_factor);
    w.f32(cfg.revive_base_lumber_factor);
    w.f32(cfg.revive_lumber_level_factor);
    w.f32(cfg.revive_max_factor);
    w.f32(cfg.revive_time_factor);
    w.f32(cfg.revive_max_time_factor);
    w.f32(cfg.awaken_base_factor);
    w.f32(cfg.awaken_level_factor);
    w.f32(cfg.awaken_base_lumber_factor);
    w.f32(cfg.awaken_lumber_level_factor);
    w.f32(cfg.awaken_max_factor);
    for value in cfg.need_hero_xp_formula {
        w.f32(value);
    }
    for value in cfg.grant_hero_xp_formula {
        w.f32(value);
    }
}

fn read_gameplay(r: &mut R<'_>) -> SaveResult<crate::GameplayConfig> {
    let mut cfg = crate::GameplayConfig::default();
    cfg.max_hero_level = r.u8()?;
    cfg.max_unit_level = r.u8()?;
    cfg.need_hero_xp = read_u32_list(r)?;
    cfg.grant_hero_xp = read_u32_list(r)?;
    cfg.grant_normal_xp = r.u32()?;
    cfg.hero_factor_xp = read_u32_list(r)?;
    cfg.hero_exp_range = r.f32()?;
    cfg.str_attack_bonus = r.f32()?;
    cfg.str_hit_point_bonus = r.i32()?;
    cfg.str_regen_bonus = r.f32()?;
    cfg.int_mana_bonus = r.i32()?;
    cfg.int_regen_bonus = r.f32()?;
    cfg.agi_defense_bonus = r.f32()?;
    cfg.agi_defense_base = r.f32()?;
    cfg.agi_move_bonus = r.f32()?;
    cfg.min_unit_speed = r.f32()?;
    cfg.max_unit_speed = r.f32()?;
    cfg.min_bldg_speed = r.f32()?;
    cfg.max_bldg_speed = r.f32()?;
    cfg.frost_move_speed_decrease = r.f32()?;
    cfg.frost_attack_speed_decrease = r.f32()?;
    cfg.revive_base_factor = r.f32()?;
    cfg.revive_level_factor = r.f32()?;
    cfg.revive_base_lumber_factor = r.f32()?;
    cfg.revive_lumber_level_factor = r.f32()?;
    cfg.revive_max_factor = r.f32()?;
    cfg.revive_time_factor = r.f32()?;
    cfg.revive_max_time_factor = r.f32()?;
    cfg.awaken_base_factor = r.f32()?;
    cfg.awaken_level_factor = r.f32()?;
    cfg.awaken_base_lumber_factor = r.f32()?;
    cfg.awaken_lumber_level_factor = r.f32()?;
    cfg.awaken_max_factor = r.f32()?;
    for slot in 0..3 {
        cfg.need_hero_xp_formula[slot] = r.f32()?;
    }
    for slot in 0..3 {
        cfg.grant_hero_xp_formula[slot] = r.f32()?;
    }
    Ok(cfg)
}

fn write_u32_list(w: &mut W, list: &[u32]) {
    w.len(list.len());
    for value in list {
        w.u32(*value);
    }
}

fn read_u32_list(r: &mut R<'_>) -> SaveResult<Vec<u32>> {
    let count = r.len()?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(r.u32()?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Entity codecs
// ---------------------------------------------------------------------------

fn write_unit(w: &mut W, unit: &Unit, version: u16) {
    w.u32(unit.id.0);
    w.u8(unit.owner);
    w.u8(unit_kind_tag(unit.kind));
    w.u16(unit.def_index);
    w.v2(unit.pos);
    w.i32(unit.hp);
    w.i32(unit.max_hp);
    w.i32(unit.mana);
    w.i32(unit.max_mana);
    w.i32(unit.armor);
    w.i32(unit.damage);
    w.f32(unit.range);
    w.f32(unit.speed);
    w.f32(unit.sight);
    w.u32(unit.attack_ticks);
    w.u32(unit.cooldown);
    w.u8(unit.level);
    w.u32(unit.xp);
    w.u8(unit.skill_points);
    w.len(unit.learned.len());
    for id in &unit.learned {
        w.str(id);
    }
    w.u64(unit.revive_at);
    w.opt_id(unit.revive_altar);
    w.u64(unit.channel_until);
    w.i32(unit.hero_strength);
    w.i32(unit.hero_agility);
    w.i32(unit.hero_intelligence);
    w.i32(unit.agi_armor_milli);
    w.len(unit.data_cd.len());
    for (id, cooldown) in &unit.data_cd {
        w.str(id);
        w.u32(*cooldown);
    }
    w.f32(unit.turn_rate);
    write_order(w, &unit.order);
    w.len(unit.order_queue.len());
    for spec in &unit.order_queue {
        write_order_spec(w, spec);
    }
    w.len(unit.path.len());
    for (x, y) in &unit.path {
        w.u32(*x);
        w.u32(*y);
    }
    w.len(unit.path_index);
    w.len(unit.production.len());
    for production in &unit.production {
        w.u8(unit_kind_tag(production.kind));
        w.u32(production.remaining);
        w.u16(production.def);
        w.bool(production.blocked);
    }
    w.f32(unit.facing);
    w.bool(unit.alive);
    w.opt_v2(unit.harvest);
    w.u32(unit.harvest_timer);
    w.i32(unit.carrying);
    w.i32(unit.carrying_lumber);
    w.opt_v2(unit.goal);
    match unit.patrol {
        Some((a, b)) => {
            w.u8(1);
            w.v2(a);
            w.v2(b);
        }
        None => w.u8(0),
    }
    w.len(unit.research.len());
    for research in &unit.research {
        w.str(research.id);
        w.u32(research.remaining);
    }
    w.len(unit.items.len());
    for item in &unit.items {
        w.u32(item.0);
    }
    match unit.camp {
        Some(camp) => {
            w.u8(1);
            w.u32(camp);
        }
        None => w.u8(0),
    }
    for cooldown in unit.spell_cd {
        w.u32(cooldown);
    }
    w.i32(unit.mana_milli);
    w.i32(unit.mana_regen_milli);
    w.u64(unit.rooted_until);
    w.u64(unit.stunned_until);
    w.u64(unit.invulnerable_until);
    match unit.construct {
        Some(construction) => {
            w.u8(1);
            w.u32(construction.remaining);
            w.u32(construction.total);
        }
        None => w.u8(0),
    }
    w.len(unit.blocked_cells.len());
    for (x, y) in &unit.blocked_cells {
        w.u32(*x);
        w.u32(*y);
    }
    w.opt_v2(unit.rally);
    w.u32(unit.stuck_ticks);
    w.u8(unit.stuck_attempts);
    w.v2(unit.last_pos);
    if version >= 3 {
        // Version 3: ethereal form is a real combat state (`EtherealDamageBonus`
        // / `EtherealHealBonus`), so a save taken mid-ability restores it.
        w.bool(unit.ethereal);
    }
    if version >= 5 {
        // Version 5: life-regeneration accumulator and rate (upgrade effects).
        w.i32(unit.hp_milli);
        w.i32(unit.hp_regen_milli);
    }
}

fn read_unit(r: &mut R<'_>, version: u16) -> SaveResult<Unit> {
    let id = UnitId(r.u32()?);
    let owner = r.u8()?;
    let kind = unit_kind_from(r.u8()?)?;
    let def_index = r.u16()?;
    let pos = r.v2()?;
    let hp = r.i32()?;
    let max_hp = r.i32()?;
    let mana = r.i32()?;
    let max_mana = r.i32()?;
    let armor = r.i32()?;
    let damage = r.i32()?;
    let range = r.f32()?;
    let speed = r.f32()?;
    let sight = r.f32()?;
    let attack_ticks = r.u32()?;
    let cooldown = r.u32()?;
    let level = r.u8()?;
    let xp = r.u32()?;
    let skill_points = r.u8()?;
    let learned_count = r.len()?;
    let mut learned = Vec::with_capacity(learned_count);
    for _ in 0..learned_count {
        let raw = r.str()?;
        learned.push(intern("unit.learned", raw)?);
    }
    let revive_at = r.u64()?;
    let revive_altar = r.opt_id()?;
    let channel_until = r.u64()?;
    let hero_strength = r.i32()?;
    let hero_agility = r.i32()?;
    let hero_intelligence = r.i32()?;
    let agi_armor_milli = r.i32()?;
    let data_cd_count = r.len()?;
    let mut data_cd = Vec::with_capacity(data_cd_count);
    for _ in 0..data_cd_count {
        let raw = r.str()?;
        let id = intern("unit.data_cd", raw)?;
        data_cd.push((id, r.u32()?));
    }
    let turn_rate = r.f32()?;
    let order = read_order(r)?;
    let queue_count = r.len()?;
    let mut order_queue = Vec::with_capacity(queue_count);
    for _ in 0..queue_count {
        order_queue.push(read_order_spec(r)?);
    }
    let path_count = r.len()?;
    let mut path = Vec::with_capacity(path_count);
    for _ in 0..path_count {
        path.push((r.u32()?, r.u32()?));
    }
    let path_index = r.count()?;
    let production_count = r.len()?;
    let mut production = Vec::with_capacity(production_count);
    for _ in 0..production_count {
        production.push(Production {
            kind: unit_kind_from(r.u8()?)?,
            remaining: r.u32()?,
            def: r.u16()?,
            blocked: r.bool("production.blocked")?,
        });
    }
    let facing = r.f32()?;
    let alive = r.bool("unit.alive")?;
    let harvest = r.opt_v2()?;
    let harvest_timer = r.u32()?;
    let carrying = r.i32()?;
    let carrying_lumber = r.i32()?;
    let goal = r.opt_v2()?;
    let patrol = match r.u8()? {
        0 => None,
        1 => Some((r.v2()?, r.v2()?)),
        other => {
            return Err(SaveError::InvalidValue {
                field: "unit.patrol",
                detail: format!("tag {other}"),
            });
        }
    };
    let research_count = r.len()?;
    let mut research = Vec::with_capacity(research_count);
    for _ in 0..research_count {
        let raw = r.str()?;
        let id = intern("unit.research", raw)?;
        research.push(Research {
            id,
            remaining: r.u32()?,
        });
    }
    let item_count = r.len()?;
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        items.push(ItemId(r.u32()?));
    }
    let camp = match r.u8()? {
        0 => None,
        1 => Some(r.u32()?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "unit.camp",
                detail: format!("tag {other}"),
            });
        }
    };
    let mut spell_cd = [0u32; SPELL_COUNT];
    for slot in spell_cd.iter_mut() {
        *slot = r.u32()?;
    }
    let mana_milli = r.i32()?;
    let mana_regen_milli = r.i32()?;
    let rooted_until = r.u64()?;
    let stunned_until = r.u64()?;
    let invulnerable_until = r.u64()?;
    let construct = match r.u8()? {
        0 => None,
        1 => Some(Construction {
            remaining: r.u32()?,
            total: r.u32()?,
        }),
        other => {
            return Err(SaveError::InvalidValue {
                field: "unit.construct",
                detail: format!("tag {other}"),
            });
        }
    };
    let blocked_count = r.len()?;
    let mut blocked_cells = Vec::with_capacity(blocked_count);
    for _ in 0..blocked_count {
        blocked_cells.push((r.u32()?, r.u32()?));
    }
    let rally = r.opt_v2()?;
    let stuck_ticks = r.u32()?;
    let stuck_attempts = r.u8()?;
    let last_pos = r.v2()?;
    let ethereal = if version >= 3 {
        r.bool("unit.ethereal")?
    } else {
        // v1/v2 predate the field: the unit was never saved in ethereal form.
        false
    };
    let (hp_milli, hp_regen_milli) = if version >= 5 {
        (r.i32()?, r.i32()?)
    } else {
        (0, 0)
    };

    Ok(Unit {
        id,
        owner,
        kind,
        def_index,
        pos,
        hp,
        max_hp,
        mana,
        max_mana,
        armor,
        damage,
        range,
        speed,
        sight,
        attack_ticks,
        cooldown,
        level,
        xp,
        skill_points,
        learned,
        revive_at,
        revive_altar,
        channel_until,
        hero_strength,
        hero_agility,
        hero_intelligence,
        agi_armor_milli,
        data_cd,
        turn_rate,
        order,
        order_queue,
        path,
        path_index,
        production,
        facing,
        alive,
        harvest,
        harvest_timer,
        carrying,
        carrying_lumber,
        goal,
        patrol,
        research,
        items,
        camp,
        spell_cd,
        mana_milli,
        mana_regen_milli,
        hp_milli,
        hp_regen_milli,
        rooted_until,
        stunned_until,
        invulnerable_until,
        // Version 3 stores the ethereal counter (`unit + 0x158` in the engine);
        // v1/v2 files decode to `false` above, exactly the state those writers
        // could represent.
        ethereal,
        construct,
        blocked_cells,
        rally,
        stuck_ticks,
        stuck_attempts,
        last_pos,
    })
}

fn write_projectile(w: &mut W, projectile: &Projectile) {
    w.u32(projectile.id.0);
    w.u8(projectile.owner);
    w.u32(projectile.source.0);
    w.opt_id(projectile.target);
    w.v2(projectile.pos);
    w.v2(projectile.start);
    w.v2(projectile.target_pos);
    w.f32(projectile.speed);
    w.f32(projectile.arc);
    w.bool(projectile.homing);
    w.i32(projectile.damage);
    w.u8(attack_type_tag(projectile.attack));
    w.str(projectile.art);
    w.f32(projectile.traveled);
    w.f32(projectile.total);
}

fn read_projectile(r: &mut R<'_>) -> SaveResult<Projectile> {
    let id = ProjectileId(r.u32()?);
    let owner = r.u8()?;
    let source = UnitId(r.u32()?);
    let target = r.opt_id()?;
    let pos = r.v2()?;
    let start = r.v2()?;
    let target_pos = r.v2()?;
    let speed = r.f32()?;
    let arc = r.f32()?;
    let homing = r.bool("projectile.homing")?;
    let damage = r.i32()?;
    let attack = attack_type_from(r.u8()?)?;
    let art_raw = r.str()?;
    let art = intern("projectile.art", art_raw)?;
    let traveled = r.f32()?;
    let total = r.f32()?;
    Ok(Projectile {
        id,
        owner,
        source,
        target,
        pos,
        start,
        target_pos,
        speed,
        arc,
        homing,
        damage,
        attack,
        art,
        traveled,
        total,
    })
}

fn write_ai_config(w: &mut W, config: &AiConfig) {
    w.len(config.workers);
    w.i32(config.army_supply);
    w.u64(config.first_attack_tick);
    w.u32(config.wave_interval);
    w.i32(config.retreat_permille);
    w.len(config.composition.len());
    for (id, weight) in config.composition {
        w.str(id);
        w.u32(*weight);
    }
    w.str(config.worker);
}

fn read_ai_config(r: &mut R<'_>) -> SaveResult<AiConfig> {
    let workers = r.count()?;
    let army_supply = r.i32()?;
    let first_attack_tick = r.u64()?;
    let wave_interval = r.u32()?;
    let retreat_permille = r.i32()?;
    let composition_count = r.len()?;
    let mut composition = Vec::with_capacity(composition_count);
    for _ in 0..composition_count {
        let raw = r.str()?;
        let id = intern("ai.composition", raw)?;
        composition.push((id, r.u32()?));
    }
    let worker_raw = r.str()?;
    let worker = intern("ai.worker", worker_raw)?;
    Ok(AiConfig {
        workers,
        army_supply,
        first_attack_tick,
        wave_interval,
        retreat_permille,
        composition: intern_composition(&composition)?,
        worker,
    })
}

fn write_ai_state(w: &mut W, state: &AiState) {
    w.u8(state.owner);
    write_ai_config(w, &state.config);
    w.u8(ai_mode_tag(state.mode));
    w.u32(state.waves);
    w.u64(state.next_wave);
    w.u64(state.next_train);
    w.u64(state.next_build);
    w.u64(state.next_expand);
    w.v2(state.home);
    w.len(state.train_cycle);
    w.bool(state.expanded);
    w.opt_v2(state.expand_mine);
    w.opt_id(state.expand_worker);
    w.opt_id(state.last_target);
}

fn read_ai_state(r: &mut R<'_>) -> SaveResult<AiState> {
    let owner = r.u8()?;
    let config = read_ai_config(r)?;
    let mode = ai_mode_from(r.u8()?)?;
    let waves = r.u32()?;
    let next_wave = r.u64()?;
    let next_train = r.u64()?;
    let next_build = r.u64()?;
    let next_expand = r.u64()?;
    let home = r.v2()?;
    let train_cycle = r.count()?;
    let expanded = r.bool("ai.expanded")?;
    let expand_mine = r.opt_v2()?;
    let expand_worker = r.opt_id()?;
    let last_target = r.opt_id()?;
    Ok(AiState {
        owner,
        config,
        mode,
        waves,
        next_wave,
        next_train,
        next_build,
        next_expand,
        home,
        train_cycle,
        expanded,
        expand_mine,
        expand_worker,
        last_target,
        // Not persisted: a watchdog timer restarts from the loaded tick.
        site_since: None,
    })
}

// ---------------------------------------------------------------------------
// Script section (version 4)
// ---------------------------------------------------------------------------

/// Stable wire order for [`HandleKind`]. The enum itself is `fieldless`, but a
/// save format must not depend on declaration order, so this table is the
/// contract and a test round-trips every entry.
const HANDLE_KINDS: [HandleKind; 41] = [
    HandleKind::Unit,
    HandleKind::Item,
    HandleKind::Destructable,
    HandleKind::Trigger,
    HandleKind::Timer,
    HandleKind::Group,
    HandleKind::Location,
    HandleKind::Rect,
    HandleKind::Region,
    HandleKind::Sound,
    HandleKind::Effect,
    HandleKind::Force,
    HandleKind::Boolexpr,
    HandleKind::ConditionFunc,
    HandleKind::TriggerCondition,
    HandleKind::Event,
    HandleKind::CameraSetup,
    HandleKind::FogModifier,
    HandleKind::Dialog,
    HandleKind::Button,
    HandleKind::Leaderboard,
    HandleKind::Multiboard,
    HandleKind::Quest,
    HandleKind::ItemPool,
    HandleKind::Trackable,
    HandleKind::UnitPool,
    HandleKind::WeatherEffect,
    HandleKind::TerrainDeformation,
    HandleKind::Image,
    HandleKind::Ubersplat,
    HandleKind::TextTag,
    HandleKind::Lightning,
    HandleKind::Hashtable,
    HandleKind::GameCache,
    HandleKind::Ability,
    HandleKind::Buff,
    HandleKind::StartLocation,
    HandleKind::TimerDialog,
    HandleKind::GameEvent,
    HandleKind::Player,
    HandleKind::Misc,
];

fn handle_kind_tag(kind: HandleKind) -> u8 {
    HANDLE_KINDS
        .iter()
        .position(|candidate| *candidate == kind)
        .expect("every HandleKind is listed in HANDLE_KINDS") as u8
}

fn read_handle_kind(r: &mut R<'_>) -> SaveResult<HandleKind> {
    let tag = r.u8()?;
    HANDLE_KINDS
        .get(tag as usize)
        .copied()
        .ok_or(SaveError::BadTag {
            section: "script.handle_kind",
            tag: u32::from(tag),
        })
}

fn write_value(w: &mut W, value: &Value) {
    match value {
        Value::Null => w.u8(0),
        Value::Int(v) => {
            w.u8(1);
            w.i32(*v);
        }
        Value::Real(v) => {
            w.u8(2);
            w.f32(*v);
        }
        Value::Bool(v) => {
            w.u8(3);
            w.bool(*v);
        }
        Value::Str(v) => {
            w.u8(4);
            w.str(v);
        }
        Value::Code(v) => {
            w.u8(5);
            w.u32(*v);
        }
        Value::Handle(handle) => {
            w.u8(6);
            w.u8(handle_kind_tag(handle.kind));
            w.u32(handle.id);
        }
    }
}

fn read_value(r: &mut R<'_>) -> SaveResult<Value> {
    Ok(match r.u8()? {
        0 => Value::Null,
        1 => Value::Int(r.i32()?),
        2 => Value::Real(r.f32()?),
        3 => Value::Bool(r.bool("script.value.bool")?),
        4 => Value::Str(r.str()?.to_string()),
        5 => Value::Code(r.u32()?),
        6 => {
            let kind = read_handle_kind(r)?;
            Value::Handle(Handle::new(kind, r.u32()?))
        }
        other => {
            return Err(SaveError::BadTag {
                section: "script.value",
                tag: u32::from(other),
            });
        }
    })
}

fn write_f64(w: &mut W, value: f64) {
    w.u64(value.to_bits());
}

fn read_f64(r: &mut R<'_>) -> SaveResult<f64> {
    Ok(f64::from_bits(r.u64()?))
}

fn write_opt_f64(w: &mut W, value: Option<f64>) {
    match value {
        Some(v) => {
            w.u8(1);
            write_f64(w, v);
        }
        None => w.u8(0),
    }
}

fn read_opt_f64(r: &mut R<'_>) -> SaveResult<Option<f64>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(read_f64(r)?)),
        other => Err(SaveError::InvalidValue {
            field: "script.opt_f64",
            detail: format!("tag {other}"),
        }),
    }
}

fn write_opt_u64(w: &mut W, value: Option<u64>) {
    match value {
        Some(v) => {
            w.u8(1);
            w.u64(v);
        }
        None => w.u8(0),
    }
}

fn read_opt_u64(r: &mut R<'_>) -> SaveResult<Option<u64>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.u64()?)),
        other => Err(SaveError::InvalidValue {
            field: "script.opt_u64",
            detail: format!("tag {other}"),
        }),
    }
}

fn write_opt_i64(w: &mut W, value: Option<i64>) {
    match value {
        Some(v) => {
            w.u8(1);
            w.u64(v as u64);
        }
        None => w.u8(0),
    }
}

fn read_opt_i64(r: &mut R<'_>) -> SaveResult<Option<i64>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.u64()? as i64)),
        other => Err(SaveError::InvalidValue {
            field: "script.opt_i64",
            detail: format!("tag {other}"),
        }),
    }
}

fn write_instr(w: &mut W, instr: &Instr) {
    match instr {
        Instr::Null => w.u8(0),
        Instr::Int(v) => {
            w.u8(1);
            w.i32(*v);
        }
        Instr::Real(v) => {
            w.u8(2);
            w.f32(*v);
        }
        Instr::Bool(v) => {
            w.u8(3);
            w.bool(*v);
        }
        Instr::Str(slot) => {
            w.u8(4);
            w.u32(*slot);
        }
        Instr::Code(index) => {
            w.u8(5);
            w.u32(*index);
        }
        Instr::PushLocal(slot) => {
            w.u8(6);
            w.u16(*slot);
        }
        Instr::PushGlobal(slot) => {
            w.u8(7);
            w.u32(*slot);
        }
        Instr::LoadLocalArray(slot) => {
            w.u8(8);
            w.u16(*slot);
        }
        Instr::LoadGlobalArray(slot) => {
            w.u8(9);
            w.u32(*slot);
        }
        Instr::StoreLocal(slot) => {
            w.u8(10);
            w.u16(*slot);
        }
        Instr::StoreGlobal(slot) => {
            w.u8(11);
            w.u32(*slot);
        }
        Instr::StoreLocalArray(slot) => {
            w.u8(12);
            w.u16(*slot);
        }
        Instr::StoreGlobalArray(slot) => {
            w.u8(13);
            w.u32(*slot);
        }
        Instr::Pop => w.u8(14),
        Instr::Dup => w.u8(15),
        Instr::Call { func, argc } => {
            w.u8(16);
            w.u32(*func);
            w.u8(*argc);
        }
        Instr::CallIndirect { argc } => {
            w.u8(17);
            w.u8(*argc);
        }
        Instr::Native { slot, argc } => {
            w.u8(18);
            w.u32(*slot);
            w.u8(*argc);
        }
        Instr::Unknown { pool, argc } => {
            w.u8(19);
            w.u32(*pool);
            w.u8(*argc);
        }
        Instr::Jump(target) => {
            w.u8(20);
            w.u32(*target);
        }
        Instr::JumpIfFalse(target) => {
            w.u8(21);
            w.u32(*target);
        }
        Instr::JumpIfTrue(target) => {
            w.u8(22);
            w.u32(*target);
        }
        Instr::Return => w.u8(23),
        Instr::Wait => w.u8(24),
        Instr::Bin(tag) => {
            w.u8(25);
            w.u8(bin_tag(*tag));
        }
        Instr::Un(tag) => {
            w.u8(26);
            w.u8(un_tag(*tag));
        }
        Instr::ResetLocal(slot) => {
            w.u8(27);
            w.u16(*slot);
        }
    }
}

fn bin_tag(tag: BinTag) -> u8 {
    match tag {
        BinTag::Add => 0,
        BinTag::Sub => 1,
        BinTag::Mul => 2,
        BinTag::Div => 3,
        BinTag::Eq => 4,
        BinTag::Ne => 5,
        BinTag::Lt => 6,
        BinTag::Le => 7,
        BinTag::Gt => 8,
        BinTag::Ge => 9,
        BinTag::And => 10,
        BinTag::Or => 11,
    }
}

fn un_tag(tag: UnTag) -> u8 {
    match tag {
        UnTag::Neg => 0,
        UnTag::Not => 1,
        UnTag::Pos => 2,
    }
}

fn read_instr(r: &mut R<'_>) -> SaveResult<Instr> {
    Ok(match r.u8()? {
        0 => Instr::Null,
        1 => Instr::Int(r.i32()?),
        2 => Instr::Real(r.f32()?),
        3 => Instr::Bool(r.bool("script.instr.bool")?),
        4 => Instr::Str(r.u32()?),
        5 => Instr::Code(r.u32()?),
        6 => Instr::PushLocal(r.u16()?),
        7 => Instr::PushGlobal(r.u32()?),
        8 => Instr::LoadLocalArray(r.u16()?),
        9 => Instr::LoadGlobalArray(r.u32()?),
        10 => Instr::StoreLocal(r.u16()?),
        11 => Instr::StoreGlobal(r.u32()?),
        12 => Instr::StoreLocalArray(r.u16()?),
        13 => Instr::StoreGlobalArray(r.u32()?),
        14 => Instr::Pop,
        15 => Instr::Dup,
        16 => Instr::Call {
            func: r.u32()?,
            argc: r.u8()?,
        },
        17 => Instr::CallIndirect { argc: r.u8()? },
        18 => Instr::Native {
            slot: r.u32()?,
            argc: r.u8()?,
        },
        19 => Instr::Unknown {
            pool: r.u32()?,
            argc: r.u8()?,
        },
        20 => Instr::Jump(r.u32()?),
        21 => Instr::JumpIfFalse(r.u32()?),
        22 => Instr::JumpIfTrue(r.u32()?),
        23 => Instr::Return,
        24 => Instr::Wait,
        25 => {
            let tag = match r.u8()? {
                0 => BinTag::Add,
                1 => BinTag::Sub,
                2 => BinTag::Mul,
                3 => BinTag::Div,
                4 => BinTag::Eq,
                5 => BinTag::Ne,
                6 => BinTag::Lt,
                7 => BinTag::Le,
                8 => BinTag::Gt,
                9 => BinTag::Ge,
                10 => BinTag::And,
                11 => BinTag::Or,
                other => {
                    return Err(SaveError::BadTag {
                        section: "script.instr.bin",
                        tag: u32::from(other),
                    });
                }
            };
            Instr::Bin(tag)
        }
        26 => {
            let tag = match r.u8()? {
                0 => UnTag::Neg,
                1 => UnTag::Not,
                2 => UnTag::Pos,
                other => {
                    return Err(SaveError::BadTag {
                        section: "script.instr.un",
                        tag: u32::from(other),
                    });
                }
            };
            Instr::Un(tag)
        }
        27 => Instr::ResetLocal(r.u16()?),
        other => {
            return Err(SaveError::BadTag {
                section: "script.instr",
                tag: u32::from(other),
            });
        }
    })
}

/// Fingerprint of the native registry: a save stores compiled instructions whose
/// `Native { slot }` indexes this table. If a build changes the table, restoring
/// the save would dispatch to the wrong handler, so the load refuses instead.
fn registry_fingerprint() -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for def in crate::jass::registry::table() {
        for byte in def.name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn write_code(w: &mut W, code: &Code) {
    w.len(code.functions.len());
    for function in &code.functions {
        w.str(&function.name);
        w.u32(function.params as u32);
        w.u32(function.nlocals as u32);
        w.len(function.code.len());
        for instr in &function.code {
            write_instr(w, instr);
        }
    }
    w.u32(code.init_function);
    w.len(code.globals.len());
    for global in &code.globals {
        w.str(&global.name);
        w.u8(match global.kind {
            GlobalKind::Scalar => 0,
            GlobalKind::Array => 1,
        });
    }
    w.len(code.strings.len());
    for text in &code.strings {
        w.str(text);
    }
    w.len(code.undeclared.len());
    for name in &code.undeclared {
        w.str(name);
    }
}

fn read_code(r: &mut R<'_>) -> SaveResult<Code> {
    let function_count = r.len()?;
    if function_count > 100_000 {
        return Err(SaveError::InvalidValue {
            field: "script.code.functions",
            detail: format!("{function_count} functions"),
        });
    }
    let mut functions = Vec::with_capacity(function_count);
    for _ in 0..function_count {
        let name = r.str()?.to_string();
        let params = r.count()?;
        let nlocals = r.count()?;
        let instr_count = r.len()?;
        let mut body = Vec::with_capacity(instr_count);
        for _ in 0..instr_count {
            body.push(read_instr(r)?);
        }
        functions.push(CodeFunc {
            name,
            params,
            nlocals,
            code: body,
        });
    }
    let init_function = r.u32()?;
    let global_count = r.len()?;
    let mut globals = Vec::with_capacity(global_count);
    for _ in 0..global_count {
        let name = r.str()?.to_string();
        let kind = match r.u8()? {
            0 => GlobalKind::Scalar,
            1 => GlobalKind::Array,
            other => {
                return Err(SaveError::BadTag {
                    section: "script.global.kind",
                    tag: u32::from(other),
                });
            }
        };
        globals.push(GlobalSlot { name, kind });
    }
    let string_count = r.len()?;
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        strings.push(r.str()?.to_string());
    }
    let undeclared_count = r.len()?;
    let mut undeclared = Vec::with_capacity(undeclared_count);
    for _ in 0..undeclared_count {
        undeclared.push(r.str()?.to_string());
    }
    // `by_name`/`global_by_name` are derived maps; rebuilding them from the
    // stored declaration order reproduces the compiler's last-wins behaviour.
    let mut by_name = BTreeMap::new();
    for (index, function) in functions.iter().enumerate() {
        by_name.insert(function.name.to_ascii_lowercase(), index as u32);
    }
    let mut global_by_name = BTreeMap::new();
    for (index, global) in globals.iter().enumerate() {
        global_by_name.insert(global.name.to_ascii_lowercase(), index as u32);
    }
    Ok(Code {
        functions,
        by_name,
        globals,
        global_by_name,
        strings,
        init_function,
        undeclared,
    })
}

/// Rejects a decoded program whose instructions index outside the tables they
/// address. A tampered save must fail with a typed error, never panic or run a
/// wild instruction stream.
fn validate_code(code: &Code) -> SaveResult<()> {
    if code.init_function as usize >= code.functions.len() {
        return Err(SaveError::OutOfRange {
            section: "script.code.init_function",
            index: code.init_function,
            limit: code.functions.len(),
        });
    }
    let native_count = crate::jass::registry::table().len();
    for (func_index, function) in code.functions.iter().enumerate() {
        if function.params > 255 {
            return Err(SaveError::InvalidValue {
                field: "script.code.params",
                detail: format!("function {func_index} declares {} parameters", function.params),
            });
        }
        if function.nlocals > u16::MAX as usize + 1 {
            return Err(SaveError::InvalidValue {
                field: "script.code.nlocals",
                detail: format!("function {func_index} has {} locals", function.nlocals),
            });
        }
        for (pc, instr) in function.code.iter().enumerate() {
            let out_of_range = |section: &'static str, index: u32, limit: usize| {
                SaveError::OutOfRange {
                    section,
                    index,
                    limit,
                }
            };
            match *instr {
                Instr::Str(slot) => {
                    if slot as usize >= code.strings.len() {
                        return Err(out_of_range(
                            "script.code.str",
                            slot,
                            code.strings.len(),
                        ));
                    }
                }
                Instr::Code(index) | Instr::Call { func: index, .. } => {
                    if index as usize >= code.functions.len() {
                        return Err(out_of_range(
                            "script.code.func",
                            index,
                            code.functions.len(),
                        ));
                    }
                }
                Instr::PushGlobal(slot)
                | Instr::LoadGlobalArray(slot)
                | Instr::StoreGlobal(slot)
                | Instr::StoreGlobalArray(slot) => {
                    if slot as usize >= code.globals.len() {
                        return Err(out_of_range(
                            "script.code.global",
                            slot,
                            code.globals.len(),
                        ));
                    }
                }
                Instr::PushLocal(slot)
                | Instr::LoadLocalArray(slot)
                | Instr::StoreLocal(slot)
                | Instr::StoreLocalArray(slot)
                | Instr::ResetLocal(slot) => {
                    if slot as usize >= function.nlocals {
                        return Err(out_of_range(
                            "script.code.local",
                            u32::from(slot),
                            function.nlocals,
                        ));
                    }
                }
                Instr::Native { slot, .. } => {
                    if slot as usize >= native_count {
                        return Err(out_of_range(
                            "script.code.native",
                            slot,
                            native_count,
                        ));
                    }
                }
                Instr::Unknown { pool, .. } => {
                    if pool as usize >= code.strings.len() {
                        return Err(out_of_range(
                            "script.code.unknown",
                            pool,
                            code.strings.len(),
                        ));
                    }
                }
                Instr::Jump(target)
                | Instr::JumpIfFalse(target)
                | Instr::JumpIfTrue(target) => {
                    if target as usize > function.code.len() {
                        return Err(out_of_range(
                            "script.code.jump",
                            target,
                            function.code.len(),
                        ));
                    }
                }
                Instr::Null
                | Instr::Int(_)
                | Instr::Real(_)
                | Instr::Bool(_)
                | Instr::CallIndirect { .. }
                | Instr::Pop
                | Instr::Dup
                | Instr::Return
                | Instr::Wait
                | Instr::Bin(_)
                | Instr::Un(_) => {}
            }
            let _ = pc;
        }
    }
    Ok(())
}

fn write_event_context(w: &mut W, event: &EventContext, version: u16) {
    w.opt_u32(event.trigger);
    w.u64(event.event_id as u64);
    w.opt_u32(event.unit);
    w.opt_u32(event.triggering_unit);
    w.opt_u32(event.dying_unit);
    w.opt_u32(event.entering_unit);
    w.opt_u32(event.leaving_unit);
    w.opt_u32(event.changing_unit);
    w.opt_u8(event.changing_unit_prev_owner);
    w.opt_u32(event.attacker);
    w.opt_u32(event.damaged);
    w.f32(event.damage);
    w.u64(event.spell_id as u64);
    w.opt_u32(event.spell_target_unit);
    w.opt_u32(event.trained_unit);
    w.u64(event.trained_unit_type as u64);
    w.opt_u32(event.expired_timer);
    w.opt_u8(event.player);
    w.opt_u32(event.dying_destructable);
    w.opt_u32(event.leveling_unit);
    w.opt_u32(event.killing_unit);
    w.opt_u32(event.cancelable);
    write_opt_i64(w, event.order_id);
    if version >= 6 {
        write_opt_i64(w, event.game_state.map(i64::from));
    }
}

fn read_event_context(r: &mut R<'_>, version: u16) -> SaveResult<EventContext> {
    Ok(EventContext {
        trigger: r.opt_u32()?,
        event_id: r.u64()? as i64,
        unit: r.opt_u32()?,
        triggering_unit: r.opt_u32()?,
        dying_unit: r.opt_u32()?,
        entering_unit: r.opt_u32()?,
        leaving_unit: r.opt_u32()?,
        changing_unit: r.opt_u32()?,
        changing_unit_prev_owner: r.opt_u8()?,
        attacker: r.opt_u32()?,
        damaged: r.opt_u32()?,
        damage: r.f32()?,
        spell_id: r.u64()? as i64,
        spell_target_unit: r.opt_u32()?,
        trained_unit: r.opt_u32()?,
        trained_unit_type: r.u64()? as i64,
        expired_timer: r.opt_u32()?,
        player: r.opt_u8()?,
        dying_destructable: r.opt_u32()?,
        leveling_unit: r.opt_u32()?,
        killing_unit: r.opt_u32()?,
        cancelable: r.opt_u32()?,
        order_id: read_opt_i64(r)?,
        game_state: if version >= 6 {
            read_opt_i64(r)?.map(|state| state as i32)
        } else {
            None
        },
    })
}

fn write_thread(w: &mut W, thread: &ThreadState, version: u16) {
    w.str(&thread.name);
    write_f64(w, thread.started_at);
    w.u64(thread.steps);
    match &thread.event {
        Some(event) => {
            w.u8(1);
            write_event_context(w, event, version);
        }
        None => w.u8(0),
    }
    match &thread.last_value {
        Some(value) => {
            w.u8(1);
            write_value(w, value);
        }
        None => w.u8(0),
    }
    write_opt_f64(w, thread.wake_at);
    write_opt_u64(w, thread.wake_tick);
    write_opt_f64(w, thread.wait_started);
    w.len(thread.frames.len());
    for frame in &thread.frames {
        w.u32(frame.func);
        w.u32(frame.pc as u32);
        w.len(frame.locals.len());
        for value in &frame.locals {
            write_value(w, value);
        }
        w.u32(frame.stack_base as u32);
    }
    w.len(thread.stack.len());
    for value in &thread.stack {
        write_value(w, value);
    }
}

fn read_thread(r: &mut R<'_>, code: &Code, version: u16) -> SaveResult<ThreadState> {
    let name = r.str()?.to_string();
    let started_at = read_f64(r)?;
    let steps = r.u64()?;
    let event = match r.u8()? {
        0 => None,
        1 => Some(read_event_context(r, version)?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.thread.event",
                detail: format!("tag {other}"),
            });
        }
    };
    let last_value = match r.u8()? {
        0 => None,
        1 => Some(read_value(r)?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.thread.last_value",
                detail: format!("tag {other}"),
            });
        }
    };
    let wake_at = read_opt_f64(r)?;
    let wake_tick = read_opt_u64(r)?;
    let wait_started = read_opt_f64(r)?;
    let frame_count = r.len()?;
    if frame_count > 4096 {
        return Err(SaveError::InvalidValue {
            field: "script.thread.frames",
            detail: format!("{frame_count} frames"),
        });
    }
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let func = r.u32()?;
        let pc = r.count()?;
        let local_count = r.len()?;
        let mut locals = Vec::with_capacity(local_count);
        for _ in 0..local_count {
            locals.push(read_value(r)?);
        }
        let stack_base = r.count()?;
        let Some(code_function) = code.functions.get(func as usize) else {
            return Err(SaveError::OutOfRange {
                section: "script.thread.func",
                index: func,
                limit: code.functions.len(),
            });
        };
        if pc > code_function.code.len() {
            return Err(SaveError::OutOfRange {
                section: "script.thread.pc",
                index: pc as u32,
                limit: code_function.code.len(),
            });
        }
        if locals.len() != code_function.nlocals {
            return Err(SaveError::InvalidValue {
                field: "script.thread.locals",
                detail: format!(
                    "function {func} frame has {} locals, the program declares {}",
                    locals.len(),
                    code_function.nlocals
                ),
            });
        }
        frames.push(Frame {
            func,
            pc,
            locals,
            stack_base,
        });
    }
    let stack_count = r.len()?;
    let mut stack = Vec::with_capacity(stack_count);
    for _ in 0..stack_count {
        stack.push(read_value(r)?);
    }
    for frame in &frames {
        if frame.stack_base > stack.len() {
            return Err(SaveError::OutOfRange {
                section: "script.thread.stack_base",
                index: frame.stack_base as u32,
                limit: stack.len(),
            });
        }
    }
    Ok(ThreadState {
        frames,
        stack,
        wake_at,
        wake_tick,
        wait_started,
        name,
        started_at,
        steps,
        event,
        last_value,
    })
}

fn write_timer_state(w: &mut W, timer: &TimerState) {
    w.f32(timer.timeout);
    w.u32(timer.ticks_remaining);
    w.u32(timer.period_ticks);
    w.bool(timer.periodic);
    w.bool(timer.running);
    w.opt_u32(timer.callback);
    w.u32(timer.handle);
}

fn read_timer_state(r: &mut R<'_>) -> SaveResult<TimerState> {
    Ok(TimerState {
        timeout: r.f32()?,
        ticks_remaining: r.u32()?,
        period_ticks: r.u32()?,
        periodic: r.bool("script.timer.periodic")?,
        running: r.bool("script.timer.running")?,
        callback: r.opt_u32()?,
        handle: r.u32()?,
    })
}

fn write_reg_event(w: &mut W, event: &RegEvent) {
    match event {
        RegEvent::TimerEvent {
            period,
            period_ticks,
            periodic,
            elapsed,
        } => {
            w.u8(0);
            w.f32(*period);
            w.u32(*period_ticks);
            w.bool(*periodic);
            w.u32(*elapsed);
        }
        RegEvent::TimerExpire { timer } => {
            w.u8(1);
            w.u32(*timer);
        }
        RegEvent::UnitEvent { unit, event } => {
            w.u8(2);
            w.u32(*unit);
            w.u64(*event as u64);
        }
        RegEvent::AnyUnitEvent { event, player } => {
            w.u8(3);
            w.u64(*event as u64);
            w.opt_u8(*player);
        }
        RegEvent::PlayerEvent { player, event } => {
            w.u8(4);
            w.u8(*player);
            w.u64(*event as u64);
        }
        RegEvent::RegionEnter { region } => {
            w.u8(5);
            w.u32(*region);
        }
        RegEvent::RegionLeave { region } => {
            w.u8(6);
            w.u32(*region);
        }
        RegEvent::DestructableDeath { destructable } => {
            w.u8(7);
            w.opt_u32(*destructable);
        }
        RegEvent::Chat {
            player,
            message,
            exact,
        } => {
            w.u8(8);
            w.u8(*player);
            w.str(message);
            w.bool(*exact);
        }
        RegEvent::DialogEvent { dialog } => {
            w.u8(9);
            w.u32(*dialog);
        }
        RegEvent::UnitInRange { unit, range, filter } => {
            w.u8(10);
            w.u32(*unit);
            w.f32(*range);
            w.opt_u32(*filter);
        }
        RegEvent::PlayerAllianceChange { player, alliance } => {
            w.u8(11);
            w.u8(*player);
            w.u32(*alliance as u32);
        }
        RegEvent::PlayerStateEvent {
            player,
            state,
            op,
            value,
        } => {
            w.u8(12);
            w.u8(*player);
            w.u32(*state as u32);
            w.u32(*op as u32);
            w.f32(*value);
        }
        RegEvent::GameStateEvent { state, op, value } => {
            w.u8(13);
            w.u32(*state as u32);
            w.u32(*op as u32);
            w.f32(*value);
        }
        RegEvent::GameEvent { event } => {
            w.u8(14);
            w.u64(*event as u64);
        }
    }
}

fn read_reg_event(r: &mut R<'_>) -> SaveResult<RegEvent> {
    Ok(match r.u8()? {
        0 => RegEvent::TimerEvent {
            period: r.f32()?,
            period_ticks: r.u32()?,
            periodic: r.bool("script.event.periodic")?,
            elapsed: r.u32()?,
        },
        1 => RegEvent::TimerExpire { timer: r.u32()? },
        2 => RegEvent::UnitEvent {
            unit: r.u32()?,
            event: r.u64()? as i64,
        },
        3 => RegEvent::AnyUnitEvent {
            event: r.u64()? as i64,
            player: r.opt_u8()?,
        },
        4 => RegEvent::PlayerEvent {
            player: r.u8()?,
            event: r.u64()? as i64,
        },
        5 => RegEvent::RegionEnter { region: r.u32()? },
        6 => RegEvent::RegionLeave { region: r.u32()? },
        7 => RegEvent::DestructableDeath {
            destructable: r.opt_u32()?,
        },
        8 => RegEvent::Chat {
            player: r.u8()?,
            message: r.str()?.to_string(),
            exact: r.bool("script.event.exact")?,
        },
        9 => RegEvent::DialogEvent { dialog: r.u32()? },
        10 => RegEvent::UnitInRange {
            unit: r.u32()?,
            range: r.f32()?,
            filter: r.opt_u32()?,
        },
        11 => RegEvent::PlayerAllianceChange {
            player: r.u8()?,
            alliance: r.u32()? as i32,
        },
        12 => RegEvent::PlayerStateEvent {
            player: r.u8()?,
            state: r.u32()? as i32,
            op: r.u32()? as i32,
            value: r.f32()?,
        },
        13 => RegEvent::GameStateEvent {
            state: r.u32()? as i32,
            op: r.u32()? as i32,
            value: r.f32()?,
        },
        14 => RegEvent::GameEvent {
            event: r.u64()? as i64,
        },
        other => {
            return Err(SaveError::BadTag {
                section: "script.event",
                tag: u32::from(other),
            });
        }
    })
}

fn write_trigger_state(w: &mut W, trigger: &TriggerState) {
    w.bool(trigger.enabled);
    w.len(trigger.actions.len());
    for action in &trigger.actions {
        w.u32(*action);
    }
    w.len(trigger.conditions.len());
    for condition in &trigger.conditions {
        w.u32(*condition);
    }
    w.len(trigger.events.len());
    for event in &trigger.events {
        write_reg_event(w, event);
    }
    w.bool(trigger.running);
}

fn read_trigger_state(r: &mut R<'_>, code_function_count: usize) -> SaveResult<TriggerState> {
    let enabled = r.bool("script.trigger.enabled")?;
    let action_count = r.len()?;
    let mut actions = Vec::with_capacity(action_count);
    for _ in 0..action_count {
        let action = r.u32()?;
        if action as usize >= code_function_count {
            return Err(SaveError::OutOfRange {
                section: "script.trigger.action",
                index: action,
                limit: code_function_count,
            });
        }
        actions.push(action);
    }
    let condition_count = r.len()?;
    let mut conditions = Vec::with_capacity(condition_count);
    for _ in 0..condition_count {
        let condition = r.u32()?;
        if condition as usize >= code_function_count {
            return Err(SaveError::OutOfRange {
                section: "script.trigger.condition",
                index: condition,
                limit: code_function_count,
            });
        }
        conditions.push(condition);
    }
    let event_count = r.len()?;
    let mut events = Vec::with_capacity(event_count);
    for _ in 0..event_count {
        events.push(read_reg_event(r)?);
    }
    let running = r.bool("script.trigger.running")?;
    // Between ticks no trigger is executing; a save that says otherwise was
    // written mid-execution and cannot be resumed faithfully.
    if running {
        return Err(SaveError::InvalidValue {
            field: "script.trigger.running",
            detail: "a trigger was executing when the save was written".into(),
        });
    }
    Ok(TriggerState {
        enabled,
        actions,
        conditions,
        events,
        running,
    })
}

fn write_sound_state(w: &mut W, sound: &SoundState) {
    w.str(&sound.path);
    w.i32(sound.duration_ms);
    w.i32(sound.volume);
    w.f32(sound.pitch);
    w.opt_v2(sound.position);
    w.opt_u32(sound.unit);
    w.bool(sound.playing);
    w.i32(sound.channel);
}

fn read_sound_state(r: &mut R<'_>) -> SaveResult<SoundState> {
    Ok(SoundState {
        path: r.str()?.to_string(),
        duration_ms: r.i32()?,
        volume: r.i32()?,
        pitch: r.f32()?,
        position: r.opt_v2()?,
        unit: r.opt_u32()?,
        playing: r.bool("script.sound.playing")?,
        channel: r.i32()?,
    })
}

fn write_camera_setup(w: &mut W, camera: &CameraSetup) {
    w.len(camera.fields.len());
    for (field, value) in &camera.fields {
        w.u64(*field as u64);
        write_value(w, value);
    }
}

fn read_camera_setup(r: &mut R<'_>) -> SaveResult<CameraSetup> {
    let count = r.len()?;
    let mut fields = BTreeMap::new();
    for _ in 0..count {
        let field = r.u64()? as i64;
        fields.insert(field, read_value(r)?);
    }
    Ok(CameraSetup { fields })
}

fn write_object(w: &mut W, object: &Object, version: u16) {
    match object {
        Object::Group(members) => {
            w.u8(0);
            w.len(members.len());
            for handle in members {
                w.u32(*handle);
            }
        }
        Object::Location(pos) => {
            w.u8(1);
            w.v2(*pos);
        }
        Object::Rect { min, max } => {
            w.u8(2);
            w.v2(*min);
            w.v2(*max);
        }
        Object::Region { rects } => {
            w.u8(3);
            w.len(rects.len());
            for rect in rects {
                w.u32(*rect);
            }
        }
        Object::Timer(timer) => {
            w.u8(4);
            write_timer_state(w, timer);
        }
        Object::Trigger(trigger) => {
            w.u8(5);
            write_trigger_state(w, trigger);
        }
        Object::Sound(sound) => {
            w.u8(6);
            write_sound_state(w, sound);
        }
        Object::Force(players) => {
            w.u8(7);
            w.bytes(players);
        }
        Object::Boolexpr { func, negated } => {
            w.u8(8);
            w.opt_u32(*func);
            w.bool(*negated);
        }
        Object::TriggerCondition { func, trigger } => {
            w.u8(9);
            w.opt_u32(*func);
            w.u32(*trigger);
        }
        Object::CameraSetup(camera) => {
            w.u8(10);
            write_camera_setup(w, camera);
        }
        Object::FogModifier { rect, enabled } => {
            w.u8(11);
            w.u32(*rect);
            w.bool(*enabled);
        }
        Object::Dialog {
            message,
            shown_to,
            asynchronous,
            buttons,
        } => {
            w.u8(12);
            w.len(buttons.len());
            for button in buttons {
                w.u32(*button);
            }
            // Version 6: the headless UI state the pre-v6 writer dropped.
            if version >= 6 {
                w.str(message);
                w.opt_u8(*shown_to);
                w.bool(*asynchronous);
            }
        }
        Object::Button { text, hotkey, quit } => {
            w.u8(13);
            w.str(text);
            w.i32(*hotkey);
            if version >= 6 {
                w.bool(*quit);
            }
        }
        Object::Leaderboard { rows } => {
            w.u8(14);
            w.len(rows.len());
            for (row, (label, value, icon)) in rows {
                w.i32(*row);
                w.str(label);
                w.i32(*value);
                w.str(icon);
            }
        }
        Object::Multiboard { rows } => {
            w.u8(15);
            w.len(rows.len());
            for (row, (label, value)) in rows {
                w.i32(*row);
                w.str(label);
                w.i32(*value);
            }
        }
        Object::Quest {
            title,
            description,
            icon,
            required,
            discovered,
            completed,
            items,
        } => {
            w.u8(16);
            w.str(title);
            w.str(description);
            w.str(icon);
            w.bool(*required);
            w.bool(*discovered);
            w.bool(*completed);
            w.len(items.len());
            for item in items {
                w.u32(*item);
            }
        }
        Object::Hashtable(cells) => {
            w.u8(17);
            w.len(cells.len());
            for (key, value) in cells {
                w.u64(key.0 as u64);
                w.u64(key.1 as u64);
                write_value(w, value);
            }
        }
        Object::ItemPool { items, mode } => {
            w.u8(18);
            w.len(items.len());
            for item in items {
                w.u64(*item as u64);
            }
            w.i32(*mode);
        }
        Object::UnitPool { units, mode } => {
            w.u8(19);
            w.len(units.len());
            for unit in units {
                w.u64(*unit as u64);
            }
            w.i32(*mode);
        }
        Object::TextTag {
            text,
            pos,
            lifespan,
        } => {
            w.u8(20);
            w.str(text);
            w.v2(*pos);
            w.f32(*lifespan);
        }
        Object::Trackable { pos } => {
            w.u8(21);
            w.v2(*pos);
        }
        Object::WeatherEffect { rawcode, enabled } => {
            w.u8(22);
            w.u64(*rawcode as u64);
            w.bool(*enabled);
        }
        Object::TerrainDeformation { radius, duration } => {
            w.u8(23);
            w.f32(*radius);
            w.f32(*duration);
        }
        Object::GameEvent { id } => {
            w.u8(24);
            w.u64(*id as u64);
        }
        Object::StartLocation(index) => {
            w.u8(25);
            w.i32(*index);
        }
        Object::TimerDialog {
            timer,
            title,
            shown,
            speed,
            remaining_override,
            time_color,
            title_color,
        } => {
            w.u8(26);
            w.u32(*timer);
            w.str(title);
            if version >= 6 {
                w.bool(*shown);
                w.f32(*speed);
                w.opt_f32(*remaining_override);
                for channel in time_color.iter().chain(title_color.iter()) {
                    w.i32(*channel);
                }
            }
        }
        Object::Ability { id, level } => {
            w.u8(27);
            w.u64(*id as u64);
            w.i32(*level);
        }
        Object::Buff { rawcode } => {
            w.u8(28);
            w.u64(*rawcode as u64);
        }
        Object::Image { pos, file } => {
            w.u8(29);
            w.v2(*pos);
            w.str(file);
        }
        Object::Ubersplat { pos, rawcode } => {
            w.u8(30);
            w.v2(*pos);
            w.u64(*rawcode as u64);
        }
        Object::Lightning { code } => {
            w.u8(31);
            w.str(code);
        }
        Object::Opaque => w.u8(32),
    }
}

fn read_object(r: &mut R<'_>, code_function_count: usize, version: u16) -> SaveResult<Object> {
    Ok(match r.u8()? {
        0 => {
            let count = r.len()?;
            let mut members = Vec::with_capacity(count);
            for _ in 0..count {
                members.push(r.u32()?);
            }
            Object::Group(members)
        }
        1 => Object::Location(r.v2()?),
        2 => Object::Rect {
            min: r.v2()?,
            max: r.v2()?,
        },
        3 => {
            let count = r.len()?;
            let mut rects = Vec::with_capacity(count);
            for _ in 0..count {
                rects.push(r.u32()?);
            }
            Object::Region { rects }
        }
        4 => Object::Timer(read_timer_state(r)?),
        5 => {
            let trigger = read_trigger_state(r, code_function_count)?;
            Object::Trigger(trigger)
        }
        6 => Object::Sound(read_sound_state(r)?),
        7 => Object::Force(r.bytes()?.to_vec()),
        8 => Object::Boolexpr {
            func: r.opt_u32()?,
            negated: r.bool("script.object.negated")?,
        },
        9 => Object::TriggerCondition {
            func: r.opt_u32()?,
            trigger: r.u32()?,
        },
        10 => Object::CameraSetup(read_camera_setup(r)?),
        11 => Object::FogModifier {
            rect: r.u32()?,
            enabled: r.bool("script.object.fog_enabled")?,
        },
        12 => {
            let count = r.len()?;
            let mut buttons = Vec::with_capacity(count);
            for _ in 0..count {
                buttons.push(r.u32()?);
            }
            // Version 6 appended the dialog's own state; a v5 file restores the
            // neutral value (no message, hidden, not async).
            let (message, shown_to, asynchronous) = if version >= 6 {
                (
                    r.str()?.to_string(),
                    r.opt_u8()?,
                    r.bool("script.dialog.async")?,
                )
            } else {
                (String::new(), None, false)
            };
            Object::Dialog {
                message,
                shown_to,
                asynchronous,
                buttons,
            }
        }
        13 => {
            let text = r.str()?.to_string();
            let hotkey = r.i32()?;
            let quit = if version >= 6 {
                r.bool("script.button.quit")?
            } else {
                false
            };
            Object::Button { text, hotkey, quit }
        }
        14 => {
            let count = r.len()?;
            let mut rows = BTreeMap::new();
            for _ in 0..count {
                let row = r.i32()?;
                let label = r.str()?.to_string();
                let value = r.i32()?;
                let icon = r.str()?.to_string();
                rows.insert(row, (label, value, icon));
            }
            Object::Leaderboard { rows }
        }
        15 => {
            let count = r.len()?;
            let mut rows = BTreeMap::new();
            for _ in 0..count {
                let row = r.i32()?;
                let label = r.str()?.to_string();
                rows.insert(row, (label, r.i32()?));
            }
            Object::Multiboard { rows }
        }
        16 => {
            let title = r.str()?.to_string();
            let description = r.str()?.to_string();
            let icon = r.str()?.to_string();
            let required = r.bool("script.object.quest_required")?;
            let discovered = r.bool("script.object.quest_discovered")?;
            let completed = r.bool("script.object.quest_completed")?;
            let count = r.len()?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(r.u32()?);
            }
            Object::Quest {
                title,
                description,
                icon,
                required,
                discovered,
                completed,
                items,
            }
        }
        17 => {
            let count = r.len()?;
            let mut cells = BTreeMap::new();
            for _ in 0..count {
                let key = (r.u64()? as i64, r.u64()? as i64);
                cells.insert(key, read_value(r)?);
            }
            Object::Hashtable(cells)
        }
        18 => {
            let count = r.len()?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(r.u64()? as i64);
            }
            Object::ItemPool {
                items,
                mode: r.i32()?,
            }
        }
        19 => {
            let count = r.len()?;
            let mut units = Vec::with_capacity(count);
            for _ in 0..count {
                units.push(r.u64()? as i64);
            }
            Object::UnitPool {
                units,
                mode: r.i32()?,
            }
        }
        20 => Object::TextTag {
            text: r.str()?.to_string(),
            pos: r.v2()?,
            lifespan: r.f32()?,
        },
        21 => Object::Trackable { pos: r.v2()? },
        22 => Object::WeatherEffect {
            rawcode: r.u64()? as i64,
            enabled: r.bool("script.object.weather_enabled")?,
        },
        23 => Object::TerrainDeformation {
            radius: r.f32()?,
            duration: r.f32()?,
        },
        24 => Object::GameEvent { id: r.u64()? as i64 },
        25 => Object::StartLocation(r.i32()?),
        26 => {
            let timer = r.u32()?;
            let title = r.str()?.to_string();
            let mut shown = false;
            let mut speed = 1.0;
            let mut remaining = None;
            let mut time_color = [255, 255, 255, 255];
            let mut title_color = [255, 255, 255, 255];
            if version >= 6 {
                shown = r.bool("script.timerdialog.shown")?;
                speed = r.f32()?;
                remaining = r.opt_f32()?;
                let mut channels = [255i32; 8];
                for channel in &mut channels {
                    *channel = r.i32()?;
                }
                time_color = [channels[0], channels[1], channels[2], channels[3]];
                title_color = [channels[4], channels[5], channels[6], channels[7]];
            }
            Object::TimerDialog {
                timer,
                title,
                shown,
                speed,
                remaining_override: remaining,
                time_color,
                title_color,
            }
        }
        27 => Object::Ability {
            id: r.u64()? as i64,
            level: r.i32()?,
        },
        28 => Object::Buff {
            rawcode: r.u64()? as i64,
        },
        29 => Object::Image {
            pos: r.v2()?,
            file: r.str()?.to_string(),
        },
        30 => Object::Ubersplat {
            pos: r.v2()?,
            rawcode: r.u64()? as i64,
        },
        31 => Object::Lightning {
            code: r.str()?.to_string(),
        },
        32 => Object::Opaque,
        other => {
            return Err(SaveError::BadTag {
                section: "script.object",
                tag: u32::from(other),
            });
        }
    })
}

fn write_unit_meta(w: &mut W, meta: &UnitMeta, version: u16) {
    w.f32(meta.acquire_range);
    w.bool(meta.hidden);
    w.i32(meta.color);
    w.i32(meta.vertex_color.0);
    w.i32(meta.vertex_color.1);
    w.i32(meta.vertex_color.2);
    w.i32(meta.vertex_color.3);
    w.f32(meta.scale);
    w.bool(meta.pathing);
    w.bool(meta.invulnerable);
    w.f32(meta.time_scale);
    w.f32(meta.move_speed);
    w.f32(meta.facing);
    match meta.facing_timed {
        Some((facing, duration)) => {
            w.u8(1);
            w.f32(facing);
            w.f32(duration);
        }
        None => w.u8(0),
    }
    w.i32(meta.user_data);
    w.i32(meta.custom_value);
    match meta.timed_life {
        Some(seconds) => {
            w.u8(1);
            w.f32(seconds);
        }
        None => w.u8(0),
    }
    w.bool(meta.paused);
    w.str(&meta.animation);
    w.i32(meta.resource_amount);
    match meta.max_life_override {
        Some(value) => {
            w.u8(1);
            w.i32(value);
        }
        None => w.u8(0),
    }
    match meta.max_mana_override {
        Some(value) => {
            w.u8(1);
            w.i32(value);
        }
        None => w.u8(0),
    }
    w.bool(meta.selected);
    w.opt_str(meta.name_override.as_deref());
    w.bool(meta.summoned);
    w.len(meta.abilities.len());
    for ability in &meta.abilities {
        w.u64(*ability as u64);
    }
    w.bool(meta.xp_suspended);
    if version >= 7 {
        let (added, removed) = meta.types.bits();
        w.u32(added);
        w.u32(removed);
    }
}

fn read_unit_meta(r: &mut R<'_>, version: u16) -> SaveResult<UnitMeta> {
    let acquire_range = r.f32()?;
    let hidden = r.bool("script.unit_meta.hidden")?;
    let color = r.i32()?;
    let vertex_color = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
    let scale = r.f32()?;
    let pathing = r.bool("script.unit_meta.pathing")?;
    let invulnerable = r.bool("script.unit_meta.invulnerable")?;
    let time_scale = r.f32()?;
    let move_speed = r.f32()?;
    let facing = r.f32()?;
    let facing_timed = match r.u8()? {
        0 => None,
        1 => Some((r.f32()?, r.f32()?)),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.unit_meta.facing_timed",
                detail: format!("tag {other}"),
            });
        }
    };
    let user_data = r.i32()?;
    let custom_value = r.i32()?;
    let timed_life = match r.u8()? {
        0 => None,
        1 => Some(r.f32()?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.unit_meta.timed_life",
                detail: format!("tag {other}"),
            });
        }
    };
    let paused = r.bool("script.unit_meta.paused")?;
    let animation = r.str()?.to_string();
    let resource_amount = r.i32()?;
    let max_life_override = match r.u8()? {
        0 => None,
        1 => Some(r.i32()?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.unit_meta.max_life_override",
                detail: format!("tag {other}"),
            });
        }
    };
    let max_mana_override = match r.u8()? {
        0 => None,
        1 => Some(r.i32()?),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.unit_meta.max_mana_override",
                detail: format!("tag {other}"),
            });
        }
    };
    let selected = r.bool("script.unit_meta.selected")?;
    let name_override = r.opt_str()?.map(str::to_string);
    let summoned = r.bool("script.unit_meta.summoned")?;
    let ability_count = r.len()?;
    let mut abilities = Vec::with_capacity(ability_count);
    for _ in 0..ability_count {
        abilities.push(r.u64()? as i64);
    }
    let xp_suspended = r.bool("script.unit_meta.xp_suspended")?;
    // Version 7 appends the runtime `UNIT_TYPE_*` delta; older files never
    // stored it, so the classification is the definition's own.
    let types = if version >= 7 {
        UnitTypes::from_bits(r.u32()?, r.u32()?)
    } else {
        UnitTypes::default()
    };
    Ok(UnitMeta {
        acquire_range,
        hidden,
        color,
        vertex_color,
        scale,
        pathing,
        invulnerable,
        time_scale,
        move_speed,
        facing,
        facing_timed,
        user_data,
        custom_value,
        timed_life,
        paused,
        animation,
        resource_amount,
        max_life_override,
        max_mana_override,
        selected,
        name_override,
        summoned,
        abilities,
        xp_suspended,
        types,
    })
}

fn write_player_state(w: &mut W, player: &PlayerState) {
    w.u8(player.index);
    w.i32(player.race);
    w.i32(player.controller);
    w.i32(player.color);
    w.i32(player.team);
    w.i32(player.start_location);
    w.bool(player.selecting);
    w.bool(player.active);
    w.len(player.states.len());
    for (key, value) in &player.states {
        w.i32(*key);
        w.i32(*value);
    }
    w.len(player.tech_max.len());
    for ((a, b), value) in &player.tech_max {
        w.u64(*a as u64);
        w.u64(*b as u64);
        w.i32(*value);
    }
    w.len(player.tech_researched.len());
    for (key, value) in &player.tech_researched {
        w.u64(*key as u64);
        w.i32(*value);
    }
    w.len(player.ability_available.len());
    for (key, value) in &player.ability_available {
        w.u64(*key as u64);
        w.bool(*value);
    }
    w.str(&player.name);
}

fn read_player_state(r: &mut R<'_>) -> SaveResult<PlayerState> {
    let index = r.u8()?;
    let race = r.i32()?;
    let controller = r.i32()?;
    let color = r.i32()?;
    let team = r.i32()?;
    let start_location = r.i32()?;
    let selecting = r.bool("script.player.selecting")?;
    let active = r.bool("script.player.active")?;
    let state_count = r.len()?;
    let mut states = BTreeMap::new();
    for _ in 0..state_count {
        states.insert(r.i32()?, r.i32()?);
    }
    let tech_max_count = r.len()?;
    let mut tech_max = BTreeMap::new();
    for _ in 0..tech_max_count {
        tech_max.insert((r.u64()? as i64, r.u64()? as i64), r.i32()?);
    }
    let tech_count = r.len()?;
    let mut tech_researched = BTreeMap::new();
    for _ in 0..tech_count {
        tech_researched.insert(r.u64()? as i64, r.i32()?);
    }
    let ability_count = r.len()?;
    let mut ability_available = BTreeMap::new();
    for _ in 0..ability_count {
        ability_available.insert(r.u64()? as i64, r.bool("script.player.ability")?);
    }
    let name = r.str()?.to_string();
    Ok(PlayerState {
        index,
        race,
        controller,
        color,
        team,
        start_location,
        selecting,
        active,
        states,
        tech_max,
        tech_researched,
        ability_available,
        name,
    })
}

fn write_cine_filter(w: &mut W, filter: &CineFilter) {
    w.str(&filter.texture);
    w.f32(filter.start_uv.0);
    w.f32(filter.start_uv.1);
    w.f32(filter.end_uv.0);
    w.f32(filter.end_uv.1);
    for value in [
        filter.start_color.0,
        filter.start_color.1,
        filter.start_color.2,
        filter.start_color.3,
    ] {
        w.i32(value);
    }
    for value in [
        filter.end_color.0,
        filter.end_color.1,
        filter.end_color.2,
        filter.end_color.3,
    ] {
        w.i32(value);
    }
    w.f32(filter.duration);
    w.i32(filter.blend_mode);
    w.i32(filter.tex_map_flags);
    w.bool(filter.displayed);
}

fn read_cine_filter(r: &mut R<'_>) -> SaveResult<CineFilter> {
    Ok(CineFilter {
        texture: r.str()?.to_string(),
        start_uv: (r.f32()?, r.f32()?),
        end_uv: (r.f32()?, r.f32()?),
        start_color: (r.i32()?, r.i32()?, r.i32()?, r.i32()?),
        end_color: (r.i32()?, r.i32()?, r.i32()?, r.i32()?),
        duration: r.f32()?,
        blend_mode: r.i32()?,
        tex_map_flags: r.i32()?,
        displayed: r.bool("script.cine_filter.displayed")?,
    })
}

fn write_script_section(w: &mut W, vm: Option<&JassVm>, version: u16) {
    w.u32(SEC_SCRIPT);
    match vm {
        Some(vm) => {
            w.u8(1);
            write_f64(w, vm.time);
            w.u64(vm.tick);
            w.u64(vm.rng);
            w.u64(vm.steps);
            w.u64(vm.budget);
            w.u32(vm.max_threads as u32);
            w.u64(registry_fingerprint());
            w.u32(crate::jass::registry::table().len() as u32);
            write_code(w, &vm.code);
            w.len(vm.globals.len());
            for value in &vm.globals {
                write_value(w, value);
            }
            w.len(vm.global_arrays.len());
            for cells in &vm.global_arrays {
                w.len(cells.len());
                for (key, value) in cells {
                    w.i32(*key);
                    write_value(w, value);
                }
            }
            w.len(vm.threads.len());
            for thread in &vm.threads {
                write_thread(w, thread, version);
            }
            // Host: unit handle table first (handle → unit id, ascending).
            let handles = vm.host.units.handles();
            w.len(handles.len());
            for handle in handles {
                let unit = vm
                    .host
                    .units
                    .unit_of(handle)
                    .expect("handle table is a bijection");
                w.u32(handle);
                w.u32(unit.0);
            }
            w.u32(vm.host.next_object_id());
            w.len(vm.host.objects.len());
            for (id, object) in &vm.host.objects {
                w.u32(*id);
                write_object(w, object, version);
            }
            w.len(vm.host.unit_meta.len());
            for (handle, meta) in &vm.host.unit_meta {
                w.u32(*handle);
                write_unit_meta(w, meta, version);
            }
            w.len(vm.host.item_binding.len());
            for (handle, item) in &vm.host.item_binding {
                w.u32(*handle);
                w.u32(item.0);
            }
            w.len(vm.host.destructable_binding.len());
            for (handle, destructable) in &vm.host.destructable_binding {
                w.u32(*handle);
                w.u32(destructable.0);
            }
            w.len(vm.host.players.len());
            for player in &vm.host.players {
                write_player_state(w, player);
            }
            w.len(vm.host.start_locations.len());
            for location in &vm.host.start_locations {
                w.i32(location.index);
                w.v2(location.pos);
            }
            w.str(&vm.host.map_name);
            w.str(&vm.host.map_description);
            w.i32(vm.host.teams);
            w.i32(vm.host.player_count);
            w.i32(vm.host.placement);
            w.len(vm.host.game_types.len());
            for (kind, enabled) in &vm.host.game_types {
                w.i32(*kind);
                w.bool(*enabled);
            }
            w.len(vm.host.float_states.len());
            for (key, value) in &vm.host.float_states {
                w.i32(*key);
                write_value(w, value);
            }
            w.i32(vm.host.game_difficulty);
            w.len(vm.host.alliances.len());
            for ((from_to, alliance), value) in &vm.host.alliances {
                w.u32(*from_to);
                w.i32(*alliance);
                w.u8(*value);
            }
            w.len(vm.host.order_log.len());
            for (handle, order) in &vm.host.order_log {
                w.u32(*handle);
                w.str(order);
            }
            w.len(vm.host.display_log.len());
            for (player, text) in &vm.host.display_log {
                w.opt_u8(*player);
                w.str(text);
            }
            w.len(vm.host.start_prio.len());
            for ((index, slot), importance) in &vm.host.start_prio {
                w.i32(*index);
                w.i32(*slot);
                w.i32(*importance);
            }
            w.len(vm.host.start_prio_count.len());
            for (index, count) in &vm.host.start_prio_count {
                w.i32(*index);
                w.i32(*count);
            }
            w.str(&vm.host.sky_model);
            w.i32(vm.host.water_color.0);
            w.i32(vm.host.water_color.1);
            w.i32(vm.host.water_color.2);
            w.i32(vm.host.water_color.3);
            match &vm.host.terrain_fog {
                Some((style, density, color, height)) => {
                    w.u8(1);
                    w.str(style);
                    w.f32(*density);
                    w.i32(color.0);
                    w.i32(color.1);
                    w.i32(color.2);
                    w.f32(*height);
                }
                None => w.u8(0),
            }
            w.len(vm.host.sound_kill_list.len());
            for handle in &vm.host.sound_kill_list {
                w.u32(*handle);
            }
            w.len(vm.host.preloaded.len());
            for path in &vm.host.preloaded {
                w.str(path);
            }
            w.i32(vm.host.all_item_type_slots);
            w.i32(vm.host.all_unit_type_slots);
            w.bool(vm.host.fog_enabled);
            w.bool(vm.host.fog_mask_enabled);
            w.bool(vm.host.user_ui_enabled);
            w.f32(vm.host.time_of_day_scale);
            write_cine_filter(w, &vm.host.cine_filter);
            w.len(vm.host.integer_game_states.len());
            for (key, value) in &vm.host.integer_game_states {
                w.i32(*key);
                w.i32(*value);
            }
            w.len(vm.host.quest_items.len());
            for (id, (description, completed)) in &vm.host.quest_items {
                w.u32(*id);
                w.str(description);
                w.bool(*completed);
            }
            w.str(&vm.host.day_night_models.0);
            w.str(&vm.host.day_night_models.1);
            w.str(&vm.host.sound_environment);
            w.str(&vm.host.map_music);
            w.opt_u8(vm.host.last_text_target);
            match vm.host.camera_bounds {
                Some((min, max)) => {
                    w.u8(1);
                    w.v2(min);
                    w.v2(max);
                }
                None => w.u8(0),
            }
            w.len(vm.host.selection.len());
            for handle in &vm.host.selection {
                w.u32(*handle);
            }
            w.len(vm.host.event_stack.len());
            for event in &vm.host.event_stack {
                write_event_context(w, event, version);
            }
            w.opt_u32(vm.host.last_created_group);
            w.len(vm.host.created_units.len());
            for handle in &vm.host.created_units {
                w.u32(*handle);
            }
            w.len(vm.region_occupancy.len());
            for (region, units) in &vm.region_occupancy {
                w.u32(*region);
                w.len(units.len());
                for unit in units {
                    w.u32(*unit);
                }
            }
            w.len(vm.unit_range_occupancy.len());
            for ((trigger, watched), units) in &vm.unit_range_occupancy {
                w.u32(*trigger);
                w.u32(*watched);
                w.len(units.len());
                for unit in units {
                    w.u32(*unit);
                }
            }
            w.opt_u32(vm.enum_unit);
            w.opt_u32(vm.filter_unit);
            w.opt_u8(vm.enum_player);
            w.opt_u32(vm.enum_destructable);
            w.opt_u32(vm.enum_item);
        }
        None => w.u8(0),
    }
}

/// Reads the version-4 script section. Returns `None` when the save was written
/// with no script attached.
fn read_script_section(
    r: &mut R<'_>,
    version: u16,
    next_unit_id: u32,
    next_item_id: u32,
    next_destructable_id: u32,
) -> SaveResult<Option<JassVm>> {
    r.section(SEC_SCRIPT, "script")?;
    if !r.bool("script.present")? {
        return Ok(None);
    }
    let time = read_f64(r)?;
    let tick = r.u64()?;
    let rng = r.u64()?;
    let steps = r.u64()?;
    let budget = r.u64()?;
    let max_threads = r.u32()? as usize;
    if !time.is_finite() || time < 0.0 {
        return Err(SaveError::InvalidValue {
            field: "script.time",
            detail: format!("{time}"),
        });
    }
    if rng == 0 {
        return Err(SaveError::InvalidValue {
            field: "script.rng",
            detail: "zero xorshift state".into(),
        });
    }
    if budget == 0 || budget > 100_000_000 {
        return Err(SaveError::InvalidValue {
            field: "script.budget",
            detail: format!("{budget}"),
        });
    }
    if max_threads == 0 || max_threads > 4096 {
        return Err(SaveError::InvalidValue {
            field: "script.max_threads",
            detail: format!("{max_threads}"),
        });
    }
    let saved_registry = r.u64()?;
    let saved_native_count = r.u32()?;
    let current_native_count = crate::jass::registry::table().len() as u32;
    if saved_registry != registry_fingerprint() || saved_native_count != current_native_count {
        return Err(SaveError::ScriptMismatch {
            detail: format!(
                "native registry differs (save {saved_registry:#x}/{saved_native_count}, build {:#x}/{current_native_count})",
                registry_fingerprint()
            ),
        });
    }
    let code = read_code(r)?;
    validate_code(&code)?;
    let function_count = code.functions.len();

    let global_count = r.len()?;
    if global_count != code.globals.len() {
        return Err(SaveError::InvalidValue {
            field: "script.globals",
            detail: format!(
                "save carries {global_count} values, the program declares {}",
                code.globals.len()
            ),
        });
    }
    let mut globals = Vec::with_capacity(global_count);
    for _ in 0..global_count {
        globals.push(read_value(r)?);
    }
    let array_count = r.len()?;
    if array_count != code.globals.len() {
        return Err(SaveError::InvalidValue {
            field: "script.global_arrays",
            detail: format!(
                "save carries {array_count} arrays, the program declares {}",
                code.globals.len()
            ),
        });
    }
    let mut global_arrays = Vec::with_capacity(array_count);
    for _ in 0..array_count {
        let cell_count = r.len()?;
        let mut cells = BTreeMap::new();
        for _ in 0..cell_count {
            let key = r.i32()?;
            if !(0..crate::jass::vm::ARRAY_BOUND).contains(&key) {
                return Err(SaveError::OutOfRange {
                    section: "script.array.key",
                    index: key as u32,
                    limit: crate::jass::vm::ARRAY_BOUND as usize,
                });
            }
            cells.insert(key, read_value(r)?);
        }
        global_arrays.push(cells);
    }
    let thread_count = r.len()?;
    if thread_count > 4096 {
        return Err(SaveError::InvalidValue {
            field: "script.threads",
            detail: format!("{thread_count} threads"),
        });
    }
    let mut threads = Vec::with_capacity(thread_count);
    for _ in 0..thread_count {
        threads.push(read_thread(r, &code, version)?);
    }

    // Host: unit handles are allocated sequentially from 0x1000_0000 and never
    // released, so replaying the saved pairs in ascending order reproduces the
    // exact handle ids (checked below, never assumed).
    let host_unit_count = r.len()?;
    let mut host = crate::jass::host::Host::new();
    let mut previous_handle = None;
    for _ in 0..host_unit_count {
        let handle = r.u32()?;
        let unit = crate::UnitId(r.u32()?);
        if unit.0 >= next_unit_id {
            return Err(SaveError::OutOfRange {
                section: "script.host.unit",
                index: unit.0,
                limit: next_unit_id as usize,
            });
        }
        if let Some(previous) = previous_handle
            && handle <= previous
        {
            return Err(SaveError::InvalidValue {
                field: "script.host.unit_handle",
                detail: format!("handle {handle} is not after {previous}"),
            });
        }
        previous_handle = Some(handle);
        let bound = host.unit_handle(unit);
        if bound.id != handle {
            return Err(SaveError::InvalidValue {
                field: "script.host.unit_handle",
                detail: format!("saved handle {handle}, replay produced {}", bound.id),
            });
        }
    }
    let next_object = r.u32()?;
    // The script handle counter is an external ID space too: it indexes nothing
    // dense (objects live in a `BTreeMap`), but a file whose counter is below the
    // handle base or more than one ID space above it is corrupt and must not be
    // accepted as a valid save.
    check_object_handle_counter(next_object)?;
    let object_count = r.len()?;
    if object_count > 1_000_000 {
        return Err(SaveError::InvalidValue {
            field: "script.host.objects",
            detail: format!("{object_count} objects"),
        });
    }
    let mut objects = BTreeMap::new();
    for _ in 0..object_count {
        let id = r.u32()?;
        if next_object != 0 && id >= next_object {
            return Err(SaveError::OutOfRange {
                section: "script.host.object",
                index: id,
                limit: next_object as usize,
            });
        }
        let object = read_object(r, function_count, version)?;
        objects.insert(id, object);
    }
    host.objects = objects;
    host.set_next_object_id(next_object);
    // Derived from the restored objects, exactly like `next_buff_id` is derived
    // from the restored buffs: `gameevent` ids are engine globals with one event
    // object each, and the set of registrations the engine itself never raises is
    // a property of the trigger table, not extra save state.
    host.game_event_handles = host
        .objects
        .iter()
        .filter_map(|(handle, object)| match object {
            Object::GameEvent { id: event } => Some((*event, *handle)),
            _ => None,
        })
        .collect();
    for object in host.objects.values() {
        if let Object::Trigger(state) = object {
            for event in &state.events {
                if let RegEvent::GameEvent { event } = event
                    && crate::jass::host::game_event_support(*event)
                        == crate::jass::host::GameEventSupport::Partial
                {
                    host.raise_only_game_events.insert(*event);
                }
            }
        }
    }

    let meta_count = r.len()?;
    let mut unit_meta = BTreeMap::new();
    for _ in 0..meta_count {
        unit_meta.insert(r.u32()?, read_unit_meta(r, version)?);
    }
    host.unit_meta = unit_meta;

    let item_count = r.len()?;
    let mut item_binding = BTreeMap::new();
    for _ in 0..item_count {
        let handle = r.u32()?;
        let item = crate::ItemId(r.u32()?);
        if item.0 >= next_item_id {
            return Err(SaveError::OutOfRange {
                section: "script.host.item",
                index: item.0,
                limit: next_item_id as usize,
            });
        }
        item_binding.insert(handle, item);
    }
    host.item_binding = item_binding;

    let destructable_count = r.len()?;
    let mut destructable_binding = BTreeMap::new();
    for _ in 0..destructable_count {
        let handle = r.u32()?;
        let destructable = crate::DestructableId(r.u32()?);
        if destructable.0 >= next_destructable_id {
            return Err(SaveError::OutOfRange {
                section: "script.host.destructable",
                index: destructable.0,
                limit: next_destructable_id as usize,
            });
        }
        destructable_binding.insert(handle, destructable);
    }
    host.destructable_binding = destructable_binding;

    let player_count = r.len()?;
    if player_count != crate::jass::host::PLAYER_SLOTS {
        return Err(SaveError::InvalidValue {
            field: "script.host.players",
            detail: format!(
                "{player_count} player slots, the host has {}",
                crate::jass::host::PLAYER_SLOTS
            ),
        });
    }
    let mut players = Vec::with_capacity(player_count);
    for _ in 0..player_count {
        players.push(read_player_state(r)?);
    }
    host.players = players;

    let location_count = r.len()?;
    let mut start_locations = Vec::with_capacity(location_count);
    for _ in 0..location_count {
        start_locations.push(crate::jass::host::StartLocation {
            index: r.i32()?,
            pos: r.v2()?,
        });
    }
    host.start_locations = start_locations;

    host.map_name = r.str()?.to_string();
    host.map_description = r.str()?.to_string();
    host.teams = r.i32()?;
    host.player_count = r.i32()?;
    host.placement = r.i32()?;

    let game_type_count = r.len()?;
    let mut game_types = BTreeMap::new();
    for _ in 0..game_type_count {
        game_types.insert(r.i32()?, r.bool("script.host.game_type")?);
    }
    host.game_types = game_types;

    let float_count = r.len()?;
    let mut float_states = BTreeMap::new();
    for _ in 0..float_count {
        float_states.insert(r.i32()?, read_value(r)?);
    }
    host.float_states = float_states;

    host.game_difficulty = r.i32()?;

    let alliance_count = r.len()?;
    let mut alliances = BTreeMap::new();
    for _ in 0..alliance_count {
        let from_to = r.u32()?;
        let alliance = r.i32()?;
        alliances.insert((from_to, alliance), r.u8()?);
    }
    host.alliances = alliances;

    let order_count = r.len()?;
    let mut order_log = Vec::with_capacity(order_count);
    for _ in 0..order_count {
        order_log.push((r.u32()?, r.str()?.to_string()));
    }
    host.order_log = order_log;

    let display_count = r.len()?;
    let mut display_log = Vec::with_capacity(display_count);
    for _ in 0..display_count {
        display_log.push((r.opt_u8()?, r.str()?.to_string()));
    }
    host.display_log = display_log;

    let prio_count = r.len()?;
    let mut start_prio = BTreeMap::new();
    for _ in 0..prio_count {
        start_prio.insert((r.i32()?, r.i32()?), r.i32()?);
    }
    host.start_prio = start_prio;

    let prio_count_count = r.len()?;
    let mut start_prio_count = BTreeMap::new();
    for _ in 0..prio_count_count {
        start_prio_count.insert(r.i32()?, r.i32()?);
    }
    host.start_prio_count = start_prio_count;

    host.sky_model = r.str()?.to_string();
    host.water_color = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
    host.terrain_fog = match r.u8()? {
        0 => None,
        1 => Some((
            r.str()?.to_string(),
            r.f32()?,
            (r.i32()?, r.i32()?, r.i32()?),
            r.f32()?,
        )),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.host.terrain_fog",
                detail: format!("tag {other}"),
            });
        }
    };

    let sound_count = r.len()?;
    let mut sound_kill_list = std::collections::BTreeSet::new();
    for _ in 0..sound_count {
        sound_kill_list.insert(r.u32()?);
    }
    host.sound_kill_list = sound_kill_list;

    let preload_count = r.len()?;
    let mut preloaded = Vec::with_capacity(preload_count);
    for _ in 0..preload_count {
        preloaded.push(r.str()?.to_string());
    }
    host.preloaded = preloaded;

    host.all_item_type_slots = r.i32()?;
    host.all_unit_type_slots = r.i32()?;
    host.fog_enabled = r.bool("script.host.fog_enabled")?;
    host.fog_mask_enabled = r.bool("script.host.fog_mask_enabled")?;
    host.user_ui_enabled = r.bool("script.host.user_ui_enabled")?;
    host.time_of_day_scale = r.f32()?;
    host.cine_filter = read_cine_filter(r)?;

    let state_count = r.len()?;
    let mut integer_game_states = BTreeMap::new();
    for _ in 0..state_count {
        integer_game_states.insert(r.i32()?, r.i32()?);
    }
    host.integer_game_states = integer_game_states;

    let quest_count = r.len()?;
    let mut quest_items = BTreeMap::new();
    for _ in 0..quest_count {
        quest_items.insert(
            r.u32()?,
            (r.str()?.to_string(), r.bool("script.host.quest_completed")?),
        );
    }
    host.quest_items = quest_items;

    host.day_night_models = (r.str()?.to_string(), r.str()?.to_string());
    host.sound_environment = r.str()?.to_string();
    host.map_music = r.str()?.to_string();
    host.last_text_target = r.opt_u8()?;
    host.camera_bounds = match r.u8()? {
        0 => None,
        1 => Some((r.v2()?, r.v2()?)),
        other => {
            return Err(SaveError::InvalidValue {
                field: "script.host.camera_bounds",
                detail: format!("tag {other}"),
            });
        }
    };

    let selection_count = r.len()?;
    let mut selection = Vec::with_capacity(selection_count);
    for _ in 0..selection_count {
        selection.push(r.u32()?);
    }
    host.selection = selection;

    let stack_count = r.len()?;
    let mut event_stack = Vec::with_capacity(stack_count);
    for _ in 0..stack_count {
        event_stack.push(read_event_context(r, version)?);
    }
    host.event_stack = event_stack;

    host.last_created_group = r.opt_u32()?;

    let created_count = r.len()?;
    let mut created_units = Vec::with_capacity(created_count);
    for _ in 0..created_count {
        created_units.push(r.u32()?);
    }
    host.created_units = created_units;

    let mut region_occupancy = BTreeMap::new();
    let region_count = r.len()?;
    for _ in 0..region_count {
        let region = r.u32()?;
        let count = r.len()?;
        let mut units = std::collections::BTreeSet::new();
        for _ in 0..count {
            units.insert(r.u32()?);
        }
        region_occupancy.insert(region, units);
    }

    let mut unit_range_occupancy = BTreeMap::new();
    let range_count = r.len()?;
    for _ in 0..range_count {
        let trigger = r.u32()?;
        let watched = r.u32()?;
        let count = r.len()?;
        let mut units = std::collections::BTreeSet::new();
        for _ in 0..count {
            units.insert(r.u32()?);
        }
        unit_range_occupancy.insert((trigger, watched), units);
    }

    let enum_unit = r.opt_u32()?;
    let filter_unit = r.opt_u32()?;
    let enum_player = r.opt_u8()?;
    let enum_destructable = r.opt_u32()?;
    let enum_item = r.opt_u32()?;

    let scratch = Sim::new(0, 1, 1);
    let mut vm = JassVm::restore_blank(code, scratch, rng);
    vm.time = time;
    vm.tick = tick;
    vm.rng = rng;
    vm.steps = steps;
    vm.budget = budget;
    vm.max_threads = max_threads.max(1);
    vm.globals = globals;
    vm.global_arrays = global_arrays;
    vm.threads = threads;
    vm.host = host;
    vm.region_occupancy = region_occupancy;
    vm.unit_range_occupancy = unit_range_occupancy;
    vm.enum_unit = enum_unit;
    vm.filter_unit = filter_unit;
    vm.enum_player = enum_player;
    vm.enum_destructable = enum_destructable;
    vm.enum_item = enum_item;
    vm.event_ids = EventIds::from_vm(&vm);
    eprintln!(
        "{}diagnostic_logs,counters,current_thread (safe: not gameplay state)",
        SCRIPT_RESTORE_PARTIAL_PREFIX
    );
    Ok(Some(vm))
}

// ---------------------------------------------------------------------------
// Payload encode / decode
// ---------------------------------------------------------------------------

fn encode_payload(sim: &Sim) -> Vec<u8> {
    encode_payload_version(sim, SAVE_FORMAT_VERSION)
}

/// Encodes `sim` with an explicit layout `version`.  Only `SAVE_FORMAT_VERSION`
/// is written by the public API; the parameter exists so a test can produce a
/// genuine v2 file (no ethereal byte) and prove the migration.
fn encode_payload_version(sim: &Sim, version: u16) -> Vec<u8> {
    let mut w = W::new();

    w.u32(SEC_HEADER);
    w.u64(sim.tick);
    w.u64(sim.seed);
    w.u64(sim.rng_state);
    w.u8(ruleset_tag(sim.ruleset));
    w.bool(sim.passive_income);

    w.u32(SEC_GRID);
    w.u32(sim.grid.width);
    w.u32(sim.grid.height);
    w.f32(sim.grid.cell_size);
    write_bits(&mut w, sim.grid.width, sim.grid.height, |x, y| {
        sim.grid.walkable(x, y)
    });

    w.u32(SEC_IDS);
    w.u32(sim.next_id);
    w.u32(sim.next_item_id);
    w.u32(sim.next_destructable_id);
    w.u32(sim.next_camp_id);
    w.u32(sim.next_projectile_id);

    w.u32(SEC_RESOURCES);
    for value in sim.resources {
        w.i32(value);
    }
    for value in sim.lumber {
        w.i32(value);
    }

    w.u32(SEC_FOG);
    w.bytes(&sim.visibility);
    w.bytes(&sim.explored[0]);
    w.bytes(&sim.explored[1]);

    w.u32(SEC_GAMEPLAY);
    write_gameplay(&mut w, &sim.gameplay);

    w.u32(SEC_MINES);
    w.len(sim.gold_mines.len());
    for mine in &sim.gold_mines {
        w.v2(mine.pos);
        w.i32(mine.remaining);
    }

    w.u32(SEC_DESTRUCTABLES);
    w.len(sim.destructables.len());
    for tree in &sim.destructables {
        w.u32(tree.id.0);
        w.u16(tree.def_index);
        w.v2(tree.pos);
        w.i32(tree.hp);
        w.i32(tree.max_hp);
        w.bool(tree.alive);
        w.opt_cell(tree.blocked_cell);
    }

    w.u32(SEC_CAMPS);
    w.len(sim.camps.len());
    for camp in &sim.camps {
        w.u32(camp.id);
        w.v2(camp.anchor);
        w.len(camp.units.len());
        for unit in &camp.units {
            w.u32(unit.0);
        }
        w.f32(camp.aggro);
        w.f32(camp.leash);
    }

    w.u32(SEC_ITEMS);
    w.len(sim.items.len());
    for item in &sim.items {
        w.u32(item.id.0);
        w.u16(item.def_index);
        w.v2(item.pos);
        w.opt_u8(item.owner);
        w.i32(item.charges);
    }

    w.u32(SEC_SHOPS);
    w.len(sim.shops.len());
    for shop in &sim.shops {
        w.u32(shop.shop.0);
        w.u8(shop.owner);
        w.len(shop.slots.len());
        for slot in &shop.slots {
            w.str(slot.item);
            w.i32(slot.stock);
            w.u64(slot.next_restock_tick);
        }
    }

    w.u32(SEC_UNITS);
    w.len(sim.units.len());
    for unit in &sim.units {
        write_unit(&mut w, unit, version);
    }

    w.u32(SEC_PROJECTILES);
    w.len(sim.projectiles.len());
    for projectile in &sim.projectiles {
        write_projectile(&mut w, projectile);
    }

    w.u32(SEC_CASTS);
    w.len(sim.pending_casts.len());
    for cast in &sim.pending_casts {
        w.u64(cast.due);
        w.u32(cast.caster.0);
        w.u8(spell_tag(cast.spell));
        w.v2(cast.at);
        w.u8(cast.waves_left);
        w.i32(cast.damage);
    }

    w.u32(SEC_AI);
    w.len(sim.ai.len());
    for state in &sim.ai {
        write_ai_state(&mut w, state);
    }

    w.u32(SEC_UPGRADES);
    for map in &sim.upgrades {
        w.len(map.len());
        for (id, level) in map {
            w.str(id);
            w.u8(*level);
        }
    }

    w.u32(SEC_BUFFS);
    w.len(sim.buffs.len());
    for buff in &sim.buffs {
        w.u32(buff.id.0);
        w.str(buff.tag);
        w.u32(buff.unit.0);
        w.u32(buff.source.0);
        w.u8(buff.level);
        match buff.kind {
            crate::BuffKind::Timed { expire_tick } => {
                w.u8(0);
                w.u64(expire_tick);
            }
            crate::BuffKind::TimedLife { expire_tick } => {
                w.u8(1);
                w.u64(expire_tick);
            }
            crate::BuffKind::AuraSource {
                radius,
                hostile,
                next_refresh,
            } => {
                w.u8(2);
                w.f32(radius);
                w.bool(hostile);
                w.u64(next_refresh);
            }
            crate::BuffKind::AuraTarget {
                source,
                radius,
                next_refresh,
            } => {
                w.u8(3);
                w.u32(source.0);
                w.f32(radius);
                w.u64(next_refresh);
            }
        }
        w.i32(buff.armor);
        w.i32(buff.damage_permille);
        w.i32(buff.attack_speed_permille);
        w.i32(buff.move_speed_permille);
        w.i32(buff.damage_taken_permille);
        w.i32(buff.bonus_life);
        w.bool(buff.root);
        w.bool(buff.stun);
        w.bool(buff.invulnerable);
        w.opt_str(buff.stack_key);
        w.i32(buff.stack_prio);
    }

    w.u32(SEC_SQUADS);
    w.len(sim.squads.len());
    for manager in &sim.squads {
        w.u8(manager.owner);
        w.len(manager.config.squad_size);
        w.len(manager.config.squad_size_bonus);
        w.u32(manager.config.attack_force_interval);
        w.u32(manager.config.rush_interval);
        w.u32(manager.config.assign_roles_interval);
        w.f32(manager.config.idle_scan_radius);
        w.f32(manager.config.attack_scan_radius);
        w.f32(manager.config.danger_radius);
        w.u32(manager.config.stuck_ticks);
        w.len(manager.config.min_attack_units);
        w.f32(manager.config.rally_radius);
        w.u32(manager.next_squad_id);
        w.u32(manager.attack_force_timer);
        w.u32(manager.rush_timer);
        w.u32(manager.assign_timer);
        w.len(manager.update_cursor);
        w.len(manager.idle.len());
        for id in &manager.idle {
            w.u32(id.0);
        }
        w.len(manager.squads.len());
        for squad in &manager.squads {
            w.u32(squad.id);
            w.u8(match squad.kind {
                crate::SquadKind::Assault => 1,
                crate::SquadKind::Rush => 2,
                crate::SquadKind::Protection => 3,
            });
            w.u8(match squad.state {
                crate::SquadState::Idle => 1,
                crate::SquadState::AttackMove => 2,
                crate::SquadState::Attack => 3,
                crate::SquadState::Flee => 4,
            });
            w.len(squad.units.len());
            for id in &squad.units {
                w.u32(id.0);
            }
            w.opt_u32(squad.target.map(|id| id.0));
            w.v2(squad.target_pos);
            w.opt_u32(squad.leader.map(|id| id.0));
            w.opt_v2(squad.last_leader_pos);
            w.u64(squad.last_progress_tick);
            w.u64(squad.last_issue_tick);
            w.v2(squad.last_issue_pos);
        }
    }

    w.u32(SEC_STAMPS);
    w.len(sim.issue_stamps.len());
    for (unit, (spec, tick)) in &sim.issue_stamps {
        w.u32(*unit);
        write_order_spec(&mut w, spec);
        w.u64(*tick);
    }

    // Version 4: the JASS script world. Older layouts have no script section.
    if version >= 4 {
        write_script_section(&mut w, sim.script.as_deref(), version);
    }

    w.u32(SEC_END);
    w.buf
}

/// Bounds one external ID counter before it can size an allocation or index a
/// table.
///
/// The format starts every space at 1 (`Sim::new` hands out `1` first) and no
/// legitimate save can carry a counter above [`MAX_ID_SPACE`]; both rules are
/// checked here so every caller reports the same typed error instead of trusting
/// the scalar it just read.
fn check_id_counter(field: &'static str, value: u32) -> SaveResult<()> {
    if value == 0 {
        return Err(SaveError::InvalidIdSpace {
            field,
            value,
            detail: "id counters start at 1".into(),
        });
    }
    if value > MAX_ID_SPACE {
        return Err(SaveError::InvalidIdSpace {
            field,
            value,
            detail: format!("above the {MAX_ID_SPACE} slot id-space cap"),
        });
    }
    Ok(())
}

/// Bounds the script-object handle counter, whose space starts at
/// [`crate::jass::host::OBJECT_HANDLE_BASE`] rather than at 1.
fn check_object_handle_counter(value: u32) -> SaveResult<()> {
    let base = crate::jass::host::OBJECT_HANDLE_BASE;
    if value < base {
        return Err(SaveError::InvalidIdSpace {
            field: "script.next_object",
            value,
            detail: format!("below the script handle base {base:#x}"),
        });
    }
    if value - base > MAX_ID_SPACE {
        return Err(SaveError::InvalidIdSpace {
            field: "script.next_object",
            value,
            detail: format!("more than {MAX_ID_SPACE} handles above the base {base:#x}"),
        });
    }
    Ok(())
}

/// Validates one whole entity ID space: the counter is inside the documented
/// range, every stored id is below it (checked), and no two entities share an id.
fn check_id_space(
    field: &'static str,
    section: &'static str,
    next: u32,
    ids: impl IntoIterator<Item = u32>,
) -> SaveResult<()> {
    check_id_counter(field, next)?;
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if id >= next {
            return Err(SaveError::OutOfRange {
                section,
                index: id,
                limit: next as usize,
            });
        }
        if !seen.insert(id) {
            return Err(SaveError::InvalidIdSpace {
                field,
                value: id,
                detail: "duplicate id in one ID space".into(),
            });
        }
    }
    Ok(())
}

fn decode_payload(payload: &[u8], version: u16) -> SaveResult<Sim> {
    let mut r = R::new(payload);

    r.section(SEC_HEADER, "header")?;
    let tick = r.u64()?;
    let seed = r.u64()?;
    let rng_state = r.u64()?;
    let ruleset = ruleset_from(r.u8()?)?;
    let passive_income = r.bool("sim.passive_income")?;

    r.section(SEC_GRID, "grid")?;
    let width = r.u32()?;
    let height = r.u32()?;
    let cell_size = r.f32()?;
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err(SaveError::InvalidGrid { width, height });
    }
    let cells = width as usize * height as usize;
    let walkable = read_bits(&mut r, cells)?;

    r.section(SEC_IDS, "ids")?;
    let next_id = r.u32()?;
    let next_item_id = r.u32()?;
    let next_destructable_id = r.u32()?;
    let next_camp_id = r.u32()?;
    let next_projectile_id = r.u32()?;

    r.section(SEC_RESOURCES, "resources")?;
    let resources = [r.i32()?, r.i32()?];
    let lumber = [r.i32()?, r.i32()?];

    r.section(SEC_FOG, "fog")?;
    let visibility = r.bytes()?.to_vec();
    let explored_a = r.bytes()?.to_vec();
    let explored_b = r.bytes()?.to_vec();
    if visibility.len() != cells || explored_a.len() != cells || explored_b.len() != cells {
        return Err(SaveError::InvalidGrid { width, height });
    }

    r.section(SEC_GAMEPLAY, "gameplay")?;
    let gameplay = read_gameplay(&mut r)?;

    r.section(SEC_MINES, "mines")?;
    let mine_count = r.len()?;
    let mut gold_mines = Vec::with_capacity(mine_count);
    for _ in 0..mine_count {
        gold_mines.push(GoldMine {
            pos: r.v2()?,
            remaining: r.i32()?,
        });
    }

    r.section(SEC_DESTRUCTABLES, "destructables")?;
    let tree_count = r.len()?;
    let mut destructables = Vec::with_capacity(tree_count);
    for _ in 0..tree_count {
        let id = DestructableId(r.u32()?);
        let def_index = r.u16()?;
        if def_index as usize >= data::DESTRUCTABLES.len() {
            return Err(SaveError::OutOfRange {
                section: "destructables.def_index",
                index: u32::from(def_index),
                limit: data::DESTRUCTABLES.len(),
            });
        }
        destructables.push(Destructable {
            id,
            def_index,
            pos: r.v2()?,
            hp: r.i32()?,
            max_hp: r.i32()?,
            alive: r.bool("destructable.alive")?,
            blocked_cell: r.opt_cell()?,
        });
    }

    r.section(SEC_CAMPS, "camps")?;
    let camp_count = r.len()?;
    let mut camps = Vec::with_capacity(camp_count);
    for _ in 0..camp_count {
        let id = r.u32()?;
        let anchor = r.v2()?;
        let unit_count = r.len()?;
        let mut units = Vec::with_capacity(unit_count);
        for _ in 0..unit_count {
            units.push(UnitId(r.u32()?));
        }
        camps.push(Camp {
            id,
            anchor,
            units,
            aggro: r.f32()?,
            leash: r.f32()?,
        });
    }

    r.section(SEC_ITEMS, "items")?;
    let item_count = r.len()?;
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        let id = ItemId(r.u32()?);
        let def_index = r.u16()?;
        if def_index as usize >= data::ITEMS.len() {
            return Err(SaveError::OutOfRange {
                section: "items.def_index",
                index: u32::from(def_index),
                limit: data::ITEMS.len(),
            });
        }
        items.push(Item {
            id,
            def_index,
            pos: r.v2()?,
            owner: r.opt_u8()?,
            charges: r.i32()?,
        });
    }

    r.section(SEC_SHOPS, "shops")?;
    let shop_count = r.len()?;
    let mut shops = Vec::with_capacity(shop_count);
    for _ in 0..shop_count {
        let shop = UnitId(r.u32()?);
        let owner = r.u8()?;
        let slot_count = r.len()?;
        let mut slots = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            let raw = r.str()?;
            let item = intern("shop.item", raw)?;
            slots.push(ShopSlot {
                item,
                stock: r.i32()?,
                next_restock_tick: r.u64()?,
            });
        }
        shops.push(Shop { shop, owner, slots });
    }

    r.section(SEC_UNITS, "units")?;
    let unit_count = r.len()?;
    let mut units = Vec::with_capacity(unit_count);
    for _ in 0..unit_count {
        let unit = read_unit(&mut r, version)?;
        if data::def_at(unit.def_index).is_none() {
            return Err(SaveError::OutOfRange {
                section: "units.def_index",
                index: u32::from(unit.def_index),
                limit: def_count(),
            });
        }
        units.push(unit);
    }

    r.section(SEC_PROJECTILES, "projectiles")?;
    let projectile_count = r.len()?;
    let mut projectiles = Vec::with_capacity(projectile_count);
    for _ in 0..projectile_count {
        projectiles.push(read_projectile(&mut r)?);
    }

    r.section(SEC_CASTS, "casts")?;
    let cast_count = r.len()?;
    let mut pending_casts = Vec::with_capacity(cast_count);
    for _ in 0..cast_count {
        pending_casts.push(PendingCast {
            due: r.u64()?,
            caster: UnitId(r.u32()?),
            spell: spell_from(r.u8()?)?,
            at: r.v2()?,
            waves_left: r.u8()?,
            damage: r.i32()?,
        });
    }

    r.section(SEC_AI, "ai")?;
    let ai_count = r.len()?;
    let mut ai = Vec::with_capacity(ai_count);
    for _ in 0..ai_count {
        ai.push(read_ai_state(&mut r)?);
    }

    r.section(SEC_UPGRADES, "upgrades")?;
    let mut upgrades: [BTreeMap<&'static str, u8>; 2] = [BTreeMap::new(), BTreeMap::new()];
    for map in upgrades.iter_mut() {
        let count = r.len()?;
        for _ in 0..count {
            let raw = r.str()?;
            let id = intern("upgrades", raw)?;
            map.insert(id, r.u8()?);
        }
    }

    let (buffs, squads, issue_stamps) = if version >= 2 {
        r.section(SEC_BUFFS, "buffs")?;
        let buff_count = r.len()?;
        let mut buffs = Vec::with_capacity(buff_count);
        for _ in 0..buff_count {
            let mut buff = crate::Buff::timed("", UnitId(0), UnitId(0), 1, 0, 1);
            buff.id = crate::BuffId(r.u32()?);
            buff.tag = crate::buffs::intern_tag(r.str()?);
            buff.unit = UnitId(r.u32()?);
            buff.source = UnitId(r.u32()?);
            buff.level = r.u8()?;
            buff.kind = match r.u8()? {
                0 => crate::BuffKind::Timed {
                    expire_tick: r.u64()?,
                },
                1 => crate::BuffKind::TimedLife {
                    expire_tick: r.u64()?,
                },
                2 => {
                    let radius = r.f32()?;
                    let hostile = r.bool("buff.hostile")?;
                    crate::BuffKind::AuraSource {
                        radius,
                        hostile,
                        next_refresh: r.u64()?,
                    }
                }
                3 => {
                    let source = UnitId(r.u32()?);
                    let radius = r.f32()?;
                    crate::BuffKind::AuraTarget {
                        source,
                        radius,
                        next_refresh: r.u64()?,
                    }
                }
                other => {
                    return Err(SaveError::BadTag {
                        section: "buff.kind",
                        tag: u32::from(other),
                    });
                }
            };
            buff.armor = r.i32()?;
            buff.damage_permille = r.i32()?;
            buff.attack_speed_permille = r.i32()?;
            buff.move_speed_permille = r.i32()?;
            buff.damage_taken_permille = r.i32()?;
            buff.bonus_life = r.i32()?;
            buff.root = r.bool("buff.root")?;
            buff.stun = r.bool("buff.stun")?;
            buff.invulnerable = r.bool("buff.invulnerable")?;
            buff.stack_key = r.opt_str()?.map(crate::buffs::intern_tag);
            buff.stack_prio = r.i32()?;
            buffs.push(buff);
        }

        r.section(SEC_SQUADS, "squads")?;
        let manager_count = r.len()?;
        let mut squads = Vec::with_capacity(manager_count);
        for _ in 0..manager_count {
            let owner = r.u8()?;
            let config = crate::SquadConfig {
                squad_size: r.count()?,
                squad_size_bonus: r.count()?,
                attack_force_interval: r.u32()?,
                rush_interval: r.u32()?,
                assign_roles_interval: r.u32()?,
                idle_scan_radius: r.f32()?,
                attack_scan_radius: r.f32()?,
                danger_radius: r.f32()?,
                stuck_ticks: r.u32()?,
                min_attack_units: r.count()?,
                rally_radius: r.f32()?,
            };
            let mut manager = crate::SquadManager::new(owner, config);
            manager.next_squad_id = r.u32()?;
            manager.attack_force_timer = r.u32()?;
            manager.rush_timer = r.u32()?;
            manager.assign_timer = r.u32()?;
            manager.update_cursor = r.count()?;
            let idle_count = r.len()?;
            manager.idle.clear();
            for _ in 0..idle_count {
                manager.idle.push(UnitId(r.u32()?));
            }
            let squad_count = r.len()?;
            for _ in 0..squad_count {
                let id = r.u32()?;
                let kind = match r.u8()? {
                    1 => crate::SquadKind::Assault,
                    2 => crate::SquadKind::Rush,
                    3 => crate::SquadKind::Protection,
                    other => {
                        return Err(SaveError::BadTag {
                            section: "squad.kind",
                            tag: u32::from(other),
                        });
                    }
                };
                let state = match r.u8()? {
                    1 => crate::SquadState::Idle,
                    2 => crate::SquadState::AttackMove,
                    3 => crate::SquadState::Attack,
                    4 => crate::SquadState::Flee,
                    other => {
                        return Err(SaveError::BadTag {
                            section: "squad.state",
                            tag: u32::from(other),
                        });
                    }
                };
                let unit_count = r.len()?;
                let mut units = Vec::with_capacity(unit_count);
                for _ in 0..unit_count {
                    units.push(UnitId(r.u32()?));
                }
                let target = r.opt_u32()?.map(UnitId);
                let target_pos = r.v2()?;
                let leader = r.opt_u32()?.map(UnitId);
                let last_leader_pos = r.opt_v2()?;
                let last_progress_tick = r.u64()?;
                let last_issue_tick = r.u64()?;
                let last_issue_pos = r.v2()?;
                manager.squads.push(crate::Squad {
                    id,
                    owner,
                    kind,
                    state,
                    units,
                    target,
                    target_pos,
                    leader,
                    last_leader_pos,
                    last_progress_tick,
                    last_issue_tick,
                    last_issue_pos,
                });
            }
            squads.push(manager);
        }

        r.section(SEC_STAMPS, "stamps")?;
        let stamp_count = r.len()?;
        let mut stamps = BTreeMap::new();
        for _ in 0..stamp_count {
            let unit = r.u32()?;
            let spec = read_order_spec(&mut r)?;
            stamps.insert(unit, (spec, r.u64()?));
        }
        (buffs, squads, stamps)
    } else {
        (Vec::new(), Vec::new(), BTreeMap::new())
    };

    let script = if version >= 4 {
        read_script_section(&mut r, version, next_id, next_item_id, next_destructable_id)?
    } else {
        None
    };

    r.section(SEC_END, "end")?;
    if r.remaining() != 0 {
        return Err(SaveError::TrailingBytes(r.remaining()));
    }
    // Structural validation, before a single derived structure is allocated:
    // every counter is inside the documented ID space, every stored id is below
    // its counter, and no ID space contains a duplicate. The counters stay the
    // counters and are never trusted as allocation sizes (see `rebuild_derived`,
    // which sizes the dense unit index from the units that exist).
    check_id_space(
        "ids.next_id",
        "units.id",
        next_id,
        units.iter().map(|unit| unit.id.0),
    )?;
    check_id_space(
        "ids.next_item_id",
        "items.id",
        next_item_id,
        items.iter().map(|item| item.id.0),
    )?;
    check_id_space(
        "ids.next_destructable_id",
        "destructables.id",
        next_destructable_id,
        destructables.iter().map(|tree| tree.id.0),
    )?;
    check_id_space(
        "ids.next_camp_id",
        "camps.id",
        next_camp_id,
        camps.iter().map(|camp| camp.id),
    )?;
    check_id_space(
        "ids.next_projectile_id",
        "projectiles.id",
        next_projectile_id,
        projectiles.iter().map(|shot| shot.id.0),
    )?;

    let mut sim = Sim::new(seed, width, height);
    sim.tick = tick;
    sim.ruleset = ruleset;
    sim.passive_income = passive_income;
    sim.rng_state = rng_state;
    sim.grid = wc3_path::Grid::from_walkable(width, height, cell_size, walkable);
    sim.next_id = next_id;
    sim.next_item_id = next_item_id;
    sim.next_destructable_id = next_destructable_id;
    sim.next_camp_id = next_camp_id;
    sim.next_projectile_id = next_projectile_id;
    sim.resources = resources;
    sim.lumber = lumber;
    sim.visibility = visibility;
    sim.explored = [explored_a, explored_b];
    sim.gameplay = gameplay;
    sim.gold_mines = gold_mines;
    sim.destructables = destructables;
    sim.camps = camps;
    sim.items = items;
    sim.shops = shops;
    sim.units = units;
    sim.projectiles = projectiles;
    sim.pending_casts = pending_casts;
    sim.ai = ai;
    sim.upgrades = upgrades;
    sim.buffs = buffs;
    sim.squads = squads;
    sim.issue_stamps = issue_stamps;
    sim.script = script.map(Box::new);
    sim.next_buff_id = sim
        .buffs
        .iter()
        .map(|buff| buff.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(1);
    sim.rebuild_derived();
    Ok(sim)
}

// ---------------------------------------------------------------------------
// Bitmap helpers
// ---------------------------------------------------------------------------

fn write_bits(w: &mut W, width: u32, height: u32, value: impl Fn(u32, u32) -> bool) {
    let cells = width as usize * height as usize;
    let bytes = cells.div_ceil(8);
    let mut bitmap = vec![0u8; bytes];
    for y in 0..height {
        for x in 0..width {
            if value(x, y) {
                let index = y as usize * width as usize + x as usize;
                bitmap[index / 8] |= 1 << (index % 8);
            }
        }
    }
    w.bytes(&bitmap);
}

fn read_bits(r: &mut R<'_>, cells: usize) -> SaveResult<Vec<bool>> {
    let bytes = r.bytes()?;
    if bytes.len() != cells.div_ceil(8) {
        return Err(SaveError::InvalidValue {
            field: "grid.walkable",
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

// ---------------------------------------------------------------------------
// Command codec (shared with `net` command frames and replays)
// ---------------------------------------------------------------------------

fn write_ability(w: &mut W, ability: crate::Ability) {
    match ability {
        crate::Ability::Heal(power) => {
            w.u8(0);
            w.i32(power);
        }
        crate::Ability::Fireball(power) => {
            w.u8(1);
            w.i32(power);
        }
    }
}

fn read_ability(r: &mut R<'_>) -> SaveResult<crate::Ability> {
    match r.u8()? {
        0 => Ok(crate::Ability::Heal(r.i32()?)),
        1 => Ok(crate::Ability::Fireball(r.i32()?)),
        other => Err(SaveError::BadTag {
            section: "command.ability",
            tag: u32::from(other),
        }),
    }
}

fn write_units(w: &mut W, units: &[UnitId]) {
    w.len(units.len());
    for unit in units {
        w.u32(unit.0);
    }
}

fn read_units(r: &mut R<'_>) -> SaveResult<Vec<UnitId>> {
    let count = r.len()?;
    let mut units = Vec::with_capacity(count);
    for _ in 0..count {
        units.push(UnitId(r.u32()?));
    }
    Ok(units)
}

fn write_command(w: &mut W, command: &crate::Command) {
    use crate::Command;
    match command {
        Command::Move { units, to } => {
            w.u8(0);
            write_units(w, units);
            w.v2(*to);
        }
        Command::Attack { units, target } => {
            w.u8(1);
            write_units(w, units);
            w.u32(target.0);
        }
        Command::Stop { units } => {
            w.u8(2);
            write_units(w, units);
        }
        Command::MoveAttack { units, to } => {
            w.u8(3);
            write_units(w, units);
            w.v2(*to);
        }
        Command::Hold { units } => {
            w.u8(4);
            write_units(w, units);
        }
        Command::Harvest { units, to } => {
            w.u8(5);
            write_units(w, units);
            w.v2(*to);
        }
        Command::CancelTrain { building, index } => {
            w.u8(6);
            w.u32(building.0);
            w.len(*index);
        }
        Command::Train { building, kind } => {
            w.u8(7);
            w.u32(building.0);
            w.u8(unit_kind_tag(*kind));
        }
        Command::Build { builder, kind, at } => {
            w.u8(8);
            w.u32(builder.0);
            w.u8(unit_kind_tag(*kind));
            w.v2(*at);
        }
        Command::Cast {
            unit,
            ability,
            target,
        } => {
            w.u8(9);
            w.u32(unit.0);
            write_ability(w, *ability);
            w.opt_id(*target);
        }
        Command::LearnAbility { unit, ability } => {
            w.u8(10);
            w.u32(unit.0);
            w.str(ability);
        }
        Command::Repair { units, target } => {
            w.u8(11);
            write_units(w, units);
            w.u32(target.0);
        }
        Command::HarvestLumber { units, to } => {
            w.u8(12);
            write_units(w, units);
            w.v2(*to);
        }
        Command::ReviveHero { hero, altar } => {
            w.u8(13);
            w.u32(hero.0);
            w.u32(altar.0);
        }
    }
}

fn read_command(r: &mut R<'_>) -> SaveResult<crate::Command> {
    use crate::Command;
    let tag = r.u8()?;
    Ok(match tag {
        0 => Command::Move {
            units: read_units(r)?,
            to: r.v2()?,
        },
        1 => Command::Attack {
            units: read_units(r)?,
            target: UnitId(r.u32()?),
        },
        2 => Command::Stop {
            units: read_units(r)?,
        },
        3 => Command::MoveAttack {
            units: read_units(r)?,
            to: r.v2()?,
        },
        4 => Command::Hold {
            units: read_units(r)?,
        },
        5 => Command::Harvest {
            units: read_units(r)?,
            to: r.v2()?,
        },
        6 => Command::CancelTrain {
            building: UnitId(r.u32()?),
            index: r.count()?,
        },
        7 => Command::Train {
            building: UnitId(r.u32()?),
            kind: unit_kind_from(r.u8()?)?,
        },
        8 => Command::Build {
            builder: UnitId(r.u32()?),
            kind: unit_kind_from(r.u8()?)?,
            at: r.v2()?,
        },
        9 => Command::Cast {
            unit: UnitId(r.u32()?),
            ability: read_ability(r)?,
            target: r.opt_id()?,
        },
        10 => {
            let unit = UnitId(r.u32()?);
            let raw = r.str()?;
            Command::LearnAbility {
                unit,
                ability: intern("command.learned", raw)?,
            }
        }
        11 => Command::Repair {
            units: read_units(r)?,
            target: UnitId(r.u32()?),
        },
        12 => Command::HarvestLumber {
            units: read_units(r)?,
            to: r.v2()?,
        },
        13 => Command::ReviveHero {
            hero: UnitId(r.u32()?),
            altar: UnitId(r.u32()?),
        },
        other => {
            return Err(SaveError::BadTag {
                section: "command",
                tag: u32::from(other),
            });
        }
    })
}

/// Encodes one command as a self-contained byte string (used by lockstep
/// command frames and replays).
pub(crate) fn command_encode(command: &crate::Command) -> Vec<u8> {
    let mut w = W::new();
    write_command(&mut w, command);
    w.buf
}

/// Decodes one command produced by [`command_encode`]; trailing bytes are
/// rejected so a corrupted frame cannot silently drop a command.
pub(crate) fn command_decode(bytes: &[u8]) -> SaveResult<crate::Command> {
    let mut r = R::new(bytes);
    let command = read_command(&mut r)?;
    if r.remaining() != 0 {
        return Err(SaveError::TrailingBytes(r.remaining()));
    }
    Ok(command)
}

// ---------------------------------------------------------------------------
// Derived state rebuild
// ---------------------------------------------------------------------------

impl Sim {
    /// Rebuilds every accelerator that is derived from saved state. Called once
    /// after a load so the restored simulation can run without re-simulating.
    ///
    /// Derived here:
    /// * `id_to_index` — unit id to `units` index map;
    /// * `pathfinder` — generation-stamped A* scratch sized to the grid;
    /// * `path_requests` / `path_budget` / `flow_cache` — per-tick scratch;
    /// * `spatial` — neighbour hash (also cleared and refilled every tick by
    ///   `advance_movement`, rebuilt here so a save can be `state_hash`-equal
    ///   immediately after loading, before the next tick);
    /// * `fog_stamp` — fog scratch stamp (the ring table itself is built by
    ///   `Sim::new` and is independent of map/save state, so it is kept as-is
    ///   rather than duplicated here and drifting from the sim);
    /// * `events` — the per-tick event log is never persisted.
    fn rebuild_derived(&mut self) {
        let width = self.grid.width;
        let height = self.grid.height;
        let cells = width as usize * height as usize;

        // Sized from the units that exist, never from the counter: the counter is
        // save input (bounded by `MAX_ID_SPACE`), but the index only has to cover
        // real ids. A save with three units and a counter at the cap therefore
        // costs three slots, not four megabytes.
        let max_id = self
            .units
            .iter()
            .map(|unit| unit.id.0 as usize)
            .max()
            .unwrap_or(0);
        let mut index_of = vec![u32::MAX; max_id.saturating_add(1)];
        for (index, unit) in self.units.iter().enumerate() {
            let slot = unit.id.0 as usize;
            if slot >= index_of.len() {
                index_of.resize(slot + 1, u32::MAX);
            }
            index_of[slot] = index as u32;
        }
        self.id_to_index = index_of;

        self.pathfinder = nav::PathFinder::new(width, height);
        self.path_requests.clear();
        self.path_budget = PATH_BUDGET;
        self.flow_cache.clear();

        let world_width = width as f32 * self.grid.cell_size;
        let world_height = height as f32 * self.grid.cell_size;
        self.spatial = spatial::SpatialHash::new(SPATIAL_CELL, world_width, world_height);
        for (index, unit) in self.units.iter().enumerate() {
            self.spatial.insert(index as u32, unit.pos);
        }
        self.spatial_buffer.clear();

        // `fog_offsets` is a pure function of the sight-radius bound chosen by
        // `Sim::new`; it is not save state and must not be rebuilt here.
        self.fog_stamp = vec![0; cells];
        self.events.clear();
    }
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// FNV-1a over the payload. Cheap, deterministic, and enough to catch a
/// truncated or bit-flipped save before any structure is trusted.
fn checksum(payload: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in payload {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn seal(payload: &[u8]) -> Vec<u8> {
    seal_version(payload, SAVE_FORMAT_VERSION)
}

fn seal_version(payload: &[u8], version: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len() + CHECKSUM_LEN);
    out.extend_from_slice(&SAVE_MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&checksum(payload).to_le_bytes());
    out
}

fn open_envelope(bytes: &[u8]) -> SaveResult<(&[u8], u16)> {
    let mut r = R::new(bytes);
    let magic = r.take(4)?;
    if magic != SAVE_MAGIC {
        let mut found = [0u8; 4];
        found.copy_from_slice(magic);
        return Err(SaveError::BadMagic(found));
    }
    let version = r.u16()?;
    if !(1..=SAVE_FORMAT_VERSION).contains(&version) {
        return Err(SaveError::UnknownVersion(version));
    }
    let flags = r.u16()?;
    if flags != 0 {
        return Err(SaveError::Flags(flags));
    }
    let declared = r.u32()? as usize;
    let actual = bytes.len().saturating_sub(HEADER_LEN + CHECKSUM_LEN);
    if declared != actual {
        return Err(SaveError::LengthMismatch { declared, actual });
    }
    let payload = r.take(declared)?;
    let stored = r.u64()?;
    let computed = checksum(payload);
    if stored != computed {
        return Err(SaveError::Checksum {
            expected: stored,
            actual: computed,
        });
    }
    if r.remaining() != 0 {
        return Err(SaveError::TrailingBytes(r.remaining()));
    }
    Ok((payload, version))
}

/// Decodes a version-1 payload (no buff/squad/stamp sections).
fn decode_v1(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 1)
}

/// Decodes a version-2 payload (buff/squad/stamp sections, no ethereal byte).
fn decode_v2(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 2)
}

/// Decodes a version-3 payload (no script section).
fn decode_v3(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 3)
}

/// Decodes the current version-4 payload (script section included).
fn decode_v4(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 4)
}

/// Decodes a version-5 payload (life-regen fields appended per unit).
fn decode_v5(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 5)
}

/// Decodes the current version-6 payload: the headless dialog/button/timer-dialog
/// state and the two game-event trigger registrations appended to their records.
fn decode_v6(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 6)
}

/// Decodes the current version-7 payload: the per-unit runtime
/// `UnitAddType`/`UnitRemoveType` classification delta appended to each
/// `UnitMeta` record.
fn decode_v7(payload: &[u8]) -> SaveResult<Sim> {
    decode_payload(payload, 7)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl Sim {
    /// Encodes the complete simulation state as a versioned save file.
    ///
    /// Round-trip contract: `Sim::from_save(&sim.to_save_bytes())` restores a
    /// simulation whose [`Sim::state_hash`] equals `sim.state_hash()` and whose
    /// re-encoded bytes are identical.
    pub fn to_save_bytes(&self) -> Vec<u8> {
        seal(&encode_payload(self))
    }

    /// Restores a simulation from a save produced by [`Sim::to_save_bytes`].
    ///
    /// Every malformed input — truncated file, wrong magic, unknown version,
    /// broken checksum, corrupt index — returns a [`SaveError`]; the function
    /// never panics on untrusted bytes.
    pub fn from_save(bytes: &[u8]) -> SaveResult<Sim> {
        let (payload, version) = open_envelope(bytes)?;
        match version {
            1 => decode_v1(payload),
            2 => decode_v2(payload),
            3 => decode_v3(payload),
            4 => decode_v4(payload),
            5 => decode_v5(payload),
            6 => decode_v6(payload),
            7 => decode_v7(payload),
            other => Err(SaveError::UnknownVersion(other)),
        }
    }

    /// Writes a save file (owner-only permissions are the caller's policy).
    pub fn save_to_path(&self, path: impl AsRef<Path>) -> SaveResult<()> {
        let bytes = self.to_save_bytes();
        std::fs::write(path.as_ref(), bytes)
            .map_err(|error| SaveError::Io(format!("{}: {error}", path.as_ref().display())))
    }

    /// Reads a save file.
    pub fn from_save_path(path: impl AsRef<Path>) -> SaveResult<Sim> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|error| SaveError::Io(format!("{}: {error}", path.as_ref().display())))?;
        Self::from_save(&bytes)
    }

    /// A compact digest over the *encoded* save payload, used by tests and by
    /// lockstep verification to compare two simulations across every saved
    /// field (including the ones `state_hash` does not cover: destructables,
    /// items, projectiles, camps, shops, upgrades, fog).
    pub fn save_digest(&self) -> u64 {
        checksum(&encode_payload(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offset of the `SEC_IDS` frame body inside an encoded payload.
    ///
    /// Frames are tag-prefixed without a length field, so reaching the ID block
    /// means walking the two frames in front of it. Both tags and the grid size
    /// are asserted, so a writer layout change makes this helper fail loudly
    /// instead of patching the wrong four bytes.
    fn ids_frame_offset(payload: &[u8]) -> usize {
        const HEADER: usize = 4 + 8 + 8 + 8 + 1 + 1;
        let tag = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        assert_eq!(tag(0), SEC_HEADER, "first frame is the header");
        assert_eq!(tag(HEADER), SEC_GRID, "second frame is the grid");
        let width = u32::from_le_bytes(payload[HEADER + 4..HEADER + 8].try_into().unwrap());
        let height = u32::from_le_bytes(payload[HEADER + 8..HEADER + 12].try_into().unwrap());
        let bits = (width as usize * height as usize).div_ceil(8);
        let ids = HEADER + 20 + bits;
        assert_eq!(tag(ids), SEC_IDS, "third frame is the id counters");
        ids + 4
    }

    #[test]
    fn save_format_roundtrip_empty_sim() {
        let sim = Sim::new(7, 32, 32);
        let bytes = sim.to_save_bytes();
        let restored = Sim::from_save(&bytes).expect("round trip");
        assert_eq!(restored.state_hash(), sim.state_hash());
        assert_eq!(restored.to_save_bytes(), bytes);
    }

    #[test]
    fn script_restore_rejects_out_of_range_instructions() {
        let mut code = Code::default();
        code.functions.push(CodeFunc {
            name: "bad".into(),
            params: 0,
            nlocals: 0,
            code: vec![Instr::PushGlobal(3)],
        });
        code.by_name.insert("bad".into(), 0);
        code.init_function = 0;
        assert!(matches!(
            validate_code(&code),
            Err(SaveError::OutOfRange {
                section: "script.code.global",
                ..
            })
        ));
    }

    #[test]
    fn script_handle_kinds_round_trip() {
        for (tag, kind) in HANDLE_KINDS.iter().enumerate() {
            assert_eq!(handle_kind_tag(*kind), tag as u8);
            let mut w = W::new();
            w.u8(tag as u8);
            let mut r = R::new(&w.buf);
            assert_eq!(read_handle_kind(&mut r).expect("kind"), *kind);
        }
    }

    #[test]
    fn save_format_rejects_bad_magic() {
        let error = Sim::from_save(b"nope").err().expect("bad magic");
        assert!(matches!(
            error,
            SaveError::Truncated { .. } | SaveError::BadMagic(_)
        ));
    }

    #[test]
    fn version_2_file_loads_with_ethereal_false() {
        let mut sim = Sim::new(11, 32, 32);
        let worker = sim
            .spawn_def(0, "hpea", Vec2::new(300.0, 300.0))
            .expect("worker");
        let index = sim.unit_index(worker).expect("unit index");
        sim.units[index].ethereal = true;
        sim.step(&[]);

        // A genuine v2 file: same layout, no per-unit ethereal byte.
        let v2 = seal_version(&encode_payload_version(&sim, 2), 2);
        assert_eq!(u16::from_le_bytes([v2[4], v2[5]]), 2);
        let restored = Sim::from_save(&v2).expect("v2 file still loads");
        assert!(restored.unit(worker).is_some(), "the unit survives the migration");
        assert!(
            !restored.unit(worker).unwrap().ethereal,
            "v2 has no ethereal field, so the unit loads non-ethereal"
        );

        // The current v3 file preserves the live state.
        let v3 = sim.to_save_bytes();
        assert_eq!(u16::from_le_bytes([v3[4], v3[5]]), SAVE_FORMAT_VERSION);
        let restored = Sim::from_save(&v3).expect("v3 round trip");
        assert!(
            restored.unit(worker).unwrap().ethereal,
            "v3 restores the ethereal combat state"
        );
        assert_eq!(restored.state_hash(), sim.state_hash());
    }

    /// A save whose `next_id` was raised past the ID space must be rejected with
    /// a typed error before the dense id index is sized from it. Reproduced
    /// pre-fix: `from_save` accepted `next_id` values that only the counter
    /// bounds, and `rebuild_derived` then allocated `next_id` slots.
    #[test]
    fn id_counters_above_the_id_space_are_rejected_before_allocating() {
        let mut sim = Sim::new(3, 32, 32);
        sim.spawn_def(0, "hpea", Vec2::new(300.0, 300.0))
            .expect("worker");
        sim.step(&[]);
        let payload = encode_payload_version(&sim, SAVE_FORMAT_VERSION);
        let ids = ids_frame_offset(&payload);
        // 2_000_000 is above the ID-space cap but small enough that the pre-fix
        // reader survives allocating it, so the reproduction is observable
        // without an OOM (the u32::MAX case below is the one that aborts).
        for value in [0u32, 2_000_000, u32::MAX] {
            let mut hostile = payload.clone();
            hostile[ids..ids + 4].copy_from_slice(&value.to_le_bytes());
            let error = Sim::from_save(&seal_version(&hostile, SAVE_FORMAT_VERSION))
                .err()
                .unwrap_or_else(|| panic!("next_id={value} must not load"));
            assert!(
                matches!(
                    error,
                    SaveError::InvalidIdSpace { .. } | SaveError::InvalidValue { .. }
                ),
                "next_id={value}: {error:?}"
            );
        }

        // The counter rule is unconditional: an empty world with `next_id=0`
        // must be rejected too (there are no ids to compare against).
        let empty = Sim::new(9, 32, 32);
        let payload = encode_payload_version(&empty, SAVE_FORMAT_VERSION);
        let ids = ids_frame_offset(&payload);
        let mut hostile = payload.clone();
        hostile[ids..ids + 4].copy_from_slice(&0u32.to_le_bytes());
        let error = Sim::from_save(&seal_version(&hostile, SAVE_FORMAT_VERSION))
            .err()
            .expect("empty save with next_id=0 must not load");
        assert!(
            matches!(error, SaveError::InvalidIdSpace { .. }),
            "{error:?}"
        );
    }

    /// Every entity ID space has the same bound: a crafted counter for items,
    /// destructables, camps or projectiles is rejected like `next_id`.
    #[test]
    fn every_entity_counter_is_bounded_by_the_id_space() {
        let mut sim = Sim::new(4, 32, 32);
        sim.spawn_def(0, "hpea", Vec2::new(300.0, 300.0))
            .expect("worker");
        sim.step(&[]);
        let payload = encode_payload_version(&sim, SAVE_FORMAT_VERSION);
        let ids = ids_frame_offset(&payload);
        for index in 0..5 {
            let mut hostile = payload.clone();
            let at = ids + index * 4;
            hostile[at..at + 4].copy_from_slice(&(MAX_ID_SPACE + 1).to_le_bytes());
            let error = Sim::from_save(&seal_version(&hostile, SAVE_FORMAT_VERSION))
                .err()
                .unwrap_or_else(|| panic!("counter {index} must not load"));
            assert!(
                matches!(error, SaveError::InvalidIdSpace { .. }),
                "counter {index}: {error:?}"
            );
        }
    }

    /// A unit whose id is not below `next_id` is rejected; the save's own IDs
    /// and its counter must agree before any derived state is built.
    #[test]
    fn unit_id_at_or_above_next_id_is_rejected() {
        let mut sim = Sim::new(5, 32, 32);
        let worker = sim
            .spawn_def(0, "hpea", Vec2::new(300.0, 300.0))
            .expect("worker");
        let index = sim.unit_index(worker).expect("index");
        for id in [sim.next_id, sim.next_id + 7, u32::MAX - 1] {
            sim.units[index].id = crate::UnitId(id);
            let error = Sim::from_save(&sim.to_save_bytes())
                .err()
                .unwrap_or_else(|| panic!("unit id {id} must not load"));
            assert!(
                matches!(
                    error,
                    SaveError::OutOfRange {
                        section: "units.id",
                        ..
                    } | SaveError::InvalidIdSpace { .. }
                ),
                "unit id {id}: {error:?}"
            );
        }
    }

    /// Duplicate unit IDs are a corrupt save: two index slots would map to one
    /// unit and the second unit would become unreachable.
    #[test]
    fn duplicate_unit_ids_are_rejected() {
        let mut sim = Sim::new(6, 32, 32);
        let first = sim
            .spawn_def(0, "hpea", Vec2::new(300.0, 300.0))
            .expect("worker");
        let second = sim
            .spawn_def(0, "hfoo", Vec2::new(400.0, 300.0))
            .expect("footman");
        let index = sim.unit_index(second).expect("index");
        sim.units[index].id = first;
        let error = Sim::from_save(&sim.to_save_bytes())
            .err()
            .expect("duplicate ids must not load");
        assert!(
            matches!(error, SaveError::InvalidIdSpace { .. }),
            "{error:?}"
        );
    }

    /// A version-5 file (written before the headless UI state existed) still
    /// loads: the dialog fields it never stored restore to their neutral values,
    /// which is what "keep old valid saves loading" means for the AUD-007 format
    /// extension.
    #[test]
    fn version_5_file_loads_with_neutral_ui_state() {
        let mut sim = Sim::new(0x55, 32, 32);
        let mut bundle = crate::jass::ScriptBundle::new();
        bundle
            .add_source(
                "dialog_v5.j",
                r#"
                globals
                    dialog probe_dialog = null
                endglobals
                function setup takes nothing returns nothing
                    set probe_dialog = DialogCreate()
                    call DialogSetMessage(probe_dialog, "written then dropped by v5")
                    call DialogAddButton(probe_dialog, "OK", 0)
                    call DialogDisplay(Player(0), probe_dialog, true)
                endfunction
                "#,
            )
            .expect("parse");
        sim.attach_script(&bundle, 0x55).expect("compile");
        sim.with_script(|vm| vm.run_function("setup"))
            .expect("script attached")
            .expect("setup runs");
        let dialog = sim
            .with_script(|vm| {
                let index = vm.code.global_by_name["probe_dialog"] as usize;
                match vm.globals[index].clone() {
                    Value::Handle(handle) => handle,
                    other => panic!("probe_dialog is {other:?}"),
                }
            })
            .expect("script");

        // A genuine v5 payload: same layout, no dialogue fields in the object.
        let v5 = seal_version(&encode_payload_version(&sim, 5), 5);
        assert_eq!(u16::from_le_bytes([v5[4], v5[5]]), 5);
        let restored = Sim::from_save(&v5).expect("v5 file still loads");
        assert_eq!(restored.state_hash(), sim.state_hash());
        match restored.script().and_then(|vm| vm.host.object(dialog)) {
            Some(Object::Dialog {
                message,
                shown_to,
                asynchronous,
                buttons,
            }) => {
                assert_eq!(message, "", "v5 stored no message: neutral value");
                assert_eq!(*shown_to, None, "v5 stored no display target");
                assert!(!*asynchronous, "v5 stored no async flag");
                assert_eq!(buttons.len(), 1, "the button list is still there");
            }
            other => panic!("restored dialog is {other:?}"),
        }
        // And the v6 file this build writes keeps the full state.
        let v6 = sim.to_save_bytes();
        let restored = Sim::from_save(&v6).expect("v6 round trip");
        assert!(
            matches!(
                restored.script().and_then(|vm| vm.host.object(dialog)),
                Some(Object::Dialog { message, shown_to: Some(0), .. })
                    if message == "written then dropped by v5"
            ),
            "v6 keeps the dialog message and display target"
        );
    }

    /// A version-6 file (written before `UnitAddType`/`UnitRemoveType` had
    /// per-instance state) still loads: the runtime `UNIT_TYPE_*` delta it never
    /// stored restores to "definition only", so `IsUnitType` answers from the
    /// shared `UnitDef` exactly as it did before the format extension
    /// (AUD-013).
    #[test]
    fn version_6_file_loads_with_neutral_unit_types() {
        let mut sim = Sim::new(0x66, 32, 32);
        let mut bundle = crate::jass::ScriptBundle::new();
        bundle
            .add_source(
                "unit_types_v6.j",
                r#"
                globals
                    unit probe = null
                    boolean hero_after_add = false
                    boolean hero_after_load = true
                endglobals
                function setup takes nothing returns nothing
                    set probe = CreateUnit(Player(0), 'hfoo', 400.0, 400.0, 0.0)
                    call UnitAddType(probe, 0)
                    set hero_after_add = IsUnitType(probe, 0)
                endfunction
                function probe_after_load takes nothing returns nothing
                    set hero_after_load = IsUnitType(probe, 0)
                endfunction
                "#,
            )
            .expect("parse");
        sim.attach_script(&bundle, 0x66).expect("compile");
        sim.with_script(|vm| vm.run_function("setup"))
            .expect("script attached")
            .expect("setup runs");
        let live = sim
            .with_script(|vm| {
                let index = vm.code.global_by_name["hero_after_add"] as usize;
                vm.globals[index].truthy()
            })
            .expect("script");
        assert!(live, "the runtime add is visible before the save");

        // A genuine v6 payload: same layout, no type words in the meta record.
        let v6 = seal_version(&encode_payload_version(&sim, 6), 6);
        assert_eq!(u16::from_le_bytes([v6[4], v6[5]]), 6);
        let mut restored = Sim::from_save(&v6).expect("v6 file still loads");
        assert_eq!(restored.state_hash(), sim.state_hash());
        restored
            .with_script(|vm| vm.run_function("probe_after_load"))
            .expect("script attached")
            .expect("probe runs");
        let after: bool = restored
            .with_script(|vm| {
                let index = vm.code.global_by_name["hero_after_load"] as usize;
                vm.globals[index].truthy()
            })
            .expect("script");
        assert!(
            !after,
            "a v6 file never stored the delta: the footman is not a hero again"
        );

        // The version this build writes keeps the full state.
        let v7 = sim.to_save_bytes();
        assert_eq!(u16::from_le_bytes([v7[4], v7[5]]), SAVE_FORMAT_VERSION);
        let mut restored = Sim::from_save(&v7).expect("v7 round trip");
        restored
            .with_script(|vm| vm.run_function("probe_after_load"))
            .expect("script attached")
            .expect("probe runs");
        let after: bool = restored
            .with_script(|vm| {
                let index = vm.code.global_by_name["hero_after_load"] as usize;
                vm.globals[index].truthy()
            })
            .expect("script");
        assert!(after, "v7 keeps the per-instance classification");
    }

    /// The scalar counter contract, property-tested over the whole `u32` domain
    /// (sampled): a counter is valid exactly inside `1..=MAX_ID_SPACE`.
    #[test]
    fn id_space_counter_bound_holds_over_sampled_values() {
        let mut samples: Vec<u32> = vec![
            0,
            1,
            2,
            MAX_ID_SPACE - 1,
            MAX_ID_SPACE,
            MAX_ID_SPACE + 1,
            MAX_ID_SPACE * 2,
            u32::MAX / 2,
            u32::MAX - 1,
            u32::MAX,
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..200 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            samples.push(state as u32);
        }
        for value in samples {
            let result = check_id_counter("ids.next_id", value);
            let expected_ok = (1..=MAX_ID_SPACE).contains(&value);
            assert_eq!(
                result.is_ok(),
                expected_ok,
                "counter {value}: {result:?} (expected ok={expected_ok})"
            );
        }
    }
}
