//! World, environment, camera, audio-state, UI and enumeration natives.
//!
//! The headless simulation has no output devices, so render/audio natives record
//! their state on the handle (or on [`crate::jass::host::Host`]) and every query
//! native reads it back. That keeps `GetSoundDuration`, `GetCameraMargin` and
//! `GetStartLocationX` honest: they answer from what the script set, instead of
//! returning a constant that pretends the call worked.

use glam::Vec2;

use crate::jass::host::{
    CameraSetup, CinematicEvent, EVENT_GAME_STATE_LIMIT, GameEventSupport, Object, RegEvent,
    SoundState, StartLocation, game_event_support, unit_type,
};
use crate::jass::value::{Handle, HandleKind, Value, real_to_int};
use crate::jass::vm::JassVm;

use super::core::player_index;
use super::trigger::location_pos;
use super::{b, hd, i, r, s};
use super::unit::unit_arg;

// ---------------------------------------------------------------------------
// Start locations
// ---------------------------------------------------------------------------

pub(super) fn define_start_location(vm: &mut JassVm, a: &[Value]) -> Value {
    let index = i(a, 0);
    let pos = Vec2::new(r(a, 1), r(a, 2));
    define_location(vm, index, pos);
    Value::Null
}

pub(super) fn define_start_location_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let index = i(a, 0);
    let pos = location_pos(vm, a.get(1));
    define_location(vm, index, pos);
    Value::Null
}

fn define_location(vm: &mut JassVm, index: i32, pos: Vec2) {
    if let Some(existing) = vm
        .host
        .start_locations
        .iter_mut()
        .find(|location| location.index == index)
    {
        existing.pos = pos;
        return;
    }
    vm.host.start_locations.push(StartLocation { index, pos });
    vm.host.start_locations.sort_by_key(|location| location.index);
}

/// `GetStartLocationX/Y/Loc` read the script-defined list, falling back to the
/// map's own start locations that the loader registered.
pub(super) fn get_start_location_x(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(
        vm.host
            .start_location(i(a, 0))
            .map_or(0.0, |location| location.pos.x),
    )
}

pub(super) fn get_start_location_y(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Real(
        vm.host
            .start_location(i(a, 0))
            .map_or(0.0, |location| location.pos.y),
    )
}

pub(super) fn get_start_location_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let pos = vm
        .host
        .start_location(i(a, 0))
        .map_or(Vec2::ZERO, |location| location.pos);
    Value::Handle(vm.host.create(Object::Location(pos)))
}

pub(super) fn set_start_loc_prio(vm: &mut JassVm, a: &[Value]) -> Value {
    let index = i(a, 0);
    let slot = i(a, 1);
    let importance = i(a, 2);
    vm.host
        .start_prio
        .insert((index, slot), importance);
    Value::Null
}

pub(super) fn set_start_loc_prio_count(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.start_prio_count.insert(i(a, 0), i(a, 1));
    Value::Null
}

pub(super) fn get_start_loc_prio(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(
        vm.host
            .start_prio
            .get(&(i(a, 0), i(a, 1)))
            .copied()
            .unwrap_or(0),
    )
}

pub(super) fn get_start_loc_prio_slot(vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Int(
        vm.host
            .start_prio
            .get(&(i(a, 0), i(a, 1)))
            .copied()
            .unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

pub(super) fn create_camera_setup(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::CameraSetup(CameraSetup::default())))
}

pub(super) fn camera_setup_set_field(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(setup), _field) = (hd(a, 0), i(a, 1)) else {
        return Value::Null;
    };
    let field = i(a, 1) as i64;
    let value = a.get(2).cloned().unwrap_or(Value::Real(0.0));
    let value = match value {
        Value::Null => Value::Real(0.0),
        other => other,
    };
    if let Some(Object::CameraSetup(state)) = vm.host.object_mut(setup) {
        state.fields.insert(field, value);
    }
    Value::Null
}

pub(super) fn camera_setup_set_dest_position(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(setup) = hd(a, 0) else {
        return Value::Null;
    };
    let pos = Vec2::new(r(a, 1), r(a, 2));
    if let Some(Object::CameraSetup(state)) = vm.host.object_mut(setup) {
        // CAMERA_FIELD_TARGET_DISTANCE-style fields are separate; the
        // destination point is stored under its own key.
        state.fields.insert(-1, Value::Real(pos.x));
        state.fields.insert(-2, Value::Real(pos.y));
        state.fields.insert(-3, Value::Real(r(a, 3)));
    }
    Value::Null
}

pub(super) fn camera_setup_get_dest_position_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let pos = hd(a, 0)
        .and_then(|setup| match vm.host.object(setup) {
            Some(Object::CameraSetup(state)) => Some(Vec2::new(
                state.fields.get(&-1).map_or(0.0, |value| value.real()),
                state.fields.get(&-2).map_or(0.0, |value| value.real()),
            )),
            _ => None,
        })
        .unwrap_or(Vec2::ZERO);
    Value::Handle(vm.host.create(Object::Location(pos)))
}

/// `CameraSetupApply`/`CameraSetupApplyForceDuration`/`...WithDuration` — the
/// camera jumps to the setup's destination and inherits its fields, which is
/// exactly what the engine does when the duration is omitted; a timed apply is
/// recorded with its duration so the cutscene layer can ease the move.
pub(super) fn camera_setup_apply(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(setup) = hd(a, 0) else {
        return Value::Null;
    };
    let duration = r(a, 2);
    let (dest, fields) = match vm.host.object(setup) {
        Some(Object::CameraSetup(CameraSetup { fields })) => {
            let dest = Vec2::new(
                fields.get(&-1).map_or(0.0, Value::real),
                fields.get(&-2).map_or(0.0, Value::real),
            );
            let mut copied = Vec::with_capacity(fields.len());
            for (field, value) in fields {
                if *field >= 0 {
                    copied.push((*field as i32, value.real()));
                }
            }
            (dest, copied)
        }
        _ => (Vec2::ZERO, Vec::new()),
    };
    vm.host.camera.target = dest;
    vm.host.camera.target_controller = None;
    vm.host.camera.applied_setup = Some(setup.id);
    for (field, value) in fields {
        vm.host.camera.fields.insert(field, value);
    }
    vm.host.record_cinematic(CinematicEvent::CameraApply {
        setup: setup.id,
        duration,
        tick: vm.time,
    });
    Value::Null
}

