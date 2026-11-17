//! The script machine: a compiler from the `wc3-re` JASS2 AST to a flat
//! instruction stream, and an explicit-frame executor.
//!
//! Why a flat stream instead of a tree walker: WC3 map scripts suspend. A BJ
//! function such as `PolledWait` loops on `TriggerSleepAction`, an action
//! thread can sleep for minutes, and 29 triggers in `(2)Justice.w3x` wait on
//! timers. A recursive AST interpreter cannot be suspended without replaying
//! side effects, so the machine compiles once to instructions and keeps its
//! frames explicit; a `wait` instruction parks the frame list and the scheduler
//! resumes it at the right tick with no re-execution.
//!
//! Determinism: instruction order is fixed, threads are indexed, the RNG is a
//! seeded xorshift, and every container is ordered.

use std::collections::BTreeMap;

use wc3_re::jass::ast::{BinOp, Expr, FunctionDecl, Program, Stmt, UnOp};

use crate::Sim;

use super::host::{Host, Object};
use super::registry;
use super::value::{HandleKind, Value, format_real, real_to_int};

/// Engine execution budget per entry run (`0x493e0` = 300 000, the TFT
/// `game_jass_exec` counter measured in `analysis/agent-re/formats/ENGINE_MAP.md`).
pub const DEFAULT_STEP_BUDGET: u64 = 300_000;
/// Call-stack cap. Deep recursion in a map script must trap with a message,
/// never overflow the native stack (H2 experience 4461: a shipped map script
/// needed a 32 MB stack).
pub const MAX_CALL_DEPTH: usize = 160;
/// JASS arrays are `0..8191`.
pub const ARRAY_BOUND: i32 = 8192;

/// Operand-stack opcode. Every payload is `Copy`, so fetching an instruction is
/// a memcpy and never clones a string.
#[derive(Debug, Clone, Copy)]
pub enum Instr {
    Null,
    Int(i32),
    Real(f32),
    Bool(bool),
    Str(u32),
    Code(u32),
    PushLocal(u16),
    PushGlobal(u32),
    LoadLocalArray(u16),
    LoadGlobalArray(u32),
    StoreLocal(u16),
    StoreGlobal(u32),
    StoreLocalArray(u16),
    StoreGlobalArray(u32),
    Pop,
    Dup,
    Call { func: u32, argc: u8 },
    CallIndirect { argc: u8 },
    Native { slot: u32, argc: u8 },
    /// A call to a name that is neither a script function nor a registry
    /// native; records an error and pushes `null`.
    Unknown { pool: u32, argc: u8 },
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),
    Return,
    /// Parks the thread for the real value on the stack; on resume the elapsed
    /// seconds are pushed (engine `TriggerSleepAction`).
    Wait,
    Bin(BinTag),
    Un(UnTag),
    ResetLocal(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinTag {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnTag {
    Neg,
    Not,
    Pos,
}

impl From<BinOp> for BinTag {
    fn from(op: BinOp) -> Self {
        match op {
            BinOp::Add => Self::Add,
            BinOp::Sub => Self::Sub,
            BinOp::Mul => Self::Mul,
            BinOp::Div => Self::Div,
            BinOp::Eq => Self::Eq,
            BinOp::Ne => Self::Ne,
            BinOp::Lt => Self::Lt,
            BinOp::Le => Self::Le,
            BinOp::Gt => Self::Gt,
            BinOp::Ge => Self::Ge,
            BinOp::And => Self::And,
            BinOp::Or => Self::Or,
        }
    }
}

impl From<UnOp> for UnTag {
    fn from(op: UnOp) -> Self {
        match op {
            UnOp::Neg => Self::Neg,
            UnOp::Not => Self::Not,
            UnOp::Pos => Self::Pos,
        }
    }
}

/// One compiled function.
#[derive(Debug, Clone, Default)]
pub struct CodeFunc {
    pub name: String,
    pub params: usize,
    pub nlocals: usize,
    pub code: Vec<Instr>,
}

/// Global storage kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKind {
    Scalar,
    Array,
}

#[derive(Debug, Clone)]
pub struct GlobalSlot {
    pub name: String,
    pub kind: GlobalKind,
}

/// Compiled program.
#[derive(Debug, Clone, Default)]
pub struct Code {
    pub functions: Vec<CodeFunc>,
    pub by_name: BTreeMap<String, u32>,
    pub globals: Vec<GlobalSlot>,
    pub global_by_name: BTreeMap<String, u32>,
    pub strings: Vec<String>,
    /// Index of the synthetic `__init_globals` function.
    pub init_function: u32,
    /// Names compiled to a typed default because the loaded headers do not
    /// declare them (newer-header constants, typos). Reported, never hidden.
    pub undeclared: Vec<String>,
}

/// Compile error for one script construct.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub function: String,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(): {}", self.function, self.message)
    }
}

/// Reason a thread stopped abnormally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    StepBudget,
    CallDepth,
    StackUnderflow,
    InvalidHandle,
    Other(String),
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StepBudget => f.write_str("step budget exhausted"),
            Self::CallDepth => f.write_str("call depth exceeded"),
            Self::StackUnderflow => f.write_str("operand stack underflow"),
            Self::InvalidHandle => f.write_str("invalid handle"),
            Self::Other(text) => f.write_str(text),
        }
    }
}

/// One activation record.
#[derive(Debug, Clone)]
pub struct Frame {
    pub func: u32,
    pub pc: usize,
    pub locals: Vec<Value>,
    pub stack_base: usize,
}

/// The event context a trigger action sees (`GetTriggerUnit`, `GetEventDamage`,
/// …). It travels with the thread, so a trigger that waits keeps its context.
#[derive(Debug, Clone, Default)]
pub struct EventContext {
    pub trigger: Option<u32>,
    pub event_id: i64,
    pub unit: Option<u32>,
    pub triggering_unit: Option<u32>,
    pub dying_unit: Option<u32>,
    pub entering_unit: Option<u32>,
    pub leaving_unit: Option<u32>,
    pub changing_unit: Option<u32>,
    pub changing_unit_prev_owner: Option<u8>,
    pub attacker: Option<u32>,
    pub damaged: Option<u32>,
    pub damage: f32,
    pub spell_id: i64,
    pub spell_target_unit: Option<u32>,
    pub trained_unit: Option<u32>,
    pub trained_unit_type: i64,
    pub expired_timer: Option<u32>,
    pub player: Option<u8>,
    pub dying_destructable: Option<u32>,
    pub leveling_unit: Option<u32>,
    pub killing_unit: Option<u32>,
    pub cancelable: Option<u32>,
    pub order_id: Option<i64>,
    /// The `gamestate` that raised a `TriggerRegisterGameStateEvent` firing,
    /// read back by `GetEventGameState`. `None` for every other event.
    pub game_state: Option<i32>,
}

