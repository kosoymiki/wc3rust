//! Stable element identity.
//!
//! An [`Id`] is a name plus an index: `("card", 3)` is the fourth command slot.
//! The pair is `Copy` and `Hash`, so a solved layout is addressable without a
//! string map, and diagnostics can print a human label (`card[3]`) without the
//! tree. Names come from a closed set of `&'static str` literals, so an id can
//! never be forged at runtime or drift with localisation.

/// Identity of one UI element inside a tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct Id {
    name: &'static str,
    index: u16,
}

impl Id {
    /// The tree root. Every solver run starts here.
    pub const ROOT: Id = Id { name: "root", index: 0 };

    /// First (and usually only) element with this name.
    pub const fn new(name: &'static str) -> Self {
        Self { name, index: 0 }
    }

    /// Indexed element: `Id::indexed("card_slot", 3)`.
    pub const fn indexed(name: &'static str, index: u16) -> Self {
        Self { name, index }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn index(self) -> u16 {
        self.index
    }

    /// Same name, different index (group 0 of a repeated widget).
    pub const fn at(self, index: u16) -> Self {
        Self {
            name: self.name,
            index,
        }
    }

    /// Diagnostic label: `card_slot` for index 0, `card_slot[3]` otherwise.
    pub fn label(self) -> String {
        if self.index == 0 {
            self.name.to_string()
        } else {
            format!("{}[{}]", self.name, self.index)
        }
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)?;
        if self.index != 0 {
            write!(f, "[{}]", self.index)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_copy_and_stable() {
        let a = Id::indexed("card_slot", 3);
        let b = a;
        assert_eq!(a, b);
        assert_eq!(a.name(), "card_slot");
        assert_eq!(a.index(), 3);
        assert_eq!(a.at(4).index(), 4);
        assert_eq!(a.label(), "card_slot[3]");
        assert_eq!(Id::new("console").label(), "console");
        assert_ne!(Id::new("a"), Id::new("b"));
    }
}
