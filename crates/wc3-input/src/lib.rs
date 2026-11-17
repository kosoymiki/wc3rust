//! Touch input for the WC3-style RTS: one gesture/router state machine, the
//! camera projection used by the game and the renderer, and minimap mapping.
//!
//! The router classifies a finger in the canonical WC3 routing order
//! (minimap -> command card -> HUD blockers -> placement -> world) and keeps the
//! multi-finger pinch/two-finger-pan maths. The game owns the effects
//! (selection, orders, HUD activation); it feeds a [`TouchLayout`] snapshot for
//! every event, so the input layer never depends on HUD types.
//!
//! There is no virtual joystick: the canonical RTS camera is driven by drag,
//! pinch, minimap and edge scroll only.

use glam::{Mat4, Vec2, Vec3};

pub mod geometry;
pub mod lifecycle;
pub mod orders;
pub mod remap;
pub mod sdl;
pub mod selection;
pub mod session;
pub mod subgroups;
pub mod trace;

pub use sdl::scancode_char;

// ---------------------------------------------------------------- geometry

/// Screen-space rectangle, field-compatible with the HUD layout rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.w
            && point.y < self.y + self.h
    }

    pub fn centre(&self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Grow the rect by `dx` horizontally and `dy` vertically on both sides.
    pub fn expanded(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x - dx, self.y - dy, self.w + dx * 2.0, self.h + dy * 2.0)
    }
}

/// One tappable HUD button. `index` is the game's button index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchButton {
    pub index: usize,
    pub rect: Rect,
    pub enabled: bool,
}

/// Per-event snapshot of the HUD regions the router may claim.
#[derive(Clone, Copy, Debug)]
pub struct TouchLayout<'a> {
    pub minimap: Option<Rect>,
    /// HUD panels that must swallow a touch instead of passing it to the world.
    pub blocked: &'a [Rect],
    pub buttons: &'a [TouchButton],
    /// True while the game is waiting for a building placement tap.
    pub placing: bool,
    /// HUD scale (finger tolerances scale with it).
    pub scale: f32,
}

impl<'a> TouchLayout<'a> {
    fn slop(&self, value: f32) -> f32 {
        value * self.scale
    }
}

// ---------------------------------------------------------------- constants

/// Finger travel in dp that turns a tap into a pan or a box.
pub const TAP_SLOP: f32 = 12.0;
/// Hold time after which a drag becomes a selection box instead of a pan.
pub const BOX_HOLD_SECONDS: f32 = 0.30;
/// Two taps closer than this are a double tap.
pub const DOUBLE_TAP_SECONDS: f32 = 0.36;
/// Maximum distance between the two taps of a double tap (scaled px).
pub const DOUBLE_TAP_DISTANCE: f32 = 36.0;
/// Extra finger-sized padding around HUD buttons, in dp. The HUD layout
/// already guarantees 48 dp rects, so this only closes the 8 dp seam between
/// neighbours (48 + 2*4 = 56 = one 48 dp cell plus one 8 dp gap): the expanded
/// bands tile exactly and a nearest-centre resolve can never be ambiguous.
pub const BUTTON_HIT_SLOP: f32 = 4.0;
/// Minimum square hit target for a HUD button: Android's 48 dp floor (TZ §8).
/// Expressed in dp; [`TouchLayout::scale`] is px-per-dp, so `slop()` turns it
/// into framebuffer pixels. (The retired 108 px constant was 36 dp at density
/// 3.0 — below the platform minimum.)
pub const MIN_BUTTON_TARGET: f32 = 48.0;
/// A pressed button cancels once the finger is this far from its centre.
pub const BUTTON_RELEASE_SLOP: f32 = 24.0;
/// Hold time on the minimap that turns a camera jump into a move order.
pub const MINIMAP_ORDER_HOLD_SECONDS: f32 = 0.45;
/// Stationary hold that opens a tooltip (world unit or command button).
pub const LONG_PRESS_SECONDS: f32 = 0.45;
/// Auto-scroll band at the safe-area edge while a box drag is held.
pub const EDGE_PAN_MARGIN: f32 = 30.0;
/// Full-deflection edge auto-scroll speed in scaled pixels per second.
pub const EDGE_PAN_PIXELS_PER_SECOND: f32 = 720.0;
/// Closest and farthest camera distance (world units).
pub const MIN_DISTANCE: f32 = 700.0;
pub const MAX_DISTANCE: f32 = 3600.0;

// ---------------------------------------------------------------- events

/// What a pointer-down landed on. The game applies the effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TouchStart {
    Minimap,
    Button(usize),
    /// HUD panel or dead space: swallow the gesture.
    Blocked,
    Place,
    World,
}

/// Continuous movement of the active pointer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TouchMove {
    /// One-finger camera drag; `delta` is `previous - current` pixels, the same
    /// convention as [`Camera::pan_pixels`].
    Pan(Vec2),
    /// Minimap scrub; the game maps the point to world space.
    Minimap(Vec2),
}

/// The completed gesture.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TouchEnd {
    Tap { at: Vec2, double: bool },
    /// A command button was released while still inside it. `hold` marks a
    /// stationary long press (tooltip/assign gesture) and `double` a second tap
    /// on the same button within the double-tap window.
    Button { index: usize, double: bool, hold: bool },
    Minimap { at: Vec2, hold: bool },
    Box { start: Vec2, end: Vec2 },
    Place { at: Vec2 },
}

