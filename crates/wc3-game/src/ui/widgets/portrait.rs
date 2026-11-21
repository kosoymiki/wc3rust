//! Portrait source resolution (polish addendum §1).
//!
//! A selected object shows its **portrait model** when the install ships one,
//! otherwise the def's 2D command icon, and only as a last resort a letter plate
//! — with the reason logged. The decision is data: the caller passes what the
//! asset database knows and this module names the source once per def.

use std::cell::RefCell;
use std::collections::BTreeSet;

/// Where a portrait comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PortraitSource {
    /// `Units\<...>\<Name>_Portrait.mdx` exists in the install.
    Model(String),
    /// The def's 2D icon (`unitUI` `Art` / `ScoreScreenIcon`).
    Icon(String),
    /// Neither exists: the letter plate is the only thing left.
    Placeholder(&'static str),
}

impl PortraitSource {
    /// `model` / `icon` / `placeholder`.
    pub fn key(&self) -> &'static str {
        match self {
            PortraitSource::Model(_) => "model",
            PortraitSource::Icon(_) => "icon",
            PortraitSource::Placeholder(_) => "placeholder",
        }
    }

    /// The asset the painter should load, when there is one.
    pub fn asset(&self) -> Option<&str> {
        match self {
            PortraitSource::Model(path) | PortraitSource::Icon(path) => Some(path),
            PortraitSource::Placeholder(_) => None,
        }
    }

    pub fn reason(&self) -> Option<&'static str> {
        match self {
            PortraitSource::Placeholder(reason) => Some(reason),
            _ => None,
        }
    }

    /// `HUD_PORTRAIT def=<id> source=<source>` (addendum §1).
    pub fn log_line(&self, def: &str) -> String {
        let mut line = format!("HUD_PORTRAIT def={def} source={}", self.key());
        if let Some(reason) = self.reason() {
            line.push_str(" reason=");
            line.push_str(reason);
        }
        line
    }
}

/// Resolve the portrait for `def`.
///
/// * `model` — the portrait model path, when the object database derives one
///   and the catalog proves it exists.
/// * `icon` — the def's 2D icon basename, when the install ships it.
pub fn resolve(def: &str, model: Option<&str>, icon: Option<&str>) -> PortraitSource {
    if let Some(model) = model {
        if !model.is_empty() {
            return PortraitSource::Model(model.to_string());
        }
    }
    if let Some(icon) = icon {
        if !icon.is_empty() {
            return PortraitSource::Icon(icon.to_string());
        }
    }
    let _ = def;
    PortraitSource::Placeholder("no portrait model and no unit icon in the install")
}

thread_local! {
    static LOGGED: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

/// Log the resolution once per def and return the line when it is new.
///
/// A HUD redraws every frame; the diagnostic is about the *def*, not the frame,
/// so a second call for `opeo` returns `None`.
pub fn log_once(def: &str, source: &PortraitSource) -> Option<String> {
    let line = source.log_line(def);
    LOGGED.with(|logged| {
        let mut logged = logged.borrow_mut();
        if logged.contains(def) {
            None
        } else {
            logged.insert(def.to_string());
            Some(line)
        }
    })
}

/// The letter drawn on a placeholder plate (first glyph of the display name).
pub fn placeholder_letter(name: &str) -> char {
    name.chars().next().unwrap_or('?')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portrait_model_wins() {
        let source = resolve("Obla", Some("Units\\Orc\\HeroBlademaster\\HeroBlademaster_Portrait.mdx"), Some("BTNBlademaster"));
        assert_eq!(source.key(), "model");
        assert!(source.asset().unwrap().contains("_Portrait"));
        assert!(source.reason().is_none());
    }

    #[test]
    fn the_icon_is_the_second_choice() {
        let source = resolve("opeo", None, Some("BTNPeon"));
        assert_eq!(source.key(), "icon");
        assert_eq!(source.asset(), Some("BTNPeon"));
    }

    #[test]
    fn the_letter_is_the_last_resort_and_carries_a_reason() {
        let source = resolve("xxxx", None, None);
        assert_eq!(source.key(), "placeholder");
        assert!(source.reason().unwrap().contains("no portrait model"));
        let line = source.log_line("xxxx");
        assert_eq!(
            line,
            "HUD_PORTRAIT def=xxxx source=placeholder reason=no portrait model and no unit icon in the install"
        );
        assert_eq!(placeholder_letter("Blademaster"), 'B');
    }

    #[test]
    fn the_log_is_once_per_def() {
        let source = resolve("opeo", None, Some("BTNPeon"));
        assert!(log_once("opeo", &source).is_some());
        assert!(log_once("opeo", &source).is_none(), "second call must be silent");
        assert!(log_once("ogre", &source).is_some(), "a new def logs");
    }
}
