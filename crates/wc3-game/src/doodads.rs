//! Real map doodads (trees, rocks, props) from the canonical merge base.
//!
//! Map container: `wc3_re::mapfiles::MapBundle`; install resources:
//! `wc3_re::storm::StormFs` through `wc3_game::re::ReLayer`; object art:
//! `wc3_re::objectdata::ObjectDb` (`Doodads`/`DestructableData` SLKs); models:
//! `wc3_re::mdx::parse`; textures: `wc3_formats::blp::decode`. The inner
//! `war3map.doo` payload is read through the canonical MPQ facade
//! (`wc3_re::mpqex::MpqEx`) using `MapBundle::mpq_base`.
//!
//! The `.doo` record layout below follows
//! `donors/HiveWE/src/base/doodads.ixx:56-156`: `W3do`, version 7/8/13, then
//! id[4], variation, position(z-up), angle, scale, state/life, item sets and
//! creation number. The canonical home for this parser is
//! `wc3-formats::map::parse_doodads`; the exact move patch is in
//! `analysis/tech-lead/merge-world.md` (this private copy is the single
//! temporary duplicate named in that manifest and is deleted when the patch
//! lands).
//!
//! Doodad model/texture resolution follows HiveWE `:504-570`: `dir\file` +
//! variation, and destructables override a matching `replaceable_id` with
//! `texFile` (e.g. `ReplaceableTextures\LordaeronTree\LordaeronSummerTree.blp`).

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use wc3_anim::Animator;
use wc3_formats::blp;
use wc3_formats::mdx::MdxModel;
use wc3_re::mpqex::MpqEx;
use wc3_re::objectdata::{ObjectDb, ObjectRecord};

use crate::assets::{Assets, MeshData, build_meshes};
use crate::re::ReLayer;

#[derive(Clone, Debug)]
pub struct RawDoodad {
    pub id: [u8; 4],
    pub variation: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub angle: f32,
    pub scale: [f32; 3],
}

pub struct DoodadModel {
    pub meshes: Vec<MeshData>,
    /// Per-geoset alpha at the model's Stand start (GEOA/corpse gate applied),
    /// so hidden stump/death geosets never render on a living tree.
    pub alphas: Vec<f32>,
    /// Model-space footprint radius on the ground plane (Z-up X/Y).
    pub footprint: f32,
}

pub struct DoodadInstance {
    pub model: usize,
    pub world: Mat4,
    pub scale: f32,
}

pub struct DoodadWorld {
    pub models: Vec<DoodadModel>,
    pub instances: Vec<DoodadInstance>,
    /// Texture images referenced by the doodad meshes; the renderer appends
    /// them after `Assets::textures` and offsets material indices by that base.
    pub textures: Vec<crate::assets::TextureImage>,
    pub stats: DoodadStats,
}

#[derive(Default, Clone, Debug)]
pub struct DoodadStats {
    pub parsed: usize,
    pub placed: usize,
    pub models: usize,
    pub model_failures: usize,
    /// Parsed models with no geosets (particle/emitter doodads such as
    /// `BlightDoodad`); nothing for the static mesh pass to draw, not a leak.
    pub meshless_models: usize,
    pub texture_failures: usize,
    pub missing_ids: usize,
    /// "re" when the canonical layer supplied map/objects/archives,
    /// "fallback" when no install/map was mounted.
    pub source: &'static str,
}

fn debug_value(assets: &Assets, prefix: &str) -> Option<String> {
    assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix(prefix).map(str::to_string))
}

/// Install the `wc3-formats` diagnostic sink for this process.
///
/// Every skipped or patched data element (missing SLK column, out-of-range
/// position, corrupt record) reaches logcat as one `GAME_DATA_SKIP reason=…`
/// line. The format layer bounds the volume, so a million-row sheet cannot
/// flood the log.
pub(crate) fn install_format_diag_sink() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        wc3_formats::diag::set_sink(|line| crate::dev_log(line.to_string()));
    });
}

