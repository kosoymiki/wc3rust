//! Per-map custom sounds and regions: `war3map.w3s` / `war3map.w3r`.
//!
//! Format (observed on `analysis/war3_map/Human04`, w3s version 1, w3r
//! version 5):
//!
//! * `w3s`: `u32 version`, `u32 count`, then per sound three NUL-terminated
//!   strings (`name`, `file`, `eaxSetting`) and a fixed 68-byte trailer. The
//!   trailer is kept as `flags` (its first `u32`; observed 0 for 2D dialogue,
//!   6 for 3D unit sounds, 7 for looping doodad ambience) plus opaque bytes —
//!   the remaining fields are not interpreted because they are not documented
//!   consistently across editor versions.
//! * `w3r`: `u32 version`, `u32 count`, then per region four `f32` bounds, a
//!   NUL-terminated name, `u32 creation`, `u32 weather`, `u8 ambient` and the
//!   4-byte RGBA colour.
//!
//! Loudness/attenuation stay with the caller: map sounds are played at a
//! position with the normal [`crate::engine::PlayParams`] 3D rules.

use glam::Vec3;

const W3S_TRAILER_V1: usize = 68;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSound {
    pub name: String,
    pub file: String,
    pub eax: String,
    /// First `u32` of the w3s trailer (0 = dialogue, 6 = 3D, 7 = looping 3D).
    pub flags: u32,
}

impl MapSound {
    /// Observed looping-doodad sounds (lake/stream loops) carry flags 7.
    pub fn is_looping_hint(&self) -> bool {
        self.flags == 7
    }

    /// Three-dimensional hint from the shipped flag values.
    pub fn is_3d_hint(&self) -> bool {
        self.flags == 6 || self.flags == 7
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapRegion {
    pub name: String,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    /// `w3r` ambient id (0 = none in the observed map).
    pub ambient: u8,
}

impl MapRegion {
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min_x + self.max_x) * 0.5,
            0.0,
            (self.min_y + self.max_y) * 0.5,
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapSounds {
    pub version: u32,
    pub sounds: Vec<MapSound>,
    pub regions: Vec<MapRegion>,
}

impl MapSounds {
    /// Parse the two inner map files. A truncated record stops the list at the
    /// last complete entry instead of fabricating fields.
    pub fn load(w3s: &[u8], w3r: &[u8]) -> Result<Self, String> {
        let (version, sounds) = parse_w3s(w3s)?;
        let regions = parse_w3r(w3r)?;
        Ok(Self {
            version,
            sounds,
            regions,
        })
    }

    pub fn sound(&self, name: &str) -> Option<&MapSound> {
        self.sounds
            .iter()
            .find(|sound| sound.name.eq_ignore_ascii_case(name))
    }

    pub fn region(&self, name: &str) -> Option<&MapRegion> {
        self.regions
            .iter()
            .find(|region| region.name.eq_ignore_ascii_case(name))
    }
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    let slice = bytes
        .get(*offset..*offset + 4)
        .ok_or_else(|| "truncated u32".to_string())?;
    *offset += 4;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_f32(bytes: &[u8], offset: &mut usize) -> Result<f32, String> {
    Ok(f32::from_bits(read_u32(bytes, offset)?))
}

fn read_cstr(bytes: &[u8], offset: &mut usize) -> Result<String, String> {
    let rest = bytes.get(*offset..).ok_or_else(|| "truncated string".to_string())?;
    let end = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "unterminated string".to_string())?;
    let text = String::from_utf8_lossy(&rest[..end]).into_owned();
    *offset += end + 1;
    Ok(text)
}

/// Parse `war3map.w3s`. Returns `(version, sounds)`.
pub fn parse_w3s(bytes: &[u8]) -> Result<(u32, Vec<MapSound>), String> {
    let mut offset = 0usize;
    let version = read_u32(bytes, &mut offset)?;
    if version != 1 {
        return Err(format!("w3s: unsupported version {version}"));
    }
    let count = read_u32(bytes, &mut offset)? as usize;
    if count > 100_000 {
        return Err(format!("w3s: implausible count {count}"));
    }
    let mut sounds = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let name = read_cstr(bytes, &mut offset)?;
        let file = read_cstr(bytes, &mut offset)?;
        let eax = read_cstr(bytes, &mut offset)?;
        let flags = read_u32(bytes, &mut offset)?;
        // Remaining trailer fields are opaque; skip them.
        if bytes.len() < offset + (W3S_TRAILER_V1 - 4) {
            break;
        }
        offset += W3S_TRAILER_V1 - 4;
        sounds.push(MapSound {
            name,
            file,
            eax,
            flags,
        });
    }
    Ok((version, sounds))
}

/// Parse `war3map.w3r` regions.
pub fn parse_w3r(bytes: &[u8]) -> Result<Vec<MapRegion>, String> {
    let mut offset = 0usize;
    let _version = read_u32(bytes, &mut offset)?;
    let count = read_u32(bytes, &mut offset)? as usize;
    if count > 100_000 {
        return Err(format!("w3r: implausible count {count}"));
    }
    let mut regions = Vec::with_capacity(count.min(4096));
    for index in 0..count {
        let Ok(min_x) = read_f32(bytes, &mut offset) else { break };
        let Ok(min_y) = read_f32(bytes, &mut offset) else { break };
        let Ok(max_x) = read_f32(bytes, &mut offset) else { break };
        let Ok(max_y) = read_f32(bytes, &mut offset) else { break };
        let name = read_cstr(bytes, &mut offset).unwrap_or_else(|_| format!("region{index}"));
        // v5 trailer: creation u32, weather u32, ambient u8, RGBA u8[4].
        let Ok(_creation) = read_u32(bytes, &mut offset) else { break };
        let Ok(_weather) = read_u32(bytes, &mut offset) else { break };
        let Some(ambient) = bytes.get(offset).copied() else { break };
        offset += 1;
        if bytes.len() < offset + 4 {
            break;
        }
        offset += 4;
        regions.push(MapRegion {
            name,
            min_x,
            min_y,
            max_x,
            max_y,
            ambient,
        });
    }
    Ok(regions)
}