impl EventContext {
    /// Context used when a native asks outside any event.
    pub fn none() -> Self {
        Self::default()
    }
}

/// A script thread: a frame list that may be parked on a wait.
#[derive(Debug, Clone)]
pub struct ThreadState {
    pub frames: Vec<Frame>,
    pub stack: Vec<Value>,
    /// Simulation time at which a parked thread resumes (reporting only).
    pub wake_at: Option<f64>,
    /// Tick at which a parked thread resumes. Tick-based so a 0.4 s wait on a
    /// 30 Hz clock resumes exactly on tick 12 instead of when the floating-point
    /// clock happens to cross the deadline.
    pub wake_tick: Option<u64>,
    /// Time the current wait started (for the elapsed value `PolledWait` uses).
    pub wait_started: Option<f64>,
    pub name: String,
    pub started_at: f64,
    pub steps: u64,
    pub event: Option<EventContext>,
    pub last_value: Option<Value>,
}

/// How a thread finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Returned,
    Suspended,
    Trapped,
}

/// Report from [`JassVm::run_map_entry_points`].
#[derive(Debug, Clone)]
pub struct EntryReport {
    pub value: Value,
    pub steps: u64,
    pub traps: Vec<String>,
    pub triggers_registered: usize,
    pub units_created: usize,
    pub natives_called: usize,
}

impl Default for EntryReport {
    fn default() -> Self {
        Self {
            value: Value::Null,
            steps: 0,
            traps: Vec::new(),
            triggers_registered: 0,
            units_created: 0,
            natives_called: 0,
        }
    }
}

/// The script machine.
pub struct JassVm {
    pub code: Code,
    pub globals: Vec<Value>,
    pub global_arrays: Vec<BTreeMap<i32, Value>>,
    pub host: Host,
    pub sim: Sim,
    pub threads: Vec<ThreadState>,
    /// Index of the thread currently driving execution.
    current_thread: usize,
    pub steps: u64,
    pub budget: u64,
    /// Natives called without a handler: name → call count.
    pub unresolved: BTreeMap<String, u64>,
    /// Every native call, name → count, canonical registry spelling.
    pub native_calls: BTreeMap<String, u64>,
    /// Script functions called, name → count.
    pub function_calls: BTreeMap<String, u64>,
    pub errors: Vec<String>,
    pub output: Vec<String>,
    pub rng: u64,
    /// Simulation time in seconds; advanced by [`JassVm::tick`].
    pub time: f64,
    pub tick: u64,
    pub max_threads: usize,
    pub text_log: Vec<String>,
    /// Event constant ids resolved from the loaded `common.j` globals.
    pub event_ids: super::scheduler::EventIds,
    /// `GetEnumUnit` while a `ForGroup` loop runs.
    pub enum_unit: Option<u32>,
    /// `GetFilterUnit` while a filter runs.
    pub filter_unit: Option<u32>,
    /// `GetEnumPlayer` while a `ForForce` loop runs.
    pub enum_player: Option<u8>,
    /// `GetEnumDestructable` / `GetEnumItem` during a rect enumeration.
    pub enum_destructable: Option<u32>,
    pub enum_item: Option<u32>,
    /// Last known occupants per region, for enter/leave triggers.
    pub region_occupancy: BTreeMap<u32, std::collections::BTreeSet<u32>>,
    /// Units inside a `TriggerRegisterUnitInRange` radius, per (trigger, watched).
    pub unit_range_occupancy: BTreeMap<(u32, u32), std::collections::BTreeSet<u32>>,
    /// Sleep requested by the native running on this thread, consumed by the
    /// executor right after the dispatch returns.
    ///
    /// `Instr::Wait` handles the direct `TriggerSleepAction` call; a call that
    /// reaches the handler through the runtime name dispatch asks the executor
    /// to park the thread instead of returning to the script. The thread index
    /// travels with the request so a nested thread can never park its parent.
    pending_sleep: Option<(usize, f32)>,
}

impl JassVm {
    /// Compiles `program` and binds the machine to `sim`.
    pub fn new(program: &Program, sim: Sim, seed: u64) -> Result<Self, Vec<CompileError>> {
        let compiler = Compiler::new(program);
        let code = compiler.compile()?;
        let globals = vec![Value::Null; code.globals.len()];
        let global_arrays = (0..code.globals.len()).map(|_| BTreeMap::new()).collect();
        let mut vm = Self {
            code,
            globals,
            global_arrays,
            host: Host::new(),
            sim,
            threads: Vec::new(),
            current_thread: 0,
            steps: 0,
            budget: DEFAULT_STEP_BUDGET,
            unresolved: BTreeMap::new(),
            native_calls: BTreeMap::new(),
            function_calls: BTreeMap::new(),
            errors: Vec::new(),
            output: Vec::new(),
            rng: seed | 1,
            time: 0.0,
            tick: 0,
            max_threads: 64,
            text_log: Vec::new(),
            event_ids: super::scheduler::EventIds::default(),
            enum_unit: None,
            filter_unit: None,
            enum_player: None,
            enum_destructable: None,
            enum_item: None,
            region_occupancy: BTreeMap::new(),
            unit_range_occupancy: BTreeMap::new(),
            pending_sleep: None,
        };
        vm.event_ids = super::scheduler::EventIds::from_vm(&vm);
        vm.bind_map_units();
        vm.run_global_initialisers();
        // Global initialisers may define event constants; resolve again.
        vm.event_ids = super::scheduler::EventIds::from_vm(&vm);
        Ok(vm)
    }

    /// Blank machine for a save restore: no compilation, no global
    /// initialisers, no unit binding. The caller owns the restored `Code` and
    /// fills the public state (`globals`, `threads`, `host`, clock, RNG) from
    /// the save, then installs the saved unit handle table in order. Used only
    /// by [`crate::save`]; a normal boot goes through [`JassVm::new`].
    pub(crate) fn restore_blank(code: Code, sim: Sim, seed: u64) -> Self {
        Self {
            code,
            globals: Vec::new(),
            global_arrays: Vec::new(),
            host: Host::new(),
            sim,
            threads: Vec::new(),
            current_thread: 0,
            steps: 0,
            budget: DEFAULT_STEP_BUDGET,
            unresolved: BTreeMap::new(),
            native_calls: BTreeMap::new(),
            function_calls: BTreeMap::new(),
            errors: Vec::new(),
            output: Vec::new(),
            rng: seed | 1,
            time: 0.0,
            tick: 0,
            max_threads: 64,
            text_log: Vec::new(),
            event_ids: super::scheduler::EventIds::default(),
            enum_unit: None,
            filter_unit: None,
            enum_player: None,
            enum_destructable: None,
            enum_item: None,
            region_occupancy: BTreeMap::new(),
            unit_range_occupancy: BTreeMap::new(),
            pending_sleep: None,
        }
    }

