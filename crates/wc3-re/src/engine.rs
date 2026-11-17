//! Engine model derived from `Game.dll!GameMain` and the RoC boot path.
//!
//! Evidence (RoC Game.dll, image base 0x6f000000, `GameMain` at 0x6f004180):
//!
//! `GameMain` (artifact `analysis/r2/artifacts/Game.dll.GameMain.pdc.txt`):
//! 1. calls host ordinals `War3.exe A372`, `A327` (host/game object setup);
//! 2. calls host `A475` with ecx = `0x67616d65` (`"emag"` = reversed `game` tag);
//! 3. formats a path into a 260-byte buffer with host `A377`;
//! 4. calls Storm ordinals `503` and `585` (log/base-path setup; the string
//!    `"Logs"` is pushed at 0x6f0041b7);
//! 5. calls `fcn.6f004200` — the real initializer. On failure it calls
//!    `War3.exe A180`, `fcn.6f004b80` and `War3.exe A340` (shutdown path).
//!
//! `fcn.6f004200` (432 bytes, artifact below in `analysis/agent-re/r2/`):
//! 1. checks the single-instance mutex `"Warcraft III Game Application"`;
//! 2. loads `ui\startupstrings.txt` through Storm (0x6f53ac1a);
//! 3. registers callbacks 0x6f004d80 and 0x6f004f80;
//! 4. initializes application/subsystems: `0x6f004b20`, `0x6f001380`,
//!    `0x6f0018a0(50)`, `0x6f004b70`, `0x6f004c10` (map load gate),
//!    `0x6f004930`;
//! 5. on success registers `war3mapMap.blp` / `war3mapMap.tga`, installs
//!    callbacks, calls `0x6f071270`, `0x6f006410`, `0x6f0048b0`, returns 1;
//! 6. on failure calls shutdown `0x6f004b80` and returns 0.
//!
//! The Rust model below mirrors that boot order and keeps a step log with the
//! original RVA for each phase. The per-frame phases (input → simulation →
//! render → audio) are marked *inferred* because the host loop lives in
//! War3.exe (Game.dll registers frame callbacks rather than owning the loop).

use std::path::{Path, PathBuf};

use crate::error::{ReError, Result};
use crate::jass::{Program, Runtime, parse};
use crate::mapfiles::MapBundle;
use crate::ai::AiScript;
use crate::objectdata::ObjectDb;
use crate::storm::StormFs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Idle,
    Booting,
    MainMenu,
    LoadingMap,
    InGame,
    ShuttingDown,
    Failed,
}

#[derive(Debug, Clone)]
pub struct BootStep {
    pub phase: &'static str,
    pub evidence: &'static str,
    pub ok: bool,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub install_root: PathBuf,
    pub data_root: Option<PathBuf>,
    pub map: Option<PathBuf>,
    pub locale: u32,
    pub windowed: bool,
    pub seed: u64,
}

