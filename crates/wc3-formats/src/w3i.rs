//! Map info (`war3map.w3i`).

use crate::FormatError;

#[derive(Debug, Clone, Default)]
pub struct MapInfo {
    pub version: u32,
    pub saves: u32,
    pub editor_version: u32,
    pub name: String,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
    pub raw_tail: Vec<u8>,
}

impl MapInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let head: [u8; 4] = bytes
            .first_chunk::<4>()
            .copied()
            .ok_or(FormatError::TooShort)?;
        let version = u32::from_le_bytes(head);
        // War3Net `MapInfo.ReadFrom`: mapVersion/editorVersion only exist from
        // v18 on, and the four-part game version only from v27 on. Older files
        // (the v8 RoC beta maps) start the strings right after the version
        // dword; reading the missing fields truncated every name.
        // Versions 18+ carry saves/editorVersion; 27+ add the 16-byte game
        // version, which the string reader skips as a positional gap.
        let mut pos = 4usize;
        let (saves, editor_version) = if version >= 18 {
            let header: [u8; 12] = bytes
                .first_chunk::<12>()
                .copied()
                .ok_or(FormatError::TooShort)?;
            if version >= 27 {
                pos = 28;
                if bytes.len() < pos {
                    return Err(FormatError::TooShort);
                }
            } else {
                pos = 12;
            }
            (
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]),
                u32::from_le_bytes([header[8], header[9], header[10], header[11]]),
            )
        } else {
            (0, 0)
        };
        let read_string = |pos: &mut usize| -> Result<String, FormatError> {
            let start = *pos;
            let end = bytes
                .get(start..)
                .ok_or(FormatError::TooShort)?
                .iter()
                .position(|&b| b == 0)
                .map(|i| start + i)
                .ok_or(FormatError::Corrupt("unterminated W3I string"))?;
            *pos = end + 1;
            Ok(String::from_utf8_lossy(
                bytes.get(start..end).ok_or(FormatError::TooShort)?,
            )
            .into_owned())
        };
        let name = read_string(&mut pos)?;
        let author = read_string(&mut pos)?;
        let description = read_string(&mut pos)?;
        let recommended_players = read_string(&mut pos)?;
        let raw_tail = bytes.get(pos..).unwrap_or_default().to_vec();
        Ok(Self {
            version,
            saves,
            editor_version,
            name,
            author,
            description,
            recommended_players,
            raw_tail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_real_map_info() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3i");
        let Ok(bytes) = fs::read(path) else {
            eprintln!("skipping: w3i fixture missing");
            return;
        };
        let info = MapInfo::parse(&bytes).expect("parse w3i");
        assert_eq!(info.version, 18);
        assert!(!info.name.is_empty());
        assert!(!info.raw_tail.is_empty());
    }
}
