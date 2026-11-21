//! Map boot matrix: every shipped `.w3m` is booted through the canonical
//! `Engine`/JASS runtime on a 32 MB scoped stack (the same fix used by
//! `game.rs` `reset_map_runtime`), runs `main` + `InitCustomTriggers`, and is
//! ticked 100 frames so trigger/timer actions execute.
//!
//! The runner is the P0-4 deliverable: `wc3re map-boot` writes
//! `analysis/tech-lead/map_boot_matrix.json` with one real row per map and
//! `ok = main_ok && errors.is_empty()`. No map is skipped.
//!
//! The same run aggregates every native the 44 maps actually call
//! (`Runtime::native_calls`) and every native that fell through the dispatch
//! (`Runtime::unimplemented`), which is the P0 native-coverage evidence.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::{Engine, EngineConfig};
use crate::error::{ReError, Result};
use crate::mapfiles::discover_maps;

/// Frames ticked after `main`/`InitCustomTriggers`.
pub const BOOT_TICKS: u32 = 100;

#[derive(Debug, Clone, Default)]
pub struct MapBootRow {
    pub path: String,
    pub ok: bool,
    pub main_ok: bool,
    pub triggers: u32,
    pub executed_triggers: u32,
    pub ticks: u32,
    pub errors: Vec<String>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct MapBootMatrix {
    pub generated_unix: u64,
    pub ticks: u32,
    pub maps: Vec<MapBootRow>,
    pub totals: BTreeMap<String, u64>,
    /// Native name -> calls across all 44 maps.
    pub native_calls: BTreeMap<String, u64>,
    /// Native name -> fallthrough count (must be empty for coverage).
    pub unimplemented: BTreeMap<String, u64>,
}

impl MapBootMatrix {
    pub fn ok(&self) -> bool {
        !self.maps.is_empty() && self.maps.iter().all(|map| map.ok)
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-map-boot-matrix-v1\",\"generated_unix\":{},\"ticks\":{},\"ok\":{},\"maps\":[",
            self.generated_unix,
            self.ticks,
            self.ok()
        );
        for (index, map) in self.maps.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let errors = map
                .errors
                .iter()
                .map(|error| crate::manifest::json_str(error))
                .collect::<Vec<_>>()
                .join(",");
            let _ = write!(
                out,
                "{{\"path\":{},\"ok\":{},\"main_ok\":{},\"triggers\":{},\"executed_triggers\":{},\"ticks\":{},\"errors\":[{}],\"duration_ms\":{}}}",
                crate::manifest::json_str(&map.path),
                map.ok,
                map.main_ok,
                map.triggers,
                map.executed_triggers,
                map.ticks,
                errors,
                map.duration_ms
            );
        }
        out.push_str("],\"totals\":{");
        let mut first = true;
        for (key, value) in &self.totals {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", crate::manifest::json_str(key), value);
        }
        out.push_str("},\"natives\":{");
        let mut first = true;
        for (name, calls) in &self.native_calls {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", crate::manifest::json_str(name), calls);
        }
        out.push_str("},\"unimplemented\":{");
        let mut first = true;
        for (name, calls) in &self.unimplemented {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{}:{}", crate::manifest::json_str(name), calls);
        }
        out.push_str("}}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# MAP BOOT MATRIX — 44 shipped maps");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Generated (unix): {} · ticks: {} · **ok={}** · maps **{}**",
            self.generated_unix,
            self.ticks,
            self.ok(),
            self.maps.len()
        );
        let _ = writeln!(out, "\n| totals | value |");
        let _ = writeln!(out, "|---|---:|");
        for (key, value) in &self.totals {
            let _ = writeln!(out, "| {key} | {value} |");
        }
        let _ = writeln!(
            out,
            "\nNative coverage: **{} unique** / **{} calls** / **{} unimplemented**",
            self.native_calls.len(),
            self.native_calls.values().sum::<u64>(),
            self.unimplemented.len()
        );
        let _ = writeln!(out, "\n| map | ok | main | triggers | executed | ticks | errors | ms |");
        let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---|---:|");
        for map in &self.maps {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |",
                map.path,
                map.ok,
                map.main_ok,
                map.triggers,
                map.executed_triggers,
                map.ticks,
                if map.errors.is_empty() {
                    "—".to_string()
                } else {
                    map.errors.join("; ").replace('|', "\\|")
                },
                map.duration_ms
            );
        }
        out
    }

    pub fn native_coverage_json(&self) -> String {
        let called = self
            .native_calls
            .iter()
            .map(|(name, calls)| format!("{}:{}", crate::manifest::json_str(name), calls))
            .collect::<Vec<_>>()
            .join(",");
        let missing = self
            .unimplemented
            .iter()
            .map(|(name, calls)| format!("{}:{}", crate::manifest::json_str(name), calls))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema\":\"wc3-native-coverage-v1\",\"maps\":{},\"unique_natives\":{},\"calls\":{},\"unimplemented\":{{{missing}}},\"called\":{{{called}}}}}",
            self.maps.len(),
            self.native_calls.len(),
            self.native_calls.values().sum::<u64>()
        )
    }
}

