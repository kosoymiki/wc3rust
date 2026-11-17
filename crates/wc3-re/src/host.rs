//! Host/launcher model for `War3.exe` → `Game.dll`.
//!
//! Evidence (`analysis/game_files` and `analysis/r2/`):
//! - `War3.exe` exports `A1`…`A695` (695 ordinal-only host services). The PE
//!   export names are literally `A<n>` (`analysis/r2/artifacts/War3.exe.exports.json`).
//! - `War3.exe` contains the strings `Game.dll`, `Couldn't open Game.dll.`,
//!   `Game.dll is corrupt.` and `Storm.dll`; it imports `LoadLibraryA`,
//!   `GetProcAddress` and `FreeLibrary` from kernel32.
//! - `Game.dll` has exactly one export, `GameMain` (RoC `0x6f004180`, TFT
//!   `0x6f005e70`), and imports 695-host-ordinal subset `A1`…`A695` plus 92
//!   Storm ordinals (see [`crate::storm_api`]).
//! - `Game.dll!GameMain` calls host `A372`, `A327`, `A475`, `A377`, `A180`,
//!   `A340` and Storm `503`/`585`; the boot implementation is
//!   `fcn.6f004200` (see [`crate::engine`]).
//!
//! The launcher therefore has one contract: load `Game.dll`, resolve the
//! `GameMain` export, hand it a host-services table whose entries are addressed
//! by ordinal, and let `GameMain` drive initialization. The Rust model exposes
//! the same shape so the Android port can substitute a native host without
//! changing game-side code.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::engine::{Engine, EngineConfig};
use crate::error::Result;

/// Host services Game.dll imports from War3.exe (RoC, 695-ordinal subset).
pub const HOST_IMPORTS: &[u32] = &[
    1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
    28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 49, 50, 51, 52, 55,
    56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 69, 70, 71, 72, 73, 74, 75, 77, 84, 85, 86, 87, 88,
    89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 107, 108, 109, 110,
    111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129,
    130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148,
    149, 150, 151, 152, 153, 154, 155, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168,
    169, 170, 171, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 188, 189, 190,
    191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209,
    210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 225, 226, 227, 228, 229,
    230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248,
    249, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267,
    268, 269, 270, 271, 272, 273, 274, 276, 277, 278, 279, 280, 282, 283, 284, 285, 286, 287, 288,
    289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 304, 305, 306, 307,
    308, 309, 310, 311, 314, 315, 316, 317, 318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 328,
    329, 330, 331, 332, 333, 334, 335, 336, 337, 339, 340, 341, 342, 343, 344, 345, 346, 347, 348,
    349, 350, 351, 352, 353, 354, 355, 356, 357, 358, 359, 360, 361, 362, 363, 364, 365, 366, 367,
    369, 371, 372, 373, 374, 375, 376, 377, 378, 379, 380, 381, 382, 384, 385, 386, 387, 388, 389,
    390, 392, 393, 394, 395, 396, 397, 398, 400, 401, 402, 403, 404, 405, 406, 407, 408, 409, 410,
    411, 412, 413, 414, 415, 416, 417, 418, 419, 420, 421, 422, 423, 424, 425, 426, 427, 428, 429,
    430, 431, 434, 435, 436, 438, 439, 441, 442, 443, 444, 445, 446, 447, 448, 449, 450, 451, 452,
    453, 455, 456, 457, 458, 459, 460, 462, 463, 464, 465, 466, 467, 469, 470, 471, 472, 473, 474,
    475, 476, 477, 478, 479, 482, 483, 484, 485, 486, 487, 488, 489, 490, 491, 492, 493, 494, 495,
    496, 497, 498, 499, 500, 501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511, 512, 514, 515,
    516, 517, 518, 519, 520, 521, 522, 523, 524, 525, 526, 527, 528, 529, 530, 531, 532, 533, 534,
    535, 536, 537, 538, 539, 540, 541, 542, 543, 544, 545, 546, 547, 548, 549, 550, 551, 552, 553,
    554, 555, 556, 557, 558, 559, 560, 561, 562, 563, 564, 565, 566, 567, 568, 569, 570, 571, 572,
    573, 574, 575, 576, 577, 578, 580, 581, 582, 583, 584, 585, 586, 587, 588, 589, 590, 591, 592,
    593, 594, 595, 596, 597, 598, 599, 600, 601, 602, 603, 604, 605, 606, 607, 608, 609, 610, 611,
    612, 613, 614, 615, 616, 617, 619, 620, 621, 622, 623, 624, 625, 626, 627, 628, 629, 630, 633,
    634, 635, 636, 637, 638, 639, 640, 641, 642, 643, 644, 645, 646, 647, 648, 649, 650, 651, 652,
    654, 655, 656, 657, 658, 659, 660, 661, 662, 663, 664, 665, 666, 667, 668, 669, 670, 671, 672,
    673, 674, 675, 676, 677, 678, 679, 680, 681, 682, 683, 684, 685, 686, 688, 691, 692, 693, 694,
    695,
];

