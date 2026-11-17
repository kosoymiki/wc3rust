//! Script values, handle identity and the native type model.
//!
//! The engine's JASS2 types are 32-bit: `integer` is `i32` and `real` is `f32`
//! (the TFT VM's `Real` is a 4-byte float — see `analysis/agent-re/formats/
//! jass_natives.json`, whose signatures only distinguish `I` and `R`). Using
//! `f32`/`i32` here is what makes script arithmetic match the game, including
//! overflow wrap-around and float rounding, instead of silently promoting to
//! 64-bit.
//!
//! Handles carry their kind so that a native can validate its argument (a
//! script may legally pass `null`, and BJ code passes handles through `integer`
//! parameters), and so a mistyped call produces an observable trap instead of a
//! plausible-looking zero.

use std::collections::BTreeMap;
use std::fmt;

/// Every handle family the script API exposes. Kept explicit so
/// `Value::Handle(kind, id)` can be checked by natives and rendered in
/// diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandleKind {
    Unit,
    Item,
    Destructable,
    Trigger,
    Timer,
    Group,
    Location,
    Rect,
    Region,
    Sound,
    Effect,
    Force,
    Boolexpr,
    ConditionFunc,
    TriggerCondition,
    Event,
    CameraSetup,
    FogModifier,
    Dialog,
    Button,
    Leaderboard,
    Multiboard,
    Quest,
    ItemPool,
    Trackable,
    UnitPool,
    WeatherEffect,
    TerrainDeformation,
    Image,
    Ubersplat,
    TextTag,
    Lightning,
    Hashtable,
    GameCache,
    Ability,
    Buff,
    StartLocation,
    TimerDialog,
    GameEvent,
    Player,
    /// A handle family the corpus uses that has no simulation counterpart; the
    /// id is allocated so `GetHandleId` stays unique and comparisons hold.
    Misc,
}

impl HandleKind {
    /// Type name as declared in `common.j` (`unit`, `trigger`, …).
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Item => "item",
            Self::Destructable => "destructable",
            Self::Trigger => "trigger",
            Self::Timer => "timer",
            Self::Group => "group",
            Self::Location => "location",
            Self::Rect => "rect",
            Self::Region => "region",
            Self::Sound => "sound",
            Self::Effect => "effect",
            Self::Force => "force",
            Self::Boolexpr => "boolexpr",
            Self::ConditionFunc => "conditionfunc",
            Self::TriggerCondition => "triggercondition",
            Self::Event => "event",
            Self::CameraSetup => "camerasetup",
            Self::FogModifier => "fogmodifier",
            Self::Dialog => "dialog",
            Self::Button => "button",
            Self::Leaderboard => "leaderboard",
            Self::Multiboard => "multiboard",
            Self::Quest => "quest",
            Self::ItemPool => "itempool",
            Self::Trackable => "trackable",
            Self::UnitPool => "unitpool",
            Self::WeatherEffect => "weathereffect",
            Self::TerrainDeformation => "terraindeformation",
            Self::Image => "image",
            Self::Ubersplat => "ubersplat",
            Self::TextTag => "texttag",
            Self::Lightning => "lightning",
            Self::Hashtable => "hashtable",
            Self::GameCache => "gamecache",
            Self::Ability => "ability",
            Self::Buff => "buff",
            Self::StartLocation => "startlocation",
            Self::TimerDialog => "timerdialog",
            Self::GameEvent => "gameevent",
            Self::Player => "player",
            Self::Misc => "handle",
        }
    }
}

/// A script handle: a kind tag plus the unique id used by `GetHandleId`,
/// comparisons and the host arenas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle {
    pub kind: HandleKind,
    pub id: u32,
}

impl Handle {
    pub const fn new(kind: HandleKind, id: u32) -> Self {
        Self { kind, id }
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.kind.type_name(), self.id)
    }
}

/// A JASS2 value.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Int(i32),
    Real(f32),
    Bool(bool),
    Str(String),
    /// `function Foo` — index into the compiled function table.
    Code(u32),
    Handle(Handle),
}

impl Value {
    pub fn int(&self) -> i32 {
        match self {
            Self::Int(v) => *v,
            Self::Real(v) => *v as i32,
            Self::Bool(v) => *v as i32,
            Self::Handle(h) => h.id as i32,
            _ => 0,
        }
    }

    pub fn real(&self) -> f32 {
        match self {
            Self::Real(v) => *v,
            Self::Int(v) => *v as f32,
            Self::Bool(v) => (*v as i32) as f32,
            Self::Handle(h) => h.id as f32,
            _ => 0.0,
        }
    }