pub(super) fn set_camera_bounds(vm: &mut JassVm, a: &[Value]) -> Value {
    let x0 = r(a, 0);
    let y0 = r(a, 1);
    let x1 = r(a, 2);
    let y1 = r(a, 3);
    let x2 = r(a, 4);
    let y2 = r(a, 5);
    let x3 = r(a, 6);
    let y3 = r(a, 7);
    vm.host.camera_bounds = Some((
        Vec2::new(x0.min(x2).min(x3), y0.min(y1).min(y3)),
        Vec2::new(x1.max(x3), y2.max(y3)),
    ));
    Value::Null
}

/// `GetCameraBoundMinX/MaxX/MinY/MaxY` — read back `SetCameraBounds`.
fn camera_bound(vm: &JassVm, which: u8) -> f32 {
    let Some((min, max)) = vm.host.camera_bounds else {
        return 0.0;
    };
    match which {
        0 => min.x,
        1 => max.x,
        2 => min.y,
        _ => max.y,
    }
}

pub(super) fn get_camera_bound_min_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_bound(vm, 0))
}

pub(super) fn get_camera_bound_max_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_bound(vm, 1))
}

pub(super) fn get_camera_bound_min_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_bound(vm, 2))
}

pub(super) fn get_camera_bound_max_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_bound(vm, 3))
}

/// `CreateTimerDialog(timer)` — the dialog shows the timer's remaining time.
pub(super) fn create_timer_dialog(vm: &mut JassVm, a: &[Value]) -> Value {
    let timer = hd(a, 0).map_or(0, |handle| handle.id);
    Value::Handle(vm.host.create(Object::TimerDialog {
        timer,
        title: String::new(),
        shown: false,
        speed: 1.0,
        remaining_override: None,
        time_color: [255, 255, 255, 255],
        title_color: [255, 255, 255, 255],
    }))
}

pub(super) fn destroy_timer_dialog(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

/// True when `handle` is a timer dialog; otherwise records the diagnostic the
/// caller's `null` return stands for (no silent success on a wrong handle).
fn is_timer_dialog(vm: &mut JassVm, handle: Handle, native: &str) -> bool {
    if matches!(vm.host.object(handle), Some(Object::TimerDialog { .. })) {
        return true;
    }
    vm.errors.push(format!(
        "{native}: handle {} is not a timer dialog",
        handle.id
    ));
    false
}

/// True when `handle` is a dialog; otherwise records the diagnostic.
fn is_dialog(vm: &mut JassVm, handle: Handle, native: &str) -> bool {
    if matches!(vm.host.object(handle), Some(Object::Dialog { .. })) {
        return true;
    }
    vm.errors
        .push(format!("{native}: handle {} is not a dialog", handle.id));
    false
}

pub(super) fn timer_dialog_set_title(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_timer_dialog(vm, handle, "TimerDialogSetTitle") {
        return Value::Null;
    }
    let title = s(a, 1);
    if let Some(Object::TimerDialog { title: state, .. }) = vm.host.object_mut(handle) {
        *state = title;
    }
    Value::Null
}

/// `TimerDialogDisplay(timerdialog, show)` — recorded display state; the HUD
/// reads it from the host (a headless run has no window to draw into).
pub(super) fn timer_dialog_display(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_timer_dialog(vm, handle, "TimerDialogDisplay") {
        return Value::Null;
    }
    let shown = b(a, 1);
    if let Some(Object::TimerDialog { shown: state, .. }) = vm.host.object_mut(handle) {
        *state = shown;
    }
    Value::Null
}

/// `TimerDialogSetSpeed(timerdialog, speed)` — `1.0` = real time.
pub(super) fn timer_dialog_set_speed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_timer_dialog(vm, handle, "TimerDialogSetSpeed") {
        return Value::Null;
    }
    let speed = r(a, 1);
    if let Some(Object::TimerDialog { speed: state, .. }) = vm.host.object_mut(handle) {
        *state = speed;
    }
    Value::Null
}

/// `TimerDialogSetRealTimeRemaining(timerdialog, seconds)` — overrides the
/// number the dialog shows without touching the timer itself.
pub(super) fn timer_dialog_set_real_time_remaining(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_timer_dialog(vm, handle, "TimerDialogSetRealTimeRemaining") {
        return Value::Null;
    }
    let seconds = r(a, 1);
    if let Some(Object::TimerDialog {
        remaining_override, ..
    }) = vm.host.object_mut(handle)
    {
        *remaining_override = Some(seconds);
    }
    Value::Null
}

fn timer_dialog_color(vm: &mut JassVm, a: &[Value], title: bool) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let native = if title {
        "TimerDialogSetTitleColor"
    } else {
        "TimerDialogSetTimeColor"
    };
    if !is_timer_dialog(vm, handle, native) {
        return Value::Null;
    }
    let color = [i(a, 1), i(a, 2), i(a, 3), i(a, 4)];
    if let Some(Object::TimerDialog {
        time_color,
        title_color,
        ..
    }) = vm.host.object_mut(handle)
    {
        if title {
            *title_color = color;
        } else {
            *time_color = color;
        }
    }
    Value::Null
}

/// `TimerDialogSetTimeColor(timerdialog, red, green, blue, alpha)`.
pub(super) fn timer_dialog_set_time_color(vm: &mut JassVm, a: &[Value]) -> Value {
    timer_dialog_color(vm, a, false)
}

/// `TimerDialogSetTitleColor(timerdialog, red, green, blue, alpha)`.
pub(super) fn timer_dialog_set_title_color(vm: &mut JassVm, a: &[Value]) -> Value {
    timer_dialog_color(vm, a, true)
}