impl EngineConfig {
    pub fn new(install_root: impl Into<PathBuf>) -> Self {
        Self {
            install_root: install_root.into(),
            data_root: None,
            map: None,
            locale: 0,
            windowed: true,
            seed: 0x5EED_1234,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MapSpawnReport {
    /// Placed units present in `war3mapUnits.doo` (0 when the file uses the
    /// known v8 random-table/truncated gap).
    pub map_units: usize,
    /// Placed doodads/destructables present in `war3map.doo`.
    pub map_doodads: usize,
    /// Units added to the JASS world because the map script did not already
    /// create an equivalent unit at the same position.
    pub spawned_units: usize,
    /// Doodads/destructables added to the JASS world.
    pub spawned_doodads: usize,
    /// Map units matched to script-created units (type + position within 4).
    pub matched_units: usize,
    /// Map doodads matched to script-created destructables.
    pub matched_doodads: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptReport {
    /// `war3map.j` `main` completed without a VM error.
    pub main_ok: bool,
    /// Declared functions in the merged common.j + Blizzard.j + map program.
    pub functions: usize,
    /// Triggers created by `InitCustomTriggers`/map script.
    pub triggers: usize,
    /// Total actions attached to those triggers.
    pub trigger_actions: usize,
    /// Triggers that have executed at least once (`TriggerExecute`/init).
    pub executed_triggers: usize,
    /// Sum of trigger execution counts.
    pub executed_actions: u32,
}

pub struct Engine {
    pub state: EngineState,
    pub config: EngineConfig,
    pub storm: StormFs,
    pub objects: Option<ObjectDb>,
    pub map: Option<MapBundle>,
    pub runtime: Option<Runtime>,
    pub time: f64,
    pub ticks: u64,
    pub boot_log: Vec<BootStep>,
    pub startup_strings: usize,
    pub script_errors: Vec<String>,
    /// True when the map `main` completed (including `InitCustomTriggers`
    /// and `RunInitializationTriggers`).
    pub script_main_ok: bool,
    /// AI scripts loaded from `Scripts/*.ai` (canonical `wc3-re::ai` parser).
    pub ai_scripts: Vec<AiScript>,
    /// Map placement → JASS world spawn audit from the last `enter_map`.
    pub map_spawn: MapSpawnReport,
    fired_timer_events: bool,
}

impl Engine {
    /// Unimplemented natives the executed script called, most-called first.
    ///
    /// `Runtime::unimplemented` is the diagnostic for a native this build does
    /// not execute. The VM still returns the native's declared default so a
    /// retail script keeps running, so this list — not the return value — is
    /// the observable record that the call did nothing (AUD-031).
    pub fn unimplemented_natives(&self) -> Vec<(&str, u64)> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<(&str, u64)> = runtime
            .unimplemented
            .iter()
            .map(|(name, count)| (name.as_str(), *count))
            .collect();
        rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        rows.truncate(24);
        rows
    }

    /// Boot in the order observed in `Game.dll!GameMain`/`fcn.6f004200`.
    pub fn boot(mut config: EngineConfig) -> Result<Self> {        let mut engine = Self {
            state: EngineState::Booting,
            storm: StormFs::new(),
            objects: None,
            map: None,
            runtime: None,
            time: 0.0,
            ticks: 0,
            boot_log: Vec::new(),
            startup_strings: 0,
            script_errors: Vec::new(),
            script_main_ok: false,
            ai_scripts: Vec::new(),
            map_spawn: MapSpawnReport::default(),
            fired_timer_events: false,
            config: EngineConfig {
                install_root: config.install_root.clone(),
                data_root: config.data_root.take(),
                map: config.map.take(),
                locale: config.locale,
                windowed: config.windowed,
                seed: config.seed,
            },
        };
        // 0x6f004200: single-instance mutex "Warcraft III Game Application".
        engine.step("host.init", "GameMain A372/A327 + fcn.6f004200 mutex", true);
        // Storm base path + archives. The loose extracted tree is the base
        // data set on this device (War3.mpq is not present in the repo).
        let data_root = engine
            .config
            .data_root
            .clone()
            .unwrap_or_else(|| engine.config.install_root.clone());
        let mounted = match StormFs::open_wc3_install(&engine.config.install_root) {
            Ok(fs) => {
                engine.storm = fs;
                engine.storm.layers().len()
            }
            Err(_) => 0,
        };
        if mounted == 0 && data_root.is_dir() {
            let _ = engine.storm.add_loose_dir("extracted", &data_root);
        }
        engine.step(
            "storm.mount",
            "GameMain Storm ordinals 503/585; StormOpenArchive=266",
            mounted > 0 || !engine.storm.listfile().is_empty(),
        );
        // fcn.6f004200: ui\startupstrings.txt through Storm.
        match engine.storm.read("ui\\startupstrings.txt") {
            Ok(bytes) => {
                engine.startup_strings = String::from_utf8_lossy(&bytes)
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .count();
                engine.step(
                    "ui.startupstrings",
                    "fcn.6f004200 loads ui\\startupstrings.txt via Storm",
                    true,
                );
            }
            Err(_) => engine.step(
                "ui.startupstrings",
                "fcn.6f004200 loads ui\\startupstrings.txt via Storm",
                false,
            ),
        }
        // fcn.6f004200: object data is loaded before the map (0x6f004c10 gate).
        if data_root.is_dir() {
            match ObjectDb::load_dir(&data_root) {
                Ok(db) => {
                    engine.objects = Some(db);
                    engine.step(
                        "data.objects",
                        "Game.dll 0x6f10de70 Units\\UnitData.slk; 0x6f115070 AbilityData",
                        true,
                    );
                }
                Err(err) => {
                    engine.script_errors.push(format!("object data: {err}"));
                    engine.step("data.objects", "Game.dll object-data load", false);
                }
            }
        }
        if let Some(map_path) = engine.config.map.clone() {
            engine.enter_map(&map_path)?;
        } else {
            engine.state = EngineState::MainMenu;
        }
        Ok(engine)
    }

    fn step(&mut self, phase: &'static str, evidence: &'static str, ok: bool) {
        self.boot_log.push(BootStep {
            phase,
            evidence,
            ok,
        });
    }

    /// Load a map and run its script with common.j + Blizzard.j merged.
    pub fn enter_map(&mut self, map_path: &Path) -> Result<()> {
        self.state = EngineState::LoadingMap;
        self.step(
            "map.load",
            "fcn.6f004200 0x6f004c10 map-load gate",
            true,
        );
        let map = MapBundle::open(map_path)?;
        let script = map
            .script
            .clone()
            .ok_or_else(|| ReError::NotFound("war3map.j".into()))?;
        let root = self
            .config
            .data_root
            .clone()
            .unwrap_or_else(|| self.config.install_root.clone());
        let mut program = Program::default();
        for (name, path) in [
            ("common.j", root.join("Scripts/common.j")),
            ("Blizzard.j", root.join("Scripts/Blizzard.j")),
        ] {
            if let Ok(source) = std::fs::read_to_string(&path) {
                let parsed = parse(&source)
                    .map_err(|err| ReError::Parse(format!("{}: {err}", path.display())))?;
                let _ = name;
                program.merge(parsed);
            }
        }
        let map_program =
            parse(&script).map_err(|err| ReError::Parse(format!("war3map.j: {err}")))?;
        program.merge(map_program);
        let mut runtime = Runtime::new(program, self.config.seed);
        runtime.max_steps = 50_000_000;
        match runtime.run_main() {
            Ok(_) => {
                self.script_main_ok = true;
                self.step(
                    "script.main",
                    "war3map.j main → InitBlizzard/InitCustomTriggers",
                    true,
                );
            }
            Err(err) => {
                self.script_main_ok = false;
                self.script_errors.push(err.to_string());
                self.step("script.main", "war3map.j main", false);
            }
        }
        self.script_errors
            .extend(runtime.errors.iter().take(50).cloned());
        self.map_spawn = spawn_map_entities(&mut runtime, &map);
        self.runtime = Some(runtime);
        self.map = Some(map);
        self.state = EngineState::InGame;
        Ok(())
    }

    /// Canonical JASS runtime after `enter_map` (game runtime wiring).
    pub fn runtime(&self) -> Option<&Runtime> {
        self.runtime.as_ref()
    }

    /// Mutable runtime for trigger/order bridges (`wc3-game::sim_bridge`).
    pub fn runtime_mut(&mut self) -> Option<&mut Runtime> {
        self.runtime.as_mut()
    }

    /// Complete one ability cast through the JASS runtime: validates that the
    /// caster owns the ability, publishes the spell event context
    /// (`GetSpellAbilityId`, `GetSpellTarget*`) and runs matching
    /// cast/effect/finish triggers.
    ///
    /// This is the game/sim bridge entry point for PARITY #11: `wc3-re` owns
    /// the script-visible ability state, `wc3-sim` owns gameplay resolution.
    pub fn cast_ability(
        &mut self,
        caster: u32,
        ability: &str,
        target: crate::jass::vm::CastTarget,
    ) -> Result<crate::jass::vm::AbilityCastReport> {
        self.runtime_mut()
            .ok_or_else(|| ReError::Unsupported("cast_ability: no JASS runtime".into()))?
            .cast_ability(caster, ability, target)
    }

    /// Use one inventory item through the JASS runtime: fires the use-item
    /// events with `GetManipulatedItem`, then consumes one charge and removes
    /// the item when the last charge is spent.
    pub fn use_item(
        &mut self,
        unit: u32,
        item: u32,
        target: crate::jass::vm::CastTarget,
    ) -> Result<bool> {
        self.runtime_mut()
            .ok_or_else(|| ReError::Unsupported("use_item: no JASS runtime".into()))?
            .use_item(unit, item, target)
    }

    /// Grant (or retarget) an ability on a JASS unit. Sim/hero data uses this
    /// to mirror the canonical `wc3-sim` ability state into the script world
    /// without duplicating gameplay logic.
    pub fn grant_ability(&mut self, unit: u32, ability: &str, level: i32) -> Result<()> {
        self.runtime_mut()
            .ok_or_else(|| ReError::Unsupported("grant_ability: no JASS runtime".into()))?
            .grant_ability(unit, ability, level)
    }

    /// Abilities currently on a JASS unit as `(rawcode, level)`.
    pub fn unit_abilities(&self, unit: u32) -> Vec<(String, i32)> {
        self.runtime()
            .map(|runtime| runtime.unit_abilities(unit))
            .unwrap_or_default()
    }

    /// Items currently held by a JASS unit as `(handle, rawcode, charges)`.
    pub fn unit_items(&self, unit: u32) -> Vec<(u32, String, i32)> {
        self.runtime()
            .map(|runtime| runtime.unit_items(unit))
            .unwrap_or_default()
    }

    /// Parse `Scripts/*.ai` through the canonical AI front end and retain the
    /// programs for the game runtime. `common.ai` is included; callers can find
    /// a race script by file name.
    pub fn load_ai_scripts(&mut self, scripts_dir: &Path) -> Result<usize> {
        let mut scripts = Vec::new();
        for entry in std::fs::read_dir(scripts_dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "ai") {
                continue;
            }
            scripts.push(AiScript::parse_file(&path)?);
        }
        scripts.sort_by(|left, right| left.path.cmp(&right.path));
        self.ai_scripts = scripts;
        Ok(self.ai_scripts.len())
    }

    /// AI scripts loaded by [`Engine::load_ai_scripts`].
    pub fn ai_scripts(&self) -> &[AiScript] {
        &self.ai_scripts
    }

    /// Map placement → JASS world spawn audit from the last `enter_map`.
    pub fn map_spawn(&self) -> &MapSpawnReport {
        &self.map_spawn
    }

    /// Map bundle loaded by the last `enter_map` (terrain, placements, objects).
    pub fn map(&self) -> Option<&MapBundle> {
        self.map.as_ref()
    }

    /// Map script initialization report: `main`, `InitCustomTriggers` and the
    /// triggers/actions that actually executed during boot.
    pub fn script_report(&self) -> ScriptReport {
        let mut report = ScriptReport {
            main_ok: self.script_main_ok,
            ..ScriptReport::default()
        };
        if let Some(runtime) = &self.runtime {
            report.functions = runtime.program.functions.len();
            for trigger in runtime.world.triggers.values() {
                report.triggers += 1;
                report.trigger_actions += trigger.actions.len();
                if trigger.exec_count > 0 {
                    report.executed_triggers += 1;
                }
                report.executed_actions += trigger.exec_count;
            }
        }
        report
    }

    /// Advance one frame. Phase order is inferred from the host/guest split.
    pub fn tick(&mut self, dt: f64) {
        if self.state != EngineState::InGame {
            return;
        }
        self.ticks += 1;
        self.time += dt;
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        runtime.world.game_time = self.time;
        // Timers: decrement and fire matching trigger actions once.
        let timer_handles: Vec<u32> = runtime.world.timers.keys().copied().collect();
        let mut expired = Vec::new();
        for handle in timer_handles {
            let Some(timer) = runtime.world.timers.get_mut(&handle) else {
                continue;
            };
            if !timer.running {
                continue;
            }
            timer.elapsed += dt;
            timer.remaining -= dt;
            if timer.remaining <= 0.0 {
                expired.push(handle);
                if timer.periodic {
                    timer.remaining += timer.timeout.max(f64::EPSILON);
                } else {
                    timer.running = false;
                }
            }
        }
        for timer in expired {
            let code = runtime
                .world
                .timers
                .get(&timer)
                .and_then(|timer| timer.code.clone());
            if let Some(code) = code {
                let _ = runtime.run_function(&code);
            }
            let triggers: Vec<u32> = runtime
                .world
                .triggers
                .iter()
                .filter(|(_, trigger)| {
                    trigger
                        .events
                        .iter()
                        .any(|event| matches!(event, crate::jass::vm::TriggerEvent::TimerExpire { timer: t } if *t == timer))
                })
                .map(|(handle, _)| *handle)
                .collect();
            for trigger in triggers {
                let _ = runtime.execute_trigger(trigger);
            }
        }
        // Zero-time map-initialization triggers (TriggerRegisterTimerEvent 0.0)
        // are fired on the first frame if the script did not already execute
        // them through RunInitializationTriggers.
        if !self.fired_timer_events {
            self.fired_timer_events = true;
            let due: Vec<u32> = runtime
                .world
                .triggers
                .iter()
                .filter(|(_, trigger)| {
                    trigger.exec_count == 0
                        && trigger.events.iter().any(
                            |event| matches!(event, crate::jass::vm::TriggerEvent::Timer { timeout, .. } if *timeout <= 0.0),
                        )
                })
                .map(|(handle, _)| *handle)
                .collect();
            for trigger in due {
                let _ = runtime.execute_trigger(trigger);
            }
        }
    }

    /// Shutdown path from GameMain failure branch / fcn.6f004b80.
    pub fn shutdown(&mut self) {
        self.state = EngineState::ShuttingDown;
        self.step(
            "shutdown",
            "fcn.6f004b80: Storm496/403 teardown, War3.exe A179/A105/A487",
            true,
        );
        self.state = EngineState::Idle;
    }
}

/// Spawn map placement records into the JASS world, de-duplicating units and
/// destructables already created by `CreateAllUnits`/`CreateAllDestructables`.
fn spawn_map_entities(runtime: &mut Runtime, map: &MapBundle) -> MapSpawnReport {
    let mut report = MapSpawnReport::default();
    if let Some(units) = &map.units {
        report.map_units = units.len();
        for placed in units {
            let id = crate::fourcc_str(&placed.type_id);
            let matched = runtime.world.units.values().any(|unit| {
                unit.id.eq_ignore_ascii_case(&id)
                    && (unit.x - placed.position.x as f64).abs() < 4.0
                    && (unit.y - placed.position.y as f64).abs() < 4.0
            });
            if matched {
                report.matched_units += 1;
                continue;
            }
            let handle = runtime.alloc();
            let hp = f64::from(placed.hp.max(1));
            runtime.world.units.insert(
                handle,
                crate::jass::vm::Unit {
                    id,
                    owner: placed.player as i32,
                    x: placed.position.x as f64,
                    y: placed.position.y as f64,
                    z: placed.position.z as f64,
                    facing: placed.rotation as f64,
                    hp,
                    max_hp: hp,
                    mana: f64::from(placed.mp.max(0)),
                    max_mana: f64::from(placed.mp.max(0)),
                    level: placed.hero_level.max(1),
                    alive: true,
                    scale: placed.scale.x as f64,
                    acquire_range: 600.0,
                    ..crate::jass::vm::Unit::default()
                },
            );
            report.spawned_units += 1;
        }
    }
    if let Some(doodads) = &map.doodads {
        report.map_doodads = doodads.len();
        for placed in doodads {
            let id = crate::fourcc_str(&placed.type_id);
            let matched = runtime.world.destructables.values().any(|destructable| {
                destructable.id.eq_ignore_ascii_case(&id)
                    && (destructable.x - placed.position.x as f64).abs() < 4.0
                    && (destructable.y - placed.position.y as f64).abs() < 4.0
            });
            if matched {
                report.matched_doodads += 1;
                continue;
            }
            let handle = runtime.alloc();
            let hp = if placed.life == 0 {
                100.0
            } else {
                f64::from(placed.life)
            };
            runtime.world.destructables.insert(
                handle,
                crate::jass::vm::Destructable {
                    id,
                    x: placed.position.x as f64,
                    y: placed.position.y as f64,
                    hp,
                    max_hp: hp,
                    alive: true,
                },
            );
            report.spawned_doodads += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_f32(bytes: &mut Vec<u8>, value: f32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn synthetic_unit_placement() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"W3do");
        bytes.extend_from_slice(&7u32.to_le_bytes()); // version
        bytes.extend_from_slice(&0u32.to_le_bytes()); // subversion
        bytes.extend_from_slice(&1u32.to_le_bytes()); // count
        bytes.extend_from_slice(b"hfoo");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // variation
        for value in [1024.0f32, 2048.0, 0.0] { push_f32(&mut bytes, value); }
        push_f32(&mut bytes, 270.0); // rotation
        for value in [1.0f32, 1.0, 1.0] { push_f32(&mut bytes, value); }
        bytes.push(0); // flags (peek < 0x20, no skin)
        bytes.extend_from_slice(&1u32.to_le_bytes()); // player
        bytes.push(0); // unk1
        bytes.push(0); // unk2
        bytes.extend_from_slice(&420i32.to_le_bytes()); // hp
        bytes.extend_from_slice(&0i32.to_le_bytes()); // mp
        bytes.extend_from_slice(&0u32.to_le_bytes()); // item sets
        bytes.extend_from_slice(&0i32.to_le_bytes()); // gold
        push_f32(&mut bytes, 0.0); // target acquisition
        bytes.extend_from_slice(&1i32.to_le_bytes()); // hero level
        bytes.extend_from_slice(&0u32.to_le_bytes()); // items
        bytes.extend_from_slice(&0u32.to_le_bytes()); // abilities
        bytes.extend_from_slice(&0u32.to_le_bytes()); // random mode
        bytes.extend_from_slice(&0u32.to_le_bytes()); // random type 0 payload (4 bytes)
        bytes
    }

    fn synthetic_doodad_placement() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"W3do");
        bytes.extend_from_slice(&7u32.to_le_bytes()); // version
        bytes.extend_from_slice(&0u32.to_le_bytes()); // subversion
        bytes.extend_from_slice(&1u32.to_le_bytes()); // count
        bytes.extend_from_slice(b"LTlt");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // variation
        for value in [512.0f32, 640.0, 0.0] { push_f32(&mut bytes, value); }
        push_f32(&mut bytes, 0.0); // rotation
        for value in [1.0f32, 1.0, 1.0] { push_f32(&mut bytes, value); }
        bytes.push(0); // state
        bytes.push(100); // life
        bytes.extend_from_slice(&0u32.to_le_bytes()); // creation
        bytes
    }

    #[test]
    fn spawns_map_units_and_doodads_into_jass_world() {
        let units = wc3_formats::map::parse_units(&synthetic_unit_placement())
            .expect("synthetic war3mapUnits.doo");
        let doodads = wc3_formats::map::parse_doodads(&synthetic_doodad_placement())
            .expect("synthetic war3map.doo");
        let map = MapBundle {
            units: Some(units),
            doodads: Some(doodads),
            ..MapBundle::default()
        };
        let mut runtime = Runtime::new(Program::default(), 1);
        let report = spawn_map_entities(&mut runtime, &map);
        assert_eq!(report.map_units, 1);
        assert_eq!(report.spawned_units, 1);
        assert_eq!(report.map_doodads, 1);
        assert_eq!(report.spawned_doodads, 1);
        let unit = runtime.world.units.values().next().expect("unit");
        assert_eq!(unit.id, "hfoo");
        assert!((unit.x - 1024.0).abs() < 0.001);
        let doodad = runtime.world.destructables.values().next().expect("doodad");
        assert_eq!(doodad.id, "LTlt");
        // Calling again is idempotent: existing type+position matches.
        let again = spawn_map_entities(&mut runtime, &map);
        assert_eq!(again.spawned_units, 0);
        assert_eq!(again.spawned_doodads, 0);
        assert_eq!(again.matched_units, 1);
        assert_eq!(again.matched_doodads, 1);
    }

    #[test]
    fn map_runtime_runs_init_custom_triggers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        if !root.is_dir() {
            eprintln!("skip: extracted install missing");
            return;
        }
        let mut config = EngineConfig::new(&root);
        config.map = Some(root.join("Maps/Campaign/Human04.w3m"));
        let mut engine = Engine::boot(config).expect("boot");
        let report = engine.script_report();
        assert!(report.main_ok, "war3map.j main failed: {:?}", engine.script_errors);
        assert!(report.functions > 1_000, "functions {}", report.functions);
        assert!(report.triggers > 0, "no triggers registered");
        assert!(report.trigger_actions > 0, "no trigger actions");
        assert!(report.executed_triggers > 0, "InitCustomTriggers/RunInitializationTriggers did not execute");
        let runtime = engine.runtime().expect("runtime");
        assert!(
            runtime
                .function_names()
                .iter()
                .any(|name| name.eq_ignore_ascii_case("InitCustomTriggers")),
            "InitCustomTriggers missing from merged program"
        );
        engine.tick(0.1);
        assert!(engine.runtime.is_some());
    }

    #[test]
    fn boots_headless_on_extracted_install() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        if !root.is_dir() {
            eprintln!("skip: extracted install missing");
            return;
        }
        let mut config = EngineConfig::new(&root);
        config.map = Some(root.join("Maps/Campaign/Human04.w3m"));
        let mut engine = Engine::boot(config).expect("boot");
        assert_eq!(engine.state, EngineState::InGame);
        assert!(engine.startup_strings > 0 || engine.storm.listfile().is_empty());
        assert!(engine.runtime.is_some());
        engine.tick(0.1);
        engine.tick(0.1);
        assert_eq!(engine.ticks, 2);
        engine.shutdown();
        assert_eq!(engine.state, EngineState::Idle);
    }
}
