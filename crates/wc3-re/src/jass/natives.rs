//! Native-function registry for the JASS interpreter.
//!
//! Every native declared by `Scripts\common.j` is callable. Implemented
//! natives mutate the deterministic [`World`](super::vm::World); natives that
//! are intentionally outside the headless port (rendering-only, platform-only)
//! fall through to a typed default and are counted in `Runtime::unimplemented`.
//! The census is printed by `wc3re jass`, which is how the map subset is
//! measured instead of guessed.

use std::collections::BTreeSet;

use crate::error::Result;

use super::vm::{
    AbilityState, BoolExpr, CameraEvent, CameraSetup, CameraState, CastTarget, CineBeat,
    CineBeatKind, Destructable, Effect, Item, ItemPool, Location, Rect, ScriptOrder, Sound,
    StockEntry, SubtitleEvent, Timer, TriggerEvent, Unit, Value, EVENT_PLAYER_UNIT_DROP_ITEM,
    EVENT_PLAYER_UNIT_PICKUP_ITEM, EVENT_UNIT_DROP_ITEM, EVENT_UNIT_PICKUP_ITEM,
};

fn a_int(args: &[Value], index: usize) -> i64 {
    args.get(index).map_or(0, Value::as_int)
}

fn a_real(args: &[Value], index: usize) -> f64 {
    args.get(index).map_or(0.0, Value::as_real)
}

fn a_bool(args: &[Value], index: usize) -> bool {
    args.get(index).is_some_and(Value::as_bool)
}

fn a_str(args: &[Value], index: usize) -> String {
    args.get(index).map_or_else(String::new, Value::as_str)
}

fn a_handle(args: &[Value], index: usize) -> Option<u32> {
    args.get(index).and_then(Value::handle)
}

/// `camerafield` ids from `common.j` (`ConvertCameraField(n)`).
const CAMERA_FIELD_TARGET_DISTANCE: i64 = 0;
const CAMERA_FIELD_FARZ: i64 = 1;
const CAMERA_FIELD_ANGLE_OF_ATTACK: i64 = 2;
const CAMERA_FIELD_FIELD_OF_VIEW: i64 = 3;
const CAMERA_FIELD_ROTATION: i64 = 5;
const CAMERA_FIELD_ZOFFSET: i64 = 6;

/// Applies `SetCameraField` to the live camera exactly as the field id means it.
fn apply_camera_field(camera: &mut CameraState, field: i64, value: f64) {
    match field {
        CAMERA_FIELD_TARGET_DISTANCE => camera.distance = value,
        CAMERA_FIELD_FARZ => camera.far_z = value,
        CAMERA_FIELD_ANGLE_OF_ATTACK => camera.pitch = value,
        CAMERA_FIELD_FIELD_OF_VIEW => camera.field_of_view = value,
        CAMERA_FIELD_ROTATION => camera.yaw = value,
        CAMERA_FIELD_ZOFFSET => camera.z_offset = value,
        _ => {}
    }
}

/// Reads one camera field back (`GetCameraField` / `CameraSetupGetField`).
fn camera_field(camera: &CameraState, field: i64) -> f64 {
    match field {
        CAMERA_FIELD_TARGET_DISTANCE => camera.distance,
        CAMERA_FIELD_FARZ => camera.far_z,
        CAMERA_FIELD_ANGLE_OF_ATTACK => camera.pitch,
        CAMERA_FIELD_FIELD_OF_VIEW => camera.field_of_view,
        CAMERA_FIELD_ROTATION => camera.yaw,
        CAMERA_FIELD_ZOFFSET => camera.z_offset,
        _ => 0.0,
    }
}

/// `rawcode` → the four-character type id the unit table keys on (`'hfoo'`), and
/// the display name of the first unit of that type on the map.
fn type_name_by_rawcode(world: &super::vm::World, rawcode: i64) -> Option<String> {
    if rawcode == 0 {
        return None;
    }
    let id = String::from_utf8_lossy(&(rawcode as u32).to_le_bytes()).to_string();
    world
        .units
        .values()
        .find(|unit| unit.id == id)
        .map(|unit| unit.name.clone())
}

fn rawcode_string(value: &Value) -> String {
    crate::fourcc_str(&crate::rawcode_bytes(value.as_int()))
}

/// Rawcode for an optional filter argument: `null` and an all-zero rawcode
/// both mean "no filter" (this is how `TriggerRegisterAnyUnitEventBJ` passes
/// its `null` filter).
fn optional_rawcode(value: Option<&Value>) -> String {
    let text = value.map_or_else(String::new, rawcode_string);
    if text.chars().all(|character| character == '\0') {
        String::new()
    } else {
        text
    }
}

fn rawcode_id(id: &str) -> i64 {
    crate::rawcode_value(&crate::fourcc(id))
}