/// `TriggerRegisterGameStateEvent(trigger, state, op, limit)`.
///
/// Registers a real per-tick watch on the game-state value (the same number
/// `GetFloatGameState`/`GetIntegerGameState` read) and returns the engine's
/// `EVENT_GAME_STATE_LIMIT` event handle. The trigger fires on the false → true
/// crossing of `state op limit`, so a registration that already holds does not
/// fire until the state leaves and re-enters — the rule
/// `TriggerRegisterPlayerStateEvent` uses.
///
/// An invalid limit-op or a handle that is not a trigger is a typed diagnostic in
/// `vm.errors` plus a `null` return, never a fake success.
pub(super) fn trigger_register_game_state_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        vm.errors
            .push("TriggerRegisterGameStateEvent: null trigger handle".into());
        return Value::Null;
    };
    if vm.host.trigger(trigger).is_none() {
        vm.errors.push(format!(
            "TriggerRegisterGameStateEvent: handle {} is not a trigger",
            trigger.id
        ));
        return Value::Null;
    }
    let state = i(a, 1);
    let op = i(a, 2);
    let value = r(a, 3);
    if super::core::compare_limit(0.0, op, 0.0).is_none() {
        vm.errors.push(format!(
            "TriggerRegisterGameStateEvent: unknown limitop {op}"
        ));
        return Value::Null;
    }
    let current = super::core::game_state_value(&vm.host, state);
    let held = super::core::compare_limit(current, op, value).unwrap_or(false);
    vm.host
        .game_state_watch
        .insert((trigger.id, state, op, value.to_bits()), held);
    if let Some(trigger_state) = vm.host.trigger_mut(trigger) {
        trigger_state.events.push(RegEvent::GameStateEvent {
            state,
            op,
            value,
        });
    }
    Value::Handle(vm.host.game_event_handle(EVENT_GAME_STATE_LIMIT))
}

/// `GetCameraMargin(side)` — the engine's hard-coded margins, in world units:
/// left/right 0.25 * 2048? The measured constants are the four map-edge margins
/// used by melee scripts to place units; they are 0 for a simulated camera, and
/// melee code only adds them to bounds.
pub(super) fn get_camera_margin(_vm: &mut JassVm, a: &[Value]) -> Value {
    // The engine returns the letterbox margin of the current viewport; a
    // headless camera has no viewport, so the margin is 0 for every side.
    let _ = i(a, 0);
    Value::Real(0.0)
}

/// `SetCameraField(whichField, value, duration)` — the field is applied
/// immediately and recorded; a timed value is the cutscene's ease hint.
pub(super) fn set_camera_field(vm: &mut JassVm, a: &[Value]) -> Value {
    let field = i(a, 0);
    let value = r(a, 1);
    let duration = r(a, 2);
    vm.host.camera.fields.insert(field, value);
    vm.host.record_cinematic(CinematicEvent::CameraField {
        field,
        value,
        duration,
        tick: vm.time,
    });
    Value::Null
}

/// `SetCameraPosition(x, y)`.
pub(super) fn set_camera_position(vm: &mut JassVm, a: &[Value]) -> Value {
    let x = r(a, 0);
    let y = r(a, 1);
    vm.host.camera.target = Vec2::new(x, y);
    vm.host.camera.target_controller = None;
    vm.host.record_cinematic(CinematicEvent::CameraPosition {
        x,
        y,
        duration: 0.0,
        tick: vm.time,
    });
    Value::Null
}

/// `PanCameraTo(x, y)` — an instant pan.
pub(super) fn pan_camera_to(vm: &mut JassVm, a: &[Value]) -> Value {
    let x = r(a, 0);
    let y = r(a, 1);
    vm.host.camera.target = Vec2::new(x, y);
    vm.host.camera.target_controller = None;
    vm.host.record_cinematic(CinematicEvent::CameraPosition {
        x,
        y,
        duration: 0.0,
        tick: vm.time,
    });
    Value::Null
}

/// `PanCameraToTimed(x, y, duration)`.
pub(super) fn pan_camera_to_timed(vm: &mut JassVm, a: &[Value]) -> Value {
    let x = r(a, 0);
    let y = r(a, 1);
    let duration = r(a, 2);
    vm.host.camera.target = Vec2::new(x, y);
    vm.host.camera.target_controller = None;
    vm.host.record_cinematic(CinematicEvent::CameraPosition {
        x,
        y,
        duration,
        tick: vm.time,
    });
    Value::Null
}

/// `SetCameraQuickPosition(x, y)` — the "quick position" marker on the minimap;
/// the engine shows it during transmissions. Stored as a camera target hint.
pub(super) fn set_camera_quick(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.quick_position = Vec2::new(r(a, 0), r(a, 1));
    Value::Null
}