/// Parse `war3map.doo` through the canonical parser (`wc3-formats/map.rs`).
///
/// This used to be a private field walker that re-parsed the same records with
/// its own offsets; a corrupt record then desynchronised the walk silently.
/// The adapter keeps the renderer-facing shape and returns the typed parse
/// error so the caller can log and skip instead of drawing an empty world.
pub fn parse_doo(bytes: &[u8]) -> Result<Vec<RawDoodad>, wc3_formats::map::MapError> {
    let file = wc3_formats::map::parse_doodads_ex(bytes)?;
    Ok(file
        .doodads
        .into_iter()
        .map(|doodad| RawDoodad {
            id: doodad.type_id,
            variation: doodad.variation,
            x: doodad.position.x,
            y: doodad.position.y,
            z: doodad.position.z,
            angle: doodad.rotation,
            scale: [doodad.scale.x, doodad.scale.y, doodad.scale.z],
        })
        .collect())
}

struct TypeInfo {
    dir: String,
    file: String,
    /// Model path stem as authored in the sheet, when the sheet stores the
    /// whole path in one column (`Doodads\Terrain\LordaeronTree\LordaeronTree`).
    ///
    /// `ObjectDb` revisions differ: the extracted-tree loader splits the art
    /// into `dir` + `file` columns, the MPQ loader keeps the full path in
    /// `file` and has no `dir` column. Requiring both columns made every doodad
    /// resolve to `None` on the MPQ install (`parsed=2509 models=0`).
    stem: Option<String>,
    variations: u32,
    replaceable_id: u32,
    replaceable_texture: Option<String>,
}

/// Resolve doodad/destructable art from the canonical object database. The
/// `Doodads` and `DestructableData` sheets are already loaded by
/// `ObjectDb::load_storm`; no private SLK table lives here. Sheet names may be
/// stored as `Doodads`, `Doodads\Doodads`, or `Units\DestructableData`
/// depending on the RE layer's loader, so resolve the actual key first.
fn find_sheet<'a>(objects: &'a ObjectDb, wanted: &str) -> Option<&'a String> {
    objects.sheets.keys().find(|name| {
        name.eq_ignore_ascii_case(wanted)
            || name.rsplit('\\').next() == Some(wanted)
            || name.rsplit('/').next() == Some(wanted)
    })
}

/// Read a field through either the full sheet key (`Doodads\\Doodads`) or its
/// stem. `ObjectDb::slk_field` has changed key normalization between RE
/// revisions; accepting both keeps the world layer independent of that detail.
fn sheet_field<'a>(record: &'a ObjectRecord, table: &str, column: &str) -> Option<&'a str> {
    record.slk_field(table, column).or_else(|| {
        let stem = table.rsplit(['\\', '/']).next().unwrap_or(table);
        record.slk_field(stem, column)
    })
}

fn lookup_type(objects: &ObjectDb, id: &[u8; 4]) -> Option<TypeInfo> {
    let id = String::from_utf8_lossy(id).into_owned();
    let record: &ObjectRecord = objects.record(&id)?;
    let doodads = find_sheet(objects, "Doodads");
    let destructables = find_sheet(objects, "DestructableData");
    for (table, is_destructable) in [(doodads, false), (destructables, true)] {
        let Some(table) = table else {
            continue;
        };
        let Some(file) = sheet_field(record, table, "file") else {
            continue;
        };
        if file.is_empty() {
            continue;
        }
        let dir_column = sheet_field(record, table, "dir")
            .map(str::trim)
            .filter(|value| !value.is_empty());
        // Two authored layouts behind the same columns:
        //  * `dir` + `file` (`dir=Doodads\Terrain`, `file=LordaeronTree`), or
        //  * one full path in `file` (`file=Doodads\Terrain\LordaeronTree\LordaeronTree`)
        //    with no `dir` column at all (the MPQ object loader).
        let (dir, file, stem) = match dir_column {
            Some(dir) => (dir.to_string(), file.to_string(), None),
            None => {
                let trimmed = file.trim_end_matches('\\');
                let (parent, last) = trimmed
                    .rsplit_once('\\')
                    .map(|(parent, last)| (parent.to_string(), last.to_string()))
                    .unwrap_or_else(|| (String::new(), trimmed.to_string()));
                (parent, last, Some(trimmed.to_string()))
            }
        };
        if dir.is_empty() && stem.is_none() {
            continue;
        }
        let variations = sheet_field(record, table, "numVar")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);
        let replaceable_id = if is_destructable {
            sheet_field(record, table, "texID")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0)
        } else {
            0
        };
        let replaceable_texture = if is_destructable {
            sheet_field(record, table, "texFile")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty() && value != "_")
        } else {
            None
        };
        return Some(TypeInfo {
            dir: dir.replace('/', "\\"),
            file,
            stem,
            variations,
            replaceable_id,
            replaceable_texture,
        });
    }
    None
}