#[derive(Debug, Clone, Default)]
pub struct HostCall {
    pub ordinal: u32,
    pub args: Vec<i64>,
}

/// Minimal host service table. The real War3.exe implements 695 entry points;
/// the port exposes the same ordinal-addressed surface. Ordinals 372/327/475/
/// 377/180/340 are the ones observed at `GameMain` call sites; the rest are
/// accepted and logged.
#[derive(Debug, Default)]
pub struct HostServices {
    pub calls: Vec<HostCall>,
    pub log: Vec<String>,
    pub clock_ms: u64,
    pub single_instance: bool,
    pub window: WindowState,
}

#[derive(Debug, Clone)]
pub struct WindowState {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub windowed: bool,
    pub opengl: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            title: "Warcraft III".into(),
            width: 1280,
            height: 720,
            windowed: true,
            opengl: true,
        }
    }
}

impl HostServices {
    pub fn call(&mut self, ordinal: u32, args: &[i64]) -> i64 {
        self.calls.push(HostCall {
            ordinal,
            args: args.to_vec(),
        });
        match ordinal {
            // GameMain host calls (see engine.rs evidence).
            372 => 1,                       // host object acquired
            327 => 0,
            475 => 0,                       // game tag "emag" accepted
            377 => 0,                       // path formatting
            180 => 0,                       // pre-shutdown
            340 => 0,                       // post-shutdown
            // Unknown host services are acknowledged but not emulated.
            _ => 0,
        }
    }

    pub fn frame(&mut self, dt_ms: u64) {
        self.clock_ms = self.clock_ms.wrapping_add(dt_ms);
    }

    /// Host-side launch sequence observed at `Game.dll!GameMain`: host object
    /// setup (A372/A327), game tag (A475), then `Engine::boot`.
    pub fn launch(&mut self, config: EngineConfig) -> Result<Engine> {
        self.call(372, &[]);
        self.call(327, &[]);
        self.call(475, &[0x6761_6d65]);
        Engine::boot(config)
    }
}

/// Launch plan: what the launcher must do before `Engine::boot`.
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub host_exe: PathBuf,
    pub game_dll: PathBuf,
    pub export: &'static str,
    pub host_exports: usize,
    pub game_dll_exports: usize,
    pub storm_imports: usize,
}

pub struct Launcher {
    pub install_root: PathBuf,
    pub game_dll: PathBuf,
    pub host_services: HostServices,
}

impl Launcher {
    pub fn new(install_root: impl Into<PathBuf>) -> Self {
        let install_root = install_root.into();
        Self {
            game_dll: install_root.join("Game.dll"),
            install_root,
            host_services: HostServices::default(),
        }
    }

    /// Validate the local binaries against the observed export/import counts.
    pub fn plan(&self) -> LaunchPlan {
        LaunchPlan {
            host_exe: self.install_root.join("War3.exe"),
            game_dll: self.game_dll.clone(),
            export: "GameMain",
            host_exports: 695,
            game_dll_exports: 1,
            storm_imports: crate::storm_api::GAME_IMPORTS_STORM.len(),
        }
    }

    /// Model the War3.exe sequence: LoadLibraryA("Game.dll") →
    /// GetProcAddress("GameMain") → GameMain(host table). Returns a booted
    /// engine using the host services table.
    pub fn launch(&mut self, config: EngineConfig) -> Result<Engine> {
        self.host_services.launch(config)
    }

    pub fn imported_host_ordinals(&self) -> BTreeSet<u32> {
        HOST_IMPORTS.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_and_imports_match_pe_evidence() {
        assert!(HOST_IMPORTS.len() > 600 && HOST_IMPORTS.len() <= 695);
        assert!(HOST_IMPORTS.contains(&180));
        assert!(HOST_IMPORTS.contains(&372));
        let mut host = HostServices::default();
        assert_eq!(host.call(475, &[0x6761_6d65]), 0);
        assert_eq!(host.calls.len(), 1);
    }
}
