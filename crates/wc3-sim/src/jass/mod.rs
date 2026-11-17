//! JASS2 script engine for the simulation.
//!
//! The map script is not decoration: `war3map.j` sets the map name, creates the
//! units a melee map spawns, registers 29 triggers on `(2)Justice.w3x`, starts
//! timers, and issues orders. This module runs it against the real simulation.
//!
//! Structure
//! ---------
//! * [`value`] — script values. `integer` is `i32`, `real` is `f32`, because the
//!   engine's JASS2 types are 32-bit.
//! * [`registry`] + `registry_gen` — every native name the engine or `common.j`
//!   declares (1665 rows), with its signature, plus whether a handler exists.
//! * [`vm`] — the compiler (AST → flat instructions) and the executor with
//!   explicit frames, a 300 000-instruction budget and a call-depth cap.
//! * [`natives`] — the handlers, bound to [`crate::Sim`].
//! * [`host`] — handles and world state the simulation does not model.
//! * [`scheduler`] — ticks → timers, trigger events, resumed threads.
//! * [`bundle`] — loading and merging `common.j` + `Blizzard.j` + the map script.
//!
//! Honesty contract
//! ----------------
//! A native with no handler still resolves (the registry knows its type), returns
//! a value of its declared type, and is counted in
//! [`JassVm::unresolved_natives`]. There is no path where a stub looks like
//! success: `tests/jass_native_coverage.rs` asserts the count is zero for the
//! measured map corpus, so a native appearing in a new map fails a test instead
//! of silently returning zero.

pub mod bundle;
pub mod host;
pub mod locale;
pub mod native_status;
pub mod natives;
pub mod registry;
mod registry_gen;
pub mod scheduler;
pub mod value;
pub mod vm;

pub use bundle::{ScriptBundle, find_game_scripts};
pub use host::{Host, Object, RegEvent, TimerState, TriggerState};
pub use locale::{LocalizedStrings, MAP_FLAG_BITS, map_flag_name, map_flags_from_w3i};
pub use native_status::{NativeStatus, StatusCounts, StatusEntry, report_json, report_rows};
pub use registry::{NativeDef, RegistryStats, is_implemented, lookup as native_def};
pub use value::{Handle, HandleKind, Signature, Type, Value};
pub use vm::{
    Code, CompileError, DEFAULT_STEP_BUDGET, EntryReport, EventContext, JassVm, MAX_CALL_DEPTH,
    Trap, VmSummary,
};

use crate::{Command, Sim};

/// A simulation with a script machine attached.
///
/// [`ScriptedSim::step`] is the integration point: it steps the simulation, then
/// advances script time by one tick, raising the trigger events the step
/// produced. [`Sim::step`] itself is untouched, so a caller that does not want
/// script behaviour keeps using `Sim` directly, and no other system changes.
pub struct ScriptedSim {
    pub vm: JassVm,
}

impl ScriptedSim {
    /// Builds a machine from a merged script bundle around `sim`.
    pub fn new(sim: Sim, bundle: &ScriptBundle, seed: u64) -> Result<Self, Vec<CompileError>> {
        let mut vm = JassVm::new(&bundle.program, sim, seed)?;
        // The bundle carries the map's own data tables: `GetLocalizedString`
        // reads the `war3map.wts` + global string table, `IsMapFlagSet` reads
        // the `war3map.w3i` flags word.
        vm.host.strings = bundle.localized.clone();
        vm.host.map_flags = bundle.map_flags;
        Ok(Self { vm })
    }

    /// Runs `config` then `main` (the engine's entry points).
    pub fn run_map_entry_points(&mut self) -> EntryReport {
        self.vm.run_map_entry_points()
    }

    /// Advances one simulation tick and the script clock.
    pub fn step(&mut self, commands: &[Command]) {
        // Events raised between ticks (by the host or a test) are dispatched
        // before `Sim::step` resets the event buffer.
        self.vm.dispatch_pending_events();
        self.vm.sim.step(commands);
        self.vm.advance(1.0 / crate::TICKS_PER_SECOND as f64);
        // Region enter/leave events are position based, so they are computed
        // after the step moved the units.
        self.vm.track_regions();
        self.vm.track_unit_ranges();
    }

    pub fn sim(&self) -> &Sim {
        &self.vm.sim
    }

    pub fn sim_mut(&mut self) -> &mut Sim {
        &mut self.vm.sim
    }

    pub fn vm(&self) -> &JassVm {
        &self.vm
    }

    pub fn vm_mut(&mut self) -> &mut JassVm {
        &mut self.vm
    }

    /// Script time in seconds.
    pub fn script_time(&self) -> f64 {
        self.vm.time
    }

    /// Natives the corpus calls that have no handler, with counts.
    pub fn unresolved_natives(&self) -> &std::collections::BTreeMap<String, u64> {
        self.vm.unresolved_natives()
    }
}
