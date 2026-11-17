//! Cinematic and campaign-script natives.
//!
//! A campaign mission is a scripted performance: `CinematicModeBJ` hides the
//! interface, `CameraSetupApplyForPlayer` moves the camera through authored
//! setups, `TransmissionFromUnitWithNameBJ` shows a portrait and a subtitle,
//! `SetCinematicScene` prints the title card, and `TriggerRegisterTimerEvent`
//! spaces the beats. The BJ layer is script (`Blizzard.j`), so what the runtime
//! must provide is the native set it calls, plus an observable record of what
//! happened.
//!
//! Every native here records into [`Host::cinematic_events`] and keeps the
//! camera state the query natives read back. Nothing is a no-op that pretends
//! to work: a camera move updates [`CameraState`], a transmission carries the
//! speaker and text, and a movie request carries the path.

use glam::Vec2;

use crate::jass::host::{CameraSetup, CinematicEvent, Object, QuestFlags};
use crate::jass::value::{HandleKind, Value};
use crate::jass::vm::JassVm;

use super::unit::unit_arg;
use super::{b, hd, i, r, s};

fn camera_field(vm: &mut JassVm, field: i32, value: f32) {
    vm.host.camera.fields.insert(field, value);
}

fn record_interface(vm: &mut JassVm) {
    vm.host.record_cinematic(CinematicEvent::Interface {
        interface_shown: vm.host.interface_shown,
        user_control: vm.host.user_control_enabled,
        occlusion: vm.host.occlusion_enabled,
        tick: vm.time,
    });
}

// ---------------------------------------------------------------------------
// Scene text and transmissions
// ---------------------------------------------------------------------------

/// `SetCinematicScene(portraitUnitId, color, speakerTitle, text, sceneDuration,
/// voiceoverDuration)`.
///
/// `TransmissionFromUnitWithNameBJ` reaches this native through
/// `SetCinematicSceneBJ`, with the speaking unit's rawcode as the portrait id;
/// the same native writes plain scene title cards with portrait 0. The event
/// log distinguishes them by the portrait id.
pub(super) fn set_cinematic_scene(vm: &mut JassVm, a: &[Value]) -> Value {
    let portrait = i(a, 0);
    let color = i(a, 1);
    let speaker = s(a, 2);
    let text = s(a, 3);
    let scene_duration = r(a, 4);
    let voice_duration = r(a, 5);
    let tick = vm.time;
    vm.host.record_cinematic(CinematicEvent::Scene {
        portrait_unit: portrait,
        color,
        speaker: speaker.clone(),
        text: text.clone(),
        scene_duration,
        voice_duration,
        tick,
    });
    if portrait != 0 {
        // The rawcode does not identify the running unit instance; the
        // transmission event carries the handle when the caller has one.
        vm.host.record_cinematic(CinematicEvent::Transmission {
            unit: None,
            speaker,
            message: text,
            duration: voice_duration.max(scene_duration),
            tick,
        });
    }
    Value::Null
}

/// `EndCinematicScene()` — closes the current portrait/subtitle band.
pub(super) fn end_cinematic_scene(vm: &mut JassVm, _a: &[Value]) -> Value {
    let tick = vm.time;
    vm.host.record_cinematic(CinematicEvent::SceneEnd { tick });
    Value::Null
}

/// `ForceCinematicSubtitles(flag)`.
pub(super) fn force_cinematic_subtitles(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cinematic_subtitles = b(a, 0);
    Value::Null
}

/// `SetCinematicAudio(flag)` — mutes the non-speech audio channels.
pub(super) fn set_cinematic_audio(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.cinematic_audio = b(a, 0);
    Value::Null
}

/// `SetCinematicCamera(cameraModelFile)` — the camera model shown during a
/// cinematic (an art request the renderer consumes).
pub(super) fn set_cinematic_camera(vm: &mut JassVm, a: &[Value]) -> Value {
    let model = s(a, 0);
    if !model.is_empty() {
        vm.host.record_cinematic(CinematicEvent::Movie {
            name: model,
            tick: vm.time,
        });
    }
    Value::Null
}

