//! Device-less replay of touch sessions.
//!
//! A trace is a small line-oriented script: a HUD layout, then the pointer and
//! pinch events a QA run captured, optionally followed by `expect` lines that
//! pin the router's decision. The `touch-trace` example runs one and exits
//! non-zero on the first mismatch, so a routing regression is reproducible
//! without a phone.
//!
//! ```text
//! # world tap
//! layout scale=1.0 minimap=20,100,200,140
//! down 640 360 0.0
//! expect start=world
//! up 640 360 0.1
//! expect up=tap:640.0:360.0:single
//! ```

use std::fmt;

use glam::Vec2;

use crate::orders::{self, OrderArmer, OrderKind, PatrolPlan};
use crate::session::{InputEvent, InputState, RecordingSink};
use crate::{Hotkey, Rect, TouchButton, TouchEnd, TouchInput, TouchLayout, TouchMove, TouchStart};

/// A parse failure with the 1-based source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TraceError {}

/// One replay step. `Expect` pins the event produced by the previous step.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Down { at: Vec2, time: f32 },
    Move { at: Vec2, time: f32 },
    Up { at: Vec2, time: f32 },
    PinchBegin { distance: f32, centre: Vec2, zoom: f32 },
    PinchUpdate { distance: f32, centre: Vec2 },
    PinchEnd,
    LongPress { time: f32 },
    /// Host add-to-selection modifier (Shift): `shift on` / `shift off`.
    Shift { on: bool },
    /// Host right-button state (minimap move order): `right on` / `right off`.
    Right { on: bool },
    /// Card hotkey: arms an order mode (A/M/P/H/S).
    Key { letter: char },
    /// Escape cancels the armed order.
    Escape,
    /// Placement mode: `placing on` / `placing off` (BUILD ghost active).
    Placing { on: bool },
    /// One decoded SDL input event replayed through [`InputState`]:
    /// `touch down|move|up|cancel <id> <x> <y>`, `mouse down|move|up <x> <y>`,
    /// `sdlkey down|up <scancode> <mods> [repeat]`.
    Input(InputEvent),
    /// `focus lost` / `focus gained` (window focus lifecycle).
    Focus { gained: bool },
    /// `rotate <w> <h>`: a display/orientation change invalidates the gesture.
    Rotate { width: f32, height: f32 },
    Expect { line: usize, expected: String },
}

/// The HUD snapshot the router receives for every event.
#[derive(Debug, Clone)]
pub struct TraceLayout {
    pub scale: f32,
    pub placing: bool,
    pub minimap: Option<Rect>,
    pub buttons: Vec<TouchButton>,
    pub blocked: Vec<Rect>,
}

impl Default for TraceLayout {
    fn default() -> Self {
        Self {
            scale: 1.0,
            placing: false,
            minimap: None,
            buttons: Vec::new(),
            blocked: Vec::new(),
        }
    }
}

impl TraceLayout {
    fn snapshot(&self) -> TouchLayout<'_> {
        TouchLayout {
            minimap: self.minimap,
            blocked: &self.blocked,
            buttons: &self.buttons,
            placing: self.placing,
            scale: self.scale,
        }
    }
}

/// A parsed trace: the layout declaration plus the ordered steps.
#[derive(Debug, Clone)]
pub struct Trace {
    pub layout: TraceLayout,
    pub steps: Vec<Step>,
}

/// Result of running a trace: the canonical event log plus expectation
/// failures (empty when the trace passes).
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub events: Vec<String>,
    pub failures: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