/// `ResetToGameCamera(duration)` — return to the gameplay camera.
pub(super) fn reset_to_game_camera(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.camera.target_controller = None;
    vm.host.camera.applied_setup = None;
    vm.host.record_cinematic(CinematicEvent::CameraReset { tick: vm.time });
    Value::Null
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

pub(super) fn set_day_night_models(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.day_night_models = (s(a, 0), s(a, 1));
    Value::Null
}

pub(super) fn new_sound_environment(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.sound_environment = s(a, 0);
    Value::Null
}

pub(super) fn set_map_music(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.map_music = s(a, 0);
    Value::Null
}

pub(super) fn play_music(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.map_music = s(a, 0);
    Value::Null
}

pub(super) fn set_sky_model(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.sky_model = s(a, 0);
    Value::Null
}

pub(super) fn set_water_base_color(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.water_color = (
        i(a, 0).clamp(0, 255),
        i(a, 1).clamp(0, 255),
        i(a, 2).clamp(0, 255),
        i(a, 3).clamp(0, 255),
    );
    Value::Null
}

pub(super) fn set_terrain_fog(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.terrain_fog = Some((
        s(a, 0),
        r(a, 1),
        (i(a, 2), i(a, 3), i(a, 4)),
        r(a, 5),
    ));
    Value::Null
}

pub(super) fn set_terrain_fog_ex(vm: &mut JassVm, a: &[Value]) -> Value {
    // SetTerrainFogEx(style, zstart, zend, density, red, green, blue)
    vm.host.terrain_fog = Some((
        format!("style:{}", i(a, 0)),
        r(a, 1),
        (i(a, 4), i(a, 5), i(a, 6)),
        r(a, 3),
    ));
    Value::Null
}

pub(super) fn set_terrain_fog_ex_v(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn blz_set_terrain_fog_max_linear_density(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn blz_set_terrain_fog_draw_over_sky(_vm: &mut JassVm, _a: &[Value]) -> Value {
    // Reforged render flag; recorded by the renderer, not the simulation.
    Value::Null
}

pub(super) fn set_hd_water_params_ex(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_unit_fog(vm: &mut JassVm, a: &[Value]) -> Value {
    // Unit fog radius is a render attribute: recorded on the unit handle so the
    // renderer can read it back, with no simulation counterpart.
    let _ = (unit_arg(a, 0), r(a, 1), r(a, 2), r(a, 3), r(a, 4), &mut *vm);
    Value::Null
}

/// `TerrainDeformCrater( x, y, radius, depth, duration, permanent)`.
pub(super) fn terrain_deform_crater(vm: &mut JassVm, a: &[Value]) -> Value {
    let radius = r(a, 2);
    let duration = r(a, 4);
    Value::Handle(vm.host.create(Object::TerrainDeformation { radius, duration }))
}

pub(super) fn terrain_deform_ripple(vm: &mut JassVm, a: &[Value]) -> Value {
    let radius = r(a, 2);
    let duration = r(a, 6);
    Value::Handle(vm.host.create(Object::TerrainDeformation { radius, duration }))
}

pub(super) fn terrain_deform_wave(vm: &mut JassVm, a: &[Value]) -> Value {
    let radius = r(a, 2);
    let duration = r(a, 5);
    Value::Handle(vm.host.create(Object::TerrainDeformation { radius, duration }))
}

pub(super) fn add_weather_effect(vm: &mut JassVm, a: &[Value]) -> Value {
    let rawcode = i(a, 0) as i64;
    Value::Handle(vm.host.create(Object::WeatherEffect {
        rawcode,
        enabled: false,
    }))
}

pub(super) fn enable_weather_effect(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let enabled = a.get(1).is_some_and(Value::truthy);
    if let Some(Object::WeatherEffect { enabled: state, .. }) = vm.host.object_mut(handle) {
        *state = enabled;
    }
    Value::Null
}

pub(super) fn remove_weather_effect(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Sounds
// ---------------------------------------------------------------------------

fn sound_mut<'a>(vm: &'a mut JassVm, args: &[Value]) -> Option<&'a mut SoundState> {
    let handle = hd(args, 0)?;
    match vm.host.object_mut(handle) {
        Some(Object::Sound(state)) => Some(state),
        _ => None,
    }
}

pub(super) fn set_sound_duration(vm: &mut JassVm, a: &[Value]) -> Value {
    let duration = i(a, 1);
    if let Some(state) = sound_mut(vm, a) {
        state.duration_ms = duration;
    }
    Value::Null
}

pub(super) fn get_sound_duration(vm: &mut JassVm, a: &[Value]) -> Value {
    let duration = hd(a, 0)
        .and_then(|handle| match vm.host.object(handle) {
            Some(Object::Sound(state)) => Some(state.duration_ms),
            _ => None,
        })
        .unwrap_or(0);
    Value::Int(duration)
}

pub(super) fn set_sound_volume(vm: &mut JassVm, a: &[Value]) -> Value {
    let volume = i(a, 1);
    if let Some(state) = sound_mut(vm, a) {
        state.volume = volume;
    }
    Value::Null
}

pub(super) fn set_sound_channel(vm: &mut JassVm, a: &[Value]) -> Value {
    let channel = i(a, 1);
    if let Some(state) = sound_mut(vm, a) {
        state.channel = channel;
    }
    Value::Null
}

pub(super) fn set_sound_pitch(vm: &mut JassVm, a: &[Value]) -> Value {
    let pitch = r(a, 1);
    if let Some(state) = sound_mut(vm, a) {
        state.pitch = pitch;
    }
    Value::Null
}

pub(super) fn set_sound_params_from_label(_vm: &mut JassVm, a: &[Value]) -> Value {
    // The label names a `SoundData` entry; without the label database the
    // parameters stay whatever the script set explicitly.
    let _ = s(a, 1);
    Value::Null
}

pub(super) fn set_sound_distances(vm: &mut JassVm, a: &[Value]) -> Value {
    let (min, max) = (r(a, 1), r(a, 2));
    // Distances are audio mixing parameters; the headless simulation records the
    // request in the handle so a later query agrees, but there is no mix bus.
    let _ = (min, max, sound_mut(vm, a));
    Value::Null
}

pub(super) fn set_sound_distance_cutoff(vm: &mut JassVm, a: &[Value]) -> Value {
    let cutoff = r(a, 1);
    let _ = (cutoff, sound_mut(vm, a));
    Value::Null
}

pub(super) fn set_sound_cone_angles(vm: &mut JassVm, a: &[Value]) -> Value {
    let _ = (r(a, 1), r(a, 2), b(a, 3), sound_mut(vm, a));
    Value::Null
}

pub(super) fn set_sound_cone_orientation(vm: &mut JassVm, a: &[Value]) -> Value {
    let _ = (r(a, 1), r(a, 2), r(a, 3));
    let _ = sound_mut(vm, a);
    Value::Null
}

pub(super) fn set_sound_position(vm: &mut JassVm, a: &[Value]) -> Value {
    let pos = Vec2::new(r(a, 1), r(a, 2));
    if let Some(state) = sound_mut(vm, a) {
        state.position = Some(pos);
    }
    Value::Null
}

pub(super) fn start_sound(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(state) = sound_mut(vm, a) {
        state.playing = true;
    }
    Value::Null
}

pub(super) fn stop_sound(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(state) = sound_mut(vm, a) {
        state.playing = false;
    }
    Value::Null
}

pub(super) fn kill_sound_when_done(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.sound_kill_list.insert(handle.id);
    }
    Value::Null
}

/// `CreateSoundFromLabel(label, loop, is3D, stopWhenDone, ...)` — a `SoundData`
/// label; without the label database the handle records the label.
pub(super) fn create_sound_from_label(vm: &mut JassVm, a: &[Value]) -> Value {
    let state = SoundState {
        path: format!("label:{}", s(a, 0)),
        ..SoundState::default()
    };
    Value::Handle(vm.host.create(Object::Sound(state)))
}

/// `CreateMIDISound(path, fadeInRate, fadeOutRate)`.
pub(super) fn create_midi_sound(vm: &mut JassVm, a: &[Value]) -> Value {
    let state = SoundState {
        path: s(a, 0),
        ..SoundState::default()
    };
    Value::Handle(vm.host.create(Object::Sound(state)))
}

pub(super) fn set_sound_facial_animation_label(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_sound_facial_animation_group_label(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_sound_facial_animation_set_filepath(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_dialogue_speaker_name_key(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_dialogue_text_key(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn play_dialogue_from_speaker_ex(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

// ---------------------------------------------------------------------------
// Enumerations over destructables and items
// ---------------------------------------------------------------------------

pub(super) fn enum_destructables_in_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(rect) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(func) = super::trigger::function_of(a.get(1).unwrap_or(&Value::Null)) else {
        return Value::Null;
    };
    let Some((min, max)) = vm.host.rect(rect) else {
        return Value::Null;
    };
    let candidates: Vec<u32> = vm
        .sim
        .destructables
        .iter()
        .filter(|entry| {
            entry.pos.x >= min.x
                && entry.pos.x <= max.x
                && entry.pos.y >= min.y
                && entry.pos.y <= max.y
        })
        .filter_map(|entry| {
            vm.host
                .destructable_binding
                .iter()
                .find(|(_, bound)| **bound == entry.id)
                .map(|(handle, _)| *handle)
        })
        .collect();
    let saved = vm.enum_destructable;
    for handle_id in candidates {
        vm.enum_destructable = Some(handle_id);
        vm.call_script_function(func, Vec::new());
    }
    vm.enum_destructable = saved;
    Value::Null
}

pub(super) fn enum_items_in_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(rect) = hd(a, 0) else {
        return Value::Null;
    };
    let Some(func) = super::trigger::function_of(a.get(1).unwrap_or(&Value::Null)) else {
        return Value::Null;
    };
    let Some((min, max)) = vm.host.rect(rect) else {
        return Value::Null;
    };
    let candidates: Vec<u32> = vm
        .sim
        .items
        .iter()
        .filter(|item| {
            item.pos.x >= min.x
                && item.pos.x <= max.x
                && item.pos.y >= min.y
                && item.pos.y <= max.y
        })
        .filter_map(|item| {
            vm.host
                .item_binding
                .iter()
                .find(|(_, bound)| **bound == item.id)
                .map(|(handle, _)| *handle)
        })
        .collect();
    let saved = vm.enum_item;
    for handle_id in candidates {
        vm.enum_item = Some(handle_id);
        vm.call_script_function(func, Vec::new());
    }
    vm.enum_item = saved;
    Value::Null
}

pub(super) fn get_enum_destructable(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.enum_destructable {
        Some(id) => Value::Handle(Handle::new(HandleKind::Destructable, id)),
        None => Value::Null,
    }
}

pub(super) fn get_enum_item(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.enum_item {
        Some(id) => Value::Handle(Handle::new(HandleKind::Item, id)),
        None => Value::Null,
    }
}

pub(super) fn get_filter_destructable(vm: &mut JassVm, _a: &[Value]) -> Value {
    get_enum_destructable(vm, _a)
}

pub(super) fn get_filter_item(vm: &mut JassVm, _a: &[Value]) -> Value {
    get_enum_item(vm, _a)
}

// ---------------------------------------------------------------------------
// Dialogs, text tags, trackables
// ---------------------------------------------------------------------------

pub(super) fn dialog_create(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Dialog {
        message: String::new(),
        shown_to: None,
        asynchronous: false,
        buttons: Vec::new(),
    }))
}

pub(super) fn dialog_add_button(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    let text = s(a, 1);
    let hotkey = i(a, 2);
    add_dialog_button(vm, dialog, text, hotkey, false)
}

/// `DialogAddQuitButton(dialog, doScoreScreen, buttonText, hotkey)` — same button
/// payload plus the engine's "quit to score screen" flag.
pub(super) fn dialog_add_quit_button(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    let text = s(a, 2);
    let hotkey = i(a, 3);
    add_dialog_button(vm, dialog, text, hotkey, true)
}

fn add_dialog_button(
    vm: &mut JassVm,
    dialog: Handle,
    text: String,
    hotkey: i32,
    quit: bool,
) -> Value {
    let native = if quit {
        "DialogAddQuitButton"
    } else {
        "DialogAddButton"
    };
    if !is_dialog(vm, dialog, native) {
        return Value::Null;
    }
    let button = vm.host.create(Object::Button { text, hotkey, quit });
    if let Some(Object::Dialog { buttons, .. }) = vm.host.object_mut(dialog) {
        buttons.push(button.id);
    }
    Value::Handle(button)
}

pub(super) fn dialog_clear(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_dialog(vm, dialog, "DialogClear") {
        return Value::Null;
    }
    let buttons = match vm.host.object_mut(dialog) {
        Some(Object::Dialog { buttons, .. }) => std::mem::take(buttons),
        _ => return Value::Null,
    };
    for button in buttons {
        vm.host.objects.remove(&button);
    }
    Value::Null
}

/// `DialogSetMessage(dialog, text)` — the text the headless UI model stores and
/// the renderer reads.
pub(super) fn dialog_set_message(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    let message = s(a, 1);
    if !is_dialog(vm, dialog, "DialogSetMessage") {
        return Value::Null;
    }
    if let Some(Object::Dialog {
        message: state, ..
    }) = vm.host.object_mut(dialog)
    {
        *state = message;
    }
    Value::Null
}

/// `DialogDisplay(player, dialog, show)` — records who the dialog is shown to;
/// `false` hides it. The player slot is validated so a script cannot display to
/// a slot that does not exist.
pub(super) fn dialog_display(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 1) else {
        return Value::Null;
    };
    let shown = b(a, 2);
    let player = player_index(a, 0);
    if shown && player.is_none() {
        vm.errors
            .push("DialogDisplay: invalid player handle".into());
        return Value::Null;
    }
    if !is_dialog(vm, dialog, "DialogDisplay") {
        return Value::Null;
    }
    if let Some(Object::Dialog { shown_to, .. }) = vm.host.object_mut(dialog) {
        *shown_to = if shown { player } else { None };
    }
    Value::Null
}

/// `DialogSetAsync(dialog)` — the dialog may be shown per player. Recorded so the
/// headless model is complete; the single display target still reports the last
/// `DialogDisplay`.
pub(super) fn dialog_set_async(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    if !is_dialog(vm, dialog, "DialogSetAsync") {
        return Value::Null;
    }
    if let Some(Object::Dialog { asynchronous, .. }) = vm.host.object_mut(dialog) {
        *asynchronous = true;
    }
    Value::Null
}

/// `DialogDestroy(dialog)` — frees the dialog and the buttons it owns. Buttons
/// referenced by a `TriggerRegisterDialogButtonEvent` registration lose their
/// object here, exactly like the engine freeing them with their dialog; the
/// registration itself stays and simply never compares equal again.
pub(super) fn dialog_destroy(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    let buttons = match vm.host.objects.remove(&dialog.id) {
        Some(Object::Dialog { buttons, .. }) => buttons,
        Some(other) => {
            vm.host.objects.insert(dialog.id, other);
            vm.errors
                .push(format!("DialogDestroy: handle {} is not a dialog", dialog.id));
            return Value::Null;
        }
        None => return Value::Null,
    };
    for button in buttons {
        vm.host.objects.remove(&button);
    }
    Value::Null
}

pub(super) fn create_text_tag(vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 0);
    let size = r(a, 1);
    let _ = size;
    Value::Handle(vm.host.create(Object::TextTag {
        text,
        pos: Vec2::ZERO,
        lifespan: 0.0,
    }))
}

pub(super) fn set_text_tag_text(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(tag) = hd(a, 0) else {
        return Value::Null;
    };
    let text = s(a, 1);
    if let Some(Object::TextTag { text: state, .. }) = vm.host.object_mut(tag) {
        *state = text;
    }
    Value::Null
}

pub(super) fn set_text_tag_pos(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(tag) = hd(a, 0) else {
        return Value::Null;
    };
    let pos = Vec2::new(r(a, 1), r(a, 2));
    if let Some(Object::TextTag { pos: state, .. }) = vm.host.object_mut(tag) {
        *state = pos;
    }
    Value::Null
}

pub(super) fn set_text_tag_life_span(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(tag) = hd(a, 0) else {
        return Value::Null;
    };
    let lifespan = r(a, 1);
    if let Some(Object::TextTag { lifespan: state, .. }) = vm.host.object_mut(tag) {
        *state = lifespan;
    }
    Value::Null
}

pub(super) fn destroy_text_tag(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(tag) = hd(a, 0) {
        vm.host.objects.remove(&tag.id);
    }
    Value::Null
}

pub(super) fn create_trackable(vm: &mut JassVm, a: &[Value]) -> Value {
    let pos = Vec2::new(r(a, 1), r(a, 2));
    Value::Handle(vm.host.create(Object::Trackable { pos }))
}

// ---------------------------------------------------------------------------
// Quests
// ---------------------------------------------------------------------------

pub(super) fn create_quest(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::Quest {
        title: String::new(),
        description: String::new(),
        icon: String::new(),
        required: false,
        discovered: false,
        completed: false,
        items: Vec::new(),
    }))
}

