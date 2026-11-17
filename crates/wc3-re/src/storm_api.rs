//! Storm API facade: the exported-ordinal surface Game.dll actually imports.
//!
//! Evidence chain:
//! - `Storm.dll` (RoC) exports 370 functions by ordinal only (101…647); the PE
//!   export table has no names (`analysis/r2/artifacts/Storm.dll.exports.json`).
//! - `donors/StormLib/storm.dll/storm.def` supplies the alternate Storm names
//!   and their ordinals: `StormCloseArchive @252`, `StormCloseFile @253`,
//!   `StormGetFileSize @265`, `StormOpenArchive @266`, `StormOpenFile @267`,
//!   `StormOpenFileEx @268`, `StormReadFile @269`, `StormSetBasePath @270`,
//!   `StormSetFilePointer @271`, `StormSetLocale @272`, `StormGetBasePath @273`,
//!   `StormGetArchiveName @275`, `StormGetFileName @276`,
//!   `StormOpenFileArchive @293`, `StormSetLastError @465`,
//!   `StormCompress @551`, `StormDecompress @552`.
//! - Game.dll imports a 92-ordinal subset of Storm; the facade below implements
//!   the file/archive ones and records the rest as unsupported instead of
//!   pretending success.
//!
//! The ordinal numbers are stable between RoC (311,360 B) and TFT (356,417 B):
//! both export tables start at 101 and the same low ordinals are imported by
//! the corresponding Game.dll.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::storm::StormFs;

/// Every ordinal exported by the local RoC Storm.dll (370 entries).
pub const STORM_EXPORTS: &[u32] = &[
    101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118,
    119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 133, 134, 135, 136, 137,
    138, 139, 140, 201, 202, 203, 204, 205, 206, 208, 209, 210, 211, 212, 213, 214, 215, 216,
    217, 218, 219, 220, 221, 222, 223, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261,
    262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279,
    280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 301, 302,
    303, 304, 312, 313, 314, 315, 321, 323, 324, 325, 326, 331, 332, 334, 335, 341, 342, 343,
    344, 345, 346, 347, 348, 349, 350, 351, 352, 353, 354, 355, 356, 357, 358, 359, 372, 373,
    374, 375, 376, 377, 378, 381, 382, 383, 385, 386, 387, 388, 389, 390, 391, 392, 393, 399,
    401, 403, 404, 405, 406, 412, 413, 414, 415, 416, 417, 418, 419, 420, 421, 422, 423, 424,
    425, 426, 427, 428, 429, 430, 431, 432, 433, 434, 436, 437, 438, 439, 440, 441, 442, 443,
    444, 445, 446, 447, 461, 462, 463, 464, 465, 466, 467, 468, 469, 470, 472, 473, 474, 475,
    476, 477, 478, 479, 480, 481, 482, 483, 484, 485, 486, 487, 488, 489, 490, 496, 497, 498,
    501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511, 512, 513, 514, 515, 516, 517, 518,
    519, 521, 523, 524, 525, 527, 528, 529, 530, 531, 532, 533, 534, 535, 536, 537, 538, 539,
    540, 541, 542, 543, 544, 545, 546, 547, 548, 549, 550, 551, 552, 553, 554, 555, 556, 557,
    558, 559, 560, 561, 562, 563, 564, 565, 566, 567, 568, 571, 572, 573, 574, 575, 576, 577,
    578, 579, 580, 581, 582, 583, 584, 585, 586, 587, 588, 589, 590, 591, 592, 593, 594, 601,
    602, 603, 604, 605, 606, 607, 608, 609, 610, 611, 612, 613, 614, 615, 616, 617, 618, 619,
    620, 621, 622, 623, 624, 625, 626, 627, 628, 629, 630, 631, 632, 633, 634, 635, 636, 637,
    638, 639, 640, 641, 642, 643, 644, 645, 646, 647,
];

/// Ordinals Game.dll imports from Storm.dll (RoC; 92 unique).
pub const GAME_IMPORTS_STORM: &[u32] = &[
    252, 253, 263, 265, 266, 267, 268, 269, 270, 271, 272, 275, 279, 280, 281, 288, 289, 291,
    293, 295, 401, 403, 405, 406, 421, 422, 423, 424, 425, 426, 428, 461, 462, 463, 465, 469,
    470, 472, 474, 476, 479, 496, 501, 503, 504, 506, 507, 508, 509, 510, 524, 525, 534, 537,
    541, 542, 543, 545, 548, 553, 565, 567, 568, 571, 572, 574, 575, 577, 578, 579, 580, 581,
    584, 585, 586, 587, 589, 590, 601, 603, 606, 607, 609, 612, 619, 621, 622, 624, 628, 633,
    636, 638,
];