    /// Mirrors pre-placed simulation units into script handles so
    /// `GetUnitX(GetEnumUnit())`-style code sees map units.
    fn bind_map_units(&mut self) {
        let units: Vec<crate::UnitId> = self.sim.units.iter().map(|unit| unit.id).collect();
        for unit in units {
            self.host.unit_handle(unit);
        }
    }

    fn run_global_initialisers(&mut self) {
        let init = self.code.init_function;
        if init as usize >= self.code.functions.len()
            || self.code.functions[init as usize].code.is_empty()
        {
            return;
        }
        self.steps = 0;
        match self.spawn_and_drive(init, Vec::new(), None) {
            Ok(_) => {}
            Err(trap) => self.errors.push(format!("global initialisers: {trap}")),
        }
        self.steps = 0;
    }

    /// Natives called with no handler, with counts. This is the "no silent
    /// success" census: a stub still returns a typed default, but it is listed
    /// here and in every report.
    pub fn unresolved_natives(&self) -> &BTreeMap<String, u64> {
        &self.unresolved
    }

    pub fn unresolved_calls(&self) -> u64 {
        self.unresolved.values().sum()
    }

    pub fn budget_remaining(&self) -> u64 {
        self.budget.saturating_sub(self.steps)
    }

    /// Runs `name` on a fresh thread and returns its value.
    pub fn run_function(&mut self, name: &str) -> Result<Value, Trap> {
        let Some(func) = self.code.by_name.get(&name.to_ascii_lowercase()).copied() else {
            return Err(Trap::Other(format!("unknown script function {name}")));
        };
        self.steps = 0;
        self.spawn_and_drive(func, Vec::new(), None)
    }

    fn spawn_and_drive(
        &mut self,
        func: u32,
        args: Vec<Value>,
        event: Option<EventContext>,
    ) -> Result<Value, Trap> {
        let name = self.code.functions[func as usize].name.clone();
        let thread = self.spawn_thread(func, name, args, event);
        let outcome = self.drive_thread(thread);
        let value = self.threads[thread].last_value.clone();
        match outcome {
            Ok(RunOutcome::Suspended) => Ok(Value::Null),
            Ok(RunOutcome::Trapped) => {
                self.threads.swap_remove(thread);
                Err(Trap::StepBudget)
            }
            Ok(_) => {
                self.threads.swap_remove(thread);
                Ok(value.unwrap_or(Value::Null))
            }
            Err(trap) => {
                let thread_name = self.threads[thread].name.clone();
                self.threads.swap_remove(thread);
                self.errors.push(format!("{thread_name}: {trap}"));
                Err(trap)
            }
        }
    }

    /// Runs the map entry points in engine order: `config`, then `main`.
    ///
    /// The engine calls `config` from `main` (`InitCustomTriggers` etc. are
    /// inside the map script), so both are run and both are reported.
    pub fn run_map_entry_points(&mut self) -> EntryReport {
        let mut report = EntryReport::default();
        self.steps = 0;
        for entry in ["config", "main"] {
            if !self.code.by_name.contains_key(entry) {
                continue;
            }
            let before = self.sim.units.len();
            let func = self.code.by_name[entry];
            match self.spawn_and_drive(func, Vec::new(), None) {
                Ok(value) => {
                    if entry == "main" {
                        report.value = value;
                    }
                }
                Err(trap) => report.traps.push(format!("{entry}: {trap}")),
            }
            report.units_created += self.sim.units.len().saturating_sub(before);
        }
        report.steps = self.steps;
        report.triggers_registered = self
            .host
            .objects
            .values()
            .filter(|object| matches!(object, Object::Trigger(_)))
            .count();
        report.natives_called = self.native_calls.values().sum::<u64>() as usize;
        report.traps.extend(self.errors.iter().cloned());
        report
    }

    fn spawn_thread(
        &mut self,
        func: u32,
        name: String,
        args: Vec<Value>,
        event: Option<EventContext>,
    ) -> usize {
        let nlocals = self.code.functions[func as usize].nlocals;
        let mut locals = vec![Value::Null; nlocals];
        for (slot, value) in args.into_iter().enumerate() {
            if slot < locals.len() {
                locals[slot] = value;
            }
        }
        self.threads.push(ThreadState {
            frames: vec![Frame {
                func,
                pc: 0,
                locals,
                stack_base: 0,
            }],
            stack: Vec::new(),
            wake_at: None,
            wake_tick: None,
            wait_started: None,
            name,
            started_at: self.time,
            steps: 0,
            event,
            last_value: None,
        });
        self.threads.len() - 1
    }

    /// Calls a script function from a native (`TriggerExecute`, timer callback)
    /// and runs it to completion or suspension.
    pub fn call_script_function(&mut self, func: u32, args: Vec<Value>) -> Value {
        self.call_script_function_with_event(func, args, None)
    }

    /// Same, with an explicit event context for `GetTriggerUnit` and friends.
    pub fn call_script_function_with_event(
        &mut self,
        func: u32,
        args: Vec<Value>,
        event: Option<EventContext>,
    ) -> Value {
        if func as usize >= self.code.functions.len() {
            self.errors.push("call of an unknown code value".into());
            return Value::Null;
        }
        if self.threads.len() >= self.max_threads {
            self.errors.push("thread limit reached".into());
            return Value::Null;
        }
        match self.spawn_and_drive(func, args, event) {
            Ok(value) => value,
            Err(_) => Value::Null,
        }
    }

    /// The event context of the thread that is currently running.
    pub fn event(&self) -> EventContext {
        self.threads
            .get(self.current_thread)
            .and_then(|thread| thread.event.clone())
            .unwrap_or_default()
    }

    /// Asks the executor to park the running thread for `seconds`, the way the
    /// `Wait` instruction does for a direct `TriggerSleepAction` call.
    ///
    /// This is the native-side half of the engine sleep: a handler reached
    /// through the runtime name dispatch (a name the compiler could not resolve
    /// statically) has no instruction of its own, so it records the request and
    /// the executor suspends the thread after the dispatch returns. On resume
    /// the scheduler pushes the elapsed seconds, which is this call's value.
    ///
    /// Returns `false` when no script thread is running (a Rust caller with an
    /// idle machine); the handler records a diagnostic in that case instead of
    /// pretending to have slept.
    pub fn request_sleep(&mut self, seconds: f32) -> bool {
        if self.current_thread >= self.threads.len() {
            return false;
        }
        self.pending_sleep = Some((self.current_thread, seconds.max(0.0)));
        true
    }