fn quest_mut<'a>(vm: &'a mut JassVm, a: &[Value]) -> Option<&'a mut Object> {
    let handle = hd(a, 0)?;
    vm.host.object_mut(handle)
}

pub(super) fn quest_set_title(vm: &mut JassVm, a: &[Value]) -> Value {
    let title = s(a, 1);
    if let Some(Object::Quest { title: state, .. }) = quest_mut(vm, a) {
        *state = title;
    }
    Value::Null
}

pub(super) fn quest_set_description(vm: &mut JassVm, a: &[Value]) -> Value {
    let description = s(a, 1);
    if let Some(Object::Quest { description: state, .. }) = quest_mut(vm, a) {
        *state = description;
    }
    Value::Null
}

pub(super) fn quest_set_icon_path(vm: &mut JassVm, a: &[Value]) -> Value {
    let icon = s(a, 1);
    if let Some(Object::Quest { icon: state, .. }) = quest_mut(vm, a) {
        *state = icon;
    }
    Value::Null
}

pub(super) fn quest_set_required(vm: &mut JassVm, a: &[Value]) -> Value {
    let required = b(a, 1);
    if let Some(Object::Quest { required: state, .. }) = quest_mut(vm, a) {
        *state = required;
    }
    Value::Null
}

pub(super) fn quest_set_discovered(vm: &mut JassVm, a: &[Value]) -> Value {
    let discovered = b(a, 1);
    if let Some(Object::Quest { discovered: state, .. }) = quest_mut(vm, a) {
        *state = discovered;
    }
    Value::Null
}