/// Result of a pinch update: the clamped camera distance plus the two-finger
/// pan delta (`previous centre - current centre`, same convention as drags).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinchUpdate {
    pub distance: f32,
    pub pan: Vec2,
}

// ---------------------------------------------------------------- hotkeys

/// Keyboard parity for the WC3 command card. The platform layer converts SDL
/// key codes into this small set; the game resolves the action against its
/// current card, so a letter can never fire a hidden command.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hotkey {
    Digit(u8),
    Letter(char),
    Escape,
}

impl Hotkey {
    pub fn parse(key: char) -> Option<Self> {
        match key {
            '0'..='9' => Some(Hotkey::Digit(key as u8 - b'0')),
            'a'..='z' => Some(Hotkey::Letter(key)),
            'A'..='Z' => Some(Hotkey::Letter(key.to_ascii_lowercase())),
            _ => None,
        }
    }
}

/// SDL3 physical scancode -> hotkey character. Scancodes are layout
/// independent (`SDL_SCANCODE_A = 4 … Z = 29`, `1 = 30 … 9 = 38`, `0 = 39`),
/// so the platform forwards the same WC3 keys on any keyboard and Shift never
/// turns group `1` into `!`.
///
/// Re-exported from [`sdl`], where the table is pinned against the SDL 3.5.0
/// donor (`include/SDL3/SDL_scancode.h`).
// (the function lives in `sdl::scancode_char` and is re-exported at the top)

/// WC3 control groups 1..9,0 (10 groups), shared by keyboard digits and the
/// ARMY-chip touch gestures. Slot 0 maps to the 10th group (index 9). Unit ids
/// stay `u32` so this crate has no simulation types.
#[derive(Clone, Debug, Default)]
pub struct ControlGroups {
    groups: [Vec<u32>; 10],
}

impl ControlGroups {
    fn index(slot: u8) -> Option<usize> {
        match slot {
            1..=9 => Some(slot as usize - 1),
            0 => Some(9),
            _ => None,
        }
    }

    fn slot_order() -> impl Iterator<Item = u8> {
        (1..=9).chain(std::iter::once(0))
    }

    pub fn assign(&mut self, slot: u8, units: &[u32]) -> bool {
        let Some(index) = Self::index(slot) else {
            return false;
        };
        self.groups[index] = units.to_vec();
        true
    }

    pub fn recall(&self, slot: u8) -> Option<&[u32]> {
        let index = Self::index(slot)?;
        let group = self.groups.get(index)?;
        if group.is_empty() { None } else { Some(group.as_slice()) }
    }

    pub fn clear(&mut self, slot: u8) {
        if let Some(index) = Self::index(slot)
            && let Some(group) = self.groups.get_mut(index)
        {
            group.clear();
        }
    }

    pub fn is_empty(&self, slot: u8) -> bool {
        self.recall(slot).is_none()
    }

    pub fn len(&self, slot: u8) -> usize {
        self.recall(slot).map_or(0, <[u32]>::len)
    }

    /// Lowest empty slot in 1..9,0 order (the touch "save" gesture target).
    pub fn next_free(&self) -> Option<u8> {
        Self::slot_order().find(|slot| self.is_empty(*slot))
    }

    /// Cyclic next non-empty slot after `after` in 1..9,0 order.
    pub fn next_non_empty(&self, after: u8) -> Option<u8> {
        let slots: Vec<u8> = Self::slot_order().collect();
        let pos = slots.iter().position(|&slot| slot == after).unwrap_or(slots.len() - 1);
        (1..=slots.len())
            .map(|step| slots[(pos + step) % slots.len()])
            .find(|slot| !self.is_empty(*slot))
    }