// ---------------------------------------------------------------------------
// Interface / control
// ---------------------------------------------------------------------------

/// `ShowInterface(show, fadeDuration)` — the letterbox switch.
pub(super) fn show_interface(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.interface_shown = b(a, 0);
    record_interface(vm);
    Value::Null
}

/// `EnableUserControl(enable)`.
pub(super) fn enable_user_control(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.user_control_enabled = b(a, 0);
    record_interface(vm);
    Value::Null
}

/// `EnableOcclusion(enable)` — occlusion (units behind trees going dark).
pub(super) fn enable_occlusion(vm: &mut JassVm, a: &[Value]) -> Value {
    vm.host.occlusion_enabled = b(a, 0);
    record_interface(vm);
    Value::Null
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

/// `SetCameraTargetController(unit, xoffset, yoffset, inheritOrientation)`.
pub(super) fn set_camera_target_controller(vm: &mut JassVm, a: &[Value]) -> Value {
    let unit = unit_arg(a, 0);
    let x = r(a, 1);
    let y = r(a, 2);
    if let Some(unit) = unit {
        vm.host.camera.target_controller = Some(unit.id);
        if let Some(pos) = vm
            .host
            .sim_unit(unit)
            .and_then(|sim_unit| vm.sim.unit(sim_unit))
            .map(|unit| unit.pos)
        {
            vm.host.camera.target = pos + Vec2::new(x, y);
        }
    }
    vm.host.record_cinematic(CinematicEvent::CameraController {
        unit: unit.map(|unit| unit.id),
        x,
        y,
        tick: vm.time,
    });
    Value::Null
}

/// `SetCameraOrientController(unit, xoffset, yoffset)` — the camera follows the
/// unit's facing instead of its position.
pub(super) fn set_camera_orient_controller(vm: &mut JassVm, a: &[Value]) -> Value {
    let unit = unit_arg(a, 0);
    let x = r(a, 1);
    let y = r(a, 2);
    if let Some(unit) = unit {
        vm.host.camera.target_controller = Some(unit.id);
    }
    vm.host.record_cinematic(CinematicEvent::CameraController {
        unit: unit.map(|unit| unit.id),
        x,
        y,
        tick: vm.time,
    });
    Value::Null
}

/// `StopCamera()` — drops the controller and returns the camera to the game
/// camera at the current target.
pub(super) fn stop_camera(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.camera.target_controller = None;
    vm.host.record_cinematic(CinematicEvent::CameraReset { tick: vm.time });
    Value::Null
}

/// `SetCameraRotateMode(x, y, radiansToSweep, duration)`.
pub(super) fn set_camera_rotate_mode(vm: &mut JassVm, a: &[Value]) -> Value {
    let x = r(a, 0);
    let y = r(a, 1);
    let duration = r(a, 3);
    vm.host.camera.target = Vec2::new(x, y);
    vm.host.record_cinematic(CinematicEvent::CameraPosition {
        x,
        y,
        duration,
        tick: vm.time,
    });
    Value::Null
}

/// `PanCameraToWithZ(x, y, zOffsetDest)`.
pub(super) fn pan_camera_to_with_z(vm: &mut JassVm, a: &[Value]) -> Value {
    pan_camera(vm, a, 0.0)
}

/// `PanCameraToTimedWithZ(x, y, zOffsetDest, duration)`.
pub(super) fn pan_camera_to_timed_with_z(vm: &mut JassVm, a: &[Value]) -> Value {
    pan_camera(vm, a, r(a, 3))
}

fn pan_camera(vm: &mut JassVm, a: &[Value], duration: f32) -> Value {
    let x = r(a, 0);
    let y = r(a, 1);
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

/// `CameraSetSmoothingFactor(factor)`.
pub(super) fn camera_set_smoothing_factor(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 100, r(a, 0));
    Value::Null
}

/// `CameraSetSourceNoise(magnitude, velocity)`.
pub(super) fn camera_set_source_noise(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 101, r(a, 0));
    camera_field(vm, 102, r(a, 1));
    Value::Null
}

