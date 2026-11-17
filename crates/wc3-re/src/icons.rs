//! Command-button icon inventory.
//!
//! Unit/ability/item icons are not columns in the SLK tables; they live in the
//! object-data text layer as `Art=ReplaceableTextures\CommandButtons\*.blp`
//! (plus `Researchart`, `Unart`, `Targetart`). Command buttons are 64×64 BLP1
//! textures (observed on BTNFootman/BTNHolyBolt); the audit below verifies the
//! path resolves and the dimensions match what the 4×3 command card expects.

use crate::error::Result;
use crate::objectdata::{IconRef, ObjectDb};
use crate::storm::StormFs;

#[derive(Debug, Clone, Default)]
pub struct IconAudit {
    pub references: usize,
    pub unique_paths: usize,
    pub resolved: usize,
    pub missing: Vec<String>,
    pub wrong_size: Vec<(String, u32, u32)>,
    pub probe_failed: Vec<(String, String)>,
    pub by_object_kind: std::collections::BTreeMap<&'static str, usize>,
    pub sample: Vec<IconRef>,
}

impl IconAudit {
    pub fn complete(&self) -> bool {
        self.missing.is_empty() && self.probe_failed.is_empty()
    }
}

/// Verify every `*art` BLP reference against the filesystem.
pub fn audit(db: &ObjectDb, fs: &StormFs) -> IconAudit {
    let refs = db.icon_paths();
    let mut audit = IconAudit {
        references: refs.len(),
        ..IconAudit::default()
    };
    let mut unique = std::collections::BTreeSet::new();
    for icon in &refs {
        unique.insert(icon.path.to_ascii_lowercase());
        *audit
            .by_object_kind
            .entry(icon.kind.label())
            .or_insert(0) += 1;
        if audit.sample.len() < 12 {
            audit.sample.push(icon.clone());
        }
        if !fs.exists(&icon.path) {
            if audit.missing.len() < 40 {
                audit.missing.push(icon.path.clone());
            }
            continue;
        }
        let bytes = match fs.read(&icon.path) {
            Ok(bytes) => bytes,
            Err(err) => {
                if audit.probe_failed.len() < 40 {
                    audit.probe_failed.push((icon.path.clone(), err.to_string()));
                }
                continue;
            }
        };
        match crate::blp::probe(&bytes) {
            Ok(probe) => {
                audit.resolved += 1;
                // Command buttons are 64x64; a few passive/command icons are
                // 64x64 too, so a mismatch is reported rather than fatal.
                if (probe.width, probe.height) != (64, 64) && audit.wrong_size.len() < 40 {
                    audit.wrong_size.push((icon.path.clone(), probe.width, probe.height));
                }
            }
            Err(err) => {
                if audit.probe_failed.len() < 40 {
                    audit.probe_failed.push((icon.path.clone(), err.to_string()));
                }
            }
        }
    }
    audit.unique_paths = unique.len();
    audit
}

/// Resolve a single icon path through the archives (used by the HUD layer).
pub fn resolve_icon(fs: &StormFs, path: &str) -> Result<Vec<u8>> {
    fs.read(path)
}
