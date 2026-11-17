//! Tab subgroup cycling (`ui.groups.tab`).
//!
//! WC3 keeps the whole selection but activates one *subgroup* at a time; Tab
//! cycles forward and Shift+Tab backward. Heroes always form the first
//! subgroup, then the remaining unit types in selection order.

use crate::selection::UnitSnapshot;

/// Ordered subgroups of the current selection plus the active index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subgroups {
    kinds: Vec<u32>,
    active: usize,
}

impl Subgroups {
    /// Build the subgroup order from `selected` ids. Hero kinds come first (in
    /// selection order), then the non-hero kinds by first appearance. Dead and
    /// unselected units never create a subgroup.
    pub fn build(units: &[UnitSnapshot], selected: &[u32]) -> Self {
        let mut heroes: Vec<u32> = Vec::new();
        let mut kinds: Vec<u32> = Vec::new();
        for id in selected {
            let Some(unit) = units.iter().find(|unit| unit.id == *id && unit.alive) else {
                continue;
            };
            let bucket = if unit.hero { &mut heroes } else { &mut kinds };
            if !bucket.contains(&unit.kind) {
                bucket.push(unit.kind);
            }
        }
        heroes.extend(kinds);
        Self {
            kinds: heroes,
            active: 0,
        }
    }

    pub fn order(&self) -> &[u32] {
        &self.kinds
    }

    pub fn active(&self) -> Option<u32> {
        self.kinds.get(self.active).copied()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Advance to the next subgroup (`reverse` = Shift+Tab). Wraps.
    pub fn cycle(&mut self, reverse: bool) -> Option<u32> {
        let len = self.kinds.len();
        if len == 0 {
            return None;
        }
        self.active = if reverse {
            (self.active + len - 1) % len
        } else {
            (self.active + 1) % len
        };
        self.active()
    }

    /// The selected ids that belong to the active subgroup, in selection order.
    pub fn active_units(&self, units: &[UnitSnapshot], selected: &[u32]) -> Vec<u32> {
        let Some(kind) = self.active() else {
            return Vec::new();
        };
        selected
            .iter()
            .copied()
            .filter(|id| {
                units
                    .iter()
                    .any(|unit| unit.id == *id && unit.alive && unit.kind == kind)
            })
            .collect()
    }
}