    /// Parks `thread` for `seconds` of simulation time.
    ///
    /// The deadline is quantized to the tick grid (`ceil`, at least one tick)
    /// exactly like the engine's frame-driven wait, so the resume tick is
    /// deterministic across builds; `resume_threads` pushes the elapsed time,
    /// which is the `Wait`/`TriggerSleepAction` return value.
    fn park_thread(&mut self, thread: usize, seconds: f32) {
        let seconds = seconds.max(0.0);
        let ticks = (seconds * crate::TICKS_PER_SECOND as f32).ceil().max(1.0) as u64;
        let state = &mut self.threads[thread];
        state.wake_at = Some(self.time + seconds as f64);
        state.wake_tick = Some(self.tick + ticks);
        state.wait_started = Some(self.time);
    }

    /// Consumes a sleep the native running on `thread` requested and parks it.
    /// `false` when nothing was requested, so the caller keeps the normal
    /// "push the handler's value" path.
    fn take_requested_sleep(&mut self, thread: usize) -> bool {
        match self.pending_sleep.take() {
            Some((requested_by, seconds)) if requested_by == thread => {
                self.park_thread(thread, seconds);
                true
            }
            Some((requested_by, _seconds)) => {
                self.errors.push(format!(
                    "TriggerSleepAction: thread {requested_by} requested a sleep while thread \
                     {thread} was running"
                ));
                false
            }
            None => false,
        }
    }

    pub(crate) fn drive_thread(&mut self, thread: usize) -> Result<RunOutcome, Trap> {
        let saved = self.current_thread;
        self.current_thread = thread;
        let outcome = self.drive_inner(thread);
        self.current_thread = saved;
        outcome
    }