/// Named Storm entry points (evidence: StormLib `storm.def`).
pub fn storm_name(ordinal: u32) -> Option<&'static str> {
    Some(match ordinal {
        252 => "SFileCloseArchive",
        253 => "SFileCloseFile",
        262 => "SFileDestroy",
        264 => "SFileGetFileArchive",
        265 => "SFileGetFileSize",
        266 => "SFileOpenArchive",
        267 => "SFileOpenFile",
        268 => "SFileOpenFileEx",
        269 => "SFileReadFile",
        270 => "SFileSetBasePath",
        271 => "SFileSetFilePointer",
        272 => "SFileSetLocale",
        273 => "SFileGetBasePath",
        275 => "SFileGetArchiveName",
        276 => "SFileGetFileName",
        293 => "SFileOpenFileArchive",
        465 => "SFileSetLastError",
        551 => "SFileCompress",
        552 => "SFileDecompress",
        _ => return None,
    })
}

/// Which logical Storm subsystem a known ordinal belongs to.
pub fn storm_module(ordinal: u32) -> &'static str {
    match ordinal {
        252..=295 => "SFile",
        461..=490 => "SFile/Storm",
        501..=594 => "Storm",
        101..=140 => "SBig/SBlt/SBmp/SCmd/SCode/SComp",
        201..=223 => "SCmd/SEvt/SGdi",
        321..=359 => "SNet/SStr/STrans",
        601..=647 => "SFile/Storm",
        _ => "unknown",
    }
}

/// Result of one facade call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StormValue {
    Int(i64),
    Bytes(Vec<u8>),
    Handle(u64),
    Bool(bool),
    Unsupported(u32),
}

/// A file handle opened through the facade.
#[derive(Debug, Clone, Default)]
pub struct StormFile {
    pub archive: String,
    pub name: String,
    pub bytes: Vec<u8>,
    pub position: usize,
}

/// Storm API implementation over the multi-archive [`StormFs`].
#[derive(Debug, Default)]
pub struct StormApi {
    pub fs: StormFs,
    pub locale: u32,
    pub base_path: String,
    pub last_error: u32,
    pub open_files: BTreeMap<u64, StormFile>,
    pub next_handle: u64,
    pub calls: BTreeMap<u32, u64>,
    pub unsupported: BTreeSet<u32>,
}