fn candidates(info: &TypeInfo, variation: u32) -> Vec<String> {
    let dir = info.dir.trim_end_matches('\\').to_string();
    let file = &info.file;
    let variation = if info.variations <= 1 { 0 } else { variation };
    // One-column layout: the authored stem already contains the model folder
    // (`Doodads\Terrain\LordaeronTree\LordaeronTree` + `6.mdx`).
    let mut out: Vec<String> = Vec::new();
    if let Some(stem) = &info.stem {
        out.push(format!("{stem}{variation}.mdx"));
        out.push(format!("{stem}{variation}.mdl"));
        out.push(format!("{stem}.mdx"));
        out.push(format!("{stem}.mdl"));
    }
    // Split layout: dir\file\fileN.mdx.
    out.extend([
        format!("{dir}\\{file}\\{file}{variation}.mdx"),
        format!("{dir}\\{file}\\{file}{variation}.mdl"),
        // Single-variation models live as dir\file\file.mdx (no number).
        format!("{dir}\\{file}\\{file}.mdx"),
        format!("{dir}\\{file}\\{file}.mdl"),
        // HiveWE-style direct append in the type directory.
        format!("{dir}\\{file}{variation}.mdx"),
        format!("{dir}\\{file}{variation}.mdl"),
        // Base model without a variation suffix.
        format!("{dir}\\{file}.mdx"),
        format!("{dir}\\{file}.mdl"),
    ]);
    out.dedup();
    out
}

fn load_model(
    storm: &wc3_re::storm::StormFs,
    info: &TypeInfo,
    variation: u32,
) -> Option<MdxModel> {
    for candidate in candidates(info, variation) {
        // Extracted archives mix `.mdx` and `.MDX`; Storm lookups here are
        // case-sensitive, so retry the swappable variants before giving up.
        let swapped = if candidate.ends_with(".mdx") {
            candidate.replace(".mdx", ".MDX")
        } else if candidate.ends_with(".MDX") {
            candidate.replace(".MDX", ".mdx")
        } else {
            candidate.clone()
        };
        let mut names = vec![candidate.clone()];
        if swapped != candidate {
            names.push(swapped);
        }
        let lower = candidate.to_ascii_lowercase();
        if lower != candidate {
            names.push(lower);
        }
        for name in names {
            let Ok(bytes) = storm.read(&name) else {
                continue;
            };
            if let Ok(model) = wc3_re::mdx::parse(&bytes) {
                return Some(model);
            }
        }
    }
    None
}

impl DoodadWorld {
    pub fn empty() -> Self {
        Self {
            models: Vec::new(),
            instances: Vec::new(),
            textures: vec![crate::assets::TextureImage::white()],
            stats: DoodadStats {
                source: "fallback",
                ..Default::default()
            },
        }
    }