    pub fn is_empty_all(&self) -> bool {
        self.groups.iter().all(Vec::is_empty)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Zone {
    Minimap,
    Button(usize),
    Blocked,
    Place,
    World,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DragMode {
    Pending,
    Pan,
    Box,
}

#[derive(Clone, Copy, Debug)]
struct PointerState {
    down: Vec2,
    last: Vec2,
    started: f32,
    zone: Zone,
    mode: DragMode,
    moved: bool,
    pressed_button: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct PinchState {
    start_distance: f32,
    start_zoom: f32,
    last_centre: Vec2,
    /// A real pan/zoom happened; a pinch that never moves is a two-finger tap
    /// (the desktop right-click on touch).
    moved: bool,
}

// ---------------------------------------------------------------- router

/// The single touch recognizer/router. Times are seconds on any monotonic
/// clock owned by the host; the layer never reads a clock itself.
#[derive(Clone, Debug, Default)]
pub struct TouchInput {
    pointer: Option<PointerState>,
    pinch: Option<PinchState>,
    last_tap: Option<(Vec2, f32)>,
    last_button: Option<(usize, f32)>,
    /// Host modifier: Shift on a keyboard (or a "select-add" HUD toggle).
    /// The router reports gestures unchanged; the game reads this at release to
    /// merge the result into the selection instead of replacing it (WC3
    /// Shift-click / Shift-box). Independent of the pointer, so it survives
    /// `pointer_up`.
    additive: bool,
}

impl TouchInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the host "add to selection" modifier (Shift).
    pub fn set_additive(&mut self, additive: bool) {
        self.additive = additive;
    }

    /// True while the host holds the add-to-selection modifier.
    pub fn additive(&self) -> bool {
        self.additive
    }

    /// Begin a one-finger gesture. Returns what the touch landed on.
    pub fn pointer_down(
        &mut self,
        point: Vec2,
        time: f32,
        layout: &TouchLayout<'_>,
    ) -> TouchStart {
        let zone = route(point, layout);
        if zone == Zone::Blocked {
            self.pointer = None;
            return TouchStart::Blocked;
        }
        let pressed_button = match zone {
            Zone::Button(index) => Some(index),
            _ => None,
        };
        self.pointer = Some(PointerState {
            down: point,
            last: point,
            started: time,
            zone,
            mode: DragMode::Pending,
            moved: false,
            pressed_button,
        });
        match zone {
            Zone::Minimap => TouchStart::Minimap,
            Zone::Button(index) => TouchStart::Button(index),
            Zone::Blocked => TouchStart::Blocked,
            Zone::Place => TouchStart::Place,
            Zone::World => TouchStart::World,
        }
    }

    /// Track the active pointer. `None` means "no camera/event this move".
    pub fn pointer_move(
        &mut self,
        point: Vec2,
        time: f32,
        layout: &TouchLayout<'_>,
    ) -> Option<TouchMove> {
        let pointer = self.pointer.as_mut()?;
        if point == pointer.last {
            return None;
        }
        let delta = pointer.last - point;
        pointer.moved = true;
        pointer.last = point;
        match pointer.zone {
            Zone::Button(index) => {
                if pointer.pressed_button == Some(index)
                    && let Some(button) = layout.buttons.iter().find(|button| button.index == index)
                    && !button.rect.contains(point)
                    && point.distance(button.rect.centre())
                        > layout.slop(BUTTON_RELEASE_SLOP)
                {
                    pointer.pressed_button = None;
                }
                None
            }
            Zone::Minimap => Some(TouchMove::Minimap(point)),
            Zone::World => {
                if pointer.mode == DragMode::Pending
                    && point.distance(pointer.down) > layout.slop(TAP_SLOP)
                {
                    pointer.mode = if time - pointer.started >= BOX_HOLD_SECONDS {
                        DragMode::Box
                    } else {
                        DragMode::Pan
                    };
                }
                if pointer.mode == DragMode::Pan {
                    Some(TouchMove::Pan(delta))
                } else {
                    None
                }
            }
            Zone::Blocked | Zone::Place => None,
        }
    }

    /// Finish the active pointer. `None` means no effect (a pan ended, a button
    /// was cancelled, a blocked panel consumed the touch, ...).
    pub fn pointer_up(
        &mut self,
        point: Vec2,
        time: f32,
        layout: &TouchLayout<'_>,
    ) -> Option<TouchEnd> {
        let pointer = self.pointer.take()?;
        match pointer.zone {
            Zone::Button(index) => {
                if pointer.pressed_button != Some(index) {
                    return None;
                }
                let button = layout.buttons.iter().find(|button| button.index == index)?;
                if !button.enabled || !button.rect.contains(point) {
                    return None;
                }
                let hold = time - pointer.started >= LONG_PRESS_SECONDS;
                let double = self.last_button.is_some_and(|(last_index, last_time)| {
                    last_index == index && time - last_time <= DOUBLE_TAP_SECONDS
                });
                self.last_button = Some((index, time));
                Some(TouchEnd::Button {
                    index,
                    double,
                    hold,
                })
            }
            Zone::Minimap => Some(TouchEnd::Minimap {
                at: point,
                hold: time - pointer.started >= MINIMAP_ORDER_HOLD_SECONDS,
            }),
            Zone::Place => {
                if pointer.moved {
                    None
                } else {
                    Some(TouchEnd::Place { at: point })
                }
            }
            Zone::Blocked => None,
            Zone::World => match pointer.mode {
                DragMode::Pan => None,
                DragMode::Box => {
                    if pointer.moved {
                        Some(TouchEnd::Box {
                            start: pointer.down,
                            end: point,
                        })
                    } else {
                        Some(self.tap(point, time, layout.scale))
                    }
                }
                DragMode::Pending => {
                    if pointer.moved {
                        None
                    } else {
                        Some(self.tap(point, time, layout.scale))
                    }
                }
            },
        }
    }

    /// Drop the pointer and pinch without emitting an end event.
    pub fn cancel(&mut self) {
        self.pointer = None;
        self.pinch = None;
    }

    /// Full reset for a new scenario: also forgets the double-tap history.
    pub fn reset(&mut self) {
        self.cancel();
        self.last_tap = None;
        self.last_button = None;
    }

    /// The second finger was lifted: keep panning with the finger that remains.
    pub fn resume_pan(&mut self, point: Vec2, time: f32) {
        self.pinch = None;
        self.pointer = Some(PointerState {
            down: point,
            last: point,
            started: time,
            zone: Zone::World,
            mode: DragMode::Pan,
            moved: true,
            pressed_button: None,
        });
    }

    /// Start a two-finger gesture. `camera_distance` is the current zoom.
    pub fn pinch_begin(&mut self, distance: f32, centre: Vec2, camera_distance: f32) {
        self.pointer = None;
        self.pinch = Some(PinchState {
            start_distance: distance.max(1.0),
            start_zoom: camera_distance,
            last_centre: centre,
            moved: false,
        });
    }

    /// Update pinch zoom and two-finger pan together.
    pub fn pinch_update(&mut self, distance: f32, centre: Vec2) -> Option<PinchUpdate> {
        let pinch = self.pinch.as_mut()?;
        let factor = (pinch.start_distance / distance.max(1.0)).clamp(0.5, 2.0);
        let distance = (pinch.start_zoom * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        let pan = pinch.last_centre - centre;
        if pan.length() > 2.0 || (pinch.start_distance - distance).abs() > 8.0 {
            pinch.moved = true;
        }
        pinch.last_centre = centre;
        Some(PinchUpdate { distance, pan })
    }

    /// End the two-finger gesture. Returns the centroid when the fingers never
    /// panned or zoomed (a two-finger tap = the touch right-click); `None` after
    /// a real pan/zoom.
    pub fn pinch_end(&mut self) -> Option<Vec2> {
        let pinch = self.pinch.take()?;
        if pinch.moved {
            None
        } else {
            Some(pinch.last_centre)
        }
    }

    /// Camera pixel delta for edge auto-scroll while a box drag is held.
    /// `bounds` is the safe-area rect; callers pass the result to
    /// [`Camera::pan_pixels`].
    pub fn edge_pan(&self, dt: f32, bounds: Rect, scale: f32) -> Vec2 {
        let Some(pointer) = self.pointer.as_ref() else {
            return Vec2::ZERO;
        };
        if pointer.zone != Zone::World || pointer.mode != DragMode::Box {
            return Vec2::ZERO;
        }
        let margin = EDGE_PAN_MARGIN * scale;
        if margin <= 0.0 || dt <= 0.0 {
            return Vec2::ZERO;
        }
        let left = bounds.x;
        let top = bounds.y;
        let right = bounds.right();
        let bottom = bounds.bottom();
        let mut depth = Vec2::ZERO;
        if pointer.last.x < left + margin {
            depth.x = -((left + margin - pointer.last.x) / margin).clamp(0.0, 1.0);
        } else if pointer.last.x > right - margin {
            depth.x = ((pointer.last.x - (right - margin)) / margin).clamp(0.0, 1.0);
        }
        if pointer.last.y < top + margin {
            depth.y = -((top + margin - pointer.last.y) / margin).clamp(0.0, 1.0);
        } else if pointer.last.y > bottom - margin {
            depth.y = ((pointer.last.y - (bottom - margin)) / margin).clamp(0.0, 1.0);
        }
        if depth == Vec2::ZERO {
            return Vec2::ZERO;
        }
        -depth * EDGE_PAN_PIXELS_PER_SECOND * scale * dt
    }

    /// Selection-box corners while a box drag is in progress (HUD drawing).
    pub fn box_rect(&self) -> Option<(Vec2, Vec2)> {
        let pointer = self.pointer.as_ref()?;
        if pointer.zone == Zone::World && pointer.mode == DragMode::Box {
            Some((pointer.down, pointer.last))
        } else {
            None
        }
    }

    /// Stationary hold on the world: the host may show a unit tooltip. Returns
    /// the down point while the finger is still on the ground and has not
    /// moved; `None` as soon as the drag/box/pan begins or the finger lifts.
    pub fn long_press(&self, time: f32) -> Option<Vec2> {
        let pointer = self.pointer.as_ref()?;
        if pointer.zone != Zone::World || pointer.mode != DragMode::Pending || pointer.moved {
            return None;
        }
        if time - pointer.started < LONG_PRESS_SECONDS {
            return None;
        }
        Some(pointer.down)
    }

    /// Button currently held down (for pressed-state rendering).
    pub fn pressed_button(&self) -> Option<usize> {
        self.pointer.as_ref().and_then(|pointer| pointer.pressed_button)
    }

    pub fn is_active(&self) -> bool {
        self.pointer.is_some()
    }

    pub fn is_pinching(&self) -> bool {
        self.pinch.is_some()
    }

    fn tap(&mut self, point: Vec2, time: f32, scale: f32) -> TouchEnd {
        let double = self.last_tap.is_some_and(|(last, tap_time)| {
            time - tap_time <= DOUBLE_TAP_SECONDS
                && last.distance(point) <= DOUBLE_TAP_DISTANCE * scale
        });
        self.last_tap = Some((point, time));
        TouchEnd::Tap { at: point, double }
    }
}

fn route(point: Vec2, layout: &TouchLayout<'_>) -> Zone {
    if layout.minimap.is_some_and(|rect| rect.contains(point)) {
        return Zone::Minimap;
    }
    if let Some(index) = hit_button(point, layout) {
        return Zone::Button(index);
    }
    if layout.blocked.iter().any(|rect| rect.contains(point)) {
        return Zone::Blocked;
    }
    if layout.placing {
        return Zone::Place;
    }
    Zone::World
}

/// Exact rect first, then a finger-sized band; the nearest centre wins so two
/// close buttons never both claim the same touch. Public for HUD diagnostics
/// and for hosts that want the same hit test the router uses.
pub fn hit_button(point: Vec2, layout: &TouchLayout<'_>) -> Option<usize> {
    if let Some(button) = layout.buttons.iter().find(|button| button.rect.contains(point)) {
        return Some(button.index);
    }
    let slop = layout.slop(BUTTON_HIT_SLOP);
    let min_target = layout.slop(MIN_BUTTON_TARGET);
    let mut best: Option<(usize, f32)> = None;
    for button in layout.buttons {
        let rect = button.rect;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let pad_x = ((min_target - rect.w) * 0.5).max(slop);
        let pad_y = ((min_target - rect.h) * 0.5).max(slop);
        if !rect.expanded(pad_x, pad_y).contains(point) {
            continue;
        }
        let distance = rect.centre().distance(point);
        if best.map(|(_, best_distance)| distance < best_distance).unwrap_or(true) {
            best = Some((button.index, distance));
        }
    }
    best.map(|(index, _)| index)
}

// ---------------------------------------------------------------- camera

/// RTS camera: fixed isometric-style pitch, screen/world projection for picking
/// and HUD overlays, pinch zoom, and sub-pixel accumulated panning.
///
/// This is the canonical camera for `wc3-game` (re-exported as
/// `wc3_game::camera::Camera`) and the touch layer.
#[derive(Clone, Copy)]
pub struct Camera {
    /// Centre of the view on the ground plane (world XZ).
    pub target: Vec2,
    /// Eye distance from the target.
    pub distance: f32,
    /// Viewport size in pixels (updated by the app).
    pub viewport: Vec2,
    /// World size in world units.
    pub world: Vec2,
    /// Left-over sub-pixel pan, so slow drags do not jitter.
    pub pan_residual: Vec2,
}

impl Camera {
    pub fn new(world: Vec2) -> Self {
        Self {
            target: world * 0.5,
            distance: 1500.0,
            viewport: Vec2::new(1280.0, 720.0),
            world,
            pan_residual: Vec2::ZERO,
        }
    }

    pub fn eye(&self) -> Vec3 {
        Vec3::new(
            self.target.x,
            self.distance * 0.92,
            self.target.y + self.distance * 0.78,
        )
    }

    pub fn view_proj(&self) -> Mat4 {
        let aspect = (self.viewport.x / self.viewport.y.max(1.0)).max(0.2);
        let projection = Mat4::perspective_rh(45f32.to_radians(), aspect, 10.0, 40_000.0);
        let view = Mat4::look_at_rh(
            self.eye(),
            Vec3::new(self.target.x, 0.0, self.target.y),
            Vec3::Y,
        );
        projection * view
    }

    pub fn viewport(&mut self, width: f32, height: f32) {
        self.viewport = Vec2::new(width.max(1.0), height.max(1.0));
    }

    /// World units covered by one screen pixel at the target plane.
    pub fn world_per_pixel(&self) -> f32 {
        let visible = 2.0 * self.distance * (45f32 * 0.5).to_radians().tan();
        visible / self.viewport.y.max(1.0)
    }

    /// Pan by a screen-space delta (pixels) with sub-pixel accumulation.
    pub fn pan_pixels(&mut self, delta: Vec2) {
        let speed = self.world_per_pixel();
        let movement = delta * speed + self.pan_residual;
        let whole = Vec2::new(movement.x.trunc(), movement.y.trunc());
        self.pan_residual = movement - whole;
        self.target -= whole;
        self.clamp_target();
    }

    pub fn clamp_target(&mut self) {
        let margin = 200.0;
        self.target.x = self.target.x.clamp(-margin, self.world.x + margin);
        self.target.y = self.target.y.clamp(-margin, self.world.y + margin);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    pub fn centre_on(&mut self, world: Vec2) {
        self.target = world;
        self.clamp_target();
    }

    /// Project a world point into viewport pixels (None when behind the camera).
    pub fn world_to_screen(&self, point: Vec3) -> Option<Vec2> {
        let clip = self.view_proj() * point.extend(1.0);
        if clip.w <= 0.0001 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.z < 0.0 || ndc.z > 1.0 {
            return None;
        }
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * self.viewport.x,
            (0.5 - ndc.y * 0.5) * self.viewport.y,
        ))
    }

    /// Screen pixels to a point on the terrain. `height_at` is sampled twice so
    /// hills pick correctly instead of always intersecting y=0.
    pub fn screen_to_world(
        &self,
        screen: Vec2,
        height_at: impl Fn(f32, f32) -> f32,
    ) -> Option<Vec2> {
        let inverse = self.view_proj().inverse();
        let ndc = Vec2::new(
            screen.x / self.viewport.x * 2.0 - 1.0,
            1.0 - screen.y / self.viewport.y * 2.0,
        );
        let near = inverse * glam::Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
        let far = inverse * glam::Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        if near.w.abs() < 1e-6 || far.w.abs() < 1e-6 {
            return None;
        }
        let origin = near.truncate() / near.w;
        let end = far.truncate() / far.w;
        let direction = end - origin;
        if direction.y.abs() < 1e-5 {
            return None;
        }
        let mut plane = 0.0f32;
        for _ in 0..2 {
            let t = (plane - origin.y) / direction.y;
            if t < 0.0 {
                return None;
            }
            let point = origin + direction * t;
            plane = height_at(point.x, point.z);
        }
        let t = (plane - origin.y) / direction.y;
        let point = origin + direction * t;
        Some(Vec2::new(point.x, point.z))
    }
}

// ---------------------------------------------------------------- minimap

/// Minimap pixel -> world position (clamped to the map).
pub fn minimap_to_world(point: Vec2, rect: Rect, world: Vec2) -> Vec2 {
    Vec2::new(
        ((point.x - rect.x) / rect.w.max(1.0)).clamp(0.0, 1.0) * world.x,
        ((point.y - rect.y) / rect.h.max(1.0)).clamp(0.0, 1.0) * world.y,
    )
}

/// World position -> minimap pixel (clamped to the map).
pub fn world_to_minimap(world: Vec2, rect: Rect, size: Vec2) -> Vec2 {
    Vec2::new(
        rect.x + (world.x / size.x.max(1.0)).clamp(0.0, 1.0) * rect.w,
        rect.y + (world.y / size.y.max(1.0)).clamp(0.0, 1.0) * rect.h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTONS: [TouchButton; 3] = [
        TouchButton { index: 0, rect: Rect::new(900.0, 600.0, 80.0, 80.0), enabled: true },
        TouchButton { index: 1, rect: Rect::new(1000.0, 600.0, 80.0, 80.0), enabled: true },
        TouchButton { index: 2, rect: Rect::new(1100.0, 600.0, 80.0, 80.0), enabled: false },
    ];

    const BLOCKED: [Rect; 2] = [
        Rect::new(0.0, 0.0, 400.0, 90.0),
        Rect::new(900.0, 400.0, 380.0, 320.0),
    ];

    fn layout<'a>(buttons: &'a [TouchButton], blocked: &'a [Rect]) -> TouchLayout<'a> {
        TouchLayout {
            minimap: Some(Rect::new(20.0, 100.0, 200.0, 140.0)),
            blocked,
            buttons,
            placing: false,
            scale: 1.0,
        }
    }

    #[test]
    fn tap_without_motion_is_a_single_tap() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        assert_eq!(input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l), TouchStart::World);
        assert_eq!(
            input.pointer_up(Vec2::new(640.0, 360.0), 0.08, &l),
            Some(TouchEnd::Tap { at: Vec2::new(640.0, 360.0), double: false })
        );
    }

    #[test]
    fn second_tap_within_the_window_is_a_double_tap() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(640.0, 360.0), 0.05, &l),
            Some(TouchEnd::Tap { at: Vec2::new(640.0, 360.0), double: false })
        );
        input.pointer_down(Vec2::new(642.0, 361.0), 0.2, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(642.0, 361.0), 0.25, &l),
            Some(TouchEnd::Tap { at: Vec2::new(642.0, 361.0), double: true })
        );
    }

    #[test]
    fn quick_drag_pans_and_never_selects() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l);
        let movement = input
            .pointer_move(Vec2::new(640.0, 320.0), 0.05, &l)
            .expect("pan delta");
        assert_eq!(movement, TouchMove::Pan(Vec2::new(0.0, 40.0)));
        assert_eq!(input.pointer_up(Vec2::new(640.0, 320.0), 0.1, &l), None);
    }

    #[test]
    fn held_drag_becomes_a_box_and_edge_scrolls() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l);
        // Held past the box threshold before crossing the slop.
        input.pointer_move(Vec2::new(600.0, 360.0), 0.4, &l);
        assert_eq!(input.box_rect(), Some((Vec2::new(640.0, 360.0), Vec2::new(600.0, 360.0))));
        // Move to the right safe edge: edge scroll points east.
        input.pointer_move(Vec2::new(1276.0, 360.0), 0.5, &l);
        let delta = input.edge_pan(0.1, Rect::new(0.0, 0.0, 1280.0, 720.0), 1.0);
        assert!(delta.x < 0.0, "edge scroll pans the camera east: {delta:?}");
        assert_eq!(
            input.pointer_up(Vec2::new(1276.0, 360.0), 0.6, &l),
            Some(TouchEnd::Box {
                start: Vec2::new(640.0, 360.0),
                end: Vec2::new(1276.0, 360.0)
            })
        );
    }

    #[test]
    fn minimap_tap_jumps_and_hold_orders() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        assert_eq!(input.pointer_down(Vec2::new(120.0, 170.0), 0.0, &l), TouchStart::Minimap);
        assert_eq!(
            input.pointer_up(Vec2::new(120.0, 170.0), 0.2, &l),
            Some(TouchEnd::Minimap { at: Vec2::new(120.0, 170.0), hold: false })
        );
        input.pointer_down(Vec2::new(120.0, 170.0), 1.0, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(120.0, 170.0), 1.6, &l),
            Some(TouchEnd::Minimap { at: Vec2::new(120.0, 170.0), hold: true })
        );
        let world = minimap_to_world(Vec2::new(120.0, 170.0), Rect::new(20.0, 100.0, 200.0, 140.0), Vec2::new(4096.0, 4096.0));
        assert!((world.x - 2048.0).abs() < 1.0 && (world.y - 2048.0).abs() < 1.0);
    }

    #[test]
    fn command_card_press_activates_only_inside_the_target() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        assert_eq!(
            input.pointer_down(Vec2::new(1040.0, 640.0), 0.0, &l),
            TouchStart::Button(1)
        );
        assert_eq!(input.pressed_button(), Some(1));
        assert_eq!(
            input.pointer_up(Vec2::new(1040.0, 640.0), 0.1, &l),
            Some(TouchEnd::Button {
                index: 1,
                double: false,
                hold: false
            })
        );

        // Dragging off cancels the press.
        input.pointer_down(Vec2::new(1040.0, 640.0), 0.2, &l);
        input.pointer_move(Vec2::new(700.0, 200.0), 0.25, &l);
        assert_eq!(input.pressed_button(), None);
        assert_eq!(input.pointer_up(Vec2::new(700.0, 200.0), 0.3, &l), None);

        // Disabled buttons never activate.
        input.pointer_down(Vec2::new(1140.0, 640.0), 0.4, &l);
        assert_eq!(input.pointer_up(Vec2::new(1140.0, 640.0), 0.5, &l), None);
    }

    #[test]
    fn stationary_long_press_reports_the_down_point() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l);
        assert_eq!(input.long_press(0.2), None, "too early");
        assert_eq!(
            input.long_press(LONG_PRESS_SECONDS + 0.1),
            Some(Vec2::new(640.0, 360.0))
        );
        input.pointer_move(Vec2::new(645.0, 360.0), 0.6, &l);
        assert_eq!(input.long_press(0.7), None, "movement cancels the tooltip");
        assert_eq!(input.pointer_up(Vec2::new(645.0, 360.0), 0.8, &l), None);
        assert_eq!(input.long_press(0.9), None, "release clears it");
    }

    #[test]
    fn button_release_reports_hold_and_double() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(1040.0, 640.0), 0.0, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(1040.0, 640.0), 0.6, &l),
            Some(TouchEnd::Button {
                index: 1,
                double: false,
                hold: true
            })
        );
        input.pointer_down(Vec2::new(1040.0, 640.0), 0.8, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(1040.0, 640.0), 0.9, &l),
            Some(TouchEnd::Button {
                index: 1,
                double: true,
                hold: false
            })
        );
    }

    #[test]
    fn scancode_chars_match_the_sdl_keyboard() {
        // SDL_SCANCODE_A = 4 .. Z = 29, 1 = 30 .. 9 = 38, 0 = 39.
        assert_eq!(scancode_char(4), Some('a'));
        assert_eq!(scancode_char(29), Some('z'));
        assert_eq!(scancode_char(30), Some('1'));
        assert_eq!(scancode_char(38), Some('9'));
        assert_eq!(scancode_char(39), Some('0'), "WC3 group 0");
        assert_eq!(scancode_char(41), None, "Escape is not a character hotkey");
        assert_eq!(scancode_char(0), None);
        // The platform path and the character path agree for every group key.
        for digit in 0..=9u8 {
            let scancode = if digit == 0 { 39 } else { 29 + u32::from(digit) };
            let ch = scancode_char(scancode).expect("digit scancode");
            assert_eq!(Hotkey::parse(ch), Some(Hotkey::Digit(digit)));
        }
    }

    #[test]
    fn hotkey_parse_and_control_group_cycle() {
        assert_eq!(Hotkey::parse('1'), Some(Hotkey::Digit(1)));
        assert_eq!(Hotkey::parse('9'), Some(Hotkey::Digit(9)));
        assert_eq!(Hotkey::parse('0'), Some(Hotkey::Digit(0)), "WC3 group 0 (10th)");
        assert_eq!(Hotkey::parse('B'), Some(Hotkey::Letter('b')));
        assert_eq!(Hotkey::parse('+'), None);

        let mut groups = ControlGroups::default();
        assert!(groups.is_empty_all());
        assert_eq!(groups.next_free(), Some(1));
        assert!(groups.assign(2, &[7, 9]));
        assert_eq!(groups.recall(2), Some([7u32, 9u32].as_slice()));
        assert_eq!(groups.len(2), 2);
        assert_eq!(groups.next_free(), Some(1));
        assert_eq!(groups.next_non_empty(1), Some(2));
        assert_eq!(groups.next_non_empty(2), Some(2), "single group cycles to itself");
        groups.clear(2);
        assert!(groups.recall(2).is_none());
        assert_eq!(groups.next_non_empty(1), None);
        assert!(groups.assign(1, &[3]));
        assert_eq!(groups.next_non_empty(9), Some(1));
        // Slot 0 is the 10th group (1..9,0 order).
        assert!(groups.assign(0, &[42]));
        assert_eq!(groups.recall(0), Some([42u32].as_slice()));
        assert_eq!(groups.len(0), 1);
        assert_eq!(groups.next_non_empty(9), Some(0), "after 9 comes 0");
        assert_eq!(groups.next_non_empty(0), Some(1), "after 0 wraps to 1");
    }

    #[test]
    fn button_band_claims_a_finger_just_outside_the_rect() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        let above = Vec2::new(1040.0, 596.0);
        assert_eq!(input.pointer_down(above, 0.0, &l), TouchStart::Button(1));
    }

    #[test]
    fn hud_blockers_swallow_the_touch() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        assert_eq!(input.pointer_down(Vec2::new(100.0, 40.0), 0.0, &l), TouchStart::Blocked);
        assert!(!input.is_active());
        assert_eq!(input.pointer_up(Vec2::new(100.0, 40.0), 0.1, &l), None);
    }

    #[test]
    fn placement_wins_over_the_world_and_requires_no_drag() {
        let mut input = TouchInput::new();
        let mut l = layout(&BUTTONS, &BLOCKED);
        l.placing = true;
        assert_eq!(input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l), TouchStart::Place);
        assert_eq!(
            input.pointer_up(Vec2::new(640.0, 360.0), 0.1, &l),
            Some(TouchEnd::Place { at: Vec2::new(640.0, 360.0) })
        );
        input.pointer_down(Vec2::new(640.0, 360.0), 0.2, &l);
        input.pointer_move(Vec2::new(700.0, 400.0), 0.25, &l);
        assert_eq!(input.pointer_up(Vec2::new(700.0, 400.0), 0.3, &l), None);
    }

    #[test]
    fn pinch_zooms_and_two_finger_pan_moves_the_camera() {
        let mut input = TouchInput::new();
        input.pinch_begin(200.0, Vec2::new(640.0, 360.0), 1500.0);
        let update = input
            .pinch_update(400.0, Vec2::new(640.0, 360.0))
            .expect("pinch update");
        assert!(update.distance < 1500.0, "spread fingers zoom in: {}", update.distance);
        assert_eq!(update.pan, Vec2::ZERO);
        let update = input
            .pinch_update(400.0, Vec2::new(700.0, 360.0))
            .expect("two-finger pan");
        assert_eq!(update.pan, Vec2::new(-60.0, 0.0));
        // Pinch distance is clamped to the camera range.
        let far = input.pinch_update(1.0, Vec2::new(700.0, 360.0)).unwrap();
        assert!(far.distance <= MAX_DISTANCE);
        let near = input.pinch_update(10_000.0, Vec2::new(700.0, 360.0)).unwrap();
        assert!(near.distance >= MIN_DISTANCE);
        input.pinch_end();
        assert!(input.pinch_update(100.0, Vec2::ZERO).is_none());
    }

    #[test]
    fn second_finger_lift_resumes_panning() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.resume_pan(Vec2::new(600.0, 300.0), 2.0);
        assert!(input.is_active());
        let movement = input.pointer_move(Vec2::new(600.0, 240.0), 2.1, &l);
        assert_eq!(movement, Some(TouchMove::Pan(Vec2::new(0.0, 60.0))));
        assert_eq!(input.pointer_up(Vec2::new(600.0, 240.0), 2.2, &l), None);
    }

    #[test]
    fn reset_forgets_the_double_tap_history() {
        let mut input = TouchInput::new();
        let l = layout(&BUTTONS, &BLOCKED);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.0, &l);
        input.pointer_up(Vec2::new(640.0, 360.0), 0.05, &l);
        input.pointer_down(Vec2::new(640.0, 360.0), 0.1, &l);
        input.reset();
        input.pointer_down(Vec2::new(640.0, 360.0), 0.15, &l);
        assert_eq!(
            input.pointer_up(Vec2::new(640.0, 360.0), 0.2, &l),
            Some(TouchEnd::Tap { at: Vec2::new(640.0, 360.0), double: false })
        );
    }

    fn camera() -> Camera {
        let mut camera = Camera::new(Vec2::new(4096.0, 4096.0));
        camera.viewport(1280.0, 720.0);
        camera.centre_on(Vec2::new(2048.0, 2048.0));
        camera
    }

    #[test]
    fn world_per_pixel_scales_with_zoom_distance() {
        let mut camera = camera();
        let near = camera.world_per_pixel();
        camera.distance *= 2.0;
        let far = camera.world_per_pixel();
        assert!((far - near * 2.0).abs() < 1e-3, "near {near} far {far}");
        camera.zoom_by(0.0);
        assert!((camera.distance - MIN_DISTANCE).abs() < 1e-3);
        camera.zoom_by(1e6);
        assert!((camera.distance - MAX_DISTANCE).abs() < 1e-3);
    }

    #[test]
    fn pan_moves_target_opposite_the_drag_and_keeps_subpixels() {
        let mut first = camera();
        let start = first.target;
        first.pan_pixels(Vec2::new(10.0, 0.0));
        assert!(first.target.x < start.x, "drag right moves the view east");
        assert!((first.target.y - start.y).abs() < 1e-6);
        let mut second = camera();
        let start = second.target;
        for _ in 0..100 {
            second.pan_pixels(Vec2::new(1.0, 0.0));
        }
        let moved = start.x - second.target.x;
        let expected = second.world_per_pixel() * 100.0;
        assert!((moved - expected).abs() < 1.0, "moved {moved} expected {expected}");
    }

    #[test]
    fn pan_is_clamped_to_the_world_with_a_margin() {
        let mut camera = camera();
        for _ in 0..10_000 {
            camera.pan_pixels(Vec2::new(-100.0, -100.0));
        }
        assert!(camera.target.x <= camera.world.x + 200.0 + 1e-3);
        assert!(camera.target.y <= camera.world.y + 200.0 + 1e-3);
        assert!(camera.target.x >= -200.0 - 1e-3);
        assert!(camera.target.y >= -200.0 - 1e-3);
    }

    #[test]
    fn screen_centre_round_trips_to_the_camera_target() {
        let camera = camera();
        let centre = camera.viewport * 0.5;
        let world = camera
            .screen_to_world(centre, |_, _| 0.0)
            .expect("centre ray hits the ground");
        assert!(world.distance(camera.target) < 1.0, "centre {world:?} target {:?}", camera.target);
        let back = camera
            .world_to_screen(Vec3::new(camera.target.x, 0.0, camera.target.y))
            .expect("target is in front of the camera");
        assert!(back.distance(centre) < 1.0, "round trip {back:?}");
    }

    #[test]
    fn screen_to_world_accounts_for_terrain_height() {
        let camera = camera();
        let screen = camera.viewport * 0.5;
        let flat = camera.screen_to_world(screen, |_, _| 0.0).unwrap();
        let hill = camera.screen_to_world(screen, |_, _| 200.0).unwrap();
        assert!(hill.distance(flat) > 1.0, "height must move the pick point");
        assert!(hill.y > flat.y, "a hill is nearer the camera on screen");
    }

    #[test]
    fn centre_on_clamps_into_the_world_margin() {
        let mut camera = camera();
        camera.centre_on(Vec2::new(-50_000.0, 50_000.0));
        assert!(camera.target.x >= -200.0 - 1e-3);
        assert!(camera.target.y <= camera.world.y + 200.0 + 1e-3);
    }

    #[test]
    fn world_to_minimap_round_trips() {
        let rect = Rect::new(20.0, 100.0, 200.0, 140.0);
        let world = Vec2::new(1024.0, 3072.0);
        let size = Vec2::new(4096.0, 4096.0);
        let point = world_to_minimap(world, rect, size);
        let back = minimap_to_world(point, rect, size);
        assert!(back.distance(world) < 1.0, "round trip {back:?}");
    }
}