impl super::vm::Runtime {
    pub(crate) fn call_native(
        &mut self,
        name: &str,
        args: &[Value],
        depth: usize,
    ) -> Result<Value> {
        let lower = name.to_ascii_lowercase();
        *self.native_calls.entry(lower.clone()).or_insert(0) += 1;
        let stamp_camera = self.world.cine_stamped_camera;
        let stamp_subtitle = self.world.cine_stamped_subtitle;
        let value = match lower.as_str() {
            // ----------------------------------------------------------------
            // Conversion natives (common.j constants are built from these).
            // ----------------------------------------------------------------
            _ if lower.starts_with("convert") => args.first().cloned().unwrap_or(Value::Null),

            // ----------------------------------------------------------------
            // Math / strings
            // ----------------------------------------------------------------
            "sin" => Value::Real(a_real(args, 0).sin()),
            "cos" => Value::Real(a_real(args, 0).cos()),
            "tan" => Value::Real(a_real(args, 0).tan()),
            "asin" => Value::Real(a_real(args, 0).asin()),
            "acos" => Value::Real(a_real(args, 0).acos()),
            "atan" => Value::Real(a_real(args, 0).atan()),
            "atan2" => Value::Real(a_real(args, 0).atan2(a_real(args, 1))),
            "pow" => Value::Real(a_real(args, 0).powf(a_real(args, 1))),
            "squareroot" => Value::Real(a_real(args, 0).sqrt()),
            "i2r" => Value::Real(a_int(args, 0) as f64),
            "r2i" => Value::Int(a_real(args, 0) as i64),
            "i2s" => Value::Str(a_int(args, 0).to_string()),
            "r2s" => Value::Str(super::vm::format_real(a_real(args, 0))),
            "r2sw" => {
                let width = a_int(args, 1).max(0) as usize;
                let precision = a_int(args, 2).max(0) as usize;
                Value::Str(format!("{:width$.precision$}", a_real(args, 0)))
            }
            "s2i" => Value::Int(a_str(args, 0).trim().parse().unwrap_or(0)),
            "s2r" => Value::Real(a_str(args, 0).trim().parse().unwrap_or(0.0)),
            "stringlength" => Value::Int(a_str(args, 0).len() as i64),
            "substring" => {
                let text = a_str(args, 0);
                let start = a_int(args, 1).max(0) as usize;
                let end = a_int(args, 2).max(0) as usize;
                let end = end.max(start).min(text.len());
                let start = start.min(end);
                Value::Str(text[start..end].to_string())
            }
            "concat" => Value::Str(format!("{}{}", a_str(args, 0), a_str(args, 1))),
            "stringcase" => {
                let text = a_str(args, 0);
                Value::Str(if a_bool(args, 1) {
                    text.to_uppercase()
                } else {
                    text.to_lowercase()
                })
            }
            "stringhash" => {
                let text = a_str(args, 0);
                let mut hash: u32 = 0x811C_9DC5;
                for byte in text.bytes() {
                    hash ^= u32::from(byte);
                    hash = hash.wrapping_mul(0x0100_0193);
                }
                Value::Int(i64::from(hash))
            }
            "getlocalizedstring" => Value::Str(a_str(args, 0)),
            "getlocalizedhotkey" => Value::Int(0),
            "getrandomint" => {
                let low = a_int(args, 0);
                let high = a_int(args, 1);
                Value::Int(self.rng.int(low, high))
            }
            "getrandomreal" => {
                let low = a_real(args, 0);
                let high = a_real(args, 1);
                Value::Real(self.rng.real(low, high))
            }
            "setrandomseed" => {
                self.rng = super::vm::Wc3Rng::new(a_int(args, 0) as u64);
                Value::Null
            }
            "deg2rad" => Value::Real(a_real(args, 0) * std::f64::consts::PI / 180.0),
            "rad2deg" => Value::Real(a_real(args, 0) * 180.0 / std::f64::consts::PI),
            "gethandleid" => Value::Int(i64::from(a_handle(args, 0).unwrap_or(0))),

            // ----------------------------------------------------------------
            // Players / forces
            // ----------------------------------------------------------------
            "player" => {
                let index = a_int(args, 0);
                if index >= 0 && (index as usize) < self.world.players.len() {
                    Value::Handle(index as u32)
                } else {
                    Value::Null
                }
            }
            "getplayerid" => Value::Int(a_handle(args, 0).map_or(-1, i64::from)),
            "getlocalplayer" | "getplayerlocal" => {
                super::vm::Runtime::local_player_value(&self.world)
            }
            "getplayername" => self
                .player_index(&args.first().cloned().unwrap_or(Value::Null))
                .map(|index| Value::Str(self.world.players[index].name.clone()))
                .unwrap_or(Value::Str(String::new())),
            "setplayername" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].name = a_str(args, 1);
                }
                Value::Null
            }
            "getplayerstate" => {
                let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) else {
                    return Ok(Value::Int(0));
                };
                let state = a_int(args, 1);
                let player = &self.world.players[index];
                Value::Int(match state {
                    0 => player.gold,
                    1 => player.lumber,
                    2 => player.food,
                    3 => player.food_max,
                    other => *player.states.get(&other).unwrap_or(&0),
                })
            }
            "setplayerstate" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    let state = a_int(args, 1);
                    let value = a_int(args, 2);
                    let player = &mut self.world.players[index];
                    match state {
                        0 => player.gold = value,
                        1 => player.lumber = value,
                        2 => player.food = value,
                        3 => player.food_max = value,
                        other => {
                            player.states.insert(other, value);
                        }
                    }
                }
                Value::Null
            }
            "getplayercontroller" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                Value::Int(self.world.players.get(index).map_or(1, |p| i64::from(p.controller)))
            }
            "setplayercontroller" => {
                // `common.j`: MAP_CONTROL_USER == ConvertMapControl(0). Campaign
                // maps use this to mark which slot the human plays; the game
                // layer reads it back to bind the HUD and the campaign camera.
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    let control = a_int(args, 1).clamp(0, 4) as u32;
                    self.world.players[index].controller = control;
                }
                Value::Null
            }
            "getplayerslotstate" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                Value::Int(self.world.players.get(index).map_or(0, |p| i64::from(p.slot_state)))
            }
            "getplayerrace" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                let race = self.world.players.get(index).map_or("human", |p| p.race.as_str());
                Value::Int(match race {
                    "orc" => 2,
                    "nightelf" => 3,
                    "undead" => 4,
                    _ => 1,
                })
            }
            "setplayerrace" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].race = match a_int(args, 1) {
                        2 => "orc",
                        3 => "nightelf",
                        4 => "undead",
                        _ => "human",
                    }
                    .into();
                }
                Value::Null
            }
            "getplayercolor" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                Value::Int(self.world.players.get(index).map_or(0, |p| i64::from(p.color)))
            }
            "setplayercolor" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].color = a_int(args, 1) as u32;
                }
                Value::Null
            }
            "getplayerteam" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                Value::Int(self.world.players.get(index).map_or(0, |p| i64::from(p.team)))
            }
            "setplayerteam" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].team = a_int(args, 1) as u32;
                }
                Value::Null
            }
            "getplayerstartlocation" => {
                let index = a_handle(args, 0).unwrap_or(0) as usize;
                Value::Int(self.world.players.get(index).map_or(-1, |p| i64::from(p.start_location)))
            }
            "setplayerstartlocation" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].start_location = a_int(args, 1) as i32;
                }
                Value::Null
            }
            "setplayeralliance" | "setplayeralliancestate" | "setplayervision"
            | "setplayertechresearched"
            | "setplayertechmaxallowed" | "setplayerhandicap" | "setplayerhandicapxp"
            | "setplayerscore" | "setplayeronscoreboard" => Value::Null,
            "getplayertechcount" | "getplayertechmaxallowed" | "getplayerhandicap"
            | "getplayerhandicapxp" | "getplayerstructcount" | "getplayerunitcount"
            | "getplayertypedunitcount" | "getplayerunittypeCount" | "getplayertotalunitcount"
            | "getplayerstructurecount" | "getplayerfoodused" | "getplayerfoodcap"
            | "getplayergold" | "getplayerlumber" | "getplayerresources"
            | "getplayerbounty" | "getplayerscore" => Value::Int(0),
            "getplayerunits" => self.alloc_group_of_player(args.first().cloned().unwrap_or(Value::Null)),
            "createforce" => {
                let handle = self.alloc();
                self.world.forces.insert(handle, BTreeSet::new());
                Value::Handle(handle)
            }
            "destroyforce" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.forces.remove(&handle);
                }
                Value::Null
            }
            "forceaddplayer" => {
                if let (Some(force), Some(player)) = (a_handle(args, 0), a_handle(args, 1)) {
                    self.world.forces.entry(force).or_default().insert(player);
                }
                Value::Null
            }
            "forceremoveplayer" => {
                if let (Some(force), Some(player)) = (a_handle(args, 0), a_handle(args, 1)) {
                    if let Some(players) = self.world.forces.get_mut(&force) {
                        players.remove(&player);
                    }
                }
                Value::Null
            }
            "forceclear" => {
                if let Some(force) = a_handle(args, 0) {
                    if let Some(players) = self.world.forces.get_mut(&force) {
                        players.clear();
                    }
                }
                Value::Null
            }
            "isplayerinforce" => {
                // `common.j`: IsPlayerInForce(whichPlayer, whichForce).
                if self.world.local_player_any
                    && a_handle(args, 0) == Some(super::vm::LOCAL_PLAYER_ANY)
                {
                    return Ok(Value::Bool(true));
                }
                let player = a_handle(args, 0).unwrap_or(u32::MAX);
                let force = a_handle(args, 1).unwrap_or(0);
                Value::Bool(
                    self.world
                        .forces
                        .get(&force)
                        .is_some_and(|players| players.contains(&player)),
                )
            }
            "getplayersall" => {
                // The `GetPlayersAll()` force every transmission is addressed
                // to. Without it `IsPlayerInForce(GetLocalPlayer(), GetPlayersAll())`
                // is false and Blizzard.j skips the whole transmission block.
                let handle = self.alloc();
                self.world
                    .forces
                    .insert(handle, (0..self.world.players.len() as u32).collect());
                Value::Handle(handle)
            }
            "getforceofplayer" => {
                let handle = self.alloc();
                let mut players = BTreeSet::new();
                if let Some(player) = a_handle(args, 0) {
                    players.insert(player);
                }
                self.world.forces.insert(handle, players);
                Value::Handle(handle)
            }
            "countplayersinforcebj" | "countplayersinforce" => {
                let force = a_handle(args, 0).unwrap_or(0);
                Value::Int(self.world.forces.get(&force).map_or(0, |p| p.len() as i64))
            }

            // ----------------------------------------------------------------
            // Units
            // ----------------------------------------------------------------
            "createunit" => self.native_create_unit(args),
            "createunitatloc" => self.native_create_unit_at_loc(args),
            "removeunit" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.units.remove(&handle);
                }
                Value::Null
            }
            "killunit" => {
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.hp = 0.0;
                    unit.alive = false;
                }
                Value::Null
            }
            "reviveunit" => {
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.alive = true;
                    unit.hp = unit.max_hp;
                }
                Value::Null
            }
            "getunitx" => Value::Real(self.unit_get(args, |unit| unit.x)),
            "getunity" => Value::Real(self.unit_get(args, |unit| unit.y)),
            "getunitz" => Value::Real(self.unit_get(args, |unit| unit.z)),
            "setunitx" => {
                let x = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.x = x;
                }
                Value::Null
            }
            "setunity" => {
                let y = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.y = y;
                }
                Value::Null
            }
            "setunitposition" => {
                let x = a_real(args, 1);
                let y = a_real(args, 2);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.x = x;
                    unit.y = y;
                }
                Value::Null
            }
            "setunitpositionloc" => {
                let location = a_handle(args, 1).unwrap_or(0);
                let (x, y) = self
                    .world
                    .locations
                    .get(&location)
                    .map(|loc| (loc.x, loc.y))
                    .unwrap_or((0.0, 0.0));
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.x = x;
                    unit.y = y;
                }
                Value::Null
            }
            "getunitfacing" => Value::Real(self.unit_get(args, |unit| unit.facing)),
            "setunitfacing" => {
                let facing = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.facing = facing;
                }
                Value::Null
            }
            "getunitstate" => {
                let state = a_int(args, 1);
                Value::Real(self.unit_get(args, |unit| match state {
                    0 => unit.hp,
                    1 => unit.mana,
                    2 => unit.max_hp,
                    3 => unit.max_mana,
                    other => *unit.states.get(&other).unwrap_or(&0.0),
                }))
            }
            "setunitstate" => {
                let state = a_int(args, 1);
                let value = a_real(args, 2);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    match state {
                        0 => unit.hp = value,
                        1 => unit.mana = value,
                        2 => unit.max_hp = value,
                        3 => unit.max_mana = value,
                        other => {
                            unit.states.insert(other, value);
                        }
                    }
                }
                Value::Null
            }
            "getunittypeid" => {
                let id = self.unit_type_id(&args.first().cloned().unwrap_or(Value::Null));
                Value::Int(rawcode_id(&id))
            }
            "getunitname" => {
                let id = self.unit_type_id(&args.first().cloned().unwrap_or(Value::Null));
                let name = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get(&handle))
                    .map(|unit| unit.name.clone())
                    .unwrap_or(id);
                Value::Str(name)
            }
            "setunitname" => {
                let value = a_str(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.name = value;
                }
                Value::Null
            }
            "getowningplayer" => {
                let owner = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get(&handle))
                    .map_or(-1, |unit| unit.owner);
                if owner >= 0 {
                    Value::Handle(owner as u32)
                } else {
                    Value::Null
                }
            }
            "setunitowner" => {
                let owner = a_handle(args, 1).map_or(-1, |handle| handle as i32);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.owner = owner;
                }
                Value::Null
            }
            "getunitlevel" | "getherolevel" => Value::Int(self.unit_get_i(args, |unit| unit.level)),
            "setherolevel" | "setunitlevel" => {
                let value = a_int(args, 1) as i32;
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.level = value;
                }
                Value::Null
            }
            "getheroxp" | "getherostrength" | "getheroagility" | "getheros intellect"
            | "getherostrength" | "getheroagility" | "getheroint" | "getunitpointvalue"
            | "getunitpointvaluebytype" | "getunitgoldcost" | "getunitwoodcost"
            | "getunitfoodmade" | "getunitfoodused" | "getunitmovementspeed" => Value::Int(0),
            "addheroxp" | "setherostrength" | "setheroagility" | "setheroint" | "sethp"
            | "setmana" => Value::Null,
            "abilityid" => Value::Int(rawcode_id(&a_str(args, 0))),
            "abilityid2string" => {
                Value::Str(rawcode_string(&Value::Int(a_int(args, 0))))
            }
            "unitaddability" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let handle = a_handle(args, 0);
                let mut added = false;
                if let Some(unit) = handle.and_then(|handle| self.world.units.get_mut(&handle)) {
                    if !unit.abilities.contains_key(&ability) {
                        unit.abilities.insert(
                            ability,
                            AbilityState {
                                level: 1,
                                ..AbilityState::default()
                            },
                        );
                        added = true;
                    }
                }
                Value::Bool(added)
            }
            "unitremoveability" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let removed = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                    .is_some_and(|unit| unit.abilities.remove(&ability).is_some());
                Value::Bool(removed)
            }
            "getunitabilitylevel" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                Value::Int(a_handle(args, 0).map_or(0, |handle| {
                    i64::from(self.ability_level(handle, &ability))
                }))
            }
            "setunitabilitylevel" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let level = a_int(args, 2).max(1) as i32;
                if let Some(unit) = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                {
                    unit.abilities
                        .entry(ability)
                        .and_modify(|state| state.level = level)
                        .or_insert(AbilityState {
                            level,
                            ..AbilityState::default()
                        });
                }
                Value::Int(i64::from(level))
            }
            "incunitabilitylevel" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let level = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                    .map_or(0, |unit| {
                        let state = unit.abilities.entry(ability).or_insert(AbilityState {
                            level: 1,
                            ..AbilityState::default()
                        });
                        state.level = state.level.saturating_add(1);
                        state.level
                    });
                Value::Int(i64::from(level))
            }
            "decunitabilitylevel" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let level = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                    .and_then(|unit| unit.abilities.get_mut(&ability))
                    .map_or(0, |state| {
                        state.level = (state.level - 1).max(1);
                        state.level
                    });
                Value::Int(i64::from(level))
            }
            "unitmakeabilitypermanent" => {
                let permanent = a_bool(args, 1);
                let ability = rawcode_string(&args.get(2).cloned().unwrap_or(Value::Null));
                let applied = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                    .is_some_and(|unit| {
                        let state = unit.abilities.entry(ability).or_insert(AbilityState {
                            level: 1,
                            ..AbilityState::default()
                        });
                        state.permanent = permanent;
                        true
                    });
                Value::Bool(applied)
            }
            "unitresetcooldown" => {
                if let Some(unit) = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                {
                    for state in unit.abilities.values_mut() {
                        state.cooldown = 0.0;
                    }
                }
                Value::Null
            }
            "selectheroskill" => {
                let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                if let Some(unit) = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get_mut(&handle))
                {
                    let state = unit.abilities.entry(ability).or_insert(AbilityState::default());
                    state.level = if state.level == 0 {
                        1
                    } else {
                        state.level.saturating_add(1)
                    };
                }
                Value::Null
            }
            "setplayerabilityavailable" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    let ability = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    let available = a_bool(args, 2);
                    let player = &mut self.world.players[index];
                    if available {
                        player.disabled_abilities.remove(&ability);
                    } else {
                        player.disabled_abilities.insert(ability);
                    }
                }
                Value::Null
            }
            "getspellabilityid" => Value::Int(
                self.world
                    .context
                    .ability
                    .as_deref()
                    .map_or(0, rawcode_id),
            ),
            "getspellabilityunit" => self.world.context.unit.map(Value::Handle).unwrap_or(Value::Null),
            "getspellability" => Value::Null,
            "getspelltargetunit" => self
                .world
                .context
                .spell_target_unit
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "getspelltargetitem" => self
                .world
                .context
                .spell_target_item
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "getspelltargetdestructable" => self
                .world
                .context
                .spell_target_destructable
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "getspelltargetx" => Value::Real(
                self.world
                    .context
                    .spell_target_point
                    .map_or(0.0, |(x, _)| x),
            ),
            "getspelltargety" => Value::Real(
                self.world
                    .context
                    .spell_target_point
                    .map_or(0.0, |(_, y)| y),
            ),
            "getspelltargetloc" => {
                if let Some((x, y)) = self.world.context.spell_target_point {
                    let handle = self.alloc();
                    self.world.locations.insert(handle, Location { x, y });
                    Value::Handle(handle)
                } else {
                    Value::Null
                }
            }
            "getmanipulateditem" => self
                .world
                .context
                .manipulated_item
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "getsolditem" => self
                .world
                .context
                .sold_item
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "getenumitem" | "getfilteritem" => self
                .world
                .context
                .enum_item
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "pauseunit" => {
                let paused = a_bool(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.paused = paused;
                }
                Value::Null
            }
            "showunit" => {
                let shown = a_bool(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.hidden = !shown;
                }
                Value::Null
            }
            "setunitscale" => {
                let scale = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.scale = scale;
                }
                Value::Null
            }
            "setunittimescale" => {
                let scale = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.time_scale = scale;
                }
                Value::Null
            }
            "unitadditem" => {
                let added = match (a_handle(args, 0), a_handle(args, 1)) {
                    (Some(unit), Some(item)) => self.attach_item(unit, item),
                    _ => false,
                };
                Value::Bool(added)
            }
            "unitadditembyname" | "unitadditembyid" => self.native_add_item_by_id(args),
            "unitremoveitem" => {
                if let (Some(unit), Some(item)) = (a_handle(args, 0), a_handle(args, 1)) {
                    self.detach_item(unit, item);
                }
                Value::Null
            }
            "unitremoveitemfromslot" => {
                let removed = a_handle(args, 0)
                    .and_then(|unit| {
                        let slot = a_int(args, 1).max(0) as usize;
                        self.detach_item_slot(unit, slot)
                    });
                removed.map(Value::Handle).unwrap_or(Value::Null)
            }
            "unititeminslot" => {
                let slot = a_int(args, 1).max(0) as usize;
                let item = a_handle(args, 0).and_then(|handle| {
                    self.world
                        .units
                        .get(&handle)
                        .and_then(|unit| unit.items.get(slot).copied())
                        .filter(|handle| *handle != 0)
                });
                item.map(Value::Handle).unwrap_or(Value::Null)
            }
            "unithasitem" => Value::Bool(
                a_handle(args, 0).is_some_and(|unit| {
                    a_handle(args, 1).is_some_and(|item| {
                        self.world
                            .units
                            .get(&unit)
                            .is_some_and(|unit| unit.items.contains(&item))
                    })
                }),
            ),
            "unituseitem" => {
                let used = match (a_handle(args, 0), a_handle(args, 1)) {
                    (Some(unit), Some(item)) => {
                        self.use_item(unit, item, CastTarget::None).unwrap_or(false)
                    }
                    _ => false,
                };
                Value::Bool(used)
            }
            "unituseitempoint" => {
                let used = match (a_handle(args, 0), a_handle(args, 1)) {
                    (Some(unit), Some(item)) => self
                        .use_item(unit, item, CastTarget::Point(a_real(args, 2), a_real(args, 3)))
                        .unwrap_or(false),
                    _ => false,
                };
                Value::Bool(used)
            }
            "unituseitemtarget" => {
                let used = match (a_handle(args, 0), a_handle(args, 1)) {
                    (Some(unit), Some(item)) => {
                        let target = a_handle(args, 2).map_or(CastTarget::None, |handle| {
                            self.cast_target_for(handle)
                        });
                        self.use_item(unit, item, target).unwrap_or(false)
                    }
                    _ => false,
                };
                Value::Bool(used)
            }
            "getinventorysize" | "unitinventorysize" => Value::Int(6),
            "isunit" | "isunittype" | "isunitally" | "isunitenemy" | "isunitownedbyplayer"
            | "isunitvisible" | "isunitdetected" | "isunitfogged" | "isunitmasked"
            | "isunitselected" | "isunitpaused" | "isunitinrange" | "isunitinrangexy"
            | "isunitinrangeofloc" | "isunitinrangeofpoint" | "isunitloaded"
            | "isunithidden" | "isunitillusion" | "isunitdeadbj" | "isunitalivebj" => {
                Value::Bool(lower == "isunit" || lower == "isunitalivebj" || lower == "isunitinrange")
            }
            "issueimmediateorder" | "issueimmediateorderloc" => {
                if let Some(handle) = a_handle(args, 0) {
                    let order = a_str(args, 1);
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: order.clone(),
                        ..ScriptOrder::default()
                    });
                    if let Some(unit) = self.world.units.get_mut(&handle) {
                        unit.order = Some(order);
                    }
                }
                Value::Bool(true)
            }
            "issueimmediateorderbyid" => {
                if let Some(handle) = a_handle(args, 0) {
                    let order = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: order.clone(),
                        ..ScriptOrder::default()
                    });
                    if let Some(unit) = self.world.units.get_mut(&handle) {
                        unit.order = Some(order);
                    }
                }
                Value::Bool(true)
            }
            "issuepointorder" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: a_str(args, 1),
                        point: Some((a_real(args, 2), a_real(args, 3))),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuepointorderbyid" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null)),
                        point: Some((a_real(args, 2), a_real(args, 3))),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuepointorderloc" => {
                if let Some(handle) = a_handle(args, 0) {
                    let location = a_handle(args, 2).unwrap_or(0);
                    let point = self
                        .world
                        .locations
                        .get(&location)
                        .map(|loc| (loc.x, loc.y));
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: a_str(args, 1),
                        point,
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuetargetorder" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: a_str(args, 1),
                        target: a_handle(args, 2),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuetargetorderbyid" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null)),
                        target: a_handle(args, 2),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuebuildorder" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: "build".into(),
                        point: Some((a_real(args, 2), a_real(args, 3))),
                        id: Some(a_str(args, 1)),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "issuebuildorderbyid" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.orders.push(ScriptOrder {
                        unit: handle,
                        order: "build".into(),
                        point: Some((a_real(args, 2), a_real(args, 3))),
                        id: Some(rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null))),
                        ..ScriptOrder::default()
                    });
                }
                Value::Bool(true)
            }
            "getunitcurrentorder" => Value::Int(0),
            "getissuedorder" => Value::Int(0),
            "getunitmovementspeed" | "getunitpropwindow" => Value::Real(270.0),
            "setunitmovementspeed" | "setunitproprwindow" | "setunitturnspeed"
            | "setunitacquirerange" => Value::Null,
            "getunitacquirerange" => Value::Real(600.0),
            "getunitturnspeed" => Value::Real(0.6),
            "getunitdefaultmovementspeed" => Value::Real(270.0),
            "getunitdefaultacquire range" => Value::Real(600.0),
            "setunitcreepguard" | "unitShareVision" | "unitstop" | "unitremoveability"
            | "unitisinvulnerable" => Value::Null,

            // ----------------------------------------------------------------
            // Groups
            // ----------------------------------------------------------------
            "creategroup" => {
                let handle = self.alloc();
                self.world.groups.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "destroygroup" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.groups.remove(&handle);
                }
                Value::Null
            }
            "groupaddunit" => {
                if let (Some(group), Some(unit)) = (a_handle(args, 0), a_handle(args, 1)) {
                    self.world.groups.entry(group).or_default().units.insert(unit);
                }
                Value::Bool(true)
            }
            "groupremoveunit" => {
                if let (Some(group), Some(unit)) = (a_handle(args, 0), a_handle(args, 1)) {
                    if let Some(group) = self.world.groups.get_mut(&group) {
                        group.units.remove(&unit);
                    }
                }
                Value::Bool(true)
            }
            "groupclear" => {
                if let Some(group) = a_handle(args, 0).and_then(|h| self.world.groups.get_mut(&h)) {
                    group.units.clear();
                }
                Value::Null
            }
            "firstofgroup" => {
                let unit = a_handle(args, 0)
                    .and_then(|handle| self.world.groups.get(&handle))
                    .and_then(|group| group.units.iter().next().copied());
                unit.map(Value::Handle).unwrap_or(Value::Null)
            }
            "countunitsingroup" => {
                let count = a_handle(args, 0)
                    .and_then(|handle| self.world.groups.get(&handle))
                    .map_or(0, |group| group.units.len() as i64);
                Value::Int(count)
            }
            "isunitingroup" => {
                let group = a_handle(args, 0);
                let unit = a_handle(args, 1);
                Value::Bool(match (group, unit) {
                    (Some(group), Some(unit)) => self
                        .world
                        .groups
                        .get(&group)
                        .is_some_and(|group| group.units.contains(&unit)),
                    _ => false,
                })
            }
            "forgroup" => {
                let units: Vec<u32> = a_handle(args, 0)
                    .and_then(|handle| self.world.groups.get(&handle))
                    .map(|group| group.units.iter().copied().collect())
                    .unwrap_or_default();
                let code = args.get(1).cloned().unwrap_or(Value::Null);
                if let Value::Code(code) = code {
                    let previous = self.world.context.enum_unit;
                    for unit in units {
                        self.world.context.enum_unit = Some(unit);
                        self.call_function(&code, &[], depth + 1)?;
                    }
                    self.world.context.enum_unit = previous;
                }
                Value::Null
            }
            "getenumunit" => self.world.context.enum_unit.map(Value::Handle).unwrap_or(Value::Null),
            "getfilterunit" => self.world.context.enum_unit.map(Value::Handle).unwrap_or(Value::Null),
            "groupenumunitsinrange" => {
                let x = a_real(args, 1);
                let y = a_real(args, 2);
                let radius = a_real(args, 3);
                let units: Vec<u32> = self
                    .world
                    .units
                    .iter()
                    .filter(|(_, unit)| {
                        let dx = unit.x - x;
                        let dy = unit.y - y;
                        dx * dx + dy * dy <= radius * radius
                    })
                    .map(|(handle, _)| *handle)
                    .collect();
                if let Some(group) = a_handle(args, 0) {
                    self.world.groups.entry(group).or_default().units.extend(units);
                }
                Value::Null
            }
            "groupenumunitsinrangeofpoint" => {
                let loc = a_handle(args, 1).unwrap_or(0);
                let (x, y) = self
                    .world
                    .locations
                    .get(&loc)
                    .map(|loc| (loc.x, loc.y))
                    .unwrap_or((0.0, 0.0));
                let radius = a_real(args, 2);
                let units: Vec<u32> = self
                    .world
                    .units
                    .iter()
                    .filter(|(_, unit)| {
                        let dx = unit.x - x;
                        let dy = unit.y - y;
                        dx * dx + dy * dy <= radius * radius
                    })
                    .map(|(handle, _)| *handle)
                    .collect();
                if let Some(group) = a_handle(args, 0) {
                    self.world.groups.entry(group).or_default().units.extend(units);
                }
                Value::Null
            }
            "groupenumunitsofplayer" => {
                let player = a_handle(args, 1).map_or(-1, |h| h as i32);
                let units: Vec<u32> = self
                    .world
                    .units
                    .iter()
                    .filter(|(_, unit)| unit.owner == player)
                    .map(|(handle, _)| *handle)
                    .collect();
                if let Some(group) = a_handle(args, 0) {
                    self.world.groups.entry(group).or_default().units.extend(units);
                }
                Value::Null
            }
            "groupenumunitsoftype" => {
                let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let units: Vec<u32> = self
                    .world
                    .units
                    .iter()
                    .filter(|(_, unit)| unit.id == id)
                    .map(|(handle, _)| *handle)
                    .collect();
                if let Some(group) = a_handle(args, 0) {
                    self.world.groups.entry(group).or_default().units.extend(units);
                }
                Value::Null
            }
            "groupenumunitsinrect" => Value::Null,
            "groupenumunitsinrangecounted" | "groupenumunitsofplayercounted"
            | "groupenumunitsoft Typecounted" => Value::Null,

            // ----------------------------------------------------------------
            // Locations / rects / regions
            // ----------------------------------------------------------------
            "location" => {
                let handle = self.alloc();
                self.world.locations.insert(
                    handle,
                    Location {
                        x: a_real(args, 0),
                        y: a_real(args, 1),
                    },
                );
                Value::Handle(handle)
            }
            "removelocation" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.locations.remove(&handle);
                }
                Value::Null
            }
            "getlocationx" => Value::Real(
                a_handle(args, 0)
                    .and_then(|handle| self.world.locations.get(&handle))
                    .map_or(0.0, |loc| loc.x),
            ),
            "getlocationy" => Value::Real(
                a_handle(args, 0)
                    .and_then(|handle| self.world.locations.get(&handle))
                    .map_or(0.0, |loc| loc.y),
            ),
            "getlocationz" => Value::Real(0.0),
            "movelocation" => {
                if let Some(location) = a_handle(args, 0).and_then(|h| self.world.locations.get_mut(&h)) {
                    location.x = a_real(args, 1);
                    location.y = a_real(args, 2);
                }
                Value::Null
            }
            "rect" => {
                let handle = self.alloc();
                self.world.rects.insert(
                    handle,
                    Rect {
                        min_x: a_real(args, 0),
                        min_y: a_real(args, 1),
                        max_x: a_real(args, 2),
                        max_y: a_real(args, 3),
                    },
                );
                Value::Handle(handle)
            }
            "rectfromloc" => {
                let (min, max) = (a_handle(args, 0), a_handle(args, 1));
                let min_loc = min.and_then(|h| self.world.locations.get(&h)).map(|l| (l.x, l.y));
                let max_loc = max.and_then(|h| self.world.locations.get(&h)).map(|l| (l.x, l.y));
                let handle = self.alloc();
                self.world.rects.insert(
                    handle,
                    Rect {
                        min_x: min_loc.map_or(0.0, |l| l.0),
                        min_y: min_loc.map_or(0.0, |l| l.1),
                        max_x: max_loc.map_or(0.0, |l| l.0),
                        max_y: max_loc.map_or(0.0, |l| l.1),
                    },
                );
                Value::Handle(handle)
            }
            "removerect" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.rects.remove(&handle);
                }
                Value::Null
            }
            "getrectcenterx" => Value::Real(
                a_handle(args, 0)
                    .and_then(|h| self.world.rects.get(&h))
                    .map_or(0.0, |rect| (rect.min_x + rect.max_x) * 0.5),
            ),
            "getrectcentery" => Value::Real(
                a_handle(args, 0)
                    .and_then(|h| self.world.rects.get(&h))
                    .map_or(0.0, |rect| (rect.min_y + rect.max_y) * 0.5),
            ),
            "getrectminx" => Value::Real(self.rect_get(args, |r| r.min_x)),
            "getrectminy" => Value::Real(self.rect_get(args, |r| r.min_y)),
            "getrectmaxx" => Value::Real(self.rect_get(args, |r| r.max_x)),
            "getrectmaxy" => Value::Real(self.rect_get(args, |r| r.max_y)),
            "getrectcenter" => {
                let (x, y) = a_handle(args, 0)
                    .and_then(|h| self.world.rects.get(&h))
                    .map(|rect| ((rect.min_x + rect.max_x) * 0.5, (rect.min_y + rect.max_y) * 0.5))
                    .unwrap_or((0.0, 0.0));
                let handle = self.alloc();
                self.world.locations.insert(handle, Location { x, y });
                Value::Handle(handle)
            }
            "setrect" | "setrectfromloc" | "movecrect" | "movecrectcenter" => Value::Null,
            "createregion" => {
                let handle = self.alloc();
                self.world.regions.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "removeregion" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.regions.remove(&handle);
                }
                Value::Null
            }
            "regionaddrect" => {
                if let (Some(region), Some(rect)) = (a_handle(args, 0), a_handle(args, 1)) {
                    self.world.regions.entry(region).or_default().rects.push(rect);
                }
                Value::Null
            }
            "regionclearrect" => {
                if let (Some(region), Some(rect)) = (a_handle(args, 0), a_handle(args, 1)) {
                    if let Some(region) = self.world.regions.get_mut(&region) {
                        region.rects.retain(|value| *value != rect);
                    }
                }
                Value::Null
            }
            "regionaddcell" => {
                if let Some(region) = a_handle(args, 0).and_then(|h| self.world.regions.get_mut(&h)) {
                    region
                        .cells
                        .insert((a_real(args, 1) as i32, a_real(args, 2) as i32));
                }
                Value::Null
            }
            "regionclear cell" | "regionclearcell" => Value::Null,
            "regionaddcellatloc" => {
                if let Some(region) = a_handle(args, 0).and_then(|h| self.world.regions.get_mut(&h)) {
                    if let Some(loc) = a_handle(args, 1).and_then(|h| self.world.locations.get(&h)) {
                        region.cells.insert((loc.x as i32, loc.y as i32));
                    }
                }
                Value::Null
            }
            "regionclearcellatloc" => Value::Null,
            "ispointinregion" => Value::Bool(false),
            "isunitinregion" => Value::Bool(false),

            // ----------------------------------------------------------------
            // Triggers / boolexprs / events
            // ----------------------------------------------------------------
            "createtrigger" => {
                let handle = self.alloc();
                self.world.triggers.insert(
                    handle,
                    super::vm::Trigger {
                        enabled: true,
                        ..Default::default()
                    },
                );
                Value::Handle(handle)
            }
            "destroytrigger" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.triggers.remove(&handle);
                }
                Value::Null
            }
            "triggeraddaction" => {
                let Some(trigger) = a_handle(args, 0) else {
                    return Ok(Value::Null);
                };
                let action = match args.get(1) {
                    Some(Value::Code(code)) => code.clone(),
                    _ => String::new(),
                };
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.actions.push(action);
                }
                let handle = self.alloc();
                self.world.boolexprs.insert(handle, BoolExpr::Code(String::new()));
                Value::Handle(handle)
            }
            "triggeraddcondition" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let condition = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.conditions.push(condition);
                }
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "triggerremoveaction" | "triggerremovecondition" => Value::Null,
            "triggerclearactions" => {
                if let Some(trigger) = a_handle(args, 0).and_then(|h| self.world.triggers.get_mut(&h)) {
                    trigger.actions.clear();
                }
                Value::Null
            }
            "triggerclearconditions" => {
                if let Some(trigger) = a_handle(args, 0).and_then(|h| self.world.triggers.get_mut(&h)) {
                    trigger.conditions.clear();
                }
                Value::Null
            }
            "triggerexecute" => {
                let handle = a_handle(args, 0).unwrap_or(0);
                self.execute_trigger_depth(handle, depth)?;
                Value::Null
            }
            "triggerevaluate" => {
                let handle = a_handle(args, 0).unwrap_or(0);
                let conditions = self
                    .world
                    .triggers
                    .get(&handle)
                    .map(|trigger| trigger.conditions.clone())
                    .unwrap_or_default();
                let mut pass = true;
                for condition in conditions {
                    if !self.eval_bool_expr(condition, depth)? {
                        pass = false;
                        break;
                    }
                }
                if let Some(trigger) = self.world.triggers.get_mut(&handle) {
                    trigger.eval_count += 1;
                }
                Value::Bool(pass)
            }
            "condition" | "filter" => {
                let handle = self.alloc();
                let code = match args.first() {
                    Some(Value::Code(code)) => code.clone(),
                    _ => String::new(),
                };
                self.world.boolexprs.insert(handle, BoolExpr::Code(code));
                Value::Handle(handle)
            }
            "and" => {
                let a = a_handle(args, 0).unwrap_or(0);
                let b = a_handle(args, 1).unwrap_or(0);
                let handle = self.alloc();
                self.world.boolexprs.insert(handle, BoolExpr::And(a, b));
                Value::Handle(handle)
            }
            "or" => {
                let a = a_handle(args, 0).unwrap_or(0);
                let b = a_handle(args, 1).unwrap_or(0);
                let handle = self.alloc();
                self.world.boolexprs.insert(handle, BoolExpr::Or(a, b));
                Value::Handle(handle)
            }
            "not" => {
                let a = a_handle(args, 0).unwrap_or(0);
                let handle = self.alloc();
                self.world.boolexprs.insert(handle, BoolExpr::Not(a));
                Value::Handle(handle)
            }
            "destroyboolexpr" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.boolexprs.remove(&handle);
                }
                Value::Null
            }
            "triggerregisterplayerevent" => self.register_trigger_event(args, |a, b| TriggerEvent::Player { player: a, event: b }),
            "triggerregisterplayerunitevent" => {
                let unit_type = optional_rawcode(args.get(3));
                self.register_trigger_event(args, move |a, b| TriggerEvent::PlayerUnit {
                    player: a,
                    event: b,
                    unit_type: unit_type.clone(),
                })
            }
            "triggerregisterunitevent" => self.register_trigger_event(args, |a, b| TriggerEvent::UnitEvent { unit: a, event: b }),
            "triggerregisteruniteventbyid" => self.register_trigger_event(args, |_a, b| TriggerEvent::UnitEvent { unit: 0, event: b }),
            "triggerregisterdeathevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let unit = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::Death { unit });
                }
                Value::Handle(trigger)
            }
            "triggerregisterenteringregion" | "triggerregisterenterregion" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let region = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::EnterRegion { region });
                }
                Value::Handle(trigger)
            }
            "triggerregisterleavingregion" | "triggerregisterleaveregion" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let region = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::LeaveRegion { region });
                }
                Value::Handle(trigger)
            }
            "triggerregistertimerevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let timeout = a_real(args, 1);
                let periodic = a_bool(args, 2);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::Timer { timeout, periodic });
                }
                Value::Handle(trigger)
            }
            "triggerregistertimerexpireevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let timer = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::TimerExpire { timer });
                }
                Value::Handle(trigger)
            }
            "triggerregistergamestateevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let state = a_int(args, 1);
                let op = a_int(args, 2);
                let value = a_real(args, 3);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::GameState { state, op, value });
                }
                Value::Handle(trigger)
            }
            "triggerregistervariableevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let name = a_str(args, 1);
                let op = a_int(args, 2);
                let value = a_real(args, 3);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::Variable { name, op, value });
                }
                Value::Handle(trigger)
            }
            "triggerregisterdialogevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let dialog = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::Dialog { dialog });
                }
                Value::Handle(trigger)
            }
            "triggerregisterdialogbuttonevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let dialog = a_handle(args, 1).unwrap_or(0);
                let button = a_handle(args, 2).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::DialogButton { dialog, button });
                }
                Value::Handle(trigger)
            }
            "triggerregistergamedialogevent" | "triggerregistergamedialogbuttonevent" => Value::Null,
            "enabletrigger" => {
                if let Some(trigger) = a_handle(args, 0).and_then(|h| self.world.triggers.get_mut(&h)) {
                    trigger.enabled = true;
                }
                Value::Null
            }
            "disabletrigger" => {
                if let Some(trigger) = a_handle(args, 0).and_then(|h| self.world.triggers.get_mut(&h)) {
                    trigger.enabled = false;
                }
                Value::Null
            }
            "istriggerenabled" => {
                let enabled = a_handle(args, 0)
                    .and_then(|h| self.world.triggers.get(&h))
                    .is_some_and(|trigger| trigger.enabled);
                Value::Bool(enabled)
            }
            "trigger syncstart" | "triggersyncstart" | "triggersyncready" => Value::Null,
            "gettriggeringtrigger" => self
                .world
                .context
                .trigger
                .map(Value::Handle)
                .unwrap_or(Value::Null),
            "gettriggerplayer" => self.world.context.player.map(Value::Handle).unwrap_or(Value::Null),
            "gettriggerunit" => self.world.context.unit.map(Value::Handle).unwrap_or(Value::Null),
            "gettriggerwidget" => self.world.context.unit.map(Value::Handle).unwrap_or(Value::Null),
            "gettriggeringregion" => Value::Null,
            "gettriggeringe ..." => Value::Null,
            "gettrigger eventid" | "gettriggereventid" => Value::Null,
            "geteventplayerchatstring" => Value::Str(self.world.context.chat.clone()),
            "geteventplayerchatstringmatchedfilter" => Value::Str(String::new()),
            "geteventplayerselectedunit" | "geteventplayerclickbutton" => Value::Null,
            "geteventplayerstate" => Value::Int(self.world.context.game_state),
            "geteventdamage" | "geteventdamagebj" => Value::Real(self.world.context.damage),
            "geteventdamagesource" => self.world.context.unit.map(Value::Handle).unwrap_or(Value::Null),
            "geteventgame state" | "geteventgamestate" => Value::Int(self.world.context.game_state),
            "geteventplayerchatstring" => Value::Str(self.world.context.chat.clone()),
            "getdyingunit" => self.world.context.dying.map(Value::Handle).unwrap_or(Value::Null),
            "getkillingunit" => self.world.context.killing.map(Value::Handle).unwrap_or(Value::Null),
            "getenteringunit" => self.world.context.entering.map(Value::Handle).unwrap_or(Value::Null),
            "getleavingunit" => self.world.context.leaving.map(Value::Handle).unwrap_or(Value::Null),
            "gettriggeringunit" => self.world.context.unit.map(Value::Handle).unwrap_or(Value::Null),
            "gettriggerexecCount" | "gettriggerexeccount" => {
                let count = a_handle(args, 0)
                    .and_then(|h| self.world.triggers.get(&h))
                    .map_or(0, |trigger| i64::from(trigger.exec_count));
                Value::Int(count)
            }
            "gettriggerevalcount" => {
                let count = a_handle(args, 0)
                    .and_then(|h| self.world.triggers.get(&h))
                    .map_or(0, |trigger| i64::from(trigger.eval_count));
                Value::Int(count)
            }

            // ----------------------------------------------------------------
            // Timers
            // ----------------------------------------------------------------
            "createtimer" => {
                let handle = self.alloc();
                self.world.timers.insert(handle, Timer::default());
                Value::Handle(handle)
            }
            "destroytimer" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.timers.remove(&handle);
                }
                Value::Null
            }
            "timerstart" => {
                if let Some(timer) = a_handle(args, 0).and_then(|h| self.world.timers.get_mut(&h)) {
                    timer.timeout = a_real(args, 1);
                    timer.remaining = timer.timeout;
                    timer.periodic = a_bool(args, 2);
                    timer.code = match args.get(3) {
                        Some(Value::Code(code)) => Some(code.clone()),
                        _ => None,
                    };
                    timer.running = true;
                }
                Value::Null
            }
            "timergetelapsed" => Value::Real(
                a_handle(args, 0)
                    .and_then(|h| self.world.timers.get(&h))
                    .map_or(0.0, |timer| timer.elapsed),
            ),
            "timergetremaining" => Value::Real(
                a_handle(args, 0)
                    .and_then(|h| self.world.timers.get(&h))
                    .map_or(0.0, |timer| timer.remaining),
            ),
            "timergettimeout" => Value::Real(
                a_handle(args, 0)
                    .and_then(|h| self.world.timers.get(&h))
                    .map_or(0.0, |timer| timer.timeout),
            ),
            "pausetimer" => {
                if let Some(timer) = a_handle(args, 0).and_then(|h| self.world.timers.get_mut(&h)) {
                    timer.running = false;
                }
                Value::Null
            }
            "resumetimer" => {
                if let Some(timer) = a_handle(args, 0).and_then(|h| self.world.timers.get_mut(&h)) {
                    timer.running = true;
                }
                Value::Null
            }
            "getexpiredtimer" => Value::Null,

            // ----------------------------------------------------------------
            // Items / destructables
            // ----------------------------------------------------------------
            "createitem" => {
                let handle = self.alloc();
                self.world.items.insert(
                    handle,
                    Item {
                        id: rawcode_string(&args.first().cloned().unwrap_or(Value::Null)),
                        x: a_real(args, 1),
                        y: a_real(args, 2),
                        ..Item::default()
                    },
                );
                self.world.last_created_item = Some(handle);
                Value::Handle(handle)
            }
            "removeitem" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.items.remove(&handle);
                    for unit in self.world.units.values_mut() {
                        for slot in &mut unit.items {
                            if *slot == handle {
                                *slot = 0;
                            }
                        }
                    }
                }
                Value::Null
            }
            "getitemx" => Value::Real(self.item_get(args, |item| item.x)),
            "getitemy" => Value::Real(self.item_get(args, |item| item.y)),
            "setitemposition" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.x = a_real(args, 1);
                    item.y = a_real(args, 2);
                }
                Value::Null
            }
            "getitemtypeid" => {
                let id = a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .map(|item| item.id.clone())
                    .unwrap_or_default();
                Value::Int(rawcode_id(&id))
            }
            "getitemtype" => Value::Null,
            "getitemplayer" => {
                let owner = a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .and_then(|item| item.owner);
                owner.map(Value::Handle).unwrap_or(Value::Null)
            }
            "setitemplayer" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.owner = a_handle(args, 1);
                }
                Value::Null
            }
            "getitemcharges" => Value::Int(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .map_or(0, |item| i64::from(item.charges)),
            ),
            "setitemcharges" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.charges = a_int(args, 1) as i32;
                }
                Value::Null
            }
            "getitemlevel" => Value::Int(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .map_or(0, |item| i64::from(item.level)),
            ),
            "setitemdropondeath" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.drop_on_death = a_bool(args, 1);
                }
                Value::Null
            }
            "setitemdroppable" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.droppable = a_bool(args, 1);
                }
                Value::Null
            }
            "setitempawnable" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.pawnable = a_bool(args, 1);
                }
                Value::Null
            }
            "setiteminvulnerable" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.invulnerable = a_bool(args, 1);
                }
                Value::Null
            }
            "isiteminvulnerable" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.invulnerable),
            ),
            "setitemvisible" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.visible = a_bool(args, 1);
                }
                Value::Null
            }
            "isitemvisible" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.visible),
            ),
            "isitemowned" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.held_by.is_some()),
            ),
            "isitempowerup" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.powerup),
            ),
            "isitemsellable" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.sellable),
            ),
            "isitempawnable" => Value::Bool(
                a_handle(args, 0)
                    .and_then(|h| self.world.items.get(&h))
                    .is_some_and(|item| item.pawnable),
            ),
            "isitemidpowerup" | "isitemidsellable" | "isitemidpawnable" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                let found = self.world.items.values().find(|item| item.id == id);
                let result = match lower.as_str() {
                    "isitemidpowerup" => found.is_some_and(|item| item.powerup),
                    "isitemidsellable" => found.is_some_and(|item| item.sellable),
                    _ => found.is_some_and(|item| item.pawnable),
                };
                Value::Bool(result)
            }
            "setitemdropid" => {
                if let Some(item) = a_handle(args, 0).and_then(|h| self.world.items.get_mut(&h)) {
                    item.drop_id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                }
                Value::Null
            }
            "enumitemsinrect" => {
                let rect = a_handle(args, 0);
                let action = match args.get(2) {
                    Some(Value::Code(code)) => Some(code.clone()),
                    _ => None,
                };
                if let (Some(rect), Some(action)) = (rect, action) {
                    let bounds = self.world.rects.get(&rect).cloned();
                    if let Some(bounds) = bounds {
                        let inside: Vec<u32> = self
                            .world
                            .items
                            .iter()
                            .filter(|(_, item)| {
                                item.x >= bounds.min_x
                                    && item.x <= bounds.max_x
                                    && item.y >= bounds.min_y
                                    && item.y <= bounds.max_y
                            })
                            .map(|(handle, _)| *handle)
                            .collect();
                        for handle in inside {
                            self.world.context.enum_item = Some(handle);
                            let _ = self.call_function(&action, &[], 1);
                        }
                        self.world.context.enum_item = None;
                    }
                }
                Value::Null
            }
            // ----------------------------------------------------------------
            // Item pools
            // ----------------------------------------------------------------
            "createitempool" => {
                let handle = self.alloc();
                self.world.item_pools.insert(handle, ItemPool::default());
                Value::Handle(handle)
            }
            "destroyitempool" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.item_pools.remove(&handle);
                }
                Value::Null
            }
            "itempooladditemtype" => {
                if let Some(pool) = a_handle(args, 0).and_then(|h| self.world.item_pools.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    let weight = a_real(args, 2).max(0.0);
                    pool.entries.push((id, weight));
                }
                Value::Null
            }
            "itempoolremoveitemtype" => {
                if let Some(pool) = a_handle(args, 0).and_then(|h| self.world.item_pools.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    pool.entries.retain(|(entry, _)| *entry != id);
                }
                Value::Null
            }
            "placerandomitem" => {
                let entries = a_handle(args, 0)
                    .and_then(|h| self.world.item_pools.get(&h))
                    .map(|pool| pool.entries.clone())
                    .unwrap_or_default();
                let total: f64 = entries.iter().map(|(_, weight)| weight.max(0.0)).sum();
                if entries.is_empty() || total <= 0.0 {
                    return Ok(Value::Null);
                }
                let mut roll = self.rng.real(0.0, total);
                let mut chosen = entries.last().map(|(id, _)| id.clone()).unwrap_or_default();
                for (id, weight) in &entries {
                    if roll < *weight {
                        chosen = id.clone();
                        break;
                    }
                    roll -= weight;
                }
                let handle = self.alloc();
                self.world.items.insert(
                    handle,
                    Item {
                        id: chosen,
                        x: a_real(args, 1),
                        y: a_real(args, 2),
                        ..Item::default()
                    },
                );
                self.world.last_created_item = Some(handle);
                Value::Handle(handle)
            }
            "chooserandomitem" | "chooserandomitemex" => {
                let level = a_int(args, args.len().saturating_sub(1)) as i32;
                let candidates: Vec<String> = self
                    .world
                    .items
                    .values()
                    .filter(|item| item.level == level)
                    .map(|item| item.id.clone())
                    .collect();
                if candidates.is_empty() {
                    Value::Int(0)
                } else {
                    let index = self.rng.int(0, candidates.len() as i64 - 1) as usize;
                    Value::Int(rawcode_id(&candidates[index]))
                }
            }
            // ----------------------------------------------------------------
            // Shops / stock
            // ----------------------------------------------------------------
            "additemtostock" => {
                if let Some(unit) = a_handle(args, 0).and_then(|h| self.world.units.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    unit.stock_items.insert(
                        id,
                        StockEntry {
                            current: a_int(args, 2) as i32,
                            max: a_int(args, 3) as i32,
                        },
                    );
                }
                Value::Null
            }
            "addunittostock" => {
                if let Some(unit) = a_handle(args, 0).and_then(|h| self.world.units.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    unit.stock_units.insert(
                        id,
                        StockEntry {
                            current: a_int(args, 2) as i32,
                            max: a_int(args, 3) as i32,
                        },
                    );
                }
                Value::Null
            }
            "removeitemfromstock" => {
                if let Some(unit) = a_handle(args, 0).and_then(|h| self.world.units.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    unit.stock_items.remove(&id);
                }
                Value::Null
            }
            "removeunitfromstock" => {
                if let Some(unit) = a_handle(args, 0).and_then(|h| self.world.units.get_mut(&h)) {
                    let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                    unit.stock_units.remove(&id);
                }
                Value::Null
            }
            "additemtoallstock" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                self.world.stock_items_default.insert(
                    id,
                    StockEntry {
                        current: a_int(args, 1) as i32,
                        max: a_int(args, 2) as i32,
                    },
                );
                Value::Null
            }
            "addunittoallstock" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                self.world.stock_units_default.insert(
                    id,
                    StockEntry {
                        current: a_int(args, 1) as i32,
                        max: a_int(args, 2) as i32,
                    },
                );
                Value::Null
            }
            "removeitemfromallstock" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                self.world.stock_items_default.remove(&id);
                Value::Null
            }
            "removeunitfromallstock" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                self.world.stock_units_default.remove(&id);
                Value::Null
            }
            "setitemtypeslots" => {
                if let Some(unit) = a_handle(args, 0).and_then(|h| self.world.units.get_mut(&h)) {
                    unit.item_type_slots = a_int(args, 1) as i32;
                }
                Value::Null
            }
            "setallitemtypeslots" => {
                self.world.item_type_slots_default = a_int(args, 0) as i32;
                Value::Null
            }
            "createdestructable" => {
                let handle = self.alloc();
                self.world.destructables.insert(
                    handle,
                    Destructable {
                        id: rawcode_string(&args.first().cloned().unwrap_or(Value::Null)),
                        x: a_real(args, 1),
                        y: a_real(args, 2),
                        hp: 100.0,
                        max_hp: 100.0,
                        alive: true,
                    },
                );
                Value::Handle(handle)
            }
            "killdestructable" => {
                if let Some(handle) = a_handle(args, 0) {
                    if let Some(destructable) = self.world.destructables.get_mut(&handle) {
                        destructable.alive = false;
                        destructable.hp = 0.0;
                    }
                }
                Value::Null
            }
            "removedestructable" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.destructables.remove(&handle);
                }
                Value::Null
            }
            "getdestructablex" => Value::Real(self.destructable_get(args, |d| d.x)),
            "getdestructabley" => Value::Real(self.destructable_get(args, |d| d.y)),
            "getdestructablelife" => Value::Real(self.destructable_get(args, |d| d.hp)),
            "getdestructablemaxlife" => Value::Real(self.destructable_get(args, |d| d.max_hp)),
            "setdestructablelife" => {
                if let Some(d) = a_handle(args, 0).and_then(|h| self.world.destructables.get_mut(&h)) {
                    d.hp = a_real(args, 1);
                }
                Value::Null
            }
            "getdestructabletypeid" => {
                let id = a_handle(args, 0)
                    .and_then(|h| self.world.destructables.get(&h))
                    .map(|d| d.id.clone())
                    .unwrap_or_default();
                Value::Int(rawcode_id(&id))
            }
            "destructableshow" | "showdestructable" => Value::Null,

            // ----------------------------------------------------------------
            // Sounds / effects
            // ----------------------------------------------------------------
            "createsound" | "createsoundfilenamewithlabel" => {
                let handle = self.alloc();
                self.world.sounds.insert(
                    handle,
                    Sound {
                        path: a_str(args, 0),
                        volume: 100,
                        pitch: 1.0,
                        ..Sound::default()
                    },
                );
                Value::Handle(handle)
            }
            "createsoundfromlabel" => {
                let handle = self.alloc();
                self.world.sounds.insert(
                    handle,
                    Sound {
                        label: a_str(args, 0),
                        volume: 100,
                        pitch: 1.0,
                        ..Sound::default()
                    },
                );
                Value::Handle(handle)
            }
            "playsound" | "playsoundbj" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.playing = true;
                }
                Value::Null
            }
            "stopsound" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.playing = false;
                }
                Value::Null
            }
            "killsoundwhendone" | "killsound" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.sounds.remove(&handle);
                }
                Value::Null
            }
            "setsoundvolume" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.volume = a_int(args, 1);
                }
                Value::Null
            }
            "setsoundpitch" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.pitch = a_real(args, 1);
                }
                Value::Null
            }
            "setsoundparamsfromlabel" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.label = a_str(args, 1);
                }
                Value::Null
            }
            "setsounddistancecutoff" | "setsoundchannel" | "setsoundloop" => Value::Null,
            "attacksoundtounit" => {
                let sound = a_handle(args, 0);
                let unit = a_handle(args, 1);
                if let (Some(sound), Some(unit)) = (sound, unit) {
                    if let Some(sound) = self.world.sounds.get_mut(&sound) {
                        sound.attached = Some(unit);
                    }
                }
                Value::Null
            }
            "addspecialeffect" => {
                let handle = self.alloc();
                self.world.effects.insert(
                    handle,
                    Effect {
                        model: a_str(args, 0),
                        x: a_real(args, 1),
                        y: a_real(args, 2),
                        ..Effect::default()
                    },
                );
                Value::Handle(handle)
            }
            "addspecialeffectloc" => {
                let location = a_handle(args, 1).unwrap_or(0);
                let (x, y) = self
                    .world
                    .locations
                    .get(&location)
                    .map(|loc| (loc.x, loc.y))
                    .unwrap_or((0.0, 0.0));
                let handle = self.alloc();
                self.world.effects.insert(
                    handle,
                    Effect {
                        model: a_str(args, 0),
                        x,
                        y,
                        ..Effect::default()
                    },
                );
                Value::Handle(handle)
            }
            "addspecialeffecttarget" => {
                let handle = self.alloc();
                self.world.effects.insert(
                    handle,
                    Effect {
                        model: a_str(args, 0),
                        attached: a_handle(args, 1),
                        attachment: a_str(args, 2),
                        ..Effect::default()
                    },
                );
                Value::Handle(handle)
            }
            "destroyeffect" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.effects.remove(&handle);
                }
                Value::Null
            }
            "addspelleffect" | "addspelleffectloc" | "addspelleffecttarget" => Value::Null,

            // ----------------------------------------------------------------
            // Quests / dialogs / leaderboards / multiboards
            // ----------------------------------------------------------------
            "createquest" => {
                let handle = self.alloc();
                self.world.quests.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "destroyquest" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.quests.remove(&handle);
                }
                Value::Null
            }
            "questcreateitem" => {
                // `common.j`: QuestCreateItem(quest) returns a questitem handle;
                // the description/completed calls address that handle.
                let handle = self.alloc();
                if let Some(quest_handle) = a_handle(args, 0) {
                    if let Some(quest) = self.world.quests.get_mut(&quest_handle) {
                        let index = quest.items.len();
                        quest.items.push((String::new(), false));
                        self.world.quest_items.insert(handle, (quest_handle, index));
                    }
                }
                Value::Handle(handle)
            }
            "questsettitle" => {
                if let Some(quest) = a_handle(args, 0).and_then(|h| self.world.quests.get_mut(&h)) {
                    quest.title = a_str(args, 1);
                }
                Value::Null
            }
            "questsetdescription" => {
                if let Some(quest) = a_handle(args, 0).and_then(|h| self.world.quests.get_mut(&h)) {
                    quest.description = a_str(args, 1);
                }
                Value::Null
            }
            "questseticonpath" => {
                if let Some(quest) = a_handle(args, 0).and_then(|h| self.world.quests.get_mut(&h)) {
                    quest.icon = a_str(args, 1);
                }
                Value::Null
            }
            "questsetrequired" => {
                if let Some(quest) = a_handle(args, 0).and_then(|h| self.world.quests.get_mut(&h)) {
                    quest.required = a_bool(args, 1);
                }
                Value::Null
            }
            "questsetcompleted" | "questsetdiscovered" | "questsetfailed" => {
                if let Some(quest) = a_handle(args, 0).and_then(|h| self.world.quests.get_mut(&h)) {
                    match lower.as_str() {
                        "questsetcompleted" => quest.completed = a_bool(args, 1),
                        "questsetdiscovered" => quest.discovered = a_bool(args, 1),
                        _ => quest.failed = a_bool(args, 1),
                    }
                }
                Value::Null
            }
            "questitemsetcompleted" => {
                // `common.j`: QuestItemSetCompleted takes **questitem**, not the
                // quest. The handle table `world.quest_items` maps it back to
                // (quest, index); addressing the quest directly was a bug that
                // left every quest checklist empty.
                if let Some(handle) = a_handle(args, 0) {
                    if let Some((quest, index)) = self.world.quest_items.get(&handle).copied() {
                        if let Some(quest) = self.world.quests.get_mut(&quest) {
                            if let Some(item) = quest.items.get_mut(index) {
                                item.1 = a_bool(args, 1);
                            }
                        }
                    }
                }
                Value::Null
            }
            "questitemsetdescription" => {
                // `common.j`: QuestItemSetDescription(questitem, string).
                if let Some(handle) = a_handle(args, 0) {
                    if let Some((quest, index)) = self.world.quest_items.get(&handle).copied() {
                        if let Some(quest) = self.world.quests.get_mut(&quest) {
                            while quest.items.len() <= index {
                                quest.items.push((String::new(), false));
                            }
                            quest.items[index].0 = a_str(args, 1);
                        }
                    }
                }
                Value::Null
            }
            "dialogcreate" => {
                let handle = self.alloc();
                self.world.dialogs.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "dialogdestroy" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.dialogs.remove(&handle);
                }
                Value::Null
            }
            "dialogclear" => {
                if let Some(dialog) = a_handle(args, 0).and_then(|h| self.world.dialogs.get_mut(&h)) {
                    dialog.buttons.clear();
                }
                Value::Null
            }
            "dialogsetmessage" => {
                if let Some(dialog) = a_handle(args, 0).and_then(|h| self.world.dialogs.get_mut(&h)) {
                    dialog.message = a_str(args, 1);
                }
                Value::Null
            }
            "dialogaddbutton" | "dialogaddquitbutton" => {
                let button = self.alloc();
                if let Some(dialog) = a_handle(args, 0).and_then(|h| self.world.dialogs.get_mut(&h)) {
                    dialog.buttons.push((a_str(args, 1), button));
                }
                Value::Handle(button)
            }
            "dialogdisplay" => {
                if let Some(dialog) = a_handle(args, 1).and_then(|h| self.world.dialogs.get_mut(&h)) {
                    dialog.visible = a_bool(args, 2);
                }
                Value::Null
            }
            "getclickedbutton" | "getclickeddialog" => Value::Null,
            "createleaderboard" => {
                let handle = self.alloc();
                self.world.leaderboards.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "destroyleaderboard" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.leaderboards.remove(&handle);
                }
                Value::Null
            }
            "leaderboardadditem" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.leaderboards.get_mut(&h)) {
                    board.items.push((a_str(args, 1), a_int(args, 2), a_str(args, 3)));
                }
                Value::Null
            }
            "leaderboardsetlabel" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.leaderboards.get_mut(&h)) {
                    board.label = a_str(args, 1);
                }
                Value::Null
            }
            "leaderboarddisplay" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.leaderboards.get_mut(&h)) {
                    board.visible = a_bool(args, 1);
                }
                Value::Null
            }
            "leaderboardsetstyle" | "leaderboardsetitemstyle" | "leaderboardsetitemlabel"
            | "leaderboardsetitemvalue" | "leaderboardsetitemicon" | "leaderboardsortitems" => Value::Null,
            "createmultiboard" => {
                let handle = self.alloc();
                self.world.multiboards.insert(handle, Default::default());
                Value::Handle(handle)
            }
            "destroymultiboard" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.multiboards.remove(&handle);
                }
                Value::Null
            }
            "multiboardsettitletext" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.multiboards.get_mut(&h)) {
                    board.title = a_str(args, 1);
                }
                Value::Null
            }
            "multiboarddisplay" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.multiboards.get_mut(&h)) {
                    board.visible = a_bool(args, 1);
                }
                Value::Null
            }
            "multiboardsetrowcount" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.multiboards.get_mut(&h)) {
                    board.rows = a_int(args, 1).max(0) as u32;
                }
                Value::Null
            }
            "multiboardsetcolumncount" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.multiboards.get_mut(&h)) {
                    board.columns = a_int(args, 1).max(0) as u32;
                }
                Value::Null
            }
            "multiboardsetitemvalue" => {
                if let Some(board) = a_handle(args, 0).and_then(|h| self.world.multiboards.get_mut(&h)) {
                    board
                        .values
                        .insert((a_int(args, 1).max(0) as u32, a_int(args, 2).max(0) as u32), a_str(args, 3));
                }
                Value::Null
            }
            "multiboardsetitemstyle" | "multiboardsetitemicon" | "multiboardsetitemwidth"
            | "multiboardsetitemsstyle" | "multiboardsetitemsvalue" | "multiboardsetitemswidth"
            | "multiboardminimize" | "multiboardreleaseitem" => Value::Null,

            // ----------------------------------------------------------------
            // Camera / game / UI
            // ----------------------------------------------------------------
            "setcamerabounds" => {
                for index in 0..8 {
                    self.world.camera_bounds[index] = a_real(args, index);
                }
                Value::Null
            }
            "getcameramargin" => {
                let index = a_int(args, 0).clamp(0, 3) as usize;
                Value::Real(self.world.camera_margin[index])
            }
            "setcameraposition" => {
                self.world.camera.target_x = a_real(args, 0);
                self.world.camera.target_y = a_real(args, 1);
                self.world.camera_events.push(CameraEvent {
                    kind: "position".into(),
                    x: a_real(args, 0),
                    y: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "setcamerafield" => {
                let field = a_int(args, 0);
                let value = a_real(args, 1);
                apply_camera_field(&mut self.world.camera, field, value);
                self.world.camera_events.push(CameraEvent {
                    kind: "field".into(),
                    field: Some(field),
                    value,
                    duration: a_real(args, 2),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "pancameratotimed" | "pancameratopointtimed" => {
                self.world.camera.target_x = a_real(args, 0);
                self.world.camera.target_y = a_real(args, 1);
                self.world.camera_events.push(CameraEvent {
                    kind: "pan".into(),
                    x: a_real(args, 0),
                    y: a_real(args, 1),
                    duration: a_real(args, 2),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "pancameratopoint" => {
                // `PanCameraToPoint(location, duration)`.
                let (x, y) = a_handle(args, 0)
                    .and_then(|handle| self.world.locations.get(&handle))
                    .map(|location| (location.x, location.y))
                    .unwrap_or((0.0, 0.0));
                self.world.camera.target_x = x;
                self.world.camera.target_y = y;
                self.world.camera_events.push(CameraEvent {
                    kind: "pan".into(),
                    x,
                    y,
                    duration: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "pancameratotimedwithz" => {
                self.world.camera.target_x = a_real(args, 0);
                self.world.camera.target_y = a_real(args, 1);
                self.world.camera.target_z = a_real(args, 2);
                self.world.camera_events.push(CameraEvent {
                    kind: "pan_z".into(),
                    x: a_real(args, 0),
                    y: a_real(args, 1),
                    value: a_real(args, 2),
                    duration: a_real(args, 3),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "pancamerato" => {
                self.world.camera.target_x = a_real(args, 0);
                self.world.camera.target_y = a_real(args, 1);
                self.world.camera_events.push(CameraEvent {
                    kind: "pan".into(),
                    x: a_real(args, 0),
                    y: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "createcamerasetup" => {
                let handle = self.alloc();
                self.world
                    .camera_setups
                    .insert(handle, CameraSetup::default());
                Value::Handle(handle)
            }
            "camerasetupsetfield" => {
                // `CameraSetupSetField(whichSetup, whichField, value, duration)`.
                if let Some(handle) = a_handle(args, 0) {
                    let field = a_int(args, 1);
                    let value = a_real(args, 2);
                    if let Some(setup) = self.world.camera_setups.get_mut(&handle) {
                        setup.fields.insert(field, value);
                    }
                }
                Value::Null
            }
            "camerasetupgetfield" => {
                let field = a_int(args, 1);
                let value = a_handle(args, 0)
                    .and_then(|handle| self.world.camera_setups.get(&handle))
                    .and_then(|setup| setup.fields.get(&field).copied())
                    .unwrap_or(0.0);
                Value::Real(value)
            }
            "camerasetupsetdestposition" => {
                if let Some(handle) = a_handle(args, 0) {
                    let x = a_real(args, 1);
                    let y = a_real(args, 2);
                    if let Some(setup) = self.world.camera_setups.get_mut(&handle) {
                        setup.dest = Some((x, y));
                    }
                }
                Value::Null
            }
            "camerasetupapplyforplayer" => {
                // `CameraSetupApplyForPlayer(doPan, whichSetup, whichPlayer, duration)`.
                self.world.camera_events.push(CameraEvent {
                    kind: "setup".into(),
                    setup: a_handle(args, 1),
                    player: a_handle(args, 2),
                    value: f64::from(a_bool(args, 0) as u8),
                    duration: a_real(args, 3),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapply" => {
                // `CameraSetupApply(whichSetup, doPan, panTimed)`.
                self.world.camera_events.push(CameraEvent {
                    kind: "setup".into(),
                    setup: a_handle(args, 0),
                    value: f64::from(a_bool(args, 1) as u8),
                    duration: f64::from(a_bool(args, 2) as u8),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapplywithz" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "setup_z".into(),
                    setup: a_handle(args, 0),
                    value: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapplyforceduration" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "setup".into(),
                    setup: a_handle(args, 0),
                    value: f64::from(a_bool(args, 1) as u8),
                    duration: a_real(args, 2),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapplyforcedurationwithz" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "setup_z".into(),
                    setup: a_handle(args, 0),
                    value: a_real(args, 1),
                    duration: a_real(args, 2),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasettargetnoise" | "camerasetsourcenoise" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "noise".into(),
                    value: a_real(args, 0),
                    duration: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "setcameratargetcontroller" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "target".into(),
                    player: a_handle(args, 0),
                    x: a_real(args, 1),
                    y: a_real(args, 2),
                    value: f64::from(a_bool(args, 3) as u8),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "setcameraoocheight" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "ooc_height".into(),
                    value: a_real(args, 0),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "resettogamecamera" | "stopcamera" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "reset".into(),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "setcameraquickposition" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "position".into(),
                    x: a_real(args, 0),
                    y: a_real(args, 1),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetsmoothingfactor" => {
                self.world.camera.smoothing = a_real(args, 0);
                self.world.camera_events.push(CameraEvent {
                    kind: "smoothing".into(),
                    value: a_real(args, 0),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "getcameratargetpositionx" => Value::Real(self.world.camera.target_x),
            "getcameratargetpositiony" => Value::Real(self.world.camera.target_y),
            "getcameratargetpositionz" => Value::Real(self.world.camera.target_z),
            "getcameraeyepositionx" => Value::Real(self.world.camera.target_x),
            "getcameraeyepositiony" => Value::Real(self.world.camera.target_y),
            "getcameraeyepositionz" => {
                Value::Real(self.world.camera.target_z + self.world.camera.z_offset)
            }
            "getcamerafield" => Value::Real(camera_field(&self.world.camera, a_int(args, 0))),
            "getcameraoocheight" | "getcameraquickposition" => Value::Real(0.0),
            "camerasetupsettargetcontroller" | "camerasetupstart" | "camerasetupstop" => Value::Null,
            "gettransmissionduration" => {
                // `GetTransmissionDuration(soundHandle, timeType, timeVal)`;
                // bj_TIMETYPE_ADD = 1, SET = 0, SUB = 2. The install's sound
                // lengths are not decoded here, so a voiceover adds 0 and the
                // game layer applies a reading-time floor for the subtitle.
                let time_type = a_int(args, 1);
                let time_val = a_real(args, 2);
                // The install's voiceover lengths are not decoded by this VM, so
                // a transmission adds 0; the game layer applies a reading-time
                // floor so the subtitle is still readable.
                let sound_duration = 0.0;
                let duration = match time_type {
                    0 => time_val,
                    2 => (sound_duration - time_val).max(0.0),
                    _ => sound_duration + time_val,
                };
                Value::Real(duration)
            }
            "setmapname" => {
                self.world.map_name = a_str(args, 0);
                Value::Null
            }
            "setmapdescription" => Value::Null,
            "setplayers" | "setteams" | "setgameplacement" | "setgamedifficulty" | "setgamespeed"
            | "setdaynightmodels" | "setambientdaysound" | "setambientnightsound"
            | "setmapmusic" | "playmusic" | "stopmusic" | "settimeofdayscale"
            | "enableusercontrol" | "enableuserui" | "enableocclusion"
            | "enableworldfogboundary" | "enableweather" | "setskymodel"
            | "setsky model" | "setwaterbasecolor" | "setwaterdeforms"
            | "setterrainfog" | "setterrainfogex" | "setunit CreepGuard"
            | "define startlocation" | "defineStartLocation" => Value::Null,
            "getgamedifficulty" => Value::Int(2),
            "getgamespeed" => Value::Int(2),
            "gettimeofday" => Value::Real(self.world.time_of_day),
            "settimeofday" => {
                self.world.time_of_day = a_real(args, 0);
                Value::Null
            }
            "gettimeofdayscale" => Value::Real(1.0),
            "displaytexttoplayer" | "displaytimedtexttoplayer" | "displaytimedtextfromplayer" => {
                // `DisplayTextToPlayer(player, x, y, message)`,
                // `DisplayTimedTextToPlayer(player, x, y, duration, message)`,
                // `DisplayTimedTextFromPlayer(player, x, y, duration, message)`.
                let timed = lower != "displaytexttoplayer";
                let text = a_str(args, if timed { 4 } else { 3 });
                let duration = if timed { a_real(args, 3) } else { 0.0 };
                let player = a_handle(args, 0).unwrap_or(0);
                let name = self
                    .world
                    .players
                    .get(player as usize)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("Player {player}"));
                self.output.push(format!("{name}: {text}"));
                if !text.is_empty() {
                    let now = self.world.game_time;
                    self.world.subtitles.push(SubtitleEvent {
                        speaker: String::new(),
                        text,
                        duration,
                        player: Some(player),
                        t: now,
                        ..SubtitleEvent::default()
                    });
                }
                Value::Null
            }
            "cleartextmessages" => Value::Null,
            "createtexttag" => {
                let handle = self.alloc();
                self.world.effects.insert(handle, Effect::default());
                Value::Handle(handle)
            }
            "settexttagtext" | "settexttagpos" | "settexttagposunit" | "settexttagcolor"
            | "settexttagvelocity" | "settexttagvisibility" | "settexttagfadepoint"
            | "settexttaglifespan" | "settexttagsuspended" | "settexttagpermanent"
            | "destroytexttag" => Value::Null,
            "playcinematic" => {
                // `common.j`: PlayCinematic(string movieName). Blizzard.j's
                // `SetCinematicAvailableBJ` is the caller. The name is recorded
                // here; the game layer resolves it to one of the nine files and
                // decodes it (`wc3-game/src/cinematic.rs`), and reports
                // `MOVIE_MISSING <name>` when the install does not carry it.
                self.world.movie_requests.push(a_str(args, 0));
                Value::Null
            }
            "setcinematicscene" => {
                // `SetCinematicScene(portraitUnitId, color, speakerTitle, text,
                // sceneDuration, voiceoverDuration)` — the transmission line
                // `TransmissionFromUnitWithNameBJ` ends up calling. The portrait
                // is a unit *type* rawcode; the game layer resolves it to a live
                // unit handle for the HUD.
                let portrait = a_int(args, 0);
                let speaker = a_str(args, 2);
                let text = a_str(args, 3);
                let scene_duration = a_real(args, 4);
                let voiceover = a_real(args, 5);
                let speaker = if speaker.trim().is_empty() {
                    type_name_by_rawcode(&self.world, portrait).unwrap_or_default()
                } else {
                    speaker
                };
                self.world.subtitles.push(SubtitleEvent {
                    t: self.world.game_time,
                    speaker,
                    text,
                    duration: scene_duration,
                    portrait_unit_id: portrait,
                    color: a_int(args, 1),
                    voiceover,
                    sound: None,
                    player: None,
                });
                self.world.camera_events.push(CameraEvent {
                    kind: "scene".into(),
                    field: Some(portrait),
                    value: voiceover,
                    duration: scene_duration,
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "cinematicfadebj" => {
                self.world.camera_events.push(CameraEvent {
                    kind: "fade".into(),
                    field: Some(a_int(args, 0)),
                    value: a_real(args, 1),
                    x: a_real(args, 3),
                    y: a_real(args, 4),
                    duration: a_real(args, 5),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "setcinematiccamera" | "setcinematicaudiovisualmode"
            | "setcinematicmodeadvanced" => Value::Null,
            "addweathereffect" => Value::Null,
            "removeweathereffect" => Value::Null,
            "createfogmodifierrect" | "createfogmodifierradiusloc" | "createfogmodifierradius" => {
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "destroyfogmodifier" | "fogmodifierstart" | "fogmodifierstop" => Value::Null,
            "setfogstate" | "setfog staterect" | "setfogstateradius" => Value::Null,
            "getterrainclifflevel" | "getterrainclifflevel" => Value::Int(0),
            "setterrainpathable" | "setterraintype" | "setterrainpathable" => Value::Null,
            "createtrackable" => {
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "settrackable*" => Value::Null,
            "trackable*" => Value::Null,

            // ----------------------------------------------------------------
            // Camera setups / sound timing / presentation no-ops
            // ----------------------------------------------------------------
            "createcamerasetup" => {
                let handle = self.alloc();
                self.world.camera_setups.insert(handle, CameraSetup::default());
                Value::Handle(handle)
            }
            "camerasetupsetfield" => {
                let setup = a_handle(args, 0);
                let field = a_int(args, 1);
                let value = a_real(args, 2);
                let duration = a_real(args, 3);
                if let Some(setup) = setup.and_then(|handle| self.world.camera_setups.get_mut(&handle)) {
                    setup.fields.insert(field, value);
                }
                self.world.camera_events.push(CameraEvent {
                    kind: "setup_field".into(),
                    setup,
                    field: Some(field),
                    value,
                    duration,
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupsetdestposition" => {
                let setup = a_handle(args, 0);
                let x = a_real(args, 1);
                let y = a_real(args, 2);
                let duration = a_real(args, 3);
                if let Some(setup) = setup.and_then(|handle| self.world.camera_setups.get_mut(&handle)) {
                    setup.dest = Some((x, y));
                }
                self.world.camera_events.push(CameraEvent {
                    kind: "setup_dest".into(),
                    setup,
                    x,
                    y,
                    duration,
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapply" | "camerasetupapplycamera" | "camerasetupapplycameratimed" => {
                let setup = a_handle(args, 0);
                self.world.camera_events.push(CameraEvent {
                    kind: "apply".into(),
                    setup,
                    value: f64::from(a_bool(args, 1) as u8),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupapplyforceduration" | "camerasetupapplyforcedurationwithz" => {
                let setup = a_handle(args, 0);
                self.world.camera_events.push(CameraEvent {
                    kind: "apply".into(),
                    setup,
                    value: f64::from(a_bool(args, 1) as u8),
                    duration: a_real(args, 2),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupstart" | "camerasetupstop" | "camerasetupsettargetcontroller" => {
                let setup = a_handle(args, 0);
                self.world.camera_events.push(CameraEvent {
                    kind: "setup_event".into(),
                    setup,
                    field: Some(a_int(args, 1)),
                    ..CameraEvent::default()
                });
                Value::Null
            }
            "camerasetupdestroy" => {
                if let Some(handle) = a_handle(args, 0) {
                    self.world.camera_setups.remove(&handle);
                }
                Value::Null
            }
            "triggersleepaction" => Value::Null,
            "setsoundduration" | "setsoundposition" | "setsoundpositionbj"
            | "registerstackedsound" | "triggerwaitforsound" | "setsounddistancecutoff"
            | "setsoundchannel" | "setsoundloop" => Value::Null,
            "getsoundduration" => Value::Real(0.0),
            "startsound" => {
                if let Some(sound) = a_handle(args, 0).and_then(|h| self.world.sounds.get_mut(&h)) {
                    sound.playing = true;
                }
                Value::Null
            }
            "showinterface" => {
                // `ShowInterface(flag, fadeDuration)`: the cutscene hides the
                // console and the command card; the HUD reads this back.
                self.world.interface_shown = a_bool(args, 0);
                Value::Null
            }
            "pingminimap" | "pingminimapex" => {
                self.world
                    .pings
                    .push((a_real(args, 0), a_real(args, 1), a_real(args, 2)));
                Value::Null
            }
            "playersetleaderboard" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].leaderboard = a_handle(args, 1);
                }
                Value::Null
            }
            "displaycinefilter" | "setcinefilterblendmode" | "setcinefilterduration"
            | "setcinefilterendcolor" | "setcinefilterenduv" | "setcinefilterstartcolor"
            | "setcinefilterstartuv" | "setcinefiltertexmapflags" | "setcinefiltertexture"
            | "setcinefiltertexture" => {
                // The fade chain `CinematicFadeBJ` builds: start colour, end
                // colour, duration, then `DisplayCineFilter(true)`. Recording
                // the native state keeps the fade in the render path instead of
                // a silent no-op.
                match lower.as_str() {
                    "setcinefiltertexture" => {
                        self.world.cine_filter.texture = a_str(args, 0);
                    }
                    "setcinefilterstartcolor" => {
                        self.world.cine_filter.start = [
                            a_int(args, 0).clamp(0, 255) as u8,
                            a_int(args, 1).clamp(0, 255) as u8,
                            a_int(args, 2).clamp(0, 255) as u8,
                            a_int(args, 3).clamp(0, 255) as u8,
                        ];
                    }
                    "setcinefilterendcolor" => {
                        self.world.cine_filter.end = [
                            a_int(args, 0).clamp(0, 255) as u8,
                            a_int(args, 1).clamp(0, 255) as u8,
                            a_int(args, 2).clamp(0, 255) as u8,
                            a_int(args, 3).clamp(0, 255) as u8,
                        ];
                    }
                    "setcinefilterduration" => {
                        self.world.cine_filter.duration = a_real(args, 0);
                    }
                    "displaycinefilter" => {
                        let flag = a_bool(args, 0);
                        self.world.cine_filter.active = flag;
                        if flag {
                            self.world.camera_events.push(CameraEvent {
                                kind: "fade".into(),
                                field: Some(i64::from(self.world.cine_filter.start[3])),
                                duration: self.world.cine_filter.duration,
                                value: f64::from(self.world.cine_filter.end[3]),
                                x: f64::from(self.world.cine_filter.end[0]),
                                y: f64::from(self.world.cine_filter.end[1]),
                                ..CameraEvent::default()
                            });
                        } else {
                            self.world.camera_events.push(CameraEvent {
                                kind: "fade_end".into(),
                                ..CameraEvent::default()
                            });
                        }
                    }
                    _ => {}
                }
                Value::Null
            }
            "iscinefilterdisplayed" => Value::Bool(self.world.cine_filter.active),
            "setunitcolor" => {
                let color = a_int(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.color = color;
                }
                Value::Null
            }
            "setunitvertexcolor" | "setunitanimation" | "setunitanimationbyindex"
            | "setunitanimationwithrarity" | "queueunitanimation" | "setunitlookat"
            | "resetunitlookat" | "unitaddsleep" | "unitsetsleep"
            | "unitwakesleep" | "removeguardposition" | "setunitinvulnerable"
            | "setunitfacingtimed" | "setunittimeflow" | "setunitflyheight"
            | "setunitflyheightbj" => {
                if lower == "setunitinvulnerable" {
                    let value = a_bool(args, 1);
                    if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                        unit.invulnerable = value;
                    }
                } else if lower == "setunitfacingtimed" {
                    let facing = a_real(args, 1);
                    if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                        unit.facing = facing;
                    }
                } else if lower == "setunitacquirerange" {
                    let range = a_real(args, 1);
                    if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                        unit.acquire_range = range;
                    }
                }
                Value::Null
            }
            "getunitloc" => {
                let (x, y) = a_handle(args, 0)
                    .and_then(|handle| self.world.units.get(&handle))
                    .map(|unit| (unit.x, unit.y))
                    .unwrap_or((0.0, 0.0));
                let handle = self.alloc();
                self.world.locations.insert(handle, Location { x, y });
                Value::Handle(handle)
            }
            "forceenumplayers" | "forceenumplayerscounted" => {
                if let Some(force) = a_handle(args, 0) {
                    let players = self.world.forces.entry(force).or_default();
                    for index in 0..self.world.players.len() as u32 {
                        players.insert(index);
                    }
                }
                Value::Null
            }
            "getunitacquirerange" => Value::Real(self.unit_get(args, |unit| unit.acquire_range)),
            "setunitacquirerange" => {
                let range = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.acquire_range = range;
                }
                Value::Null
            }

            // ----------------------------------------------------------------
            // AI natives (parse/execute of .ai; headless defaults)
            // ----------------------------------------------------------------
            "debugs" | "debugfi" | "debugunitid" | "displaytext" | "displaytexti" => {
                self.output.push(a_str(args, 0));
                Value::Null
            }
            "doaiscriptdebug" => Value::Bool(false),
            "geta iplayer" | "getaiplayer" => Value::Int(0),
            "getheroids" | "getheroid" | "getherolevelai" => Value::Int(0),
            "getunitcount" | "getunitcountdone" => {
                let id = rawcode_string(&args.first().cloned().unwrap_or(Value::Null));
                let count = self
                    .world
                    .units
                    .values()
                    .filter(|unit| unit.id == id)
                    .count() as i64;
                Value::Int(count)
            }
            "getplayerunittypecount" => {
                let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let count = self
                    .world
                    .units
                    .values()
                    .filter(|unit| unit.id == id)
                    .count() as i64;
                Value::Int(count)
            }
            "gettow nunitcount" | "gettownunitcount" => Value::Int(0),
            "getunitgoldcost" | "getunitwoodcost" | "getupgradegoldcost" | "getupgradewoodcost"
            | "getupgradelevel" | "getminesowned" | "getgoldowned" | "getenemypower"
            | "getnextxpansion" | "getnext expansion" | "getmega target" => Value::Int(0),
            "townwithmine" | "townhasmine" | "townhashall" => Value::Bool(false),
            "getbuilding" | "getalliancetarget" => Value::Null,
            "setalliancetarget" | "setproduce" | "unsummon" | "setexpansion" | "setupgrade"
            | "setherolevels" | "setnewheroes" | "purchasezeppelin" | "mergeunits"
            | "setcampaignai" | "setmeleeai" | "settargetheroes" | "setdifficulty"
            | "setdefenders" | "setattackgroups" | "setattackgroup" | "setattackcondition"
            | "setattackwealth" | "setattackgold" | "setattackwood" | "setattackunit"
            | "setattackunits" | "setattackpriority" | "setairsupport" | "setcampaignai"
            | "setharassbase" | "setharassplayers" | "setharassgold" | "setharasswood"
            | "setsuicide" | "setsuicideunits" | "setbuilding" | "setbuildingunits"
            | "setexpansiò n" | "setupexpansion" | "setheroes" | "setharassunit"
            | "setharassunits" | "setharassunitfactor" | "setharassunits" => Value::Null,
            "getunitcountdone" => Value::Int(0),

            // ----------------------------------------------------------------
            // Round-2 parity: remaining common.j natives with map call sites.
            // Gameplay-visible ones mutate the deterministic world; UI/camera/
            // audio/weather/campaign controls are recorded no-ops.
            // ----------------------------------------------------------------
            "triggerregisterplayerstateevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let state = a_int(args, 2);
                let op = a_int(args, 3);
                let value = a_real(args, 4);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::GameState { state, op, value });
                }
                Value::Handle(trigger)
            }
            "triggerregisterunitstateevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let unit = a_handle(args, 1).unwrap_or(0);
                let event = a_int(args, 2);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::UnitEvent { unit, event });
                }
                Value::Handle(trigger)
            }
            "triggerregisterunitinrange" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                let unit = a_handle(args, 1).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::UnitEvent { unit, event: -1 });
                }
                Value::Handle(trigger)
            }
            "triggerregistergameevent" => {
                let trigger = a_handle(args, 0).unwrap_or(0);
                if let Some(trigger) = self.world.triggers.get_mut(&trigger) {
                    trigger.events.push(TriggerEvent::Custom);
                }
                Value::Handle(trigger)
            }
            "attachsoundtounit" => {
                let sound = a_handle(args, 0);
                let unit = a_handle(args, 1);
                if let (Some(sound), Some(unit)) = (sound, unit) {
                    if let Some(sound) = self.world.sounds.get_mut(&sound) {
                        sound.attached = Some(unit);
                    }
                }
                Value::Null
            }
            "setresourceamount" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].gold = a_int(args, 1);
                }
                Value::Null
            }
            "getresourceamount" => {
                let index = self.player_index(&args.first().cloned().unwrap_or(Value::Null)).unwrap_or(0);
                Value::Int(self.world.players.get(index).map_or(0, |player| player.gold))
            }
            "createmidisound" => {
                let handle = self.alloc();
                self.world.sounds.insert(handle, Sound {
                    path: a_str(args, 0),
                    volume: 100,
                    pitch: 1.0,
                    ..Sound::default()
                });
                Value::Handle(handle)
            }
            "getfloatgamestate" => Value::Real(0.0),
            "setfloatgamestate" => Value::Null,
            "getcameraboundminx" => Value::Real(self.world.camera_bounds[0]),
            "getcameraboundminy" => Value::Real(self.world.camera_bounds[1]),
            "getcameraboundmaxx" => Value::Real(self.world.camera_bounds[2]),
            "getcameraboundmaxy" => Value::Real(self.world.camera_bounds[3]),
            "getcameratargetpositionloc" => {
                let handle = self.alloc();
                self.world.locations.insert(handle, Location {
                    x: (self.world.camera_bounds[0] + self.world.camera_bounds[2]) * 0.5,
                    y: (self.world.camera_bounds[1] + self.world.camera_bounds[3]) * 0.5,
                });
                Value::Handle(handle)
            }
            "isfogenabled" | "isfogmaskenabled" => Value::Bool(false),
            "fogenable" | "fogmaskenable" | "resetterrainfog" | "setfogstate"
            | "setfogstaterect" | "setfogstateradius" | "enableweathereffect"
            | "setmapflag" | "suspendtimeofday" | "suspendheroxp" => Value::Null,
            "setunitmovespeed" => {
                let speed = a_real(args, 1);
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.states.insert(0x1000, speed);
                }
                Value::Null
            }
            "getunitdefaultmovespeed" => Value::Real(270.0),
            "getunitdefaultacquirerange" => Value::Real(600.0),
            "volumegroupsetvolume" => {
                self.world.group_volumes.insert(a_int(args, 0), a_real(args, 1));
                Value::Null
            }
            "volumegroupreset" => {
                self.world.group_volumes.insert(a_int(args, 0), 1.0);
                Value::Null
            }
            "volumegroupgetvolume" => {
                Value::Real(self.world.group_volumes.get(&a_int(args, 0)).copied().unwrap_or(1.0))
            }
            "playthematicmusic" | "endthematicmusic" | "clearmapmusic" | "setmusicvolume"
            | "setsounddistances"
            | "startcampaignai" | "commandai" | "setcampaignmenurace" | "preloader"
            | "setunitpathing"
            | "setunitblendtime" | "unitsuspenddecay" | "setdestructableanimation"
            | "setdestructableinvulnerable" | "setdoodadanimationrect"
            | "removeallguardpositions" | "addunitanimationproperties"
            | "flashquestdialogbutton" | "timerdialogdisplay" | "timerdialogsettitle"
            | "setmissionavailable" | "waygateactivate" | "waygatesetdestination"
            | "setblightloc" | "setunitexploded" => Value::Null,
            "unitadditemtoslotbyid" => {
                let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
                let slot = a_int(args, 2).max(0) as usize;
                let unit = a_handle(args, 0);
                let owner = unit
                    .and_then(|handle| self.world.units.get(&handle))
                    .map(|unit| unit.owner)
                    .unwrap_or(-1);
                let placed = if slot < 6 {
                    unit.is_some_and(|handle| {
                        !self
                            .world
                            .units
                            .get(&handle)
                            .is_some_and(|unit| unit.items.get(slot).copied().unwrap_or(0) != 0)
                    })
                } else {
                    false
                };
                if !placed {
                    Value::Bool(false)
                } else {
                    let item = self.alloc();
                    self.world.items.insert(
                        item,
                        Item {
                            id,
                            owner: if owner >= 0 { Some(owner as u32) } else { None },
                            held_by: unit,
                            ..Item::default()
                        },
                    );
                    if let Some(unit_handle) = unit {
                        if let Some(unit_state) = self.world.units.get_mut(&unit_handle) {
                            while unit_state.items.len() <= slot {
                                unit_state.items.push(0);
                            }
                            unit_state.items[slot] = item;
                        }
                        self.item_event(EVENT_PLAYER_UNIT_PICKUP_ITEM, unit_handle, item);
                        self.item_event(EVENT_UNIT_PICKUP_ITEM, unit_handle, item);
                    }
                    self.world.last_created_item = Some(item);
                    Value::Bool(true)
                }
            }
            "restoreunit" => {
                if let Some(unit) = self.unit_field(&args.first().cloned().unwrap_or(Value::Null)) {
                    unit.alive = true;
                    unit.hp = unit.max_hp;
                }
                Value::Null
            }
            "destructablerestorelife" => {
                if let Some(destructable) = a_handle(args, 0).and_then(|handle| self.world.destructables.get_mut(&handle)) {
                    destructable.hp = destructable.max_hp;
                    destructable.alive = true;
                }
                Value::Null
            }
            "createdestructablez" | "createdeaddestructablez" => {
                let position = (a_real(args, 2), a_real(args, 3));
                let handle = self.alloc();
                self.world.destructables.insert(handle, Destructable {
                    id: rawcode_string(&args.first().cloned().unwrap_or(Value::Null)),
                    x: position.0,
                    y: position.1,
                    hp: 100.0,
                    max_hp: 100.0,
                    alive: !lower.contains("dead"),
                });
                Value::Handle(handle)
            }
            "createcorpse" => {
                let handle = self.alloc();
                self.world.units.insert(handle, Unit {
                    id: rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null)),
                    owner: a_handle(args, 0).map_or(-1, |player| player as i32),
                    x: a_real(args, 2),
                    y: a_real(args, 3),
                    facing: a_real(args, 4),
                    hp: 0.0,
                    max_hp: 1.0,
                    alive: false,
                    ..Unit::default()
                });
                Value::Handle(handle)
            }
            "createblightedgoldmine" => {
                let handle = self.alloc();
                self.world.destructables.insert(handle, Destructable {
                    id: rawcode_string(&args.first().cloned().unwrap_or(Value::Null)),
                    x: a_real(args, 1),
                    y: a_real(args, 2),
                    hp: 100.0,
                    max_hp: 100.0,
                    alive: true,
                });
                Value::Handle(handle)
            }
            "initgamecache" => {
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "getstoredboolean" => Value::Bool(false),
            "addindicator" | "unitaddindicator" => {
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "clearselection" | "selectunit" => Value::Null,
            "enumdestructablesinrect" => Value::Null,
            "moverecttoloc" => {
                let location = a_handle(args, 1).unwrap_or(0);
                let (x, y) = self.world.locations.get(&location).map(|loc| (loc.x, loc.y)).unwrap_or((0.0, 0.0));
                if let Some(rect) = a_handle(args, 0).and_then(|handle| self.world.rects.get_mut(&handle)) {
                    let dx = x - (rect.min_x + rect.max_x) * 0.5;
                    let dy = y - (rect.min_y + rect.max_y) * 0.5;
                    rect.min_x += dx;
                    rect.max_x += dx;
                    rect.min_y += dy;
                    rect.max_y += dy;
                }
                Value::Null
            }
            "grouppointorderloc" => {
                let units: Vec<u32> = a_handle(args, 0)
                    .and_then(|handle| self.world.groups.get(&handle))
                    .map(|group| group.units.iter().copied().collect())
                    .unwrap_or_default();
                let order = a_str(args, 1);
                let location = a_handle(args, 2).unwrap_or(0);
                let point = self.world.locations.get(&location).map(|loc| (loc.x, loc.y));
                for unit in units {
                    self.world.orders.push(super::vm::ScriptOrder {
                        unit,
                        order: order.clone(),
                        point,
                        target: None,
                        id: None,
                    });
                }
                Value::Bool(true)
            }
            "forforce" => {
                let players: Vec<u32> = a_handle(args, 0)
                    .and_then(|handle| self.world.forces.get(&handle))
                    .map(|force| force.iter().copied().collect())
                    .unwrap_or_default();
                if let Some(Value::Code(code)) = args.get(1).cloned() {
                    let previous = self.world.context.player;
                    for player in players {
                        self.world.context.player = Some(player);
                        self.call_function(&code, &[], depth + 1)?;
                    }
                    self.world.context.player = previous;
                }
                Value::Null
            }
            // `ForForce` sets the enumeration player; `GetEnumPlayer` reads it
            // inside the enumerated function (common.j contract).
            "getenumplayer" => self.world.context.player.map_or(Value::Null, Value::Handle),
            "groupenumunitsinrangeofloc" => {
                let location = a_handle(args, 1).unwrap_or(0);
                let (x, y) = self.world.locations.get(&location).map(|loc| (loc.x, loc.y)).unwrap_or((0.0, 0.0));
                let radius = a_real(args, 2);
                let units: Vec<u32> = self.world.units.iter()
                    .filter(|(_, unit)| (unit.x - x).powi(2) + (unit.y - y).powi(2) <= radius * radius)
                    .map(|(handle, _)| *handle)
                    .collect();
                if let Some(group) = a_handle(args, 0) {
                    self.world.groups.entry(group).or_default().units.extend(units);
                }
                Value::Null
            }
            "leaderboardgetitemcount" => Value::Int(
                a_handle(args, 0).and_then(|handle| self.world.leaderboards.get(&handle)).map_or(0, |board| board.items.len() as i64),
            ),
            "leaderboardgetlabeltext" => Value::Str(
                a_handle(args, 0).and_then(|handle| self.world.leaderboards.get(&handle)).map_or_else(String::new, |board| board.label.clone()),
            ),
            "leaderboardgetplayerindex" | "leaderboardsetitemlabelcolor"
            | "leaderboardsetitemvaluecolor" | "leaderboardsetsizebyitemcount"
            | "leaderboardhasplayeritem" => Value::Int(0),
            "removeplayer" => {
                if let Some(index) = self.player_index(&args.first().cloned().unwrap_or(Value::Null)) {
                    self.world.players[index].slot_state = 0;
                }
                Value::Null
            }
            "isnovictorycheat" | "isnodefeatcheat" => Value::Bool(false),
            "getstartlocationloc" => {
                let handle = self.alloc();
                self.world.locations.insert(handle, Location { x: 0.0, y: 0.0 });
                Value::Handle(handle)
            }
            "getworldbounds" => {
                let handle = self.alloc();
                self.world.rects.insert(handle, Rect {
                    min_x: self.world.camera_bounds[0],
                    min_y: self.world.camera_bounds[1],
                    max_x: self.world.camera_bounds[2],
                    max_y: self.world.camera_bounds[3],
                });
                Value::Handle(handle)
            }
            "camerasetupgetdestpositionloc" => {
                let handle = self.alloc();
                self.world.locations.insert(handle, Location { x: 0.0, y: 0.0 });
                Value::Handle(handle)
            }
            "createtimerdialog" => {
                let handle = self.alloc();
                Value::Handle(handle)
            }
            "unitid" => Value::Int(a_int(args, 0)),
            "unitsharevision" | "setplayeronscorescreen" => Value::Null,

            _ => {
                self.unimplemented
                    .entry(lower.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                return Ok(self.default_native_value(&lower));
            }
        };
        // Stamp every camera/subtitle record with the script clock: the
        // campaign director and the subtitle band order their stream by these
        // times, and a native never runs outside the map's timeline.
        let now = self.world.game_time;
        for index in stamp_camera..self.world.camera_events.len() {
            self.world.camera_events[index].t = now;
            self.world.cinematic_log.push(CineBeat {
                kind: CineBeatKind::Camera(index),
                t: now,
            });
            self.world.cine_stamped_camera = index + 1;
        }
        for index in stamp_subtitle..self.world.subtitles.len() {
            self.world.subtitles[index].t = now;
            self.world.cinematic_log.push(CineBeat {
                kind: CineBeatKind::Subtitle(index),
                t: now,
            });
            self.world.cine_stamped_subtitle = index + 1;
        }
        Ok(value)
    }

    fn default_native_value(&self, name: &str) -> Value {
        match self.program.native(name) {
            Some(native) => match native.ret.to_ascii_lowercase().as_str() {
                "integer" => Value::Int(0),
                "real" => Value::Real(0.0),
                "boolean" => Value::Bool(false),
                "string" => Value::Str(String::new()),
                "nothing" => Value::Null,
                _ => Value::Null,
            },
            None => Value::Null,
        }
    }

    fn native_create_unit(&mut self, args: &[Value]) -> Value {
        let owner = a_handle(args, 0).map_or(-1, |handle| handle as i32);
        let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
        let handle = self.alloc();
        self.world.units.insert(
            handle,
            Unit {
                id: id.clone(),
                owner,
                x: a_real(args, 2),
                y: a_real(args, 3),
                facing: a_real(args, 4),
                hp: 100.0,
                max_hp: 100.0,
                mana: 0.0,
                max_mana: 0.0,
                level: 1,
                alive: true,
                name: id,
                scale: 1.0,
                time_scale: 1.0,
                ..Unit::default()
            },
        );
        self.world.last_created_unit = Some(handle);
        Value::Handle(handle)
    }

    fn native_create_unit_at_loc(&mut self, args: &[Value]) -> Value {
        let location = a_handle(args, 2).unwrap_or(0);
        let (x, y) = self
            .world
            .locations
            .get(&location)
            .map(|loc| (loc.x, loc.y))
            .unwrap_or((0.0, 0.0));
        self.native_create_unit(&[
            args.first().cloned().unwrap_or(Value::Null),
            args.get(1).cloned().unwrap_or(Value::Null),
            Value::Real(x),
            Value::Real(y),
            args.get(3).cloned().unwrap_or(Value::Real(0.0)),
        ])
    }

    fn native_add_item_by_id(&mut self, args: &[Value]) -> Value {
        let Some(unit) = a_handle(args, 0) else {
            return Value::Null;
        };
        if !self.world.units.contains_key(&unit) {
            return Value::Null;
        }
        let id = rawcode_string(&args.get(1).cloned().unwrap_or(Value::Null));
        let handle = self.alloc();
        self.world.items.insert(
            handle,
            Item {
                id,
                x: 0.0,
                y: 0.0,
                ..Item::default()
            },
        );
        if !self.attach_item(unit, handle) {
            self.world.items.remove(&handle);
            return Value::Null;
        }
        self.world.last_created_item = Some(handle);
        Value::Handle(handle)
    }

    /// Put `item` into `unit`'s inventory, respecting the six-slot capacity and
    /// firing the pickup events. Returns `false` when the unit is unknown, the
    /// inventory is full, or the item is held by a different unit.
    fn attach_item(&mut self, unit: u32, item: u32) -> bool {
        let Some(unit_state) = self.world.units.get(&unit) else {
            return false;
        };
        if unit_state.items.contains(&item) {
            return true;
        }
        if unit_state.items.iter().filter(|handle| **handle != 0).count() >= 6 {
            return false;
        }
        if !self.world.items.contains_key(&item) {
            return false;
        }
        let owner = unit_state.owner;
        let Some(item_state) = self.world.items.get_mut(&item) else {
            return false;
        };
        if item_state.held_by.is_some_and(|holder| holder != unit) {
            return false;
        }
        item_state.held_by = Some(unit);
        if item_state.owner.is_none() && owner >= 0 {
            item_state.owner = Some(owner as u32);
        }
        if let Some(unit_state) = self.world.units.get_mut(&unit) {
            if let Some(slot) = unit_state.items.iter().position(|handle| *handle == 0) {
                unit_state.items[slot] = item;
            } else {
                unit_state.items.push(item);
            }
        }
        self.item_event(EVENT_PLAYER_UNIT_PICKUP_ITEM, unit, item);
        self.item_event(EVENT_UNIT_PICKUP_ITEM, unit, item);
        true
    }

    /// Clear one inventory slot and fire the drop events. Returns the item that
    /// was held in the slot.
    fn detach_item_slot(&mut self, unit: u32, slot: usize) -> Option<u32> {
        let item = self
            .world
            .units
            .get_mut(&unit)
            .and_then(|unit_state| unit_state.items.get_mut(slot))
            .copied()
            .filter(|handle| *handle != 0)?;
        if let Some(unit_state) = self.world.units.get_mut(&unit) {
            unit_state.items[slot] = 0;
        }
        if let Some(item_state) = self.world.items.get_mut(&item) {
            item_state.held_by = None;
        }
        self.item_event(EVENT_PLAYER_UNIT_DROP_ITEM, unit, item);
        self.item_event(EVENT_UNIT_DROP_ITEM, unit, item);
        Some(item)
    }

    /// Remove `item` from `unit`'s inventory wherever it sits.
    fn detach_item(&mut self, unit: u32, item: u32) -> bool {
        let slot = self
            .world
            .units
            .get(&unit)
            .and_then(|unit_state| unit_state.items.iter().position(|handle| *handle == item));
        match slot {
            Some(slot) => self.detach_item_slot(unit, slot).is_some(),
            None => false,
        }
    }

    /// Resolve a widget handle to the typed target used by `CastTarget`.
    fn cast_target_for(&self, handle: u32) -> CastTarget {
        if self.world.units.contains_key(&handle) {
            CastTarget::Unit(handle)
        } else if self.world.items.contains_key(&handle) {
            CastTarget::Item(handle)
        } else if self.world.destructables.contains_key(&handle) {
            CastTarget::Destructable(handle)
        } else {
            CastTarget::None
        }
    }

    fn alloc_group_of_player(&mut self, player: Value) -> Value {
        let player = player.handle().map_or(-1, |h| h as i32);
        let units: Vec<u32> = self
            .world
            .units
            .iter()
            .filter(|(_, unit)| unit.owner == player)
            .map(|(handle, _)| *handle)
            .collect();
        let handle = self.alloc();
        self.world
            .groups
            .entry(handle)
            .or_default()
            .units
            .extend(units);
        Value::Handle(handle)
    }

    fn register_trigger_event<F>(&mut self, args: &[Value], make: F) -> Value
    where
        F: FnOnce(u32, i64) -> TriggerEvent,
    {
        let trigger = a_handle(args, 0).unwrap_or(0);
        let player = a_handle(args, 1).unwrap_or(0);
        let event = a_int(args, 2);
        if let Some(trigger_ref) = self.world.triggers.get_mut(&trigger) {
            trigger_ref.events.push(make(player, event));
        }
        Value::Handle(trigger)
    }

    fn unit_get(&self, args: &[Value], get: impl Fn(&Unit) -> f64) -> f64 {
        a_handle(args, 0)
            .and_then(|handle| self.world.units.get(&handle))
            .map_or(0.0, get)
    }

    fn unit_get_i(&self, args: &[Value], get: impl Fn(&Unit) -> i32) -> i64 {
        a_handle(args, 0)
            .and_then(|handle| self.world.units.get(&handle))
            .map_or(0, |unit| i64::from(get(unit)))
    }

    fn item_get(&self, args: &[Value], get: impl Fn(&Item) -> f64) -> f64 {
        a_handle(args, 0)
            .and_then(|handle| self.world.items.get(&handle))
            .map_or(0.0, get)
    }

    fn destructable_get(&self, args: &[Value], get: impl Fn(&Destructable) -> f64) -> f64 {
        a_handle(args, 0)
            .and_then(|handle| self.world.destructables.get(&handle))
            .map_or(0.0, get)
    }

    fn rect_get(&self, args: &[Value], get: impl Fn(&Rect) -> f64) -> f64 {
        a_handle(args, 0)
            .and_then(|handle| self.world.rects.get(&handle))
            .map_or(0.0, get)
    }
}

#[cfg(test)]
mod parity_tests {
    //! Per-native gates for PARITY_MATRIX #7: every arm here used to fall
    //! through to `Runtime::unimplemented`. Each test asserts the observable
    //! world mutation, not just "no error".

    use super::*;
    use crate::jass::Program;
    use crate::jass::vm::{Quest, Runtime, Sound, Trigger, TriggerEvent, Unit};

    fn runtime() -> Runtime {
        Runtime::new(Program::default(), 1)
    }

    fn insert_trigger(runtime: &mut Runtime) -> u32 {
        let handle = runtime.alloc();
        runtime.world.triggers.insert(handle, Trigger::default());
        handle
    }

    #[test]
    fn player_state_event_registers_on_trigger() {
        let mut runtime = runtime();
        let trigger = insert_trigger(&mut runtime);
        let value = runtime
            .call_native(
                "TriggerRegisterPlayerStateEvent",
                &[
                    Value::Handle(trigger),
                    Value::Handle(0),
                    Value::Int(7),
                    Value::Int(0),
                    Value::Real(100.0),
                ],
                0,
            )
            .expect("call");
        assert_eq!(value.handle(), Some(trigger));
        assert!(matches!(
            runtime.world.triggers[&trigger].events.as_slice(),
            [TriggerEvent::GameState { state: 7, value, .. }] if (*value - 100.0).abs() < f64::EPSILON
        ));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn unit_in_range_registers_unit_event() {
        let mut runtime = runtime();
        let trigger = insert_trigger(&mut runtime);
        let unit = runtime.alloc();
        runtime.world.units.insert(unit, Unit::default());
        runtime
            .call_native(
                "TriggerRegisterUnitInRange",
                &[
                    Value::Handle(trigger),
                    Value::Handle(unit),
                    Value::Real(500.0),
                    Value::Null,
                ],
                0,
            )
            .expect("call");
        assert!(matches!(
            runtime.world.triggers[&trigger].events.as_slice(),
            [TriggerEvent::UnitEvent { unit: u, event: -1 }] if *u == unit
        ));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn quest_create_item_appends_and_returns_handle() {
        let mut runtime = runtime();
        let quest = runtime.alloc();
        runtime.world.quests.insert(quest, Quest::default());
        let item = runtime
            .call_native("QuestCreateItem", &[Value::Handle(quest)], 0)
            .expect("call");
        assert!(item.handle().is_some(), "QuestCreateItem returns an item handle");
        assert_eq!(runtime.world.quests[&quest].items.len(), 1);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn set_resource_amount_writes_player_gold() {
        let mut runtime = runtime();
        runtime
            .call_native(
                "SetResourceAmount",
                &[Value::Handle(3), Value::Int(1234)],
                0,
            )
            .expect("call");
        assert_eq!(runtime.world.players[3].gold, 1234);
        let read = runtime
            .call_native("GetResourceAmount", &[Value::Handle(3)], 0)
            .expect("call");
        assert_eq!(read.as_int(), 1234);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn attach_sound_to_unit_records_attachment() {
        let mut runtime = runtime();
        let sound = runtime.alloc();
        let unit = runtime.alloc();
        runtime.world.sounds.insert(sound, Sound::default());
        runtime.world.units.insert(unit, Unit::default());
        runtime
            .call_native(
                "AttachSoundToUnit",
                &[Value::Handle(sound), Value::Handle(unit)],
                0,
            )
            .expect("call");
        assert_eq!(runtime.world.sounds[&sound].attached, Some(unit));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn camera_bounds_are_readable() {
        let mut runtime = runtime();
        runtime.world.camera_bounds = [-1000.0, -2000.0, 3000.0, 4000.0, 0.0, 0.0, 0.0, 0.0];
        for (native, expected) in [
            ("GetCameraBoundMinX", -1000.0),
            ("GetCameraBoundMinY", -2000.0),
            ("GetCameraBoundMaxX", 3000.0),
            ("GetCameraBoundMaxY", 4000.0),
        ] {
            let value = runtime.call_native(native, &[], 0).expect("call");
            assert!((value.as_real() - expected).abs() < f64::EPSILON, "{native}");
        }
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn fog_and_weather_controls_are_recorded_noops() {
        let mut runtime = runtime();
        for native in [
            "FogEnable",
            "FogMaskEnable",
            "ResetTerrainFog",
            "EnableWeatherEffect",
            "SetMapFlag",
            "SuspendTimeOfDay",
        ] {
            runtime.call_native(native, &[Value::Bool(true)], 0).expect("call");
        }
        assert!(
            runtime.unimplemented.is_empty(),
            "no-op controls must not be counted as missing: {:?}",
            runtime.unimplemented
        );
    }

    #[test]
    fn get_enum_player_reads_for_force_context() {
        let source = r#"
globals
    integer count = 0
    integer last_seen = 0
endglobals
function cb takes nothing returns nothing
    set count = count + 1
    set last_seen = GetPlayerId(GetEnumPlayer())
endfunction
native GetEnumPlayer takes nothing returns player
native GetPlayerId takes player whichPlayer returns integer
"#;
        let program = crate::jass::parse(source).expect("parse");
        let mut runtime = Runtime::new(program, 1);
        let force = runtime.alloc();
        runtime
            .world
            .forces
            .insert(force, [2u32, 5u32].into_iter().collect());
        runtime
            .call_native(
                "ForForce",
                &[Value::Handle(force), Value::Code("cb".into())],
                0,
            )
            .expect("forforce");
        assert_eq!(runtime.globals["count"].scalar(), Value::Int(2));
        assert_eq!(runtime.globals["last_seen"].scalar(), Value::Int(5));
        assert_eq!(runtime.world.context.player, None, "context restored");
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn unit_move_speed_writes_unit_state() {
        let mut runtime = runtime();
        let unit = runtime.alloc();
        runtime.world.units.insert(unit, Unit::default());
        runtime
            .call_native("SetUnitMoveSpeed", &[Value::Handle(unit), Value::Real(320.0)], 0)
            .expect("call");
        assert_eq!(runtime.world.units[&unit].states.get(&0x1000), Some(&320.0));
        assert!(runtime.unimplemented.is_empty());
    }
}

#[cfg(test)]
mod ability_item_tests {
    //! Runtime gates for PARITY #11 (hero abilities) and #14 (items/shops).
    //! Each test asserts the observable `World` state and the trigger context,
    //! not merely that a native did not fall through to `unimplemented`.

    use super::*;
    use crate::jass::Program;
    use crate::jass::vm::{
        CastTarget, EventContext, Rect, Runtime, Trigger, Unit,
        EVENT_PLAYER_UNIT_DROP_ITEM, EVENT_PLAYER_UNIT_PICKUP_ITEM,
        EVENT_PLAYER_UNIT_SPELL_EFFECT, EVENT_PLAYER_UNIT_USE_ITEM, EVENT_UNIT_SPELL_CAST,
    };

    fn runtime() -> Runtime {
        Runtime::new(Program::default(), 1)
    }

    fn source_runtime(source: &str) -> Runtime {
        Runtime::from_sources(&[("test.j", source)], 1).expect("test program parses")
    }

    fn insert_unit(runtime: &mut Runtime, owner: i32) -> u32 {
        let handle = runtime.alloc();
        runtime.world.units.insert(
            handle,
            Unit {
                id: "hfoo".into(),
                owner,
                hp: 100.0,
                max_hp: 100.0,
                alive: true,
                ..Unit::default()
            },
        );
        handle
    }

    fn insert_trigger(
        runtime: &mut Runtime,
        events: Vec<TriggerEvent>,
        actions: &[&str],
    ) -> u32 {
        let handle = runtime.alloc();
        runtime.world.triggers.insert(
            handle,
            Trigger {
                enabled: true,
                events,
                actions: actions.iter().map(|action| (*action).to_string()).collect(),
                ..Trigger::default()
            },
        );
        handle
    }

    fn create_item(runtime: &mut Runtime, id: &str, x: f64, y: f64) -> u32 {
        runtime
            .call_native(
                "CreateItem",
                &[Value::Int(rawcode_id(id)), Value::Real(x), Value::Real(y)],
                0,
            )
            .expect("create item")
            .handle()
            .expect("item handle")
    }

    fn call(runtime: &mut Runtime, name: &str, args: &[Value]) -> Value {
        runtime.call_native(name, args, 0).expect(name)
    }

    #[test]
    fn ability_level_roundtrip() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let ability = rawcode_id("AHbz");

        assert_eq!(
            call(&mut runtime, "UnitAddAbility", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "GetUnitAbilityLevel", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Int(1)
        );
        assert_eq!(
            call(
                &mut runtime,
                "SetUnitAbilityLevel",
                &[Value::Handle(unit), Value::Int(ability), Value::Int(3)]
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(&mut runtime, "IncUnitAbilityLevel", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Int(4)
        );
        assert_eq!(
            call(&mut runtime, "DecUnitAbilityLevel", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Int(3)
        );
        assert_eq!(
            call(&mut runtime, "UnitRemoveAbility", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "GetUnitAbilityLevel", &[Value::Handle(unit), Value::Int(ability)]),
            Value::Int(0)
        );
        assert_eq!(runtime.ability_level(unit, "AHbz"), 0);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn ability_permanent_and_cooldown_reset() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let ability = rawcode_id("AHbz");
        call(&mut runtime, "UnitAddAbility", &[Value::Handle(unit), Value::Int(ability)]);
        assert_eq!(
            call(
                &mut runtime,
                "UnitMakeAbilityPermanent",
                &[Value::Handle(unit), Value::Bool(true), Value::Int(ability)]
            ),
            Value::Bool(true)
        );
        let state = &runtime.world.units[&unit].abilities["AHbz"];
        assert!(state.permanent);
        runtime
            .world
            .units
            .get_mut(&unit)
            .expect("unit")
            .abilities
            .get_mut("AHbz")
            .expect("ability")
            .cooldown = 7.0;
        call(&mut runtime, "UnitResetCooldown", &[Value::Handle(unit)]);
        assert_eq!(runtime.world.units[&unit].abilities["AHbz"].cooldown, 0.0);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn ability_id_roundtrip() {
        let mut runtime = runtime();
        assert_eq!(
            call(&mut runtime, "AbilityId", &[Value::Str("AHbz".into())]),
            Value::Int(rawcode_id("AHbz"))
        );
        assert_eq!(
            call(
                &mut runtime,
                "AbilityId2String",
                &[Value::Int(rawcode_id("AHbz"))]
            ),
            Value::Str("AHbz".into())
        );
    }

    #[test]
    fn select_hero_skill_and_player_availability() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let ability = rawcode_id("AHbz");
        call(&mut runtime, "SelectHeroSkill", &[Value::Handle(unit), Value::Int(ability)]);
        call(&mut runtime, "SelectHeroSkill", &[Value::Handle(unit), Value::Int(ability)]);
        assert_eq!(runtime.ability_level(unit, "AHbz"), 2);
        call(
            &mut runtime,
            "SetPlayerAbilityAvailable",
            &[Value::Handle(0), Value::Int(ability), Value::Bool(false)],
        );
        assert!(runtime.world.players[0].disabled_abilities.contains("AHbz"));
        call(
            &mut runtime,
            "SetPlayerAbilityAvailable",
            &[Value::Handle(0), Value::Int(ability), Value::Bool(true)],
        );
        assert!(!runtime.world.players[0].disabled_abilities.contains("AHbz"));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn spell_cast_fires_effect_and_unit_triggers_with_target_context() {
        let source = r#"
native GetSpellAbilityId takes nothing returns integer
native GetSpellTargetX takes nothing returns real
native GetSpellTargetY takes nothing returns real

globals
    integer castAbility = 0
    real castX = 0.0
    real castY = 0.0
endglobals

function OnSpell takes nothing returns nothing
    set castAbility = GetSpellAbilityId()
    set castX = GetSpellTargetX()
    set castY = GetSpellTargetY()
endfunction

function CastAbility takes nothing returns integer
    return castAbility
endfunction

function CastX takes nothing returns real
    return castX
endfunction

function CastY takes nothing returns real
    return castY
endfunction
"#;
        let mut runtime = source_runtime(source);
        let unit = insert_unit(&mut runtime, 0);
        let ability = rawcode_id("AHbz");
        call(&mut runtime, "UnitAddAbility", &[Value::Handle(unit), Value::Int(ability)]);
        insert_trigger(
            &mut runtime,
            vec![TriggerEvent::PlayerUnit {
                player: 0,
                event: EVENT_PLAYER_UNIT_SPELL_EFFECT,
                unit_type: String::new(),
            }],
            &["OnSpell"],
        );
        insert_trigger(
            &mut runtime,
            vec![TriggerEvent::UnitEvent {
                unit: 0,
                event: EVENT_UNIT_SPELL_CAST,
            }],
            &["OnSpell"],
        );
        let report = runtime
            .cast_ability(unit, "AHbz", CastTarget::Point(100.0, 250.0))
            .expect("cast");
        assert_eq!(report.level, 1);
        assert!(report.triggers >= 2, "effect + any-unit cast triggers ran: {}", report.triggers);
        assert_eq!(
            runtime.run_function("CastAbility").expect("ability"),
            Value::Int(ability)
        );
        assert_eq!(
            runtime.run_function("CastX").expect("x"),
            Value::Real(100.0)
        );
        assert_eq!(
            runtime.run_function("CastY").expect("y"),
            Value::Real(250.0)
        );
        assert_eq!(runtime.world.context, EventContext::default(), "context restored");
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn spell_cast_requires_the_ability() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        assert!(runtime.cast_ability(unit, "AHbz", CastTarget::None).is_err());
    }

    #[test]
    fn item_create_and_inventory_roundtrip() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let item = create_item(&mut runtime, "phea", 10.0, 20.0);
        assert_eq!(
            call(&mut runtime, "GetItemLevel", &[Value::Handle(item)]),
            Value::Int(1)
        );
        assert_eq!(
            call(&mut runtime, "IsItemVisible", &[Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "UnitHasItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "UnitItemInSlot", &[Value::Handle(unit), Value::Int(0)]),
            Value::Handle(item)
        );
        assert_eq!(
            call(&mut runtime, "GetItemPlayer", &[Value::Handle(item)]),
            Value::Handle(0)
        );
        assert_eq!(
            call(
                &mut runtime,
                "UnitRemoveItemFromSlot",
                &[Value::Handle(unit), Value::Int(0)]
            ),
            Value::Handle(item)
        );
        assert_eq!(
            call(&mut runtime, "UnitHasItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(false)
        );
        assert_eq!(runtime.world.items[&item].held_by, None);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn inventory_rejects_a_seventh_item_and_keeps_one_stack() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        for _ in 0..6 {
            let item = create_item(&mut runtime, "phea", 0.0, 0.0);
            assert_eq!(
                call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(item)]),
                Value::Bool(true)
            );
        }
        let seventh = create_item(&mut runtime, "phea", 0.0, 0.0);
        assert_eq!(
            call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(seventh)]),
            Value::Bool(false)
        );
        assert_eq!(runtime.world.units[&unit].items.len(), 6);
        let first = runtime.world.units[&unit].items[0];
        assert_eq!(
            call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(first)]),
            Value::Bool(true)
        );
        assert_eq!(runtime.world.units[&unit].items.len(), 6);
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn item_charges_and_use_consume_then_remove() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let item = create_item(&mut runtime, "phea", 0.0, 0.0);
        call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(item)]);
        call(&mut runtime, "SetItemCharges", &[Value::Handle(item), Value::Int(2)]);
        assert_eq!(
            call(&mut runtime, "GetItemCharges", &[Value::Handle(item)]),
            Value::Int(2)
        );
        assert_eq!(
            call(&mut runtime, "UnitUseItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "GetItemCharges", &[Value::Handle(item)]),
            Value::Int(1)
        );
        assert_eq!(
            call(&mut runtime, "UnitUseItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(true)
        );
        assert!(!runtime.world.items.contains_key(&item), "last charge removes the item");
        assert_eq!(
            call(&mut runtime, "UnitItemInSlot", &[Value::Handle(unit), Value::Int(0)]),
            Value::Null
        );
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn item_events_carry_the_manipulated_item() {
        let source = r#"
globals
    integer pickups = 0
    integer uses = 0
    integer drops = 0
endglobals

function OnPickup takes nothing returns nothing
    set pickups = pickups + 1
endfunction

function OnUse takes nothing returns nothing
    set uses = uses + 1
endfunction

function OnDrop takes nothing returns nothing
    set drops = drops + 1
endfunction

function Pickups takes nothing returns integer
    return pickups
endfunction

function Uses takes nothing returns integer
    return uses
endfunction

function Drops takes nothing returns integer
    return drops
endfunction
"#;
        let mut runtime = source_runtime(source);
        let unit = insert_unit(&mut runtime, 0);
        insert_trigger(
            &mut runtime,
            vec![TriggerEvent::PlayerUnit {
                player: 0,
                event: EVENT_PLAYER_UNIT_PICKUP_ITEM,
                unit_type: String::new(),
            }],
            &["OnPickup"],
        );
        insert_trigger(
            &mut runtime,
            vec![TriggerEvent::PlayerUnit {
                player: 0,
                event: EVENT_PLAYER_UNIT_USE_ITEM,
                unit_type: String::new(),
            }],
            &["OnUse"],
        );
        insert_trigger(
            &mut runtime,
            vec![TriggerEvent::PlayerUnit {
                player: 0,
                event: EVENT_PLAYER_UNIT_DROP_ITEM,
                unit_type: String::new(),
            }],
            &["OnDrop"],
        );
        let item = call(
            &mut runtime,
            "UnitAddItemById",
            &[Value::Handle(unit), Value::Int(rawcode_id("phea"))],
        )
        .handle()
        .expect("item");
        assert_eq!(runtime.run_function("Pickups").expect("pickups"), Value::Int(1));
        call(&mut runtime, "UnitUseItem", &[Value::Handle(unit), Value::Handle(item)]);
        assert_eq!(runtime.run_function("Uses").expect("uses"), Value::Int(1));
        let second = call(
            &mut runtime,
            "UnitAddItemById",
            &[Value::Handle(unit), Value::Int(rawcode_id("phea"))],
        )
        .handle()
        .expect("second item");
        call(
            &mut runtime,
            "UnitRemoveItem",
            &[Value::Handle(unit), Value::Handle(second)],
        );
        assert_eq!(runtime.run_function("Drops").expect("drops"), Value::Int(1));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn item_flags_and_owner_are_writable() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let item = create_item(&mut runtime, "phea", 0.0, 0.0);
        call(
            &mut runtime,
            "SetItemInvulnerable",
            &[Value::Handle(item), Value::Bool(true)],
        );
        assert_eq!(
            call(&mut runtime, "IsItemInvulnerable", &[Value::Handle(item)]),
            Value::Bool(true)
        );
        call(
            &mut runtime,
            "SetItemVisible",
            &[Value::Handle(item), Value::Bool(false)],
        );
        assert_eq!(
            call(&mut runtime, "IsItemVisible", &[Value::Handle(item)]),
            Value::Bool(false)
        );
        call(
            &mut runtime,
            "SetItemDroppable",
            &[Value::Handle(item), Value::Bool(false)],
        );
        assert!(!runtime.world.items[&item].droppable);
        call(
            &mut runtime,
            "SetItemPawnable",
            &[Value::Handle(item), Value::Bool(false)],
        );
        assert_eq!(
            call(&mut runtime, "IsItemPawnable", &[Value::Handle(item)]),
            Value::Bool(false)
        );
        call(
            &mut runtime,
            "SetItemPlayer",
            &[Value::Handle(item), Value::Handle(3), Value::Bool(false)],
        );
        assert_eq!(
            call(&mut runtime, "GetItemPlayer", &[Value::Handle(item)]),
            Value::Handle(3)
        );
        call(
            &mut runtime,
            "SetItemDropID",
            &[Value::Handle(item), Value::Int(rawcode_id("hfoo"))],
        );
        assert_eq!(runtime.world.items[&item].drop_id, "hfoo");
        assert_eq!(runtime.world.items[&item].held_by, None);
        assert_eq!(
            call(&mut runtime, "UnitAddItem", &[Value::Handle(unit), Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "IsItemOwned", &[Value::Handle(item)]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, "GetItemTypeId", &[Value::Handle(item)]),
            Value::Int(rawcode_id("phea"))
        );
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn shop_stock_and_slots_are_recorded() {
        let mut runtime = runtime();
        let unit = insert_unit(&mut runtime, 0);
        let item_id = rawcode_id("phea");
        let unit_id = rawcode_id("hfoo");
        call(
            &mut runtime,
            "AddItemToStock",
            &[
                Value::Handle(unit),
                Value::Int(item_id),
                Value::Int(2),
                Value::Int(5),
            ],
        );
        call(
            &mut runtime,
            "AddUnitToStock",
            &[
                Value::Handle(unit),
                Value::Int(unit_id),
                Value::Int(1),
                Value::Int(4),
            ],
        );
        let stock = &runtime.world.units[&unit];
        assert_eq!(stock.stock_items["phea"], StockEntry { current: 2, max: 5 });
        assert_eq!(stock.stock_units["hfoo"], StockEntry { current: 1, max: 4 });
        call(
            &mut runtime,
            "RemoveItemFromStock",
            &[Value::Handle(unit), Value::Int(item_id)],
        );
        assert!(!runtime.world.units[&unit].stock_items.contains_key("phea"));
        call(
            &mut runtime,
            "SetItemTypeSlots",
            &[Value::Handle(unit), Value::Int(3)],
        );
        call(&mut runtime, "SetAllItemTypeSlots", &[Value::Int(4)]);
        assert_eq!(runtime.world.units[&unit].item_type_slots, 3);
        assert_eq!(runtime.world.item_type_slots_default, 4);
        call(
            &mut runtime,
            "AddItemToAllStock",
            &[Value::Int(item_id), Value::Int(1), Value::Int(3)],
        );
        assert_eq!(
            runtime.world.stock_items_default["phea"],
            StockEntry { current: 1, max: 3 }
        );
        call(&mut runtime, "RemoveItemFromAllStock", &[Value::Int(item_id)]);
        assert!(runtime.world.stock_items_default.is_empty());
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn item_pool_places_a_known_type_at_the_requested_point() {
        let mut runtime = runtime();
        let item_id = rawcode_id("phea");
        let pool = call(&mut runtime, "CreateItemPool", &[])
            .handle()
            .expect("pool");
        call(
            &mut runtime,
            "ItemPoolAddItemType",
            &[Value::Handle(pool), Value::Int(item_id), Value::Real(1.0)],
        );
        let placed = call(
            &mut runtime,
            "PlaceRandomItem",
            &[Value::Handle(pool), Value::Real(10.0), Value::Real(20.0)],
        )
        .handle()
        .expect("placed item");
        let item = &runtime.world.items[&placed];
        assert_eq!(item.id, "phea");
        assert_eq!((item.x, item.y), (10.0, 20.0));
        assert_eq!(
            call(&mut runtime, "ChooseRandomItem", &[Value::Int(1)]),
            Value::Int(item_id)
        );
        call(
            &mut runtime,
            "ItemPoolRemoveItemType",
            &[Value::Handle(pool), Value::Int(item_id)],
        );
        assert!(runtime.world.item_pools[&pool].entries.is_empty());
        call(&mut runtime, "DestroyItemPool", &[Value::Handle(pool)]);
        assert!(!runtime.world.item_pools.contains_key(&pool));
        assert!(runtime.unimplemented.is_empty());
    }

    #[test]
    fn enum_items_in_rect_calls_the_action_for_items_inside_only() {
        let source = r#"
globals
    integer seen = 0
endglobals

function OnItem takes nothing returns nothing
    set seen = seen + 1
endfunction

function Seen takes nothing returns integer
    return seen
endfunction
"#;
        let mut runtime = source_runtime(source);
        let rect = runtime.alloc();
        runtime.world.rects.insert(
            rect,
            Rect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            },
        );
        create_item(&mut runtime, "phea", 10.0, 10.0);
        create_item(&mut runtime, "phea", 1000.0, 1000.0);
        call(
            &mut runtime,
            "EnumItemsInRect",
            &[
                Value::Handle(rect),
                Value::Null,
                Value::Code("OnItem".into()),
            ],
        );
        assert_eq!(runtime.run_function("Seen").expect("seen"), Value::Int(1));
        assert_eq!(runtime.world.context.enum_item, None);
        assert!(runtime.unimplemented.is_empty());
    }
}