impl Trace {
    /// Replay the trace against a fresh [`TouchInput`] plus the platform
    /// [`InputState`] (multi-touch/keyboard/focus/rotation commands).
    pub fn run(&self) -> Report {
        let mut input = TouchInput::new();
        let mut session = InputState::new();
        let mut sink = RecordingSink::new();
        let mut orders = OrderArmer::default();
        let mut patrol = PatrolPlan::default();
        let mut right = false;
        let mut layout_state = self.layout.clone();
        let mut report = Report::default();
        for step in &self.steps {
            let event = match step {
                Step::Down { at, time } => {
                    let layout = layout_state.snapshot();
                    Some(format!("start={}", start_str(input.pointer_down(*at, *time, &layout))))
                }
                Step::Move { at, time } => {
                    let layout = layout_state.snapshot();
                    Some(match input.pointer_move(*at, *time, &layout) {
                        None => "move=none".to_string(),
                        Some(TouchMove::Pan(delta)) => {
                            format!("move=pan:{:.1}:{:.1}", delta.x, delta.y)
                        }
                        Some(TouchMove::Minimap(point)) => {
                            format!("move=minimap:{:.1}:{:.1}", point.x, point.y)
                        }
                    })
                }
                Step::Up { at, time } => {
                    let layout = layout_state.snapshot();
                    Some(match input.pointer_up(*at, *time, &layout) {
                        None => "up=none".to_string(),
                        Some(end) => {
                            let mut event = up_str(end, input.additive());
                            // A world tap consumes an armed order exactly once;
                            // patrol needs two taps (near point, then far point).
                            if event.starts_with("up=tap") {
                                if orders.armed() == Some(OrderKind::Patrol) {
                                    orders.take();
                                    match patrol.tap(*at) {
                                        Some((from, to)) => event.push_str(&format!(
                                            ":order=patrol:second:from:{:.1}:{:.1}:to:{:.1}:{:.1}",
                                            from.x, from.y, to.x, to.y
                                        )),
                                        None => event.push_str(":order=patrol:first"),
                                    }
                                } else if let Some(kind) = orders.take() {
                                    event.push_str(&format!(":order={}", kind.id()));
                                } else if patrol.is_armed() {
                                    match patrol.tap(*at) {
                                        Some((from, to)) => event.push_str(&format!(
                                            ":order=patrol:second:from:{:.1}:{:.1}:to:{:.1}:{:.1}",
                                            from.x, from.y, to.x, to.y
                                        )),
                                        None => event.push_str(":order=patrol:first"),
                                    }
                                }
                            }
                            if let TouchEnd::Minimap { hold, .. } = end {
                                let action = orders::minimap_action(hold, right);
                                event.push_str(&format!(":action:{}", action.id()));
                            }
                            event
                        }
                    })
                }
                Step::PinchBegin { distance, centre, zoom } => {
                    input.pinch_begin(*distance, *centre, *zoom);
                    None
                }
                Step::PinchUpdate { distance, centre } => {
                    input.pinch_update(*distance, *centre).map(|update| {
                        format!(
                            "pinch=distance:{:.1}:pan:{:.1}:{:.1}",
                            update.distance, update.pan.x, update.pan.y
                        )
                    })
                }
                Step::PinchEnd => Some(match input.pinch_end() {
                    Some(centre) => format!("two_finger_tap={:.1}:{:.1}", centre.x, centre.y),
                    None => "two_finger_tap=none".to_string(),
                }),
                Step::LongPress { time } => Some(match input.long_press(*time) {
                    Some(point) => format!("long_press={:.1}:{:.1}", point.x, point.y),
                    None => "long_press=none".to_string(),
                }),
                Step::Shift { on } => {
                    input.set_additive(*on);
                    Some(format!("shift={}", if *on { "on" } else { "off" }))
                }
                Step::Right { on } => {
                    right = *on;
                    Some(format!("right={}", if *on { "on" } else { "off" }))
                }
                Step::Key { letter } => match orders::order_for_hotkey(Hotkey::Letter(*letter)) {
                    Some(kind) => {
                        orders.arm(kind);
                        if kind == OrderKind::Patrol {
                            patrol.arm();
                        }
                        Some(format!("order=armed:{}", kind.id()))
                    }
                    None => Some("key=none".to_string()),
                },
                Step::Escape => Some(match orders.cancel() {
                    Some(kind) => {
                        patrol.cancel();
                        format!("order=cancelled:{}", kind.id())
                    }
                    None if patrol.cancel() => "order=cancelled:patrol".to_string(),
                    None => "order=none".to_string(),
                }),
                Step::Placing { on } => {
                    layout_state.placing = *on;
                    Some(format!("placing={}", if *on { "on" } else { "off" }))
                }
                Step::Input(input_event) => {
                    session.apply(&mut sink, *input_event);
                    Some(format!("touch={}", join_events(sink.take())))
                }
                Step::Focus { gained } => {
                    if !*gained {
                        session.reset(&mut sink);
                    }
                    Some(format!(
                        "focus={}:{}",
                        if *gained { "gained" } else { "lost" },
                        join_events(sink.take())
                    ))
                }
                Step::Rotate { width, height } => {
                    session.rotate(&mut sink);
                    Some(format!(
                        "rotate={:.1}:{:.1}:{}",
                        width,
                        height,
                        join_events(sink.take())
                    ))
                }
                Step::Expect { line, expected } => {
                    match report.events.last() {
                        Some(actual) if actual == expected => {}
                        Some(actual) => report
                            .failures
                            .push(format!("line {line}: expected `{expected}`, got `{actual}`")),
                        None => report
                            .failures
                            .push(format!("line {line}: expected `{expected}` before any event")),
                    }
                    None
                }
            };
            if let Some(event) = event {
                report.events.push(event);
            }
        }
        report
    }
}

