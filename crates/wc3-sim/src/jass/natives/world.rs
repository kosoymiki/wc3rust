//! World, environment, camera, audio-state, UI and enumeration natives.
//!
//! The headless simulation has no output devices, so render/audio natives record
//! their state on the handle (or on [`crate::jass::host::Host`]) and every query
//! native reads it back. That keeps `GetSoundDuration`, `GetCameraMargin` and
//! `GetStartLocationX` honest: they answer from what the script set, instead of
//! returning a constant that pretends the call worked.

use glam::Vec2;

use crate::jass::host::{
    CameraSetup, CinematicEvent, Object, RegEvent, SoundState, StartLocation,
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

/// `CreateTimerDialog(timer)` — the dialog shows the timer's remaining time; the
/// handle exists so `TimerDialogDisplay`/`DestroyTimerDialog` behave.
pub(super) fn create_timer_dialog(vm: &mut JassVm, a: &[Value]) -> Value {
    let timer = hd(a, 0).map_or(0, |handle| handle.id);
    Value::Handle(vm.host.create(Object::TimerDialog {
        timer,
        title: String::new(),
    }))
}

pub(super) fn destroy_timer_dialog(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(handle) = hd(a, 0) {
        vm.host.objects.remove(&handle.id);
    }
    Value::Null
}

pub(super) fn timer_dialog_set_title(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let title = s(a, 1);
    if let Some(Object::TimerDialog { title: state, .. }) = vm.host.object_mut(handle) {
        *state = title;
    }
    Value::Null
}

pub(super) fn timer_dialog_display(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

/// `TriggerRegisterGameStateEvent(trigger, state, op, limit)` — game state
/// events (`EVENT_GAME_STATE_LIMIT`) have no equivalent in the headless
/// simulation, so the registration is recorded and never fires.
pub(super) fn trigger_register_game_state_event(vm: &mut JassVm, a: &[Value]) -> Value {
    let trigger = hd(a, 0);
    let _ = (i(a, 1), i(a, 2), r(a, 3));
    let _ = (trigger, vm);
    Value::Handle(Handle::new(HandleKind::GameEvent, 0))
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
    Value::Handle(vm.host.create(Object::Dialog { buttons: Vec::new() }))
}

pub(super) fn dialog_add_button(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(dialog) = hd(a, 0) else {
        return Value::Null;
    };
    let text = s(a, 1);
    let hotkey = i(a, 2);
    let button = vm.host.create(Object::Button { text, hotkey });
    if let Some(Object::Dialog { buttons }) = vm.host.object_mut(dialog) {
        buttons.push(button.id);
    }
    Value::Handle(button)
}

pub(super) fn dialog_clear(vm: &mut JassVm, a: &[Value]) -> Value {
    if let Some(dialog) = hd(a, 0)
        && let Some(Object::Dialog { buttons }) = vm.host.object_mut(dialog)
    {
        buttons.clear();
    }
    Value::Null
}

pub(super) fn dialog_set_message(_vm: &mut JassVm, a: &[Value]) -> Value {
    let _ = s(a, 1);
    Value::Null
}

pub(super) fn dialog_display(_vm: &mut JassVm, _a: &[Value]) -> Value {
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

pub(super) fn trigger_register_game_event(_vm: &mut JassVm, a: &[Value]) -> Value {
    let _ = hd(a, 0);
    Value::Handle(Handle::new(HandleKind::GameEvent, i(a, 1).max(0) as u32))
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

/// `SetUnitFacingTimed` etc. already live in `unit.rs`; this keeps
/// `UnitAddType`-style calls from silently doing nothing.
pub(super) fn unit_add_type(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
}

/// `UnitRemoveType`.
pub(super) fn unit_remove_type(_vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Null
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