    fn drive_inner(&mut self, thread: usize) -> Result<RunOutcome, Trap> {
        loop {
            if self.steps >= self.budget {
                self.errors.push(format!(
                    "{}: step budget exhausted ({} steps)",
                    self.threads[thread].name, self.steps
                ));
                self.threads[thread].frames.clear();
                self.threads[thread].stack.clear();
                return Ok(RunOutcome::Trapped);
            }
            if self.threads[thread].frames.is_empty() {
                return Ok(RunOutcome::Returned);
            }
            let (func, pc) = {
                let frame = self.threads[thread].frames.last().expect("frame present");
                (frame.func as usize, frame.pc)
            };
            let code = &self.code.functions[func].code;
            if pc >= code.len() {
                // Implicit return: the compiler always appends `Return`, so this
                // path only runs for hand-built functions.
                self.do_return(thread, Value::Null);
                continue;
            }
            let instr = code[pc];
            self.steps += 1;
            self.threads[thread].steps += 1;
            {
                let frame = self.threads[thread].frames.last_mut().expect("frame");
                frame.pc = pc + 1;
            }
            match instr {
                Instr::Null => self.push(thread, Value::Null),
                Instr::Int(v) => self.push(thread, Value::Int(v)),
                Instr::Real(v) => self.push(thread, Value::Real(v)),
                Instr::Bool(v) => self.push(thread, Value::Bool(v)),
                Instr::Str(slot) => {
                    let text = self.code.strings[slot as usize].clone();
                    self.push(thread, Value::Str(text));
                }
                Instr::Code(slot) => self.push(thread, Value::Code(slot)),
                Instr::PushLocal(slot) => {
                    let value = self.threads[thread]
                        .frames
                        .last()
                        .and_then(|frame| frame.locals.get(slot as usize))
                        .cloned()
                        .unwrap_or(Value::Null);
                    self.push(thread, value);
                }
                Instr::PushGlobal(slot) => {
                    let value = self
                        .globals
                        .get(slot as usize)
                        .cloned()
                        .unwrap_or(Value::Null);
                    self.push(thread, value);
                }
                Instr::LoadLocalArray(slot) => {
                    let index = self.pop(thread)?.int();
                    let base = self.threads[thread]
                        .frames
                        .last()
                        .and_then(|frame| frame.locals.get(slot as usize))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let value = match base {
                        Value::Handle(handle) => {
                            self.host.array_cell(handle, array_key(index)).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    self.push(thread, value);
                }
                Instr::LoadGlobalArray(slot) => {
                    let index = self.pop(thread)?.int();
                    let key = array_key(index);
                    let value = self
                        .global_arrays
                        .get(slot as usize)
                        .and_then(|cells| cells.get(&key))
                        .cloned()
                        .unwrap_or(Value::Null);
                    self.push(thread, value);
                }
                Instr::StoreLocal(slot) => {
                    let value = self.pop(thread)?;
                    if let Some(cell) = self
                        .threads[thread]
                        .frames
                        .last_mut()
                        .and_then(|frame| frame.locals.get_mut(slot as usize))
                    {
                        *cell = value;
                    }
                }
                Instr::StoreGlobal(slot) => {
                    let value = self.pop(thread)?;
                    if let Some(cell) = self.globals.get_mut(slot as usize) {
                        *cell = value;
                    }
                }
                Instr::StoreLocalArray(slot) => {
                    let index = self.pop(thread)?.int();
                    let value = self.pop(thread)?;
                    let base = self.threads[thread]
                        .frames
                        .last()
                        .and_then(|frame| frame.locals.get(slot as usize))
                        .cloned()
                        .unwrap_or(Value::Null);
                    if let Value::Handle(handle) = base {
                        self.host.array_set(handle, array_key(index), value);
                    }
                }
                Instr::StoreGlobalArray(slot) => {
                    let index = self.pop(thread)?.int();
                    let value = self.pop(thread)?;
                    let key = array_key(index);
                    if let Some(cells) = self.global_arrays.get_mut(slot as usize) {
                        cells.insert(key, value);
                    }
                }
                Instr::Pop => {
                    self.pop(thread)?;
                }
                Instr::Dup => {
                    let value = self.threads[thread]
                        .stack
                        .last()
                        .cloned()
                        .ok_or(Trap::StackUnderflow)?;
                    self.push(thread, value);
                }
                Instr::Call { func, argc } => {
                    let args = self.pop_args(thread, argc as usize)?;
                    if self.threads[thread].frames.len() >= MAX_CALL_DEPTH {
                        return Err(Trap::CallDepth);
                    }
                    let nlocals = self.code.functions[func as usize].nlocals;
                    let mut locals = vec![Value::Null; nlocals];
                    for (slot, value) in args.into_iter().enumerate() {
                        if slot < locals.len() {
                            locals[slot] = value;
                        }
                    }
                    let name = self.code.functions[func as usize].name.clone();
                    *self.function_calls.entry(name).or_insert(0) += 1;
                    let stack_base = self.threads[thread].stack.len();
                    self.threads[thread].frames.push(Frame {
                        func,
                        pc: 0,
                        locals,
                        stack_base,
                    });
                }
                Instr::CallIndirect { argc } => {
                    let args = self.pop_args(thread, argc as usize)?;
                    let target = self.pop(thread)?;
                    let Value::Code(func) = target else {
                        self.errors.push("call of a non-code value".into());
                        self.push(thread, Value::Null);
                        continue;
                    };
                    if func as usize >= self.code.functions.len() {
                        self.errors.push("call of an unknown code value".into());
                        self.push(thread, Value::Null);
                        continue;
                    }
                    let nlocals = self.code.functions[func as usize].nlocals;
                    let mut locals = vec![Value::Null; nlocals];
                    for (slot, value) in args.into_iter().enumerate() {
                        if slot < locals.len() {
                            locals[slot] = value;
                        }
                    }
                    let stack_base = self.threads[thread].stack.len();
                    self.threads[thread].frames.push(Frame {
                        func,
                        pc: 0,
                        locals,
                        stack_base,
                    });
                }
                Instr::Native { slot, argc } => {
                    let args = self.pop_args(thread, argc as usize)?;
                    let def = &registry::table()[slot as usize];
                    let name = def.name;
                    *self.native_calls.entry(name.to_string()).or_insert(0) += 1;
                    let value = match super::natives::dispatch(self, name, &args) {
                        Some(value) => value,
                        None => {
                            *self.unresolved.entry(name.to_string()).or_insert(0) += 1;
                            def.return_default()
                        }
                    };
                    if self.take_requested_sleep(thread) {
                        return Ok(RunOutcome::Suspended);
                    }
                    self.push(thread, value);
                }
                Instr::Unknown { pool, argc } => {
                    // A name no header in this tree declares (a Reforged-era
                    // native, or a typo). A handler may still exist by explicit
                    // registration; otherwise the call is a counted default.
                    let args = self.pop_args(thread, argc as usize)?;
                    let name = self.code.strings[pool as usize].clone();
                    match super::natives::dispatch(self, &name, &args) {
                        Some(value) => {
                            if self.take_requested_sleep(thread) {
                                return Ok(RunOutcome::Suspended);
                            }
                            self.push(thread, value);
                        }
                        None => {
                            *self.unresolved.entry(name.clone()).or_insert(0) += 1;
                            self.errors.push(format!("unknown function {name}"));
                            self.push(thread, Value::Null);
                        }
                    }
                }
                Instr::Jump(target) => {
                    self.threads[thread].frames.last_mut().expect("frame").pc = target as usize;
                }
                Instr::JumpIfFalse(target) => {
                    if !self.pop(thread)?.truthy() {
                        self.threads[thread].frames.last_mut().expect("frame").pc =
                            target as usize;
                    }
                }
                Instr::JumpIfTrue(target) => {
                    if self.pop(thread)?.truthy() {
                        self.threads[thread].frames.last_mut().expect("frame").pc =
                            target as usize;
                    }
                }
                Instr::Return => {
                    let value = self.pop(thread)?;
                    self.do_return(thread, value);
                }
                Instr::Wait => {
                    let seconds = self.pop(thread)?.real().max(0.0);
                    self.park_thread(thread, seconds);
                    return Ok(RunOutcome::Suspended);
                }
                Instr::Bin(tag) => {
                    let right = self.pop(thread)?;
                    let left = self.pop(thread)?;
                    let value = eval_bin(tag, left, right);
                    self.push(thread, value);
                }
                Instr::Un(tag) => {
                    let value = self.pop(thread)?;
                    let value = match tag {
                        UnTag::Neg => match value {
                            Value::Int(v) => Value::Int(v.wrapping_neg()),
                            Value::Real(v) => Value::Real(-v),
                            Value::Bool(v) => Value::Int(-(v as i32)),
                            other => Value::Int(other.int().wrapping_neg()),
                        },
                        UnTag::Not => Value::Bool(!value.truthy()),
                        UnTag::Pos => value,
                    };
                    self.push(thread, value);
                }
                Instr::ResetLocal(slot) => {
                    if let Some(cell) = self
                        .threads[thread]
                        .frames
                        .last_mut()
                        .and_then(|frame| frame.locals.get_mut(slot as usize))
                    {
                        *cell = Value::Null;
                    }
                }
            }
        }
    }

    fn do_return(&mut self, thread: usize, value: Value) {
        let frame = self.threads[thread].frames.pop().expect("frame on return");
        self.threads[thread].stack.truncate(frame.stack_base);
        if self.threads[thread].frames.is_empty() {
            self.threads[thread].last_value = Some(value);
            return;
        }
        self.threads[thread].stack.push(value);
    }

    fn push(&mut self, thread: usize, value: Value) {
        self.threads[thread].stack.push(value);
    }

    fn pop(&mut self, thread: usize) -> Result<Value, Trap> {
        self.threads[thread].stack.pop().ok_or(Trap::StackUnderflow)
    }

    fn pop_args(&mut self, thread: usize, argc: usize) -> Result<Vec<Value>, Trap> {
        let stack = &mut self.threads[thread].stack;
        if stack.len() < argc {
            return Err(Trap::StackUnderflow);
        }
        Ok(stack.split_off(stack.len() - argc))
    }

    /// Clamps a JASS array index into `0..8191` like the engine: out-of-range
    /// indices address slot 0 instead of trapping.
    pub fn array_key(index: i32) -> i32 {
        array_key(index)
    }

    /// Deterministic RNG step (xorshift32 seeded from the map seed).
    pub fn random_u32(&mut self) -> u32 {
        let mut x = self.rng as u32;
        if x == 0 {
            x = 0x9E37_79B9;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x as u64 | 1;
        x
    }

    /// `GetRandomInt` with the engine's inclusive range.
    pub fn random_int(&mut self, low: i32, high: i32) -> i32 {
        if high <= low {
            return low;
        }
        let span = (high as i64 - low as i64 + 1) as u64;
        low + (self.random_u32() as u64 % span) as i32
    }

    pub fn random_real(&mut self, low: f32, high: f32) -> f32 {
        if high <= low {
            return low;
        }
        let unit = (self.random_u32() as f64) / (u32::MAX as f64);
        low + (unit as f32) * (high - low)
    }

    /// Appends a line to the script output log.
    pub fn emit_text(&mut self, text: String) {
        self.text_log.push(text.clone());
        self.output.push(text);
    }

    /// Machine counters for tests and the HUD.
    pub fn summary(&self) -> VmSummary {
        VmSummary {
            steps: self.steps,
            threads: self.threads.len(),
            native_calls: self.native_calls.values().sum(),
            unresolved_calls: self.unresolved_calls(),
            unresolved_natives: self.unresolved.len(),
            errors: self.errors.len(),
            time: self.time,
            tick: self.tick,
        }
    }

    /// Renders a value the way the engine prints it (`R2S` for reals).
    pub fn render(value: &Value) -> String {
        match value {
            Value::Real(v) => format_real(*v),
            other => other.text(),
        }
    }
}

/// Machine counters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmSummary {
    pub steps: u64,
    pub threads: usize,
    pub native_calls: u64,
    pub unresolved_calls: u64,
    pub unresolved_natives: usize,
    pub errors: usize,
    pub time: f64,
    pub tick: u64,
}

fn array_key(index: i32) -> i32 {
    if (0..ARRAY_BOUND).contains(&index) {
        index
    } else {
        0
    }
}

/// Mixed-type binary arithmetic with the engine's rules: `+` concatenates when
/// either side is a string, integer division truncates toward zero and by zero
/// yields 0, comparisons promote to real.
fn eval_bin(tag: BinTag, left: Value, right: Value) -> Value {
    match tag {
        BinTag::Add => match (&left, &right) {
            (Value::Str(a), _) => Value::Str(format!("{a}{}", right.text())),
            (_, Value::Str(b)) => Value::Str(format!("{}{b}", left.text())),
            _ => {
                if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                    Value::Real(left.real() + right.real())
                } else {
                    Value::Int(left.int().wrapping_add(right.int()))
                }
            }
        },
        BinTag::Sub => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                Value::Real(left.real() - right.real())
            } else {
                Value::Int(left.int().wrapping_sub(right.int()))
            }
        }
        BinTag::Mul => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                Value::Real(left.real() * right.real())
            } else {
                Value::Int(left.int().wrapping_mul(right.int()))
            }
        }
        BinTag::Div => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                let divisor = right.real();
                if divisor == 0.0 {
                    Value::Real(0.0)
                } else {
                    Value::Real(left.real() / divisor)
                }
            } else {
                let divisor = right.int();
                if divisor == 0 {
                    Value::Int(0)
                } else {
                    Value::Int(left.int().wrapping_div(divisor))
                }
            }
        }
        BinTag::Eq => Value::Bool(left.eq_value(&right)),
        BinTag::Ne => Value::Bool(!left.eq_value(&right)),
        BinTag::Lt => Value::Bool(compare(&left, &right) == std::cmp::Ordering::Less),
        BinTag::Le => Value::Bool(compare(&left, &right) != std::cmp::Ordering::Greater),
        BinTag::Gt => Value::Bool(compare(&left, &right) == std::cmp::Ordering::Greater),
        BinTag::Ge => Value::Bool(compare(&left, &right) != std::cmp::Ordering::Less),
        BinTag::And => Value::Bool(left.truthy() && right.truthy()),
        BinTag::Or => Value::Bool(left.truthy() || right.truthy()),
    }
}