fn start_str(start: TouchStart) -> String {
    match start {
        TouchStart::Minimap => "minimap".to_string(),
        TouchStart::Button(index) => format!("button:{index}"),
        TouchStart::Blocked => "blocked".to_string(),
        TouchStart::Place => "place".to_string(),
        TouchStart::World => "world".to_string(),
    }
}

/// Join one step's sink events into a single trace event, so one `expect`
/// line pins the whole step (e.g. `pinch_end+pointer_resume_pan:...`).
fn join_events(events: Vec<String>) -> String {
    if events.is_empty() {
        return "none".to_string();
    }
    events
        .into_iter()
        .map(|event| event.strip_prefix("touch=").map(str::to_string).unwrap_or(event))
        .collect::<Vec<_>>()
        .join("+")
}

fn up_str(end: TouchEnd, additive: bool) -> String {
    let add = if additive { ":add" } else { "" };
    match end {
        TouchEnd::Tap { at, double } => format!(
            "up=tap:{:.1}:{:.1}:{}{add}",
            at.x,
            at.y,
            if double { "double" } else { "single" }
        ),
        TouchEnd::Button { index, double, hold } => {
            format!("up=button:{index}:hold:{hold}:double:{double}")
        }
        TouchEnd::Minimap { at, hold } => {
            format!("up=minimap:{:.1}:{:.1}:hold:{hold}", at.x, at.y)
        }
        TouchEnd::Box { start, end } => format!(
            "up=box:{:.1}:{:.1}:{:.1}:{:.1}{add}",
            start.x, start.y, end.x, end.y
        ),
        TouchEnd::Place { at } => format!("up=place:{:.1}:{:.1}", at.x, at.y),
    }
}