    /// JASS truthiness: only `boolean` is a condition in well-formed code, but
    /// the VM treats ints/reals/handles like the engine's `0 is false`.
    pub fn truthy(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Int(v) => *v != 0,
            Self::Real(v) => *v != 0.0,
            Self::Str(v) => !v.is_empty(),
            Self::Handle(_) => true,
            Self::Code(_) => true,
            Self::Null => false,
        }
    }

    /// `R2S`-style rendering used by `I2S`/string concatenation/`BJDebugMsg`.
    pub fn text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Int(v) => v.to_string(),
            Self::Real(v) => format_real(*v),
            Self::Bool(v) => {
                if *v {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Self::Str(v) => v.clone(),
            Self::Code(index) => format!("code#{index}"),
            Self::Handle(handle) => handle.to_string(),
        }
    }

    pub fn handle(&self) -> Option<Handle> {
        match self {
            Self::Handle(handle) => Some(*handle),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Equality with the engine's mixed-type rules: numbers compare by value,
    /// handles by identity, strings by content.
    pub fn eq_value(&self, other: &Value) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Null, _) | (_, Self::Null) => match (self, other) {
                (Self::Null, Self::Int(v)) => *v == 0,
                (Self::Null, Self::Real(v)) => *v == 0.0,
                (Self::Null, Self::Bool(v)) => !*v,
                (Self::Null, Self::Handle(_)) => false,
                (Self::Int(v), Self::Null) => *v == 0,
                (Self::Real(v), Self::Null) => *v == 0.0,
                (Self::Bool(v), Self::Null) => !*v,
                (Self::Handle(_), Self::Null) => false,
                _ => false,
            },
            (Self::Handle(a), Self::Handle(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Code(a), Self::Code(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Str(a), _) => other.text() == *a,
            (_, Self::Str(b)) => self.text() == *b,
            _ => self.real() == other.real() && !self.is_stringy() && !other.is_stringy(),
        }
    }

    fn is_stringy(&self) -> bool {
        matches!(self, Self::Str(_))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.eq_value(other)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Str(v) => f.write_str(v),
            other => f.write_str(&other.text()),
        }
    }
}

/// `real` to string exactly like the engine's `R2S`: up to three decimals,
/// trailing zeros trimmed (`1.5` → `"1.500"` in TFT — the engine keeps three
/// decimals, which is what map scripts expect to see in text frames).
pub fn format_real(value: f32) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "Inf".into()
        } else {
            "-Inf".into()
        };
    }
    format!("{value:.3}")
}

/// Native parameter/return type as parsed from `common.j`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Void,
    Bool,
    Int,
    Real,
    Str,
    Code,
    /// Named handle type (`unit`, `unitstate`, …) exactly as in `common.j`.
    Handle(String),
}

impl Type {
    /// The value a native of this type returns when the simulation has no
    /// implementation for it: `nothing`→null, `boolean`→false, numbers→0,
    /// `string`→"", handles/`code`→null.
    pub fn default_value(&self) -> Value {
        match self {
            Self::Void | Self::Handle(_) | Self::Code => Value::Null,
            Self::Bool => Value::Bool(false),
            Self::Int => Value::Int(0),
            Self::Real => Value::Real(0.0),
            Self::Str => Value::Str(String::new()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Void => "nothing",
            Self::Bool => "boolean",
            Self::Int => "integer",
            Self::Real => "real",
            Self::Str => "string",
            Self::Code => "code",
            Self::Handle(name) => name,
        }
    }
}

/// A compiled signature: parameter types and the return type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Type>,
    pub ret: Type,
}

/// Encode a signature the way `registry_gen.rs` stores it: one char per scalar
/// type, `Hname;` per handle. Returns `None` when the text is malformed.
pub fn parse_type_list(text: &str) -> Option<Vec<Type>> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            'V' => out.push(Type::Void),
            'B' => out.push(Type::Bool),
            'I' => out.push(Type::Int),
            'R' => out.push(Type::Real),
            'S' => out.push(Type::Str),
            'C' => out.push(Type::Code),
            'H' => {
                let start = index + 1;
                let mut end = text.len();
                let mut seen = false;
                for (offset, inner) in text[start..].char_indices() {
                    if inner == ';' {
                        end = start + offset;
                        seen = true;
                        break;
                    }
                }
                if !seen {
                    return None;
                }
                out.push(Type::Handle(text[start..end].to_string()));
                // Consume the handle body from the iterator.
                while let Some((offset, _)) = chars.peek().copied() {
                    if offset < end + 1 {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// A single `real` rounded like the engine's `R2I`: truncation toward zero.
pub fn real_to_int(value: f32) -> i32 {
    // The engine's R2I is `(int)value`; NaN/overflow map to 0 like x87's
    // invalid-operation result, which the VM guards explicitly instead of
    // producing a platform-defined `i32::MIN`.
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc();
    if truncated >= i32::MAX as f32 {
        i32::MAX
    } else if truncated <= i32::MIN as f32 {
        i32::MIN
    } else {
        truncated as i32
    }
}

/// Growable string pool used to keep `Value::Str` cheap for repeated literals.
pub type StringPool = BTreeMap<String, u32>;