/// `CameraSetSourceNoiseEx(magnitude, velocity, vertOnly)`.
pub(super) fn camera_set_source_noise_ex(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 101, r(a, 0));
    camera_field(vm, 102, r(a, 1));
    camera_field(vm, 103, b(a, 2) as i32 as f32);
    Value::Null
}

/// `CameraSetTargetNoise(magnitude, velocity)`.
pub(super) fn camera_set_target_noise(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 104, r(a, 0));
    camera_field(vm, 105, r(a, 1));
    Value::Null
}

/// `CameraSetTargetNoiseEx(magnitude, velocity, vertOnly)`.
pub(super) fn camera_set_target_noise_ex(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 104, r(a, 0));
    camera_field(vm, 105, r(a, 1));
    camera_field(vm, 106, b(a, 2) as i32 as f32);
    Value::Null
}

/// `CameraSetDepthOfFieldScale(scale)`.
pub(super) fn camera_set_depth_of_field_scale(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 107, r(a, 0));
    Value::Null
}

/// `CameraSetFocalDistance(distance)`.
pub(super) fn camera_set_focal_distance(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_field(vm, 108, r(a, 0));
    Value::Null
}

/// `GetCameraTargetPositionX/Y/Z`.
pub(super) fn get_camera_target_position_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(vm.host.camera.target.x)
}

pub(super) fn get_camera_target_position_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(vm.host.camera.target.y)
}

pub(super) fn get_camera_target_position_z(vm: &mut JassVm, a: &[Value]) -> Value {
    // The target is a ground point; `GetCameraTargetPositionZ` adds the terrain
    // height, which the headless world reports through the eye offset instead.
    let z = vm.host.camera.fields.get(&6).copied().unwrap_or(0.0);
    let _ = a;
    Value::Real(z)
}

pub(super) fn get_camera_target_position_loc(vm: &mut JassVm, _a: &[Value]) -> Value {
    let target = vm.host.camera.target;
    Value::Handle(vm.host.create(Object::Location(target)))
}

/// `GetCameraEyePositionX/Y/Z/Loc`. The eye point is the target pulled back by
/// `CAMERA_FIELD_TARGET_DISTANCE` along `CAMERA_FIELD_ROTATION`.
pub(super) fn get_camera_eye_position_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_eye(vm).x)
}

pub(super) fn get_camera_eye_position_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_eye(vm).y)
}

pub(super) fn get_camera_eye_position_z(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(camera_eye(vm).z)
}

pub(super) fn get_camera_eye_position_loc(vm: &mut JassVm, _a: &[Value]) -> Value {
    let eye = camera_eye(vm);
    Value::Handle(vm.host.create(Object::Location(eye.truncate())))
}

fn camera_eye(vm: &JassVm) -> glam::Vec3 {
    let distance = vm.host.camera.fields.get(&0).copied().unwrap_or(1650.0);
    let rotation = vm.host.camera.fields.get(&5).copied().unwrap_or(90.0);
    let angle = vm.host.camera.fields.get(&2).copied().unwrap_or(304.0);
    // `ROTATION` is degrees clockwise from +x; the eye sits behind and above
    // the target. `ANGLE_OF_ATTACK` is measured from the vertical, exactly as
    // the engine's `CAMERA_FIELD_ANGLE_OF_ATTACK` describes it.
    let yaw = rotation.to_radians();
    let pitch = (90.0 - angle.min(89.9)).to_radians();
    let horizontal = distance * pitch.cos();
    glam::Vec3::new(
        vm.host.camera.target.x - horizontal * yaw.cos(),
        vm.host.camera.target.y - horizontal * yaw.sin(),
        distance * pitch.sin(),
    )
}

/// `GetCameraField(whichField)`.
pub(super) fn get_camera_field(vm: &mut JassVm, a: &[Value]) -> Value {
    let field = i(a, 0);
    let value = vm
        .host
        .camera
        .fields
        .get(&field)
        .copied()
        .unwrap_or_else(|| default_camera_field(field));
    Value::Real(value)
}