    /// Load the map's doodads through the canonical base. Never panics; a
    /// missing install/map yields the labelled fallback world.
    pub fn load(assets: &Assets) -> Self {
        install_format_diag_sink();
        let Some(map_path) = debug_value(assets, "map=") else {
            return Self::empty();
        };
        // The asset layer opens StormFs itself and no longer writes `mpq=`;
        // locate the install root from the map's directory (device) or the
        // known archive roots (tests).
        let Some(install_root) = crate::terrain_render::install_root(assets) else {
            crate::dev_log!("GAME_DOODADS no_install_root map={map_path}");
            return Self::empty();
        };
        let layer = ReLayer::open(&install_root, Some(std::path::Path::new(&map_path)));
        let Some(map) = layer.map.as_ref() else {
            crate::dev_log!("GAME_DOODADS no_map notes={:?}", layer.notes);
            return Self::empty();
        };
        let Some(terrain) = map.terrain.as_ref() else {
            crate::dev_log!("GAME_DOODADS no_terrain");
            return Self::empty();
        };
        let center = terrain.center;
        let Some(objects) = layer.objects.as_ref() else {
            crate::dev_log!("GAME_DOODADS no_objects notes={:?}", layer.notes);
            return Self::empty();
        };
        let archive = match MpqEx::open_at(&map.path, map.mpq_base) {
            Ok(archive) => archive,
            Err(error) => {
                crate::dev_log!("GAME_DOODADS no_archive error={error}");
                return Self::empty();
            }
        };
        let bytes = match archive.read("war3map.doo") {
            Ok(bytes) => bytes,
            Err(error) => {
                crate::dev_log!("GAME_DOODADS no_doo error={error}");
                return Self::empty();
            }
        };
        let raw = match parse_doo(&bytes) {
            Ok(raw) => raw,
            Err(error) => {
                crate::dev_log!("GAME_DATA_SKIP reason=doo_parse file=war3map.doo error={error}");
                return Self::empty();
            }
        };
        if raw.is_empty() {
            return Self::empty();
        }

        let mut stats = DoodadStats {
            parsed: raw.len(),
            source: "re",
            ..Default::default()
        };
        let mut world = Self {
            models: Vec::new(),
            instances: Vec::with_capacity(raw.len()),
            textures: vec![crate::assets::TextureImage::white()],
            stats: DoodadStats::default(),
        };
        let mut model_cache: HashMap<(String, u32), Option<usize>> = HashMap::new();
        let mut texture_cache: HashMap<String, usize> = HashMap::new();
        texture_cache.insert("__white".to_string(), 0);

        for doodad in &raw {
            let id = String::from_utf8_lossy(&doodad.id).into_owned();
            let key = (id.clone(), doodad.variation);
            let model_index = if let Some(cached) = model_cache.get(&key) {
                *cached
            } else {
                let meshless_before = stats.meshless_models;
                let entry = lookup_type(objects, &doodad.id).and_then(|info| {
                    let mdx = load_model(&layer.storm, &info, doodad.variation)?;
                    let mut map_indices = Vec::with_capacity(mdx.textures.len());
                    for texture in &mdx.textures {
                        let replaceable = texture.replaceable_id;
                        let path = if replaceable != 0
                            && replaceable == info.replaceable_id
                            && info.replaceable_texture.is_some()
                        {
                            let base = info.replaceable_texture.clone().unwrap_or_default();
                            if base.to_ascii_lowercase().ends_with(".blp") {
                                base
                            } else {
                                format!("{base}.blp")
                            }
                        } else if replaceable != 0
                            || texture.path.trim_end_matches('\0').is_empty()
                        {
                            "__white".to_string()
                        } else {
                            texture.path.trim_end_matches('\0').replace('/', "\\")
                        };
                        if let Some(index) = texture_cache.get(&path) {
                            map_indices.push(*index);
                            continue;
                        }
                        match layer.storm.read(&path) {
                            Ok(bytes) => match blp::decode(&bytes) {
                                Ok(image) => {
                                    let source = crate::assets::TextureImage {
                                        width: image.width.max(1),
                                        height: image.height.max(1),
                                        rgba: image.rgba,
                                    };
                                    let resized = if source.width == 256 && source.height == 256 {
                                        source
                                    } else {
                                        let rgba = crate::terrain_render::resize_rgba(
                                            source.width.max(1),
                                            source.height.max(1),
                                            &source.rgba,
                                            256,
                                        );
                                        crate::assets::TextureImage {
                                            width: 256,
                                            height: 256,
                                            rgba,
                                        }
                                    };
                                    world.textures.push(resized);
                                    let index = world.textures.len() - 1;
                                    texture_cache.insert(path, index);
                                    map_indices.push(index);
                                }
                                Err(_) => {
                                    stats.texture_failures += 1;
                                    map_indices.push(0);
                                }
                            },
                            Err(_) => {
                                stats.texture_failures += 1;
                                map_indices.push(0);
                            }
                        }
                    }
                    let mut meshes = build_meshes(&mdx, &map_indices);
                    if meshes.is_empty() {
                        // Particle-only doodads (blight, glows) have no geosets.
                        stats.meshless_models += 1;
                        return None;
                    }
                    let stand = mdx
                        .sequences
                        .iter()
                        .find(|sequence| {
                            sequence.name.to_ascii_lowercase().contains("stand")
                        })
                        .or_else(|| mdx.sequences.first());
                    let alphas = crate::assets::mesh_alphas_for(
                        &mdx,
                        &meshes,
                        stand,
                        stand.map(|sequence| sequence.start).unwrap_or(0),
                    );
                    // Doodads are static, but many WC3 models (ruined towers,
                    // walls, trees) place their pieces with node transforms in
                    // Stand. Drawing the bind pose left those pieces exploded
                    // into huge flat planes. Pose once at Stand using the same
                    // weighted skinning the unit renderer uses.
                    let mut animator = Animator::from_model(&mdx);
                    if let Some(sequence) = stand {
                        animator.play(sequence);
                        animator.update(0.0);
                    }
                    let nodes = animator.world_matrices();
                    for mesh in &mut meshes {
                        if mesh.bones.is_empty() {
                            continue;
                        }
                        let mut posed = Vec::with_capacity(mesh.positions.len());
                        let mut posed_normals = Vec::with_capacity(mesh.positions.len());
                        for (index, position) in mesh.positions.iter().enumerate() {
                            let mut local = Vec3::ZERO;
                            let mut normal = Vec3::ZERO;
                            let mut total = 0.0f32;
                            if let Some(list) = mesh.bones.get(index) {
                                for bone in list {
                                    if (bone.bone as usize) < nodes.len() && bone.weight > 0.0 {
                                        let node = nodes[bone.bone as usize];
                                        local += node.transform_point3(*position) * bone.weight;
                                        normal += node.transform_vector3(
                                            mesh.normals.get(index).copied().unwrap_or(Vec3::Y),
                                        ) * bone.weight;
                                        total += bone.weight;
                                    }
                                }
                            }
                            if total > 1e-5 {
                                posed.push(local / total);
                                posed_normals.push(normal.normalize_or_zero());
                            } else {
                                posed.push(*position);
                                posed_normals.push(
                                    mesh.normals.get(index).copied().unwrap_or(Vec3::Y),
                                );
                            }
                        }
                        mesh.positions = posed;
                        mesh.normals = posed_normals;
                    }
                    let mut footprint: f32 = 0.0;
                    for (mesh_index, mesh) in meshes.iter().enumerate() {
                        if mesh.hide
                            || alphas.get(mesh_index).copied().unwrap_or(1.0) <= 0.01
                        {
                            continue;
                        }
                        for position in &mesh.positions {
                            footprint = footprint.max(position.x.abs().max(position.y.abs()));
                        }
                    }
                    if footprint > 400.0 {
                        crate::dev_log!(
                            "GAME_DOODAD_BIG id={} file={} footprint={:.1}",
                            String::from_utf8_lossy(&doodad.id),
                            info.file,
                            footprint
                        );
                    }
                    world.models.push(DoodadModel {
                        meshes,
                        alphas,
                        footprint,
                    });
                    stats.models += 1;
                    Some(world.models.len() - 1)
                });
                let meshless = stats.meshless_models > meshless_before;
                if entry.is_none() && !meshless {
                    stats.model_failures += 1;
                    if stats.missing_ids < 20 {
                        stats.missing_ids += 1;
                        match lookup_type(objects, &doodad.id) {
                            Some(info) => {
                                let probe: Vec<String> = candidates(&info, doodad.variation)
                                    .into_iter()
                                    .map(|path| {
                                        format!(
                                            "{path}:{}",
                                            if layer.storm.exists(&path) { "yes" } else { "no" }
                                        )
                                    })
                                    .collect();
                                crate::dev_log!(
                                    "DOODAD no_model id={id} dir={} file={} numVar={} texID={} texFile={:?} var={} candidates={probe:?}",
                                    info.dir,
                                    info.file,
                                    info.variations,
                                    info.replaceable_id,
                                    info.replaceable_texture,
                                    doodad.variation
                                );
                            }
                            None => crate::dev_log!("DOODAD no_art id={id}"),
                        }
                    }
                }
                model_cache.insert(key, entry);
                entry
            };
            let Some(model_index) = model_index else {
                continue;
            };
            let ground = assets.height_at(
                doodad.x - center.0,
                doodad.y - center.1,
            );
            let position = Vec3::new(
                doodad.x - center.0,
                ground + 1.5,
                doodad.y - center.1,
            );
            // W3C yaw is around +Z; render space is Y-up with Z negated, so
            // the angle flips sign. Non-uniform scale maps (x,y,z)->(x,z,y).
            let rotation = Quat::from_rotation_y(-doodad.angle);
            let scale = Vec3::new(doodad.scale[0], doodad.scale[2], doodad.scale[1]);
            let world_matrix = Mat4::from_scale_rotation_translation(scale, rotation, position);
            let uniform = doodad.scale[0]
                .abs()
                .max(doodad.scale[1].abs())
                .max(doodad.scale[2].abs());
            world.instances.push(DoodadInstance {
                model: model_index,
                world: world_matrix,
                scale: uniform,
            });
            stats.placed += 1;
        }
        world.stats = stats;
        crate::dev_log!(
            "GAME_DOODADS source={} parsed={} placed={} models={} model_failures={} texture_failures={}",
            world.stats.source,
            world.stats.parsed,
            world.stats.placed,
            world.stats.models,
            world.stats.model_failures,
            world.stats.texture_failures
        );
        world
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic: the only doodad model that fails to load on Felwood/Fallen
    /// maps is `Environment\\BlightDoodad\\BlightDoodad.mdx`; the candidate
    /// probe says the file exists, so either the read or the parse fails.
    #[test]
    #[ignore = "diagnostic: BlightDoodad mdx read/parse; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn blight_doodad_read_parse() {
        let mpq = std::path::Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq");
        if !mpq.exists() {
            return;
        }
        let root = mpq.parent().unwrap();
        let mut storm = wc3_re::storm::StormFs::open_wc3_install(root).unwrap_or_else(|_| wc3_re::storm::StormFs::new());
        if storm.listfile().is_empty() {
            let _ = storm.add_archive("War3", mpq);
        }
        for path in [
            "Environment\\BlightDoodad\\BlightDoodad.mdx",
            "Environment\\BlightDoodad\\BlightDoodad0.mdx",
        ] {
            crate::dev_log!(
                "BLIGHT exists={} path={path}",
                storm.exists(path)
            );
            match storm.read(path) {
                Ok(bytes) => {
                    crate::dev_log!("BLIGHT read_ok len={} path={path}", bytes.len());
                    match wc3_re::mdx::parse(&bytes) {
                        Ok(model) => crate::dev_log!(
                            "BLIGHT parse_ok meshes={} sequences={}",
                            model.geosets.len(),
                            model.sequences.len()
                        ),
                        Err(error) => crate::dev_log!("BLIGHT parse_err {error:?} path={path}"),
                    }
                }
                Err(error) => crate::dev_log!("BLIGHT read_err {error:?} path={path}"),
            }
        }
    }