fn compare(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => left
            .real()
            .partial_cmp(&right.real())
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

/// Per-function compilation context.
#[derive(Default)]
struct FnCtx {
    name: String,
    /// Lowercased local/parameter name → slot.
    slots: BTreeMap<String, u16>,
    nlocals: u16,
    /// Array-typed locals promoted to synthetic globals (`JASSHelper` style).
    array_locals: BTreeMap<String, u32>,
    loop_stack: Vec<Vec<usize>>,
}

/// Compiles a `Program` to [`Code`].
pub struct Compiler<'a> {
    program: &'a Program,
    code: Code,
    errors: Vec<CompileError>,
    /// Names the script reads/writes that the loaded headers do not declare.
    /// They compile to a typed default (`null`) and are reported, because a map
    /// built against a newer header must still run; a silent zero would be the
    /// bug this list prevents.
    undeclared: Vec<String>,
    func_index: BTreeMap<String, u32>,
    global_index: BTreeMap<String, u32>,
    native_slot: BTreeMap<String, u32>,
}

impl<'a> Compiler<'a> {
    pub fn new(program: &'a Program) -> Self {
        let mut code = Code::default();
        let mut func_index = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        for (index, function) in program.functions.iter().enumerate() {
            // Later definitions win, matching the engine's last-registration
            // semantics for duplicate `function` names.
            func_index.insert(function.name.to_ascii_lowercase(), index as u32);
            by_name.insert(function.name.to_ascii_lowercase(), index as u32);
            code.functions.push(CodeFunc {
                name: function.name.clone(),
                params: function.params.len(),
                ..CodeFunc::default()
            });
        }
        let mut global_index = BTreeMap::new();
        for (index, global) in program.globals.iter().enumerate() {
            global_index.insert(global.name.to_ascii_lowercase(), index as u32);
            code.globals.push(GlobalSlot {
                name: global.name.clone(),
                kind: if global.array {
                    GlobalKind::Array
                } else {
                    GlobalKind::Scalar
                },
            });
        }
        let mut native_slot = BTreeMap::new();
        for (index, def) in registry::table().iter().enumerate() {
            native_slot.insert(def.name.to_ascii_lowercase(), index as u32);
        }
        code.by_name = by_name;
        code.global_by_name = global_index.clone();
        Self {
            program,
            code,
            errors: Vec::new(),
            undeclared: Vec::new(),
            func_index,
            global_index,
            native_slot,
        }
    }

    pub fn compile(mut self) -> Result<Code, Vec<CompileError>> {
        let init_index = self.code.functions.len() as u32;
        self.code.functions.push(CodeFunc {
            name: "__init_globals".into(),
            ..CodeFunc::default()
        });
        self.code.by_name.insert("__init_globals".into(), init_index);
        self.code.init_function = init_index;
        self.compile_globals(init_index);
        for index in 0..self.program.functions.len() {
            let function = self.program.functions[index].clone();
            let (nlocals, body) = self.compile_function(&function);
            self.code.functions[index].nlocals = nlocals;
            self.code.functions[index].code = body;
        }
        self.code.undeclared = std::mem::take(&mut self.undeclared);
        if self.errors.is_empty() {
            Ok(self.code)
        } else {
            Err(self.errors)
        }
    }

    /// Global initialisers run in declaration order inside `__init_globals`.
    fn compile_globals(&mut self, init_index: u32) {
        let globals = self.program.globals.clone();
        let mut body: Vec<Instr> = Vec::new();
        for (index, global) in globals.iter().enumerate() {
            let Some(expr) = &global.init else {
                continue;
            };
            let mut ctx = FnCtx::default();
            ctx.name = "__init_globals".into();
            if self.expr(expr, &mut ctx, &mut body).is_err() {
                continue;
            }
            body.push(Instr::StoreGlobal(index as u32));
        }
        body.push(Instr::Null);
        body.push(Instr::Return);
        self.code.functions[init_index as usize].code = body;
    }