/// Parse a trace. Unknown commands and malformed numbers report the line.
pub fn parse(text: &str) -> Result<Trace, TraceError> {
    let mut layout = TraceLayout::default();
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let line_text = raw.split('#').next().unwrap_or("").trim();
        if line_text.is_empty() {
            continue;
        }
        let mut parts = line_text.split_whitespace();
        let command = parts.next().unwrap_or("");
        let error = |message: String| TraceError { line, message };
        match command {
            "layout" => {
                for token in parts {
                    let (key, value) = token
                        .split_once('=')
                        .ok_or_else(|| error(format!("layout token `{token}` is not key=value")))?;
                    match key {
                        "scale" => {
                            layout.scale = value
                                .parse::<f32>()
                                .map_err(|_| error(format!("bad scale `{value}`")))?;
                        }
                        "placing" => {
                            layout.placing = match value {
                                "true" | "1" => true,
                                "false" | "0" => false,
                                _ => return Err(error(format!("bad placing `{value}`"))),
                            };
                        }
                        "minimap" => {
                            layout.minimap = Some(parse_rect(value).map_err(|m| error(format!("minimap: {m}")))?);
                        }
                        _ => return Err(error(format!("unknown layout key `{key}`"))),
                    }
                }
            }
            "button" => {
                let index = parts
                    .next()
                    .ok_or_else(|| error("button needs an index".to_string()))?
                    .parse::<usize>()
                    .map_err(|_| error("button index is not a number".to_string()))?;
                let rect = parts
                    .next()
                    .ok_or_else(|| error("button needs a rect".to_string()))?;
                let enabled = match parts.next().unwrap_or("on") {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    other => return Err(error(format!("button enabled flag `{other}`"))),
                };
                if layout.buttons.iter().any(|button| button.index == index) {
                    return Err(error(format!("duplicate button index {index}")));
                }
                layout.buttons.push(TouchButton {
                    index,
                    rect: parse_rect(rect).map_err(|m| error(format!("button: {m}")))?,
                    enabled,
                });
            }
            "blocked" => {
                let rect = parts
                    .next()
                    .ok_or_else(|| error("blocked needs a rect".to_string()))?;
                layout
                    .blocked
                    .push(parse_rect(rect).map_err(|m| error(format!("blocked: {m}")))?);
            }
            "down" | "move" | "up" => {
                let at = parse_point(&mut parts).map_err(&error)?;
                let time = parse_f32(parts.next(), "time").map_err(&error)?;
                steps.push(match command {
                    "down" => Step::Down { at, time },
                    "move" => Step::Move { at, time },
                    _ => Step::Up { at, time },
                });
            }
            "pinch_begin" => {
                let distance = parse_f32(parts.next(), "distance").map_err(&error)?;
                let centre = parse_point(&mut parts).map_err(&error)?;
                let zoom = parse_f32(parts.next(), "zoom").map_err(&error)?;
                steps.push(Step::PinchBegin {
                    distance,
                    centre,
                    zoom,
                });
            }
            "pinch_update" => {
                let distance = parse_f32(parts.next(), "distance").map_err(&error)?;
                let centre = parse_point(&mut parts).map_err(&error)?;
                steps.push(Step::PinchUpdate { distance, centre });
            }
            "pinch_end" => steps.push(Step::PinchEnd),
            "long_press" => {
                let time = parse_f32(parts.next(), "time").map_err(&error)?;
                steps.push(Step::LongPress { time });
            }
            "shift" => {
                let on = match parts.next().unwrap_or("") {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    other => return Err(error(format!("shift expects on|off, got `{other}`"))),
                };
                steps.push(Step::Shift { on });
            }
            "right" => {
                let on = match parts.next().unwrap_or("") {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    other => return Err(error(format!("right expects on|off, got `{other}`"))),
                };
                steps.push(Step::Right { on });
            }
            "key" => {
                let letter = parts
                    .next()
                    .and_then(|value| value.chars().next())
                    .ok_or_else(|| error("key needs a letter".to_string()))?;
                steps.push(Step::Key {
                    letter: letter.to_ascii_lowercase(),
                });
            }
            "escape" => steps.push(Step::Escape),
            "placing" => {
                let on = match parts.next().unwrap_or("") {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    other => return Err(error(format!("placing expects on|off, got `{other}`"))),
                };
                steps.push(Step::Placing { on });
            }
            "touch" => {
                let kind = parts
                    .next()
                    .ok_or_else(|| error("touch needs down|move|up|cancel".to_string()))?;
                let id = parts
                    .next()
                    .ok_or_else(|| error("touch needs a finger id".to_string()))?
                    .parse::<i64>()
                    .map_err(|_| error("finger id is not an integer".to_string()))?;
                let pos = parse_point(&mut parts).map_err(&error)?;
                let event = match kind {
                    "down" => InputEvent::FingerDown { id, pos },
                    "move" => InputEvent::FingerMove { id, pos },
                    "up" => InputEvent::FingerUp { id, pos },
                    "cancel" => InputEvent::FingerCancel { id, pos },
                    other => return Err(error(format!("touch event `{other}`"))),
                };
                steps.push(Step::Input(event));
            }
            "mouse" => {
                let kind = parts
                    .next()
                    .ok_or_else(|| error("mouse needs down|move|up".to_string()))?;
                let pos = parse_point(&mut parts).map_err(&error)?;
                let event = match kind {
                    "down" => InputEvent::MouseDown { pos },
                    "move" => InputEvent::MouseMove { pos },
                    "up" => InputEvent::MouseUp { pos },
                    other => return Err(error(format!("mouse event `{other}`"))),
                };
                steps.push(Step::Input(event));
            }
            "sdlkey" => {
                let kind = parts
                    .next()
                    .ok_or_else(|| error("sdlkey needs down|up".to_string()))?;
                let scancode = parts
                    .next()
                    .ok_or_else(|| error("sdlkey needs a scancode".to_string()))?
                    .parse::<u32>()
                    .map_err(|_| error("scancode is not a number".to_string()))?;
                let modifiers = parts
                    .next()
                    .ok_or_else(|| error("sdlkey needs modifiers".to_string()))?
                    .parse::<u16>()
                    .map_err(|_| error("modifiers are not a number".to_string()))?;
                let repeat = match parts.next().unwrap_or("once") {
                    "repeat" | "held" | "1" | "true" => true,
                    "once" | "0" | "false" => false,
                    other => return Err(error(format!("sdlkey repeat `{other}`"))),
                };
                let event = match kind {
                    "down" => InputEvent::KeyDown {
                        scancode,
                        modifiers,
                        repeat,
                    },
                    "up" => InputEvent::KeyUp {
                        scancode,
                        modifiers,
                    },
                    other => return Err(error(format!("sdlkey event `{other}`"))),
                };
                steps.push(Step::Input(event));
            }
            "focus" => {
                let gained = match parts.next().unwrap_or("") {
                    "gained" | "on" => true,
                    "lost" | "off" => false,
                    other => return Err(error(format!("focus expects lost|gained, got `{other}`"))),
                };
                steps.push(Step::Focus { gained });
            }
            "rotate" => {
                let width = parse_f32(parts.next(), "width").map_err(&error)?;
                let height = parse_f32(parts.next(), "height").map_err(&error)?;
                steps.push(Step::Rotate { width, height });
            }
            "expect" => {
                let expected = parts.collect::<Vec<_>>().join(" ");
                if expected.is_empty() {
                    return Err(error("expect needs an event".to_string()));
                }
                steps.push(Step::Expect { line, expected });
            }
            other => return Err(error(format!("unknown command `{other}`"))),
        }
    }
    Ok(Trace { layout, steps })
}