    fn justice_install() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.exists())?;
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        map.exists().then_some((mpq, map))
    }

    /// Gate: every doodad on Justice that has an art row must load a model.
    ///
    /// The MPQ object loader stores the model path in one column and has no
    /// `dir` column; requiring both columns silently dropped every doodad
    /// (`models=0`). This test pins the one-column layout and the resulting
    /// model count.
    #[test]
    fn doodads_justice_models_load() {
        let Some((mpq, map)) = justice_install() else {
            crate::dev_log!("SKIP doodads_justice_models_load: install/map missing");
            return;
        };
        let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
        let world = DoodadWorld::load(&assets);
        crate::dev_log!(
            "JUSTICE_DOODADS parsed={} placed={} models={} failures={} textures={} instances={}",
            world.stats.parsed,
            world.stats.placed,
            world.models.len(),
            world.stats.model_failures,
            world.textures.len(),
            world.instances.len()
        );
        assert!(
            world.stats.parsed >= 2500,
            "Justice carries ~2900 doodads, parsed {}",
            world.stats.parsed
        );
        assert!(
            world.models.len() >= 90,
            "distinct doodad models on Justice: {}",
            world.models.len()
        );
        assert_eq!(
            world.stats.model_failures, 0,
            "every doodad id with an art row resolves a model: {:?}",
            world.stats.missing_ids
        );
        assert!(
            world.stats.placed as f32 > world.stats.parsed as f32 * 0.9,
            "most placements resolve: {} / {}",
            world.stats.placed,
            world.stats.parsed
        );
        // Static-batch input contract: geometry + both texture-coordinate sets
        // are either full-length or empty (the renderer reuses set 0).
        for model in &world.models {
            for mesh in &model.meshes {
                assert!(!mesh.positions.is_empty(), "doodad mesh has vertices");
                assert_eq!(mesh.uvs.len(), mesh.positions.len());
                if !mesh.uvs2.is_empty() {
                    assert_eq!(mesh.uvs2.len(), mesh.positions.len());
                }
            }
        }
    }

    /// The doodad batch actually reaches the GPU: a frame with doodads submits
    /// far more draws than one with `skip_doodads`, which proves the stride-60
    /// vertex layout and the per-layer specs are valid.
    #[test]
    #[ignore = "heavy: headless GPU render of the Justice doodad batch; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn doodads_justice_render_draws() {
        let Some((mpq, map)) = justice_install() else {
            crate::dev_log!("SKIP doodads_justice_render_draws: install/map missing");
            return;
        };
        let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
        let world = DoodadWorld::load(&assets);
        assert!(world.models.len() >= 90, "models {}", world.models.len());
        let mut game = crate::Game::new(assets, 0x5EED);
        game.update(crate::game::TICK_SECONDS);
        game.screen = crate::game::Screen::Playing;
        game.update(crate::game::TICK_SECONDS);
        let width = 960u32;
        let height = 540u32;
        game.camera.viewport = glam::Vec2::new(width as f32, height as f32);
        game.camera.target = game.base_pos;
        game.camera.distance = 1600.0;
        let scene = game.scene();
        let font = crate::hud::Font::load();
        let with = crate::render::RenderOptions {
            skip_ui: true,
            ..Default::default()
        };
        let without = crate::render::RenderOptions {
            skip_ui: true,
            skip_doodads: true,
            ..Default::default()
        };
        let (_, full) = crate::render::render_scene_with_skin(
            &game.assets,
            &font.atlas,
            None,
            &scene,
            width,
            height,
            with,
            None,
        )
        .expect("doodad frame");
        let (_, bare) = crate::render::render_scene_with_skin(
            &game.assets,
            &font.atlas,
            None,
            &scene,
            width,
            height,
            without,
            None,
        )
        .expect("bare frame");
        let doodad_draws = full.draws.saturating_sub(bare.draws);
        crate::dev_log!(
            "JUSTICE_DOODAD_DRAWS full={} bare={} doodads={doodad_draws}",
            full.draws,
            bare.draws
        );
        assert!(
            doodad_draws > 50,
            "the merged doodad batch must submit its draws: {doodad_draws}"
        );
    }

    /// Bisect the doodad pipeline stage by stage for one known id on the same
    /// ReLayer root the world loader uses (install root from `Assets`).
    #[test]
    fn doodad_probe_stages() {
        let Some((mpq, map)) = justice_install() else {
            crate::dev_log!("SKIP doodad_probe: install/map missing");
            return;
        };
        let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
        let Some(root) = crate::terrain_render::install_root(&assets) else {
            crate::dev_log!("DOODAD_PROBE no_install_root");
            return;
        };
        let layer = crate::re::ReLayer::open(&root, Some(&map));
        let Some(objects) = layer.objects.as_ref() else {
            crate::dev_log!("DOODAD_PROBE no_objects notes={:?}", layer.notes);
            return;
        };
        crate::dev_log!(
            "DOODAD_PROBE root={} objects={} sheets={}",
            root.display(),
            objects.objects.len(),
            objects.sheets.len()
        );
        // Record lookup is case-sensitive; WC3 stores mixed-case fourccs in the
        // map and lowercase ids in the SLK, so probe both.
        for id in ["VTlt", "vtlt", "VTlt"] {
            let hit = objects.record(id);
            crate::dev_log!(
                "DOODAD_PROBE record id={id:?} hit={} first_key={:?}",
                hit.is_some(),
                hit.map(|_| ())
            );
        }
        let sample: Vec<&String> = objects.objects.keys().take(8).collect();
        let sheets: Vec<&String> = objects.sheets.keys().collect();
        crate::dev_log!("DOODAD_PROBE keys={sample:?} sheets={sheets:?}");
        for sheet in &sheets {
            let stem = sheet.rsplit(['\\', '/']).next().unwrap_or(sheet);
            if stem.to_ascii_lowercase().contains("doodad")
                || stem.to_ascii_lowercase().contains("destruct")
            {
                let record = objects.record("VTlt");
                crate::dev_log!(
                    "DOODAD_PROBE sheet={sheet:?} stem={stem:?} dir={:?} file={:?}",
                    record.and_then(|record| record.slk_field(sheet, "dir")),
                    record.and_then(|record| record.slk_field(sheet, "file"))
                );
            }
        }
        let info = lookup_type(objects, b"VTlt");
        crate::dev_log!("DOODAD_PROBE lookup={:?}", info.as_ref().map(|i| (i.dir.clone(), i.file.clone(), i.variations)));
        if let Some(info) = info {
            let mdx = load_model(&layer.storm, &info, 6);
            crate::dev_log!(
                "DOODAD_PROBE mdx_ok={} geosets={}",
                mdx.is_some(),
                mdx.as_ref().map(|m| m.geosets.len()).unwrap_or(0)
            );
        }
    }
}
