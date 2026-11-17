//! Pathing map (`war3map.wpm`).

use crate::FormatError;

#[derive(Debug, Clone, Default)]
pub struct PathingMap {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<u8>,
    /// Bytes left after the cell array. A well-formed file has zero; War3Net
    /// `MapPathingMap.ReadFrom` reads `width * height` cells and nothing else.
    pub trailing_bytes: usize,
}

impl PathingMap {
    /// Pathing flags are *disable* bits (authoritative `PathingMap::Flags`
    /// semantics): 0x02 = unwalkable, 0x04 = unflyable, 0x08 = unbuildable,
    /// 0x10 = harvestable, 0x20 = blight, 0x40 = water, 0x80 = amphibious.
    /// A set bit removes the capability, so a walkable cell is one without 0x02.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let header: [u8; 16] = bytes.first_chunk::<16>().copied().ok_or_else(|| {
            FormatError::UnknownMagic(bytes.first_chunk::<4>().copied().unwrap_or([0; 4]))
        })?;
        if &header[..4] != b"MP3W" {
            return Err(FormatError::UnknownMagic([
                header[0], header[1], header[2], header[3],
            ]));
        }
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version != 0 {
            return Err(FormatError::Unsupported("WPM version != 0"));
        }
        let width = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let height = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(FormatError::Corrupt("invalid pathing dimensions"));
        }
        let count = width as usize * height as usize;
        let cells = bytes
            .get(16..16 + count)
            .ok_or(FormatError::Corrupt("WPM cell data truncated"))?
            .to_vec();
        Ok(Self {
            width,
            height,
            cells,
            trailing_bytes: bytes.len() - (16 + count),
        })
    }

    pub fn walkable(&self, x: u32, y: u32) -> bool {
        self.cell(x, y).map(|c| c & 0x02 == 0).unwrap_or(false)
    }

    pub fn buildable(&self, x: u32, y: u32) -> bool {
        self.cell(x, y).map(|c| c & 0x08 == 0).unwrap_or(false)
    }

    /// Water flag (0x40). Ground units cannot enter; amphibious units (0x80)
    /// may cross water that also carries the amphibious bit.
    pub fn water(&self, x: u32, y: u32) -> bool {
        self.cell(x, y).map(|c| c & 0x40 != 0).unwrap_or(false)
    }

    pub fn amphibious(&self, x: u32, y: u32) -> bool {
        self.cell(x, y).map(|c| c & 0x80 != 0).unwrap_or(false)
    }

    pub fn cell(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_real_pathing_map() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.wpm");
        let Ok(bytes) = fs::read(path) else {
            eprintln!("skipping: wpm fixture missing");
            return;
        };
        let map = PathingMap::parse(&bytes).expect("parse wpm");
        assert_eq!((map.width, map.height), (512, 512));
        assert_eq!(map.cells.len(), 512 * 512);
        assert!(map.cells.iter().any(|c| c & 0x02 != 0), "some cells are blocked");
        assert!(map.cells.iter().any(|c| c & 0x02 == 0), "some cells are walkable");
    }

    #[test]
    fn flags_are_disable_bits() {
        let map = PathingMap {
            width: 5,
            height: 1,
            cells: vec![0x00, 0x02, 0x40, 0x80 | 0x40, 0x08],
            trailing_bytes: 0,
        };
        assert!(map.walkable(0, 0), "no flags = walkable");
        assert!(!map.walkable(1, 0), "0x02 marks unwalkable");
        assert!(map.walkable(2, 0), "water bit alone does not block the WPM walk flag");
        assert!(map.water(2, 0));
        assert!(map.water(3, 0) && map.amphibious(3, 0));
        assert!(map.buildable(0, 0));
        assert!(!map.buildable(4, 0), "0x08 marks unbuildable");
    }
}