/// `AdjustCameraField(whichField, value, duration)` — relative to the current
/// value.
pub(super) fn adjust_camera_field(vm: &mut JassVm, a: &[Value]) -> Value {
    let field = i(a, 0);
    let delta = r(a, 1);
    let duration = r(a, 2);
    let current = vm
        .host
        .camera
        .fields
        .get(&field)
        .copied()
        .unwrap_or_else(|| default_camera_field(field));
    let value = current + delta;
    camera_field(vm, field, value);
    vm.host.record_cinematic(CinematicEvent::CameraField {
        field,
        value,
        duration,
        tick: vm.time,
    });
    Value::Null
}

fn default_camera_field(field: i32) -> f32 {
    match field {
        0 => 1650.0,
        2 => 304.0,
        3 => 70.0,
        5 => 90.0,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Camera setups
// ---------------------------------------------------------------------------

/// `CameraSetupGetField(whichSetup, whichField)`.
pub(super) fn camera_setup_get_field(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(setup) = hd(a, 0) else {
        return Value::Real(0.0);
    };
    let field = i(a, 1) as i64;
    match vm.host.object(setup) {
        Some(Object::CameraSetup(CameraSetup { fields })) => {
            Value::Real(fields.get(&field).map_or(0.0, Value::real))
        }
        _ => Value::Real(0.0),
    }
}

/// `CameraSetupGetDestPositionX/Y` — `CameraSetupSetDestPosition` stores the
/// destination under the private keys -1/-2 (x/y) and -3 (z).
pub(super) fn camera_setup_get_dest_position_x(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_setup_dest(vm, a, -1)
}

pub(super) fn camera_setup_get_dest_position_y(vm: &mut JassVm, a: &[Value]) -> Value {
    camera_setup_dest(vm, a, -2)
}

fn camera_setup_dest(vm: &mut JassVm, a: &[Value], key: i64) -> Value {
    let Some(setup) = hd(a, 0) else {
        return Value::Real(0.0);
    };
    match vm.host.object(setup) {
        Some(Object::CameraSetup(CameraSetup { fields })) => {
            Value::Real(fields.get(&key).map_or(0.0, Value::real))
        }
        _ => Value::Real(0.0),
    }
}

// ---------------------------------------------------------------------------
// Movies
// ---------------------------------------------------------------------------

/// `PlayCinematic(name)` — a campaign movie (`Movies\...bik`).
pub(super) fn play_cinematic(vm: &mut JassVm, a: &[Value]) -> Value {
    let name = s(a, 0);
    vm.host.record_cinematic(CinematicEvent::Movie {
        name,
        tick: vm.time,
    });
    Value::Null
}

/// `PlayModelCinematic(unit)` — the unit's portrait "movie".
pub(super) fn play_model_cinematic(vm: &mut JassVm, a: &[Value]) -> Value {
    let unit = unit_arg(a, 0);
    vm.host.record_cinematic(CinematicEvent::Movie {
        name: unit
            .and_then(|unit| vm.host.sim_unit(unit))
            .and_then(|sim_unit| vm.sim.unit(sim_unit))
            .map(|unit| unit.def().name.to_string())
            .unwrap_or_else(|| "model".to_string()),
        tick: vm.time,
    });
    Value::Null
}

// ---------------------------------------------------------------------------
// Quests
// ---------------------------------------------------------------------------

fn quest_bool(vm: &mut JassVm, a: &[Value], read: impl Fn(&Object) -> Option<bool>) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Bool(false);
    };
    let value = vm.host.object(handle).and_then(read).unwrap_or(false);
    Value::Bool(value)
}

/// `IsQuestCompleted(whichQuest)`.
pub(super) fn is_quest_completed(vm: &mut JassVm, a: &[Value]) -> Value {
    quest_bool(vm, a, |object| match object {
        Object::Quest { completed, .. } => Some(*completed),
        _ => None,
    })
}