fn boot_one(root: &Path, map: &Path) -> (MapBootRow, BTreeMap<String, u64>, BTreeMap<String, u64>) {
    let started = Instant::now();
    let map_path = map.to_path_buf();
    let map_label = map
        .strip_prefix(root)
        .unwrap_or(map)
        .display()
        .to_string();
    let root_owned = root.to_path_buf();
    // Match the game runtime: map scripts recurse deeply enough to overflow the
    // default 2 MB stack, so the entry point runs on a dedicated 32 MB stack.
    let result: std::result::Result<
        (bool, u32, u32, Vec<String>, BTreeMap<String, u64>, BTreeMap<String, u64>),
        String,
    > = std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("wc3-map-boot".into())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, move || {
                let mut config = EngineConfig::new(&root_owned);
                config.map = Some(map_path.clone());
                config.seed = 0x5EED_1234;
                let mut engine = Engine::boot(config).map_err(|error| error.to_string())?;
                let before = engine.script_report();
                for _ in 0..BOOT_TICKS {
                    engine.tick(1.0 / 30.0);
                }
                let after = engine.script_report();
                let mut errors = engine.script_errors.clone();
                let mut native_calls = BTreeMap::new();
                let mut unimplemented = BTreeMap::new();
                if let Some(runtime) = engine.runtime() {
                    errors.extend(runtime.errors.iter().cloned());
                    native_calls = runtime.native_calls.clone();
                    unimplemented = runtime.unimplemented.clone();
                }
                errors.sort();
                errors.dedup();
                Ok::<_, String>((
                    before.main_ok && after.main_ok,
                    after.triggers as u32,
                    after.executed_triggers as u32,
                    errors,
                    native_calls,
                    unimplemented,
                ))
            })
            .map_err(|error| error.to_string())?;
        handle
            .join()
            .map_err(|_| "map boot thread panicked".to_string())?
    });
    match result {
        Ok((
            main_ok,
            triggers,
            executed_triggers,
            errors,
            native_calls,
            unimplemented,
        )) => (
            MapBootRow {
                path: map_label,
                ok: main_ok && errors.is_empty(),
                main_ok,
                triggers,
                executed_triggers,
                ticks: BOOT_TICKS,
                errors,
                duration_ms: started.elapsed().as_millis(),
            },
            native_calls,
            unimplemented,
        ),
        Err(error) => (
            MapBootRow {
                path: map_label,
                ok: false,
                main_ok: false,
                triggers: 0,
                executed_triggers: 0,
                ticks: BOOT_TICKS,
                errors: vec![error],
                duration_ms: started.elapsed().as_millis(),
            },
            BTreeMap::new(),
            BTreeMap::new(),
        ),
    }
}

