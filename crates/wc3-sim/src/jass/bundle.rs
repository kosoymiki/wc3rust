//! Loading and merging JASS sources: `common.j`, `Blizzard.j`, map scripts.
//!
//! A map script is never standalone: it calls BJ functions, which call natives,
//! and it reads constants (`EVENT_UNIT_DEATH`, `PLAYER_STATE_RESOURCE_GOLD`)
//! declared in `common.j`. Running a map script without those headers produces a
//! wall of "unknown function" errors, so the loader merges the three sources in
//! the engine's order and keeps the provenance of each one.
//!
//! Parsing is delegated to `wc3_re::jass::parse` — the same parser the install
//! scanner uses on all 44 campaign scripts and 91 AI scripts — so this layer adds
//! loading and merge semantics, not a second grammar.

use std::path::{Path, PathBuf};

use wc3_re::jass::ast::Program;

use super::locale::{LocalizedStrings, map_flags_from_w3i};

/// Three script sources in engine load order, plus the map's own data tables.
#[derive(Debug, Clone, Default)]
pub struct ScriptBundle {
    pub program: Program,
    /// `(source name, byte length, function count)` per merged source.
    pub sources: Vec<(String, usize, usize)>,
    /// `GetLocalizedString` tables: the map's `war3map.wts` first, the install's
    /// `UI\FrameDef\GlobalStrings.fdf` second. Populated by discovery from the
    /// loaded paths, or explicitly from an archive (`set_wts`).
    pub localized: LocalizedStrings,
    /// The map's `war3map.w3i` `MapFlags` word, when a header was supplied
    /// (`add_map_w3i`, or a `war3map.w3i` next to the map script).
    pub map_flags: Option<u32>,
}

impl ScriptBundle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses and merges one source. Returns the number of functions it added.
    pub fn add_source(&mut self, name: &str, text: &str) -> Result<usize, String> {
        let program = wc3_re::jass::parse(text)
            .map_err(|error| format!("{name}: {error}"))?;
        let functions = program.functions.len();
        self.program.merge(program);
        self.sources
            .push((name.to_string(), text.len(), functions));
        Ok(functions)
    }

    pub fn add_source_file(&mut self, path: &Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        self.add_source(&name, &text)
    }

    /// Adds `Scripts\common.j` and `Scripts\Blizzard.j` from a game data
    /// directory. Missing headers are an error: a map script without them cannot
    /// run, and silently continuing would produce a fake "unknown function" wall.
    ///
    /// The same install's global string table is picked up on the way
    /// (`UI\FrameDef\GlobalStrings.fdf`, the `GetLocalizedString` fallback), so a
    /// script that shows a dialog gets the install's real text.
    pub fn add_game_scripts(&mut self, scripts_dir: &Path) -> Result<(), String> {
        let common = scripts_dir.join("common.j");
        let blizzard = scripts_dir.join("Blizzard.j");
        if !common.is_file() {
            return Err(format!("missing {}", common.display()));
        }
        self.add_source_file(&common)?;
        if blizzard.is_file() {
            self.add_source_file(&blizzard)?;
        }
        for candidate in [
            scripts_dir.join("../UI/FrameDef/GlobalStrings.fdf"),
            scripts_dir.join("../UI/GlobalStrings.fdf"),
        ] {
            if candidate.is_file()
                && let Ok(text) = std::fs::read_to_string(&candidate)
            {
                self.localized.load_global_strings(&text);
                break;
            }
        }
        Ok(())
    }

    /// Loads an unpacked `war3map.wts` payload (the map's trigger strings).
    pub fn set_wts(&mut self, bytes: &[u8]) -> usize {
        self.localized
            .load_wts(&String::from_utf8_lossy(bytes).as_ref())
    }

    /// Reads the `MapFlags` word out of an unpacked `war3map.w3i` payload.
    /// Returns `false` (and leaves the previous word in place) when the header
    /// layout is not one this reader decodes.
    pub fn add_map_w3i(&mut self, bytes: &[u8]) -> bool {
        match map_flags_from_w3i(bytes) {
            Some(word) => {
                self.map_flags = Some(word);
                true
            }
            None => false,
        }
    }

    pub fn add_map_script_file(&mut self, path: &Path) -> Result<usize, String> {
        self.add_source_file(path)
    }

    /// Adds an extracted `war3map.j`. Archive extraction lives outside this
    /// crate (`wc3re extract <map> war3map.j <out>`), so the runtime never
    /// depends on the MPQ stack to run a script.
    ///
    /// The map's own data tables are discovered next to the script when they
    /// were extracted with it: `X.w3x.wts` / `war3map.wts` for the trigger
    /// strings and `war3map.w3i` for the map flags. A missing table is not an
    /// error — the natives answer from what is loaded and record the gap.
    pub fn add_map_script(&mut self, path: &Path) -> Result<usize, String> {
        let added = self.add_source_file(path)?;
        let mut wts_paths: Vec<PathBuf> = Vec::new();
        let sibling = path.with_extension("wts");
        if sibling.is_file() {
            wts_paths.push(sibling);
        }
        if let Some(parent) = path.parent() {
            let plain = parent.join("war3map.wts");
            if plain.is_file() && !wts_paths.contains(&plain) {
                wts_paths.push(plain);
            }
            let info = parent.join("war3map.w3i");
            if info.is_file()
                && let Ok(bytes) = std::fs::read(&info)
            {
                self.add_map_w3i(&bytes);
            }
        }
        for wts in &wts_paths {
            if let Ok(bytes) = std::fs::read(wts) {
                self.set_wts(&bytes);
            }
        }
        Ok(added)
    }

    pub fn function_count(&self) -> usize {
        self.program.functions.len()
    }

    pub fn native_count(&self) -> usize {
        self.program.natives.len()
    }

    pub fn global_count(&self) -> usize {
        self.program.globals.len()
    }
}

/// Finds the game script directory: `WC3_SCRIPTS_DIR`, then the repo copies.
pub fn find_game_scripts() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("WC3_SCRIPTS_DIR") {
        let path = PathBuf::from(dir);
        if path.join("common.j").is_file() {
            return Some(path);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Newest header first: the shipped maps in this tree are 1.31/1.32 scripts
    // and they reference natives and constants the 1.24-era `analysis/war3_full`
    // copy does not declare (`ITEM_TYPE_ANY`, `EVENT_UNIT_CHANGE_OWNER`,
    // `bj_lastDyingWidget`). Loading the older header first produces a wall of
    // "undeclared variable" compile errors on a correct map script.
    let candidates = [
        manifest.join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Jass/1.32.9",
        ),
        manifest.join("../../analysis/war3_full/Scripts"),
        manifest.join("../../analysis/war3_data/Scripts"),
        manifest.join("../../analysis/war3x_data/Scripts"),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("common.j").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_scripts_load_and_merge() {
        let Some(dir) = find_game_scripts() else {
            eprintln!("skip: no game script directory");
            return;
        };
        let mut bundle = ScriptBundle::new();
        bundle.add_game_scripts(&dir).expect("game scripts");
        assert!(bundle.function_count() > 500, "{}", bundle.function_count());
        assert!(bundle.native_count() > 500, "{}", bundle.native_count());
        // common.j globals carry the event constants the scheduler resolves.
        assert!(
            bundle
                .program
                .globals
                .iter()
                .any(|global| global.name == "EVENT_UNIT_DEATH"),
            "EVENT_UNIT_DEATH must be declared by common.j"
        );
    }
}