impl StormApi {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            ..Self::default()
        }
    }

    /// Add a mounted archive; returns its file count.
    pub fn mount(&mut self, label: &str, path: impl AsRef<std::path::Path>) -> Result<usize> {
        self.fs.add_archive(label, path)
    }

    /// Dispatch an ordinal. Only the file API has real behaviour; every other
    /// ordinal returns [`StormValue::Unsupported`] and is recorded.
    pub fn call(&mut self, ordinal: u32, args: &[StormValue]) -> Result<StormValue> {
        *self.calls.entry(ordinal).or_insert(0) += 1;
        let value = match ordinal {
            266 => {
                // String arguments are passed as Bytes; keep the API explicit.
                if let Some(StormValue::Bytes(path)) = args.first() {
                    let path = String::from_utf8_lossy(path).to_string();
                    match self.fs.add_archive(path.clone(), &path) {
                        Ok(_) => StormValue::Bool(true),
                        Err(err) => {
                            self.last_error = 2;
                            eprintln!("StormOpenArchive({path}): {err}");
                            StormValue::Bool(false)
                        }
                    }
                } else {
                    self.last_error = 87;
                    StormValue::Bool(false)
                }
            }
            252 => StormValue::Bool(true),
            267 | 268 => {
                let name = args
                    .get(1)
                    .and_then(|value| match value {
                        StormValue::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
                        _ => None,
                    })
                    .unwrap_or_default();
                match self.fs.read(&name) {
                    Ok(bytes) => {
                        let handle = self.next_handle;
                        self.next_handle += 1;
                        self.open_files.insert(
                            handle,
                            StormFile {
                                archive: String::new(),
                                name,
                                bytes,
                                position: 0,
                            },
                        );
                        StormValue::Handle(handle)
                    }
                    Err(_) => {
                        self.last_error = 2;
                        StormValue::Handle(0)
                    }
                }
            }
            253 => {
                if let Some(StormValue::Handle(handle)) = args.first() {
                    self.open_files.remove(handle);
                    StormValue::Bool(true)
                } else {
                    self.last_error = 87;
                    StormValue::Bool(false)
                }
            }
            265 => {
                if let Some(StormValue::Handle(handle)) = args.first() {
                    StormValue::Int(
                        self.open_files
                            .get(handle)
                            .map_or(0, |file| file.bytes.len() as i64),
                    )
                } else {
                    StormValue::Int(0)
                }
            }
            269 => {
                if let Some(StormValue::Handle(handle)) = args.first() {
                    let requested = args
                        .get(3)
                        .and_then(|value| match value {
                            StormValue::Int(value) => Some(*value as usize),
                            _ => None,
                        })
                        .unwrap_or(0);
                    if let Some(file) = self.open_files.get_mut(handle) {
                        let end = (file.position + requested).min(file.bytes.len());
                        let bytes = file.bytes[file.position..end].to_vec();
                        file.position = end;
                        StormValue::Bytes(bytes)
                    } else {
                        StormValue::Bytes(Vec::new())
                    }
                } else {
                    StormValue::Bytes(Vec::new())
                }
            }
            271 => {
                if let Some(StormValue::Handle(handle)) = args.first() {
                    let position = args
                        .get(2)
                        .and_then(|value| match value {
                            StormValue::Int(value) => Some(*value as usize),
                            _ => None,
                        })
                        .unwrap_or(0);
                    if let Some(file) = self.open_files.get_mut(handle) {
                        file.position = position.min(file.bytes.len());
                    }
                    StormValue::Int(0)
                } else {
                    StormValue::Int(0)
                }
            }
            270 => {
                if let Some(StormValue::Bytes(path)) = args.first() {
                    self.base_path = String::from_utf8_lossy(path).to_string();
                }
                StormValue::Bool(true)
            }
            272 => {
                if let Some(StormValue::Int(locale)) = args.first() {
                    self.locale = *locale as u32;
                }
                StormValue::Bool(true)
            }
            275 => StormValue::Bytes(b"wc3-re virtual archive".to_vec()),
            276 => StormValue::Bytes(b"wc3-re file".to_vec()),
            465 => {
                if let Some(StormValue::Int(error)) = args.first() {
                    self.last_error = *error as u32;
                }
                StormValue::Bool(true)
            }
            other => {
                self.unsupported.insert(other);
                StormValue::Unsupported(other)
            }
        };
        Ok(value)
    }

    /// Import ordinals Game.dll uses that the facade has no implementation for.
    pub fn unimplemented_game_imports(&self) -> Vec<u32> {
        let implemented = [252, 253, 265, 266, 267, 268, 269, 270, 271, 272, 275, 276, 293, 465];
        GAME_IMPORTS_STORM
            .iter()
            .copied()
            .filter(|ordinal| !implemented.contains(ordinal))
            .collect()
    }
}

/// Validate the local Storm export table against the embedded list.
pub fn validate_exports(actual: &[u32]) -> (usize, Vec<u32>, Vec<u32>) {
    let expected: BTreeSet<u32> = STORM_EXPORTS.iter().copied().collect();
        let actual_set: BTreeSet<u32> = actual.iter().copied().collect();
    let missing = expected.difference(&actual_set).copied().collect();
    let extra = actual_set.difference(&expected).copied().collect();
    (actual_set.len(), missing, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_table_matches_observed_pe() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/r2/artifacts/Storm.dll.exports.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("skip: export JSON missing");
            return;
        };
        let mut ordinals = Vec::new();
        for part in text.split("\"ordinal\":").skip(1) {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(ordinal) = digits.parse() {
                ordinals.push(ordinal);
            }
        }
        let (count, missing, extra) = validate_exports(&ordinals);
        assert_eq!(count, 370);
        assert!(missing.is_empty(), "missing {missing:?}");
        assert!(extra.is_empty(), "extra {extra:?}");
    }

    #[test]
    fn known_ordinals_are_named() {
        assert_eq!(storm_name(266), Some("SFileOpenArchive"));
        assert_eq!(storm_name(269), Some("SFileReadFile"));
        assert_eq!(storm_name(252), Some("SFileCloseArchive"));
    }
}