/// `IsQuestDiscovered(whichQuest)`.
pub(super) fn is_quest_discovered(vm: &mut JassVm, a: &[Value]) -> Value {
    quest_bool(vm, a, |object| match object {
        Object::Quest { discovered, .. } => Some(*discovered),
        _ => None,
    })
}

/// `IsQuestRequired(whichQuest)`.
pub(super) fn is_quest_required(vm: &mut JassVm, a: &[Value]) -> Value {
    quest_bool(vm, a, |object| match object {
        Object::Quest { required, .. } => Some(*required),
        _ => None,
    })
}

/// `IsQuestFailed(whichQuest)` — state lives in [`Host::quest_flags`] so the v4
/// save layout stays unchanged.
pub(super) fn is_quest_failed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Bool(false);
    };
    Value::Bool(
        vm.host
            .quest_flags
            .get(&handle.id)
            .is_some_and(|flags| flags.failed),
    )
}

/// `IsQuestEnabled(whichQuest)`.
pub(super) fn is_quest_enabled(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Bool(false);
    };
    Value::Bool(
        vm.host
            .quest_flags
            .get(&handle.id)
            .map_or(true, |flags| flags.enabled),
    )
}

/// `IsQuestItemCompleted(whichQuestItem)`.
pub(super) fn is_quest_item_completed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Bool(false);
    };
    Value::Bool(
        vm.host
            .quest_items
            .get(&handle.id)
            .is_some_and(|state| state.1),
    )
}

/// `QuestSetFailed(whichQuest, failed)`.
pub(super) fn quest_set_failed(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let failed = b(a, 1);
    vm.host
        .quest_flags
        .entry(handle.id)
        .or_default()
        .failed = failed;
    Value::Null
}

/// `QuestSetEnabled(whichQuest, enabled)`.
pub(super) fn quest_set_enabled(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    let enabled = b(a, 1);
    vm.host
        .quest_flags
        .entry(handle.id)
        .or_insert_with(QuestFlags::default)
        .enabled = enabled;
    Value::Null
}

/// `DestroyQuest(whichQuest)` — drops the object; queries then answer false.
pub(super) fn destroy_quest(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(handle) = hd(a, 0) else {
        return Value::Null;
    };
    vm.host.objects.remove(&handle.id);
    vm.host.quest_flags.remove(&handle.id);
    Value::Null
}

/// `FlashQuestDialogButton()` / `ForceQuestDialogUpdate()` — UI pulses. The
/// headless host records them on the quest UI channel so the HUD can animate.
pub(super) fn flash_quest_dialog_button(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.quest_flash = vm.time;
    Value::Null
}

pub(super) fn force_quest_dialog_update(vm: &mut JassVm, _a: &[Value]) -> Value {
    vm.host.quest_flash = vm.time;
    Value::Null
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

fn order_string_from_rawcode(rawcode: i32) -> String {
    let bytes = rawcode.to_be_bytes();
    let text: String = bytes
        .iter()
        .filter(|byte| byte.is_ascii_graphic())
        .map(|byte| *byte as char)
        .collect();
    text.to_ascii_lowercase()
}

/// `OrderId(orderString)` — the four-character rawcode as an integer.
pub(super) fn order_id(_vm: &mut JassVm, a: &[Value]) -> Value {
    let text = s(a, 0);
    let bytes = text.as_bytes();
    if bytes.len() != 4 {
        return Value::Int(0);
    }
    Value::Int(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// `OrderId2String(orderId)`.
pub(super) fn order_id2string(_vm: &mut JassVm, a: &[Value]) -> Value {
    Value::Str(order_string_from_rawcode(i(a, 0)))
}

/// `IssuePointOrderById(whichUnit, order, x, y)`.
pub(super) fn issue_point_order_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = order_string_from_rawcode(i(a, 1));
    let point = Vec2::new(r(a, 2), r(a, 3));
    super::unit::issue_point_order_impl(vm, a, &order, point)
}

/// `IssuePointOrderByIdLoc(whichUnit, order, whichLocation)`.
pub(super) fn issue_point_order_by_id_loc(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = order_string_from_rawcode(i(a, 1));
    let Some(point) = super::unit::location_arg(vm, a, 2, "IssuePointOrderByIdLoc") else {
        return Value::Bool(false);
    };
    super::unit::issue_point_order_impl(vm, a, &order, point)
}

/// `IssueTargetOrderById(whichUnit, order, targetWidget)`.
pub(super) fn issue_target_order_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = order_string_from_rawcode(i(a, 1));
    super::unit::issue_target_order_impl(vm, a, &order, a.get(2))
}

/// `IssueImmediateOrderById(whichUnit, order)`.
pub(super) fn issue_immediate_order_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = order_string_from_rawcode(i(a, 1));
    super::unit::issue_immediate_order_impl(vm, a, &order)
}