    fn compile_function(&mut self, function: &FunctionDecl) -> (usize, Vec<Instr>) {
        let mut ctx = FnCtx {
            name: function.name.clone(),
            ..FnCtx::default()
        };
        for (ty, name) in &function.params {
            if ty.eq_ignore_ascii_case("nothing") {
                continue;
            }
            let slot = ctx.nlocals;
            ctx.nlocals += 1;
            ctx.slots.insert(name.to_ascii_lowercase(), slot);
        }
        for local in &function.locals {
            if local.array {
                let name = format!(
                    "{}${}",
                    function.name.to_ascii_lowercase(),
                    local.name.to_ascii_lowercase()
                );
                let index = self.code.globals.len() as u32;
                self.code.globals.push(GlobalSlot {
                    name: name.clone(),
                    kind: GlobalKind::Array,
                });
                self.global_index.insert(name, index);
                ctx.array_locals
                    .insert(local.name.to_ascii_lowercase(), index);
            } else {
                let slot = ctx.nlocals;
                ctx.nlocals += 1;
                ctx.slots.insert(local.name.to_ascii_lowercase(), slot);
            }
        }
        // Locals declared as statements get a slot on first sight.
        let mut body = Vec::new();
        for stmt in &function.body {
            if let Err(message) = self.stmt(stmt, &mut ctx, &mut body) {
                self.error(&function.name, message);
            }
        }
        body.push(Instr::Null);
        body.push(Instr::Return);
        (ctx.nlocals as usize, body)
    }

    fn error(&mut self, function: &str, message: impl Into<String>) {
        self.errors.push(CompileError {
            function: function.to_string(),
            message: message.into(),
        });
    }

    fn intern(&mut self, text: &str) -> u32 {
        if let Some(index) = self.code.strings.iter().position(|entry| entry == text) {
            return index as u32;
        }
        self.code.strings.push(text.to_string());
        (self.code.strings.len() - 1) as u32
    }

    fn local_slot(&self, ctx: &FnCtx, name: &str) -> Option<u16> {
        ctx.slots.get(&name.to_ascii_lowercase()).copied()
    }

    /// Global slot for `name`; an undeclared name gets a synthetic slot so the
    /// script compiles, and is recorded in `undeclared`.
    fn global_slot(&mut self, name: &str) -> Option<u32> {
        let key = name.to_ascii_lowercase();
        if let Some(index) = self.global_index.get(&key).copied() {
            return Some(index);
        }
        let index = self.code.globals.len() as u32;
        self.code.globals.push(GlobalSlot {
            name: name.to_string(),
            kind: GlobalKind::Scalar,
        });
        self.global_index.insert(key, index);
        if !self.undeclared.iter().any(|entry| entry.eq_ignore_ascii_case(name)) {
            self.undeclared.push(name.to_string());
        }
        Some(index)
    }

    fn stmt(&mut self, stmt: &Stmt, ctx: &mut FnCtx, out: &mut Vec<Instr>) -> Result<(), String> {
        match stmt {
            Stmt::Local(local) => {
                if ctx.array_locals.contains_key(&local.name.to_ascii_lowercase()) {
                    return Ok(());
                }
                let Some(slot) = self.local_slot(ctx, &local.name) else {
                    // Local introduced inside a block: allocate a slot now.
                    let slot = ctx.nlocals;
                    ctx.nlocals += 1;
                    ctx.slots.insert(local.name.to_ascii_lowercase(), slot);
                    match &local.init {
                        Some(expr) => {
                            self.expr(expr, ctx, out)?;
                            out.push(Instr::StoreLocal(slot));
                        }
                        None => out.push(Instr::ResetLocal(slot)),
                    }
                    return Ok(());
                };
                match &local.init {
                    Some(expr) => {
                        self.expr(expr, ctx, out)?;
                        out.push(Instr::StoreLocal(slot));
                    }
                    None => out.push(Instr::ResetLocal(slot)),
                }
                Ok(())
            }
            Stmt::Set { name, index, value, .. } => {
                if let Some(global) = ctx.array_locals.get(&name.to_ascii_lowercase()).copied() {
                    self.expr(value, ctx, out)?;
                    match index {
                        Some(index) => {
                            self.expr(index, ctx, out)?;
                            out.push(Instr::StoreGlobalArray(global));
                        }
                        None => out.push(Instr::StoreGlobal(global)),
                    }
                    return Ok(());
                }
                if let Some(slot) = self.local_slot(ctx, name) {
                    self.expr(value, ctx, out)?;
                    match index {
                        Some(index) => {
                            self.expr(index, ctx, out)?;
                            out.push(Instr::StoreLocalArray(slot));
                        }
                        None => out.push(Instr::StoreLocal(slot)),
                    }
                    return Ok(());
                }
                let global = self
                    .global_slot(name)
                    .ok_or_else(|| format!("missing global slot for {name}"))?;
                self.expr(value, ctx, out)?;
                match index {
                    Some(index) => {
                        self.expr(index, ctx, out)?;
                        out.push(Instr::StoreGlobalArray(global));
                    }
                    None => out.push(Instr::StoreGlobal(global)),
                }
                Ok(())
            }
            Stmt::Call { name, args, .. } => {
                self.call(name, args, ctx, out)?;
                out.push(Instr::Pop);
                Ok(())
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                let mut end_jumps = Vec::new();
                for (cond, body) in branches {
                    self.expr(cond, ctx, out)?;
                    let skip = out.len();
                    out.push(Instr::JumpIfFalse(0));
                    for stmt in body {
                        self.stmt(stmt, ctx, out)?;
                    }
                    end_jumps.push(out.len());
                    out.push(Instr::Jump(0));
                    let next = out.len() as u32;
                    out[skip] = Instr::JumpIfFalse(next);
                }
                if let Some(else_body) = else_body {
                    for stmt in else_body {
                        self.stmt(stmt, ctx, out)?;
                    }
                }
                let end = out.len() as u32;
                for jump in end_jumps {
                    out[jump] = Instr::Jump(end);
                }
                Ok(())
            }
            Stmt::Loop { body, .. } => {
                let start = out.len();
                ctx.loop_stack.push(Vec::new());
                for stmt in body {
                    self.stmt(stmt, ctx, out)?;
                }
                out.push(Instr::Jump(start as u32));
                let end = out.len() as u32;
                let jumps = ctx.loop_stack.pop().unwrap_or_default();
                for jump in jumps {
                    // Keep the conditional: `exitwhen` must only leave the loop
                    // when its condition is true.
                    out[jump] = Instr::JumpIfTrue(end);
                }
                Ok(())
            }
            Stmt::ExitWhen { cond, .. } => {
                self.expr(cond, ctx, out)?;
                let Some(jumps) = ctx.loop_stack.last_mut() else {
                    return Err("exitwhen outside a loop".into());
                };
                let slot = out.len();
                out.push(Instr::JumpIfTrue(0));
                jumps.push(slot);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                match value {
                    Some(expr) => self.expr(expr, ctx, out)?,
                    None => out.push(Instr::Null),
                }
                out.push(Instr::Return);
                Ok(())
            }
        }
    }

