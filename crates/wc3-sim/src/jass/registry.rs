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

use super::native_status::{self, NativeStatus};
use super::value::{Signature, Type, Value, parse_type_list};

/// One registry row.
#[derive(Debug, Clone)]
pub struct NativeDef {
    pub name: &'static str,
    pub signature: Signature,
    /// True when the shipped TFT engine registers this name (`0x6f62f140`).
    pub engine_registered: bool,
    /// True when [`super::natives`] has a dispatch handler for it.
    ///
    /// A handler is necessary, not sufficient: [`Self::status`] records how
    /// much of the behaviour the handler actually implements.
    pub dispatch: bool,
    /// Semantic status of the row (audit AUD-016).
    pub status: NativeStatus,
    /// Why the row is not `Implemented`; empty for `Implemented`.
    pub status_reason: &'static str,
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
                let mut ret = parse_type_list(ret)
                    .and_then(|mut types| types.pop())
                    .unwrap_or_else(|| {
                        panic!("registry: unparsable return type for {name}: {ret:?}")
                    });
                // The shipped script is the authority where it differs from the
                // donor tables the registry was generated from: the byte-level
                // check (scripts/native_sig_compare.py) found GroupAddUnit and
                // GroupRemoveUnit declared `returns nothing` in this install's
                // common.j while the donor 1.32.9 declares `boolean`.
                if let Some(original_ret) = shipped_return_override(name) {
                    ret = original_ret;
                }
                let dispatch = super::natives::is_handled(name);
                let (status, status_reason) = native_status::resolve(name, dispatch);
                NativeDef {
                    name,
                    signature: Signature { params, ret },
                    engine_registered: *engine_registered,
                    dispatch,
                    status,
                    status_reason,
                }
            })
            .collect()
    })
}

/// Return types that the SHIPPED `Scripts/common.j` declares differently from
/// the donor-generated table. Every row is justified by
/// `scripts/native_sig_compare.py` against
/// `analysis/war3_full3/Scripts/common.j`: the original wins because that is the
/// script this installation actually runs.
fn shipped_return_override(name: &str) -> Option<Type> {
    match name {
        // common.j: `native GroupAddUnit takes group whichGroup, unit whichUnit returns nothing`
        "GroupAddUnit" | "GroupRemoveUnit" => Some(Type::Void),
        _ => None,
    }
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

/// True when a dispatch handler exists for the name (not a claim about how
/// much of the native is implemented — see [`NativeDef::status`]).
pub fn is_handled(name: &str) -> bool {
    lookup(name).is_some_and(|def| def.dispatch)
}

/// True when the native is fully implemented (`NativeStatus::Implemented`).
pub fn is_implemented(name: &str) -> bool {
    lookup(name).is_some_and(|def| def.status == NativeStatus::Implemented)
}

/// Registry statistics for reports and tests.
///
/// `dispatch` counts handlers; the per-status counters count semantics, which
/// is the number a coverage report should quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryStats {
    pub total: usize,
    pub engine_registered: usize,
    /// Rows with a dispatch handler.
    pub dispatch: usize,
    pub dispatch_engine_registered: usize,
    pub implemented: usize,
    pub implemented_engine_registered: usize,
    pub partial: usize,
    pub recorded_only: usize,
    pub no_op: usize,
    pub unsupported: usize,
}

pub fn stats() -> RegistryStats {
    let mut stats = RegistryStats {
        total: 0,
        engine_registered: 0,
        dispatch: 0,
        dispatch_engine_registered: 0,
        implemented: 0,
        implemented_engine_registered: 0,
        partial: 0,
        recorded_only: 0,
        no_op: 0,
        unsupported: 0,
    };
    for def in table() {
        stats.total += 1;
        if def.engine_registered {
            stats.engine_registered += 1;
        }
        if def.dispatch {
            stats.dispatch += 1;
            if def.engine_registered {
                stats.dispatch_engine_registered += 1;
            }
        }
        match def.status {
            NativeStatus::Implemented => {
                stats.implemented += 1;
                if def.engine_registered {
                    stats.implemented_engine_registered += 1;
                }
            }
            NativeStatus::Partial => stats.partial += 1,
            NativeStatus::RecordedOnly => stats.recorded_only += 1,
            NativeStatus::NoOp => stats.no_op += 1,
            NativeStatus::Unsupported => stats.unsupported += 1,
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
        assert!(stats.dispatch > 0, "handlers must be registered");
        assert_eq!(
            stats.implemented + stats.partial + stats.recorded_only + stats.no_op + stats.unsupported,
            stats.total,
            "every row has exactly one status"
        );
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

#[cfg(test)]
mod shipped_signature_tests {
    use super::*;

    /// The shipped common.j declares these `returns nothing`; the donor table said
    /// `boolean`. Pinned so a registry regeneration cannot silently re-diverge.
    #[test]
    fn group_add_remove_return_nothing_like_the_shipped_script() {
        for name in ["GroupAddUnit", "GroupRemoveUnit"] {
            let def = lookup(name).unwrap_or_else(|| panic!("{name} missing from registry"));
            assert_eq!(
                def.signature.ret,
                Type::Void,
                "{name} must return nothing per common.j"
            );
            assert_eq!(def.signature.params.len(), 2, "{name} takes (group, unit)");
        }
    }
}
