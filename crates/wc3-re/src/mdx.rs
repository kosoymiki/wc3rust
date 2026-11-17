//! MDX chunk scanner and typed model access.
//!
//! Classic MDX layout (all 2,048 local models):
//!
//! ```text
//! 0x00 char[4] "MDLX"
//! then a sequence of chunks:
//!   char[4] tag
//!   u32     size (payload bytes)
//!   u8[size] payload
//! ```
//!
//! Chunk tags observed in the local corpus: VERS (u32 version 800), MODL
//! (name[80], animation bounds, blend time), SEQS (sequence records 132 B),
//! GLBS, MTLS, TEXS (texture records 268 B: replaceable_id u32, path[260],
//! flags u32), GEOS (geoset: vertex/index/primitive arrays, matrices, groups,
//! extents, material id), GEOA (geoset animation: alpha + colour tracks),
//! BONE, HELP, ATCH, PIVT, PREM/PRE2, CAMS, EVTS, CLID, LAYS/HIT? (unknown tags
//! are preserved by [`MdxSummary::unknown_chunks`] instead of failing).
//!
//! `TEXS.replaceable_id` is the runtime skin slot: 0 = literal path, 1 = team
//! colour, 2 = team glow, 11+ = `ReplaceableTextures\...` ids resolved by the
//! renderer's replaceable-texture table. `MTLS` layers reference `TEXS` by
//! index; `GEOS.material_id` references `MTLS`. `GEOA` supplies per-sequence
//! alpha; `SEQS` supplies the frame ranges used by animation.

use wc3_formats::mdx::{Layer, Material, MdxModel, Texture};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdxChunk {
    pub tag: String,
    /// Offset of the chunk header in the file.
    pub offset: usize,
    /// Payload size, excluding the 8-byte header.
    pub size: usize,
}

/// Walk the chunk list without interpreting payloads. Returns an error only
/// when the file is not MDLX or a chunk length runs past EOF.
///
/// The byte walker is owned by `wc3-formats::mdx::MdxModel::chunks`; this
/// projects the payload-relative offsets to the header offsets used by the
/// resource layer (and keeps the `String` tag for the scanner).
pub fn chunks(bytes: &[u8]) -> Result<Vec<MdxChunk>> {
    let list = MdxModel::chunks(bytes)?;
    Ok(list
        .into_iter()
        .map(|chunk| MdxChunk {
            tag: String::from_utf8_lossy(&chunk.tag).into_owned(),
            offset: chunk.offset.saturating_sub(8),
            size: chunk.size,
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct MdxSummary {
    pub version: u32,
    pub name: String,
    pub chunk_count: usize,
    pub sequences: usize,
    pub global_sequences: usize,
    pub textures: usize,
    pub materials: usize,
    pub geosets: usize,
    pub geoset_animations: usize,
    pub bones: usize,
    pub helpers: usize,
    pub attachments: usize,
    pub events: usize,
    pub pivots: usize,
    /// Node objects that own a pivot (PIVT.len() must equal this).
    pub node_objects: usize,
    pub unknown_chunks: Vec<String>,
}

impl MdxSummary {
    pub fn from_model(model: &MdxModel, chunks: &[MdxChunk]) -> Self {
        const KNOWN: &[&str] = &[
            "VERS", "MODL", "SEQS", "GLBS", "MTLS", "TEXS", "GEOS", "GEOA", "BONE", "HELP",
            "ATCH", "PIVT", "PREM", "PRE2", "CAMS", "EVTS", "CLID", "BPOS", "FACs", "LITE",
        ];
        let mut unknown = Vec::new();
        for chunk in chunks {
            if !KNOWN.contains(&chunk.tag.as_str()) && !unknown.contains(&chunk.tag) {
                unknown.push(chunk.tag.clone());
            }
        }
        Self {
            version: model.version,
            name: model.name.clone(),
            chunk_count: chunks.len(),
            sequences: model.sequences.len(),
            global_sequences: model.global_sequences.len(),
            textures: model.textures.len(),
            materials: model.materials.len(),
            geosets: model.geosets.len(),
            geoset_animations: model.geoset_animations.len(),
            bones: model.bones.len(),
            helpers: model.helpers.len(),
            attachments: model.attachments.len(),
            events: model.events.len(),
            pivots: model.pivots.len(),
            node_objects: 0,
            unknown_chunks: unknown,
        }
    }
}

/// Parse the model with the `wc3-formats` MDX reader and summarize it.
pub fn parse(bytes: &[u8]) -> Result<MdxModel> {
    Ok(wc3_formats::mdx::MdxModel::parse(bytes)?)
}

pub fn summarize(bytes: &[u8]) -> Result<MdxSummary> {
    let chunk_list = chunks(bytes)?;
    let model = parse(bytes)?;
    let mut summary = MdxSummary::from_model(&model, &chunk_list);
    summary.node_objects = MdxModel::node_object_count(bytes)?;
    Ok(summary)
}

/// Texture path plus replaceable-slot id for every `TEXS` entry.
pub fn textures(model: &MdxModel) -> Vec<&Texture> {
    model.textures.iter().collect()
}

/// Material/layer access used by the renderer: `(material_index, layer)` in
/// declaration order, with texture references resolved to `TEXS` indices.
pub fn layers(model: &MdxModel) -> Vec<(usize, &Layer)> {
    model
        .materials
        .iter()
        .enumerate()
        .flat_map(|(index, material): (usize, &Material)| {
            material.layers.iter().map(move |layer| (index, layer))
        })
        .collect()
}

/// Every texture referenced by a material layer, as the exact `TEXS` path.
pub fn referenced_textures(model: &MdxModel) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for (_, layer) in layers(model) {
        if let Some(texture) = model.textures.get(layer.texture_id as usize)
            && !out.iter().any(|(path, id)| path == &texture.path && *id == texture.replaceable_id)
        {
            out.push((texture.path.clone(), texture.replaceable_id));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture() -> Option<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Units/Human/Footman/Footman.mdx");
        std::fs::read(path).ok()
    }

    #[test]
    fn scans_real_footman_model() {
        let Some(bytes) = fixture() else {
            eprintln!("skip: Footman.mdx missing");
            return;
        };
        let chunk_list = chunks(&bytes).expect("chunks");
        assert!(chunk_list.iter().any(|c| c.tag == "VERS"));
        assert!(chunk_list.iter().any(|c| c.tag == "GEOS"));
        let summary = summarize(&bytes).expect("summarize");
        assert_eq!(summary.version, 800);
        assert!(summary.geosets > 0);
        assert!(summary.textures > 0);
    }
}