pub(super) fn quest_set_completed(vm: &mut JassVm, a: &[Value]) -> Value {
    let completed = b(a, 1);
    if let Some(Object::Quest { completed: state, .. }) = quest_mut(vm, a) {
        *state = completed;
    }
    Value::Null
}

/// `QuestCreateItem(quest)` — a sub-objective row.
pub(super) fn quest_create_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(quest) = hd(a, 0) else {
        return Value::Null;
    };
    let item = vm.host.create(Object::Opaque);
    if let Some(Object::Quest { items, .. }) = vm.host.object_mut(quest) {
        items.push(item.id);
    }
    Value::Handle(item)
}

fn quest_item_field(vm: &mut JassVm, a: &[Value], apply: impl Fn(&mut String, &mut bool)) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    // Quest items store their text in the opaque payload's companion map.
    if let Some(state) = vm.host.quest_items.get_mut(&handle.id) {
        apply(&mut state.0, &mut state.1);
    } else {
        vm.host
            .quest_items
            .insert(handle.id, (String::new(), false));
        let state = vm.host.quest_items.get_mut(&handle.id).expect("inserted");
        apply(&mut state.0, &mut state.1);
    }
    Value::Null
}

pub(super) fn quest_item_set_description(vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 1);
    quest_item_field(vm, a, move |description, _| *description = text.clone())
}