/// `IssueBuildOrder(whichUnit, unitToBuild, x, y)`.
pub(super) fn issue_build_order(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = s(a, 1);
    let point = Vec2::new(r(a, 2), r(a, 3));
    super::unit::issue_point_order_impl(vm, a, &order.to_ascii_lowercase(), point)
}

/// `IssueBuildOrderById(whichUnit, unitId, x, y)`.
pub(super) fn issue_build_order_by_id(vm: &mut JassVm, a: &[Value]) -> Value {
    let order = order_string_from_rawcode(i(a, 1));
    let point = Vec2::new(r(a, 2), r(a, 3));
    super::unit::issue_point_order_impl(vm, a, &order, point)
}

/// `GetIssuedOrderId()` — the rawcode of the order being dispatched.
pub(super) fn get_issued_order_id(vm: &mut JassVm, _a: &[Value]) -> Value {
    let order = vm
        .host
        .order_log
        .last()
        .map(|(_, order)| order.clone())
        .unwrap_or_default();
    let bytes = order.as_bytes();
    if bytes.len() == 4 {
        return Value::Int(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
    }
    Value::Int(0)
}

/// `GetOrderPointX/Y/Loc` — the last point order's destination.
pub(super) fn get_order_point_x(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(vm.host.last_order_point.x)
}

pub(super) fn get_order_point_y(vm: &mut JassVm, _a: &[Value]) -> Value {
    Value::Real(vm.host.last_order_point.y)
}

pub(super) fn get_order_point_loc(vm: &mut JassVm, _a: &[Value]) -> Value {
    let point = vm.host.last_order_point;
    Value::Handle(vm.host.create(Object::Location(point)))
}

/// `GetUnitCurrentOrder(whichUnit)` — the last order string issued to the unit.
pub(super) fn get_unit_current_order(vm: &mut JassVm, a: &[Value]) -> Value {
    let Some(unit) = unit_arg(a, 0) else {
        return Value::Int(0);
    };
    let order = vm
        .host
        .order_log
        .iter()
        .rev()
        .find(|(id, _)| *id == unit.id)
        .map(|(_, order)| order.clone())
        .unwrap_or_default();
    let bytes = order.as_bytes();
    if bytes.len() == 4 {
        return Value::Int(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
    }
    Value::Int(0)
}

/// `GetOrderTargetUnit` / `GetOrderTargetDestructable` / `GetOrderTarget` — the
/// widget a target order named. The host records it when the order is issued.
pub(super) fn get_order_target_unit(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.host.last_order_target {
        Some(handle) if handle.kind == HandleKind::Unit => Value::Handle(handle),
        _ => Value::Null,
    }
}

pub(super) fn get_order_target_destructable(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.host.last_order_target {
        Some(handle) if handle.kind == HandleKind::Destructable => Value::Handle(handle),
        _ => Value::Null,
    }
}

pub(super) fn get_order_target_item(vm: &mut JassVm, _a: &[Value]) -> Value {
    match vm.host.last_order_target {
        Some(handle) if handle.kind == HandleKind::Item => Value::Handle(handle),
        _ => Value::Null,
    }
}