/// Boot every map under `maps_dir` (normally `<install>/Maps`).
pub fn boot_all(install_root: &Path, maps_dir: &Path) -> Result<MapBootMatrix> {
    let maps = discover_maps(maps_dir)?;
    if maps.is_empty() {
        return Err(ReError::NotFound(format!(
            "no .w3m/.w3x maps under {}",
            maps_dir.display()
        )));
    }
    let mut rows = Vec::with_capacity(maps.len());
    let mut native_calls = BTreeMap::new();
    let mut unimplemented = BTreeMap::new();
    for map in &maps {
        let (row, calls, missing) = boot_one(install_root, map);
        for (name, count) in calls {
            *native_calls.entry(name).or_insert(0) += count;
        }
        for (name, count) in missing {
            *unimplemented.entry(name).or_insert(0) += count;
        }
        rows.push(row);
    }
    let mut totals = BTreeMap::new();
    totals.insert("maps".to_string(), rows.len() as u64);
    totals.insert("ok".to_string(), rows.iter().filter(|row| row.ok).count() as u64);
    totals.insert(
        "main_ok".to_string(),
        rows.iter().filter(|row| row.main_ok).count() as u64,
    );
    totals.insert(
        "executed_triggers".to_string(),
        rows.iter().map(|row| u64::from(row.executed_triggers)).sum(),
    );
    totals.insert(
        "triggers".to_string(),
        rows.iter().map(|row| u64::from(row.triggers)).sum(),
    );
    Ok(MapBootMatrix {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        ticks: BOOT_TICKS,
        maps: rows,
        totals,
        native_calls,
        unimplemented,
    })
}

pub fn write_json(matrix: &MapBootMatrix, json: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(json, matrix.to_json())?;
    Ok(())
}

pub fn write_native_coverage(matrix: &MapBootMatrix, json: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(json, matrix.native_coverage_json())?;
    Ok(())
}

pub fn write_markdown(matrix: &MapBootMatrix, md: &Path) -> Result<()> {
    if let Some(parent) = md.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(md, matrix.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn fixture() -> &'static MapBootMatrix {
        static MATRIX: OnceLock<MapBootMatrix> = OnceLock::new();
        MATRIX.get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
            boot_all(&root, &root.join("Maps")).expect("map boot matrix")
        })
    }

    #[test]
    fn maps_boot_matrix_44_of_44() {
        let matrix = fixture();
        assert_eq!(matrix.maps.len(), 44, "map count");
        let failed: Vec<_> = matrix
            .maps
            .iter()
            .filter(|map| !map.ok)
            .map(|map| (&map.path, &map.errors))
            .collect();
        assert!(failed.is_empty(), "failed maps: {failed:?}");
        assert_eq!(matrix.totals["main_ok"], 44);
        assert!(matrix.totals["executed_triggers"] > 0);
        assert!(matrix.maps.iter().all(|map| map.triggers > 0));
        assert!(matrix.maps.iter().all(|map| map.ticks == BOOT_TICKS));
    }

    #[test]
    fn maps_jass_natives_cover_called_set() {
        let matrix = fixture();
        assert!(
            matrix.unimplemented.is_empty(),
            "unimplemented: {:?}",
            matrix.unimplemented
        );
        assert!(
            matrix.native_calls.len() > 300,
            "unique natives {}",
            matrix.native_calls.len()
        );
        // The boot window exercises the camera path through the setup natives.
        // `PanCameraToTimed` is not reached by any of the 44 maps inside
        // BOOT_TICKS: the campaign lane's pre-change matrix
        // (`analysis/agent-jass/native_coverage_20261121.json`) does not contain
        // it either, so requiring it here was a stale assertion that used to be
        // masked by the unimplemented-set panic. `CameraSetupSetDestPosition`
        // (1000 calls) is the camera native the boot actually drives.
        for native in [
            "createcamerasetup",
            "camerasetupapplyforceduration",
            "camerasetupsetdestposition",
            "setcamerafield",
        ] {
            assert!(
                matrix.native_calls.contains_key(native),
                "camera native {native} not called; sample {:?}",
                matrix.native_calls.keys().take(10).collect::<Vec<_>>()
            );
        }
    }
}