    fn call(
        &mut self,
        name: &str,
        args: &[Expr],
        ctx: &mut FnCtx,
        out: &mut Vec<Instr>,
    ) -> Result<(), String> {
        for arg in args {
            self.expr(arg, ctx, out)?;
        }
        let argc = args.len().min(u8::MAX as usize) as u8;
        let lowered = name.to_ascii_lowercase();
        if let Some(func) = self.func_index.get(&lowered).copied()
            && func as usize != self.code.init_function as usize
        {
            out.push(Instr::Call { func, argc });
            return Ok(());
        }
        if let Some(slot) = self.native_slot.get(&lowered).copied() {
            if lowered == "triggersleepaction" {
                out.push(Instr::Wait);
                return Ok(());
            }
            out.push(Instr::Native { slot, argc });
            return Ok(());
        }
        let pool = self.intern(name);
        out.push(Instr::Unknown { pool, argc });
        Ok(())
    }

    fn expr(&mut self, expr: &Expr, ctx: &mut FnCtx, out: &mut Vec<Instr>) -> Result<(), String> {
        match expr {
            Expr::Int(v) => {
                out.push(Instr::Int(*v as i32));
                Ok(())
            }
            Expr::Real(v) => {
                out.push(Instr::Real(*v as f32));
                Ok(())
            }
            Expr::Bool(v) => {
                out.push(Instr::Bool(*v));
                Ok(())
            }
            Expr::Str(text) => {
                let pool = self.intern(text);
                out.push(Instr::Str(pool));
                Ok(())
            }
            Expr::Null => {
                out.push(Instr::Null);
                Ok(())
            }
            Expr::Var(name) => {
                if let Some(global) = ctx.array_locals.get(&name.to_ascii_lowercase()).copied() {
                    out.push(Instr::PushGlobal(global));
                    return Ok(());
                }
                if let Some(slot) = self.local_slot(ctx, name) {
                    out.push(Instr::PushLocal(slot));
                    return Ok(());
                }
                let global = self
                    .global_slot(name)
                    .ok_or_else(|| format!("missing global slot for {name}"))?;
                out.push(Instr::PushGlobal(global));
                Ok(())
            }
            Expr::Array(name, index) => {
                if let Some(global) = ctx.array_locals.get(&name.to_ascii_lowercase()).copied() {
                    self.expr(index, ctx, out)?;
                    out.push(Instr::LoadGlobalArray(global));
                    return Ok(());
                }
                if let Some(slot) = self.local_slot(ctx, name) {
                    self.expr(index, ctx, out)?;
                    out.push(Instr::LoadLocalArray(slot));
                    return Ok(());
                }
                let global = self
                    .global_slot(name)
                    .ok_or_else(|| format!("missing global slot for {name}"))?;
                self.expr(index, ctx, out)?;
                out.push(Instr::LoadGlobalArray(global));
                Ok(())
            }
            Expr::Call(name, args) => self.call(name, args, ctx, out),
            Expr::Code(name) => {
                let func = self
                    .func_index
                    .get(&name.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(u32::MAX);
                out.push(Instr::Code(func));
                Ok(())
            }
            Expr::Unary(op, inner) => {
                // Constant folding keeps `-1` from becoming a negate in every
                // array index, the hottest expression form in map scripts.
                if let (UnOp::Neg, Expr::Int(v)) = (op, inner.as_ref()) {
                    out.push(Instr::Int((*v as i32).wrapping_neg()));
                    return Ok(());
                }
                if let (UnOp::Neg, Expr::Real(v)) = (op, inner.as_ref()) {
                    out.push(Instr::Real(-(*v as f32)));
                    return Ok(());
                }
                self.expr(inner, ctx, out)?;
                out.push(Instr::Un((*op).into()));
                Ok(())
            }
            Expr::Binary(op, left, right) => {
                self.expr(left, ctx, out)?;
                self.expr(right, ctx, out)?;
                out.push(Instr::Bin((*op).into()));
                Ok(())
            }
        }
    }
}

/// Script arrays that are not globals are keyed on a handle's cell table (used
/// by `local <type> array` promotion and `group`-like containers).
impl Host {
    pub fn array_cell(&self, handle: super::value::Handle, key: i32) -> Option<Value> {
        match self.object(handle)? {
            Object::Hashtable(cells) => cells.get(&(0, key as i64)).cloned(),
            _ => None,
        }
    }

    pub fn array_set(&mut self, handle: super::value::Handle, key: i32, value: Value) {
        if let Some(Object::Hashtable(cells)) = self.object_mut(handle) {
            cells.insert((0, key as i64), value);
        }
    }
}

/// Unit handle of a value, when it is one.
pub fn unit_handle(value: &Value) -> Option<super::value::Handle> {
    match value {
        Value::Handle(handle) if handle.kind == HandleKind::Unit => Some(*handle),
        _ => None,
    }
}

/// Converts a script real to an integer with the engine's truncation.
pub fn to_int(value: f32) -> i32 {
    real_to_int(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_keys_clamp_like_the_engine() {
        assert_eq!(array_key(0), 0);
        assert_eq!(array_key(8191), 8191);
        assert_eq!(array_key(8192), 0);
        assert_eq!(array_key(-4), 0);
    }

    #[test]
    fn arithmetic_matches_jass_rules() {
        assert_eq!(
            eval_bin(BinTag::Div, Value::Int(7), Value::Int(2)),
            Value::Int(3)
        );
        assert_eq!(
            eval_bin(BinTag::Div, Value::Int(7), Value::Int(0)),
            Value::Int(0)
        );
        assert_eq!(
            eval_bin(BinTag::Add, Value::Str("a".into()), Value::Int(2)),
            Value::Str("a2".into())
        );
        assert_eq!(
            eval_bin(BinTag::Add, Value::Int(2), Value::Str("a".into())),
            Value::Str("2a".into())
        );
        assert_eq!(
            eval_bin(BinTag::Add, Value::Real(1.5), Value::Int(2)),
            Value::Real(3.5)
        );
        assert_eq!(real_to_int(-2.9), -2);
    }
}
