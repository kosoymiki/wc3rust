//! Native registry: the name → signature table the engine exposes.
//!
//! The table is generated from two measured sources (see
//! `analysis/agent-jass/gen_registry.py`): the 1534 `native` declarations of
//! `Scripts\common.j` (1.32.9 donor copy) and the 1096 registrations the TFT
//! engine performs at `0x6f62f140` (r2 lane 407). Every name in either source
//! resolves here, which is what lets a map script load without an
//! "unknown function" wall — resolution and *implementation* are separate
//! questions, and the difference is measured, not hidden.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::value::{Signature, Type, Value, parse_type_list};

/// One registry row.
#[derive(Debug, Clone)]
pub struct NativeDef {
    pub name: &'static str,
    pub signature: Signature,
    /// True when the shipped TFT engine registers this name (`0x6f62f140`).
    pub engine_registered: bool,
    /// True when [`super::natives`] has a handler for it.
    pub handled: bool,
}

impl NativeDef {
    pub fn arity(&self) -> usize {
        self.signature.params.len()
    }

    pub fn return_default(&self) -> Value {
        self.signature.ret.default_value()
    }
}

/// Registry index: lowercased name → row.
static INDEX: OnceLock<BTreeMap<String, usize>> = OnceLock::new();

/// All registry rows, in generated order.
pub fn table() -> &'static [NativeDef] {
    static DEFS: OnceLock<Vec<NativeDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        super::registry_gen::NATIVE_TABLE
            .iter()
            .map(|(name, params, ret, engine_registered)| {
                let params = parse_type_list(params).unwrap_or_else(|| {
                    panic!("registry: unparsable parameter list for {name}: {params:?}")
                });
                let ret = parse_type_list(ret)
                    .and_then(|mut types| types.pop())
                    .unwrap_or_else(|| {
                        panic!("registry: unparsable return type for {name}: {ret:?}")
                    });
                NativeDef {
                    name,
                    signature: Signature { params, ret },
                    engine_registered: *engine_registered,
                    handled: super::natives::is_handled(name),
                }
            })
            .collect()
    })
}

fn index() -> &'static BTreeMap<String, usize> {
    INDEX.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (slot, def) in table().iter().enumerate() {
            map.insert(def.name.to_ascii_lowercase(), slot);
        }
        map
    })
}

/// Case-insensitive lookup, matching JASS (identifiers are case-insensitive).
pub fn lookup(name: &str) -> Option<&'static NativeDef> {
    let lowered = name.to_ascii_lowercase();
    index().get(&lowered).map(|slot| &table()[*slot])
}

/// True when the name is a registry native.
pub fn is_native(name: &str) -> bool {
    lookup(name).is_some()
}

/// True when a handler exists for the name.
pub fn is_handled(name: &str) -> bool {
    lookup(name).is_some_and(|def| def.handled)
}

/// Registry statistics for reports and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryStats {
    pub total: usize,
    pub engine_registered: usize,
    pub handled: usize,
    pub handled_engine_registered: usize,
}

pub fn stats() -> RegistryStats {
    let mut stats = RegistryStats {
        total: 0,
        engine_registered: 0,
        handled: 0,
        handled_engine_registered: 0,
    };
    for def in table() {
        stats.total += 1;
        if def.engine_registered {
            stats.engine_registered += 1;
        }
        if def.handled {
            stats.handled += 1;
            if def.engine_registered {
                stats.handled_engine_registered += 1;
            }
        }
    }
    stats
}

/// Parameter type of `index` in `def`, or `None` when the script passed more
/// arguments than declared (legal in JASS: extra arguments are ignored by the
/// engine, missing ones arrive as zero values).
pub fn param_type(def: &NativeDef, index: usize) -> Option<&Type> {
    def.signature.params.get(index)
}

/// Formatting helper used by diagnostics: `CreateUnit(player, integer, real,
/// real, real) -> unit`.
pub fn describe(def: &NativeDef) -> String {
    let params: Vec<&str> = def.signature.params.iter().map(Type::name).collect();
    format!(
        "{}({}) -> {}",
        def.name,
        params.join(", "),
        def.signature.ret.name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_every_generated_row() {
        let stats = stats();
        assert_eq!(stats.total, table().len());
        assert!(
            stats.total >= 1600,
            "registry should cover common.j + engine: {stats:?}"
        );
        assert!(stats.engine_registered >= 1096, "{stats:?}");
        assert!(stats.handled > 0, "handlers must be registered");
    }

    #[test]
    fn lookups_are_case_insensitive() {
        assert!(is_native("CreateUnit"));
        assert!(is_native("createunit"));
        assert!(is_native("CREATEUNIT"));
        assert!(!is_native("NotANativeAtAll"));
    }

    #[test]
    fn signatures_match_common_j_for_known_natives() {
        let create = lookup("CreateUnit").expect("CreateUnit");
        assert_eq!(create.signature.params.len(), 5);
        assert_eq!(
            create.signature.ret,
            Type::Handle("unit".to_string())
        );
        let add = lookup("TriggerAddAction").expect("TriggerAddAction");
        assert_eq!(add.signature.params.len(), 2);
        assert_eq!(add.signature.params[1], Type::Code);
        let action = lookup("ModuloInteger");
        assert!(
            action.is_none(),
            "ModuloInteger is a Blizzard.j function, not a native"
        );
    }

    #[test]
    fn typed_defaults_follow_the_declared_return_type() {
        assert_eq!(lookup("GetUnitX").unwrap().return_default(), Value::Real(0.0));
        assert_eq!(lookup("GetHandleId").unwrap().return_default(), Value::Int(0));
        assert_eq!(
            lookup("UnitAddAbility").unwrap().return_default(),
            Value::Bool(false)
        );
        assert_eq!(
            lookup("CreateTrigger").unwrap().return_default(),
            Value::Null
        );
    }
}