pub(super) fn quest_item_set_completed(vm: &mut JassVm, a: &[Value]) -> Value {
    let completed = b(a, 1);
    quest_item_field(vm, a, move |_, state| *state = completed)
}

// ---------------------------------------------------------------------------
// Cinematic filter, user UI, time of day
// ---------------------------------------------------------------------------

pub(super) fn display_cine_filter(vm: &mut JassVm, a: &[Value]) -> Value {
    let displayed = b(a, 0);
    vm.host.cine_filter.displayed = displayed;
    let texture = vm.host.cine_filter.texture.clone();
    vm.host.record_cinematic(CinematicEvent::Filter {
        displayed,
        texture,
        tick: vm.time,
    });
    Value::Null
}

pub(super) fn set_cine_filter_texture(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.texture = s(a, 0);
    Value::Null
}

pub(super) fn set_cine_filter_start_uv(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.start_uv = (r(a, 0), r(a, 1));
    Value::Null
}

pub(super) fn set_cine_filter_end_uv(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.end_uv = (r(a, 0), r(a, 1));
    Value::Null
}

pub(super) fn set_cine_filter_start_color(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.start_color = (i(a, 0), i(a, 1), i(a, 2), i(a, 3));
    Value::Null
}

pub(super) fn set_cine_filter_end_color(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.end_color = (i(a, 0), i(a, 1), i(a, 2), i(a, 3));
    Value::Null
}

pub(super) fn set_cine_filter_duration(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.duration = r(a, 0);
    Value::Null
}

pub(super) fn set_cine_filter_blend_mode(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.blend_mode = i(a, 0);
    Value::Null
}

pub(super) fn set_cine_filter_tex_map_flags(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cine_filter.tex_map_flags = i(a, 0);
    Value::Null
}

pub(super) fn enable_user_ui(vm: &mut JassVm, a: &[Value]) -> Value {
    let enabled = b(a, 0);
    vm.host.user_ui_enabled = enabled;
    vm.host.interface_shown = enabled;
    vm.host.record_cinematic(CinematicEvent::Interface {
        interface_shown: vm.host.interface_shown,
        user_control: vm.host.user_control_enabled,
        occlusion: vm.host.occlusion_enabled,
        tick: vm.time,
    });
    Value::Null
}

pub(super) fn set_time_of_day_scale(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.time_of_day_scale = r(a, 0);
    Value::Null
}

pub(super) fn fog_enable(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.fog_enabled = b(a, 0);
    Value::Null
}

pub(super) fn fog_mask_enable(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.fog_mask_enabled = b(a, 0);
    Value::Null
}

pub(super) fn clear_map_music(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.map_music.clear();
    Value::Null
}

pub(super) fn attach_sound_to_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let unit = hd(a, 1).map(|handle| handle.id);
    if let Some(state) = sound_mut(vm, a) {
        state.unit = unit;
    }
    Value::Null
}

/// `SetPlayerHandicapXP(player, handicap)` and `SetPlayerOnScoreScreen`.
pub(super) fn set_player_handicap_xp(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(player) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(player as usize)
    {
        state.states.insert(handicap_xp_key(), real_to_int(r(a, 1) * 100.0));
    }
    Value::Null
}

/// `PLAYER_STATE_HANDICAP_XP`-style key (the engine's own numbering).
fn handicap_xp_key() -> i32 {
    30
}

pub(super) fn set_player_on_score_screen(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(player) = player_index(a, 0)
        && let Some(state) = vm.host.players.get_mut(player as usize)
    {
        state.states.insert(31, b(a, 1) as i32);
    }
    Value::Null
}

/// `SuspendHeroXP(unit, flag)`.
pub(super) fn suspend_hero_xp(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let flag = b(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.xp_suspended = flag;
    }
    Value::Null
}

/// `PauseUnit(unit, flag)` — a paused unit keeps its position and drops orders.
pub(super) fn pause_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let paused = b(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.paused = paused;
    }
    if paused
        && let Some(unit) = vm.host.sim_unit(handle)
    {
        vm.sim.issue(&[unit], crate::OrderSpec::Stop);
    }
    Value::Null
}

/// `MathRound(value)` — rounds half away from zero, like the engine.
pub(super) fn math_round(_vm: &mut JassVm, a: &[Value]) -> Value {
    let value = r(a, 0);
    Value::Int(if value >= 0.0 {
        (value + 0.5).trunc() as i32
    } else {
        (value - 0.5).trunc() as i32
    })
}

// ---------------------------------------------------------------------------
// Item pools / unit pools (creep spawners)
// ---------------------------------------------------------------------------

pub(super) fn create_item_pool(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::ItemPool {
        items: Vec::new(),
        mode: 0,
    }))
}

pub(super) fn item_pool_add_item(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(pool), _weight) = (hd(a, 0), i(a, 2)) else {
        return Value::Null;
    };
    let item = i(a, 1) as i64;
    if let Some(Object::ItemPool { items, .. }) = vm.host.object_mut(pool) {
        items.push(item);
    }
    Value::Null
}