fn parse_rect(value: &str) -> Result<Rect, String> {
    let numbers: Vec<&str> = value.split(',').collect();
    if numbers.len() != 4 {
        return Err(format!("`{value}` is not x,y,w,h"));
    }
    let mut parsed = [0.0f32; 4];
    for (slot, number) in parsed.iter_mut().zip(numbers) {
        *slot = number
            .parse::<f32>()
            .map_err(|_| format!("`{number}` is not a number"))?;
    }
    Ok(Rect::new(parsed[0], parsed[1], parsed[2], parsed[3]))
}

fn parse_point<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Result<Vec2, String> {
    let x = parse_f32(parts.next(), "x")?;
    let y = parse_f32(parts.next(), "y")?;
    Ok(Vec2::new(x, y))
}

fn parse_f32(value: Option<&str>, name: &str) -> Result<f32, String> {
    let value = value.ok_or_else(|| format!("missing {name}"))?;
    value
        .parse::<f32>()
        .map_err(|_| format!("`{value}` is not a {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Report {
        parse(text).expect("trace parses").run()
    }

    #[test]
    fn world_tap_replays_to_a_single_tap() {
        let report = run("layout scale=1.0\ndown 640 360 0.0\nup 640 360 0.1\n");
        assert_eq!(
            report.events,
            vec!["start=world", "up=tap:640.0:360.0:single"]
        );
        assert!(report.ok());
    }

    #[test]
    fn hold_then_drag_replays_to_a_box() {
        let report = run(
            "layout scale=1.0\ndown 500 300 0.0\nmove 760 470 0.35\nup 760 470 0.4\n",
        );
        assert_eq!(
            report.events,
            vec!["start=world", "move=none", "up=box:500.0:300.0:760.0:470.0"]
        );
    }

    #[test]
    fn button_hold_then_double_replays() {
        let report = run(
            "layout scale=1.0\nbutton 3 100,100,80,80 on\ndown 120 120 0.0\nup 120 120 0.6\ndown 120 120 0.7\nup 120 120 0.8\n",
        );
        assert_eq!(
            report.events,
            vec![
                "start=button:3",
                "up=button:3:hold:true:double:false",
                "start=button:3",
                "up=button:3:hold:false:double:true",
            ]
        );
    }

    #[test]
    fn minimap_and_blocked_route_like_the_device() {
        let report = run(
            "layout scale=1.0 minimap=20,100,200,140\nblocked 100,10,300,60\ndown 120 170 0.0\nup 120 170 0.2\ndown 200 30 0.3\nup 200 30 0.4\n",
        );
        assert_eq!(
            report.events,
            vec![
                "start=minimap",
                "up=minimap:120.0:170.0:hold:false:action:camera",
                "start=blocked",
                "up=none",
            ]
        );
    }

    #[test]
    fn long_press_waits_for_the_threshold() {
        let report = run(
            "layout scale=1.0\ndown 640 360 0.0\nlong_press 0.2\nlong_press 0.5\n",
        );
        assert_eq!(
            report.events,
            vec!["start=world", "long_press=none", "long_press=640.0:360.0"]
        );
    }

    #[test]
    fn pinch_update_reports_distance_and_pan() {
        let report = run(
            "pinch_begin 200 640 360 1500\npinch_update 400 640 360\npinch_end\npinch_update 100 0 0\n",
        );
        assert_eq!(
            report.events,
            vec!["pinch=distance:750.0:pan:0.0:0.0", "two_finger_tap=none"]
        );
    }

    #[test]
    fn expect_lines_pin_the_event_log() {
        let parsed = parse(
            "layout\ndown 640 360 0.0\nexpect start=world\nup 640 360 0.1\nexpect up=tap:640.0:360.0:single\n",
        )
        .expect("parses");
        assert!(parsed.run().ok(), "matching expectations pass");

        let bad = parse("layout\ndown 640 360 0.0\nexpect start=minimap\n").expect("parses");
        let report = bad.run();
        assert!(!report.ok());
        assert!(
            report.failures[0].contains("expected `start=minimap`, got `start=world`"),
            "{:?}",
            report.failures
        );
    }

    #[test]
    fn malformed_traces_report_the_line() {
        let error = parse("layout\ndown 640 360\n").expect_err("missing time");
        assert_eq!(error.line, 2);
        let error = parse("layout\nwarp 1 2 3\n").expect_err("unknown command");
        assert_eq!(error.line, 2);
        let error = parse("layout minimap=1,2,3\n").expect_err("bad rect");
        assert_eq!(error.line, 1);
    }
}