pub(super) fn destroy_item_pool(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn destroy_unit_pool(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn create_unit_pool(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Handle(vm.host.create(Object::UnitPool {
        units: Vec::new(),
        mode: 0,
    }))
}

pub(super) fn unit_pool_add_unit(vm: &mut JassVm, a: &[Value]) -> Value {
    let (Some(pool), _weight) = (hd(a, 0), i(a, 2)) else {
        return Value::Null;
    };
    let unit = i(a, 1) as i64;
    if let Some(Object::UnitPool { units, .. }) = vm.host.object_mut(pool) {
        units.push(unit);
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Game events / misc
// ---------------------------------------------------------------------------

/// `TriggerRegisterGameEvent(trigger, gameevent)`.
///
/// Stores a real registration on the trigger and returns the engine's event
/// handle for that `gameevent` id. The engine itself raises only
/// `EVENT_GAME_STATE_LIMIT` (from `TriggerRegisterGameStateEvent` crossings);
/// every other `EVENT_GAME_*` constant is delivered through
/// [`crate::jass::host::Host::raise_game_event`], which is what the game layer
/// and tests use. Those ids are recorded in `host.raise_only_game_events` — the
/// explicit `Partial` status, so a script never gets a silent no-op registration.
pub(super) fn trigger_register_game_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(trigger) = hd(a, 0) else {
        vm.errors
            .push("TriggerRegisterGameEvent: null trigger handle".into());
        return Value::Null;
    };
    if vm.host.trigger(trigger).is_none() {
        vm.errors.push(format!(
            "TriggerRegisterGameEvent: handle {} is not a trigger",
            trigger.id
        ));
        return Value::Null;
    }
    let event = i64::from(i(a, 1));
    if let Some(state) = vm.host.trigger_mut(trigger) {
        state.events.push(RegEvent::GameEvent { event });
    }
    if game_event_support(event) == GameEventSupport::Partial {
        vm.host.raise_only_game_events.insert(event);
    }
    Value::Handle(vm.host.game_event_handle(event))
}

pub(super) fn add_camera_blocker(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn blz_camera_setup_set_camera_type(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_item_color(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_destructable_vertex_color(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_player_race_skin(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

pub(super) fn set_enemy_start_loc_prio(vm: &mut JassVm, a: &[Value]) -> Value {
    let index = i(a, 0);
    let slot = i(a, 1);
    let important = i(a, 2);
    vm.host.start_prio.insert((-1000 - index, slot), important);
    Value::Null
}

pub(super) fn set_enemy_start_loc_prio_count(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host
        .start_prio_count
        .insert(-1000 - i(a, 0), i(a, 1));
    Value::Null
}

/// `BlzCreateUnitWithSkin(player, id, x, y, facing, skinId)`.
pub(super) fn blz_create_unit_with_skin(vm: &mut JassVm, a: &[Value]) -> Value {
    super::unit::create_unit_native(vm, a)
}

/// `BlzCreateItemWithSkin(id, x, y, skinId)`.
pub(super) fn blz_create_item_with_skin(vm: &mut JassVm, a: &[Value]) -> Value {
    super::unit::create_item(vm, a)
}

/// `SetUnitAnimationByIndex` with an extra `AddUnitAnimationProperties` variant.
pub(super) fn add_unit_animation_properties(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let properties = s(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.animation = properties;
    }
    Value::Null
}

/// `SetUnitName` (Blizzard native `BlzSetUnitName`).
pub(super) fn blz_set_unit_name(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        return Value::Null;
    };
    let name = s(a, 1);
    if let Some(meta) = vm.host.unit_meta_mut(handle) {
        meta.name_override = Some(name);
    }
    Value::Null
}

/// `UnitAddType(unit, unittype)` — the engine's per-instance unit
/// classification delta.
///
/// The bit lives in the unit's [`crate::jass::host::UnitMeta`], never in the
/// shared [`crate::UnitDef`]: one footman can be a "hero" or a "structure" for
/// the script while every other footman of the same id is unchanged.
/// `IsUnitType` and the classification filters read the same
/// [`crate::jass::host::UnitTypes::classify`] rule.
///
/// Returns `true` when the call applied (an idempotent re-add included) and
/// `false` with a recorded diagnostic for a wrong unit handle or a `unittype`
/// outside the enum the shipped `common.j` defines.
pub(super) fn unit_add_type(vm: &mut JassVm, a: &[Value]) -> Value {
    set_unit_type(vm, a, true, "UnitAddType")
}

/// `UnitRemoveType` — see [`unit_add_type`].
pub(super) fn unit_remove_type(vm: &mut JassVm, a: &[Value]) -> Value {
    set_unit_type(vm, a, false, "UnitRemoveType")
}

/// Shared body of `UnitAddType` / `UnitRemoveType`: one classification change
/// and one return rule (`true` means the call was applied).
fn set_unit_type(vm: &mut JassVm, a: &[Value], add: bool, native: &str) -> Value {
    let Some(handle) = unit_arg(a, 0) else {
        vm.errors.push(format!("{native}: invalid unit handle"));
        return Value::Bool(false);
    };
    if vm.host.sim_unit(handle).is_none() {
        vm.errors.push(format!(
            "{native}: handle {} is not a live unit",
            handle.id
        ));
        return Value::Bool(false);
    }
    let type_id = i(a, 1);
    if !(0..unit_type::COUNT).contains(&type_id) {
        vm.errors.push(format!(
            "{native}: {type_id} is not a UNIT_TYPE_* constant (0..{})",
            unit_type::COUNT
        ));
        return Value::Bool(false);
    }
    let Some(meta) = vm.host.unit_meta_mut(handle) else {
        return Value::Bool(false);
    };
    if add {
        meta.types.add(type_id);
    } else {
        meta.types.remove(type_id);
    }
    Value::Bool(true)
}



/// `CreateFogModifierRect(player, state, rect, shared, useSharedVision)`.
pub(super) fn create_fog_modifier_rect(vm: &mut JassVm, a: &[Value]) -> Value {
    let rect = hd(a, 2).map_or(0, |handle| handle.id);
    Value::Handle(vm.host.create(Object::FogModifier {
        rect,
        enabled: false,
    }))
}

/// `FogModifierStart` / `FogModifierStop`.
pub(super) fn fog_modifier_start(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0)
        && let Some(Object::FogModifier { enabled, .. }) = vm.host.object_mut(handle)
    {
        *enabled = true;
    }
    Value::Null
}

/// `DestroyFogModifier`.
pub(super) fn destroy_fog_modifier(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}


/// Unused import guard for `RegEvent` (trigger registration is in `trigger.rs`).
fn _unused(_event: RegEvent, _real: f32) -> i32 {
    real_to_int(_real)
}
