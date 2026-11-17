//! Whole-install verification scanner.
//!
//! `wc3re scan` runs the resource-access layer over every local asset and
//! prints exact counts and failures. The JSON output is the machine-readable
//! evidence used by `analysis/agent-re/COVERAGE.md`; no count in that document
//! is typed by hand.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::ai::{AiCensus, scan_ai_dir};
use crate::error::Result;
use crate::icons::IconAudit;
use crate::mapfiles::{MapAudit, MapBundle, discover_maps};
use crate::objectdata::ObjectDb;
use crate::skin::{DecorateAudit, SkinTable, audit_fdf_decorate};
use crate::storm::StormFs;
use crate::terrain::TerrainArt;

#[derive(Debug, Clone, Default)]
pub struct BlpScan {
    pub total: usize,
    pub probed: usize,
    pub decoded: usize,
    pub probe_failed: Vec<(String, String)>,
    pub decode_failed: Vec<(String, String)>,
    pub by_encoding: BTreeMap<String, usize>,
    pub by_alpha_bits: BTreeMap<u32, usize>,
    pub dimensions: BTreeMap<String, usize>,
    pub zero_mips: usize,
    pub total_pixels: u64,
}

#[derive(Debug, Clone, Default)]
pub struct MdxScan {
    pub total: usize,
    pub chunk_scans: usize,
    pub parsed: usize,
    pub failed: Vec<(String, String)>,
    pub chunks: usize,
    pub sequences: usize,
    pub geosets: usize,
    pub materials: usize,
    pub textures: usize,
    pub geoset_animations: usize,
    pub bones: usize,
    pub attachments: usize,
    pub unknown_tags: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SlkScan {
    pub total: usize,
    pub parsed: usize,
    pub rows: usize,
    pub columns: usize,
    pub failed: Vec<(String, String)>,
    pub per_sheet: BTreeMap<String, (usize, usize)>,
}

#[derive(Debug, Clone, Default)]
pub struct TxtScan {
    pub total: usize,
    pub parsed: usize,
    pub sections: usize,
    pub entries: usize,
    pub failed: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct FdfScan {
    pub total: usize,
    pub parsed: usize,
    pub nodes: usize,
    pub props: usize,
    pub failed: Vec<(String, String)>,
    pub decorate: DecorateAudit,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptScan {
    pub jass_files: usize,
    pub jass_parsed: usize,
    pub jass_failed: Vec<(String, String)>,
    pub functions: usize,
    pub natives: usize,
    pub globals: usize,
    pub map_scripts: usize,
    pub map_parsed: usize,
    pub map_main_ok: usize,
    pub map_main_failed: Vec<(String, String)>,
    pub script_steps: u64,
    pub unimplemented: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default)]
pub struct MapScan {
    pub total: usize,
    pub opened: usize,
    pub errors: Vec<(String, String)>,
    pub inner_entries: usize,
    pub scripts: usize,
    pub terrain_tiles: usize,
    pub object_entries: usize,
    pub wts_entries: usize,
    pub audits: Vec<MapAudit>,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectScan {
    pub sheets: usize,
    pub objects: usize,
    pub counts: BTreeMap<String, usize>,
    pub metadata: usize,
    pub icon_paths: usize,
    pub unique_icon_paths: usize,
    pub icons: IconAudit,
    pub files: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TerrainScan {
    pub tiles: usize,
    pub cliffs: usize,
    pub water: usize,
    pub weather: usize,
    pub missing_textures: Vec<String>,
    pub texture_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TriggerScan {
    pub categories: usize,
    pub types: usize,
    pub events: usize,
    pub conditions: usize,
    pub actions: usize,
    pub calls: usize,
    pub functions: usize,
    pub strings_resolved: usize,
    pub strings_missing: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SkinScan {
    pub skins: Vec<String>,
    pub sections: usize,
    pub texture_entries: usize,
    pub missing_textures: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveScan {
    pub archives: Vec<ArchiveInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveInfo {
    pub label: String,
    pub path: String,
    pub opened: bool,
    pub listfile_entries: usize,
    pub block_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub root: String,
    pub maps_root: String,
    pub elapsed: Duration,
    pub archives: ArchiveScan,
    pub blp: BlpScan,
    pub mdx: MdxScan,
    pub slk: SlkScan,
    pub txt: TxtScan,
    pub fdf: FdfScan,
    pub scripts: ScriptScan,
    pub ai: AiCensus,
    pub maps: MapScan,
    pub objects: ObjectScan,
    pub terrain: TerrainScan,
    pub skins: SkinScan,
    pub trigger: TriggerScan,
}

impl ScanReport {
    pub fn run(root: &Path, maps_root: &Path, seed: u64) -> Result<Self> {
        let started = Instant::now();
        let mut report = Self {
            root: root.display().to_string(),
            maps_root: maps_root.display().to_string(),
            elapsed: Duration::default(),
            ..Self::default()
        };
        let files = walk_files(root);
        let mut storm = StormFs::new();
        let _ = storm.add_loose_dir("extracted", root);

        // Archives present in the repository (TFT War3x/War3xlocal).
        let analysis = root.parent().unwrap_or(root);
        for name in ["tft_files_Files100_War3x.mpq", "tft_files_Files100_War3xlocal.mpq"] {
            let path = analysis.join(name);
            if !path.is_file() {
                continue;
            }
            let mut info = ArchiveInfo {
                label: name.to_string(),
                path: path.display().to_string(),
                ..ArchiveInfo::default()
            };
            match wc3_mpq::MpqArchive::open(&path) {
                Ok(archive) => {
                    info.opened = true;
                    info.listfile_entries = archive.listfile().len();
                    info.block_count = archive.block_count();
                }
                Err(err) => info.error = Some(err.to_string()),
            }
            report.archives.archives.push(info);
        }

        // BLP: probe every file, decode every file.
        for path in &files {
            if !has_ext(path, "blp") {
                continue;
            }
            report.blp.total += 1;
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    report.blp.decode_failed.push((display(path), err.to_string()));
                    continue;
                }
            };
            let probe = match crate::blp::probe(&bytes) {
                Ok(probe) => probe,
                Err(err) => {
                    report.blp.probe_failed.push((display(path), err.to_string()));
                    continue;
                }
            };
            report.blp.probed += 1;
            *report
                .blp
                .by_encoding
                .entry(probe.encoding.label().to_string())
                .or_insert(0) += 1;
            *report.blp.by_alpha_bits.entry(probe.alpha_bits).or_insert(0) += 1;
            *report
                .blp
                .dimensions
                .entry(format!("{}x{}", probe.width, probe.height))
                .or_insert(0) += 1;
            if probe.mip_count == 0 {
                report.blp.zero_mips += 1;
            }
            report.blp.total_pixels += u64::from(probe.width) * u64::from(probe.height);
            match crate::blp::decode(&bytes) {
                Ok(image) => {
                    if image.rgba().len() == (probe.width * probe.height * 4) as usize {
                        report.blp.decoded += 1;
                    } else {
                        report.blp.decode_failed.push((
                            display(path),
                            format!(
                                "rgba len {} != {}x{}x4",
                                image.rgba().len(),
                                probe.width,
                                probe.height
                            ),
                        ));
                    }
                }
                Err(err) => report.blp.decode_failed.push((display(path), err.to_string())),
            }
        }

        // MDX: chunk scan + full model parse.
        for path in &files {
            if !has_ext(path, "mdx") {
                continue;
            }
            report.mdx.total += 1;
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    report.mdx.failed.push((display(path), err.to_string()));
                    continue;
                }
            };
            match crate::mdx::summarize(&bytes) {
                Ok(summary) => {
                    report.mdx.chunk_scans += 1;
                    report.mdx.parsed += 1;
                    report.mdx.chunks += summary.chunk_count;
                    report.mdx.sequences += summary.sequences;
                    report.mdx.geosets += summary.geosets;
                    report.mdx.materials += summary.materials;
                    report.mdx.textures += summary.textures;
                    report.mdx.geoset_animations += summary.geoset_animations;
                    report.mdx.bones += summary.bones;
                    report.mdx.attachments += summary.attachments;
                    report.mdx.unknown_tags.extend(summary.unknown_chunks);
                }
                Err(err) => report.mdx.failed.push((display(path), err.to_string())),
            }
        }

        // SLK.
        for path in &files {
            if !has_ext(path, "slk") {
                continue;
            }
            report.slk.total += 1;
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            match fs::read(path)
                .map_err(crate::error::ReError::Io)
                .and_then(|bytes| crate::slk::Sheet::parse(name.to_string(), &bytes))
            {
                Ok(sheet) => {
                    report.slk.parsed += 1;
                    report.slk.rows += sheet.len();
                    report.slk.columns += sheet.columns.len();
                    report
                        .slk
                        .per_sheet
                        .insert(name.to_string(), (sheet.len(), sheet.columns.len()));
                }
                Err(err) => report.slk.failed.push((display(path), err.to_string())),
            }
        }

        // TXT.
        for path in &files {
            if !has_ext(path, "txt") {
                continue;
            }
            report.txt.total += 1;
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            match fs::read(path)
                .map_err(crate::error::ReError::Io)
                .and_then(|bytes| crate::txt::TextTable::parse(name.to_string(), &bytes))
            {
                Ok(table) => {
                    report.txt.parsed += 1;
                    report.txt.sections += table.sections.len();
                    report.txt.entries += table
                        .sections
                        .iter()
                        .map(|section| section.entries.len())
                        .sum::<usize>();
                }
                Err(err) => report.txt.failed.push((display(path), err.to_string())),
            }
        }

        // FDF.
        for path in &files {
            if !has_ext(path, "fdf") {
                continue;
            }
            report.fdf.total += 1;
            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            match wc3_formats::fdf::FdfDocument::parse_named(&text, display(path)) {
                Ok(document) => {
                    report.fdf.parsed += 1;
                    let (nodes, props) = document.count();
                    report.fdf.nodes += nodes;
                    report.fdf.props += props;
                }
                Err(err) => report.fdf.failed.push((display(path), err.to_string())),
            }
        }

        // Scripts: common.j, Blizzard.j, Cheats.j (and any .j in the install).
        let mut jass_sources: Vec<(String, String)> = Vec::new();
        for path in &files {
            if !has_ext(path, "j") {
                continue;
            }
            report.scripts.jass_files += 1;
            match fs::read_to_string(path) {
                Ok(source) => {
                    match crate::jass::parse(&source) {
                        Ok(program) => {
                            report.scripts.jass_parsed += 1;
                            report.scripts.functions += program.functions.len();
                            report.scripts.natives += program.natives.len();
                            report.scripts.globals += program.globals.len();
                            jass_sources.push((display(path), source));
                        }
                        Err(err) => report
                            .scripts
                            .jass_failed
                            .push((display(path), err.to_string())),
                    }
                }
                Err(err) => report
                    .scripts
                    .jass_failed
                    .push((display(path), err.to_string())),
            }
        }

        // AI scripts.
        let scripts_dir = root.join("Scripts");
        if scripts_dir.is_dir() {
            report.ai = scan_ai_dir(&scripts_dir, seed)?;
        }

        // Maps: open every map, parse inner files, execute scripts with
        // common.j + Blizzard.j merged.
        let map_paths = discover_maps(maps_root).unwrap_or_default();
        let common = fs::read_to_string(root.join("Scripts/common.j")).ok();
        let blizzard = fs::read_to_string(root.join("Scripts/Blizzard.j")).ok();
        for path in map_paths {
            report.maps.total += 1;
            let bundle = match MapBundle::open(&path) {
                Ok(bundle) => bundle,
                Err(err) => {
                    report.maps.errors.push((display(&path), err.to_string()));
                    continue;
                }
            };
            report.maps.opened += 1;
            report.maps.inner_entries += bundle.entries.len();
            if bundle.script.is_some() {
                report.maps.scripts += 1;
            }
            if let Some(terrain) = &bundle.terrain {
                report.maps.terrain_tiles += terrain.tiles.len();
            }
            report.maps.object_entries += bundle.object_entries();
            if let Some(wts) = &bundle.wts {
                report.maps.wts_entries += wts.entries.len();
            }
            let audit = MapAudit::from_bundle(&bundle);
            if !audit.errors.is_empty() {
                report
                    .maps
                    .errors
                    .push((display(&path), audit.errors.join("; ")));
            }
            report.maps.audits.push(audit);
            let Some(script) = bundle.script.clone() else {
                continue;
            };
            report.scripts.map_scripts += 1;
            let mut program = crate::jass::Program::default();
            let mut parsed_all = true;
            for source in [common.as_deref(), blizzard.as_deref()] {
                if let Some(source) = source {
                    match crate::jass::parse(source) {
                        Ok(parsed) => program.merge(parsed),
                        Err(_) => parsed_all = false,
                    }
                }
            }
            match crate::jass::parse(&script) {
                Ok(parsed) => program.merge(parsed),
                Err(err) => {
                    parsed_all = false;
                    report
                        .scripts
                        .map_main_failed
                        .push((display(&path), format!("parse: {err}")));
                }
            }
            if !parsed_all {
                continue;
            }
            report.scripts.map_parsed += 1;
            let mut runtime = crate::jass::Runtime::new(program, seed);
            runtime.max_steps = 8_000_000;
            match runtime.run_main() {
                Ok(_) => report.scripts.map_main_ok += 1,
                Err(err) => report
                    .scripts
                    .map_main_failed
                    .push((display(&path), err.to_string())),
            }
            report.scripts.script_steps += runtime.steps;
            for (name, count) in runtime.unimplemented {
                *report.scripts.unimplemented.entry(name).or_insert(0) += count;
            }
        }

        // TriggerData / TriggerStrings registry.
        if let Ok(trigger) = crate::trigger::TriggerData::load_dir(&root.join("UI")) {
            report.trigger.categories = trigger.categories.len();
            report.trigger.types = trigger.types.len();
            report.trigger.events = trigger.events.len();
            report.trigger.conditions = trigger.conditions.len();
            report.trigger.actions = trigger.actions.len();
            report.trigger.calls = trigger.calls.len();
            let (total, resolved, missing) = trigger.string_coverage();
            report.trigger.functions = total;
            report.trigger.strings_resolved = resolved;
            report.trigger.strings_missing = missing;
        }

        // Object data + icons + terrain + skin.
        if let Ok(db) = ObjectDb::load_dir(root) {
            report.objects.sheets = db.sheets.len();
            report.objects.objects = db.objects.len();
            report.objects.counts = db
                .counts()
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect();
            report.objects.metadata = db.metadata.len();
            let icons = crate::icons::audit(&db, &storm);
            report.objects.icon_paths = icons.references;
            report.objects.unique_icon_paths = icons.unique_paths;
            report.objects.icons = icons;
            report.objects.files = db.loaded_files.len();
        }
        if let Ok(art) = TerrainArt::load_dir(&root.join("TerrainArt")) {
            report.terrain.tiles = art.tiles.len();
            report.terrain.cliffs = art.cliffs.len();
            report.terrain.water = art.water.len();
            report.terrain.weather = art.weather.len();
            report.terrain.texture_count = art.texture_paths().len();
            for (_, path) in art.texture_paths() {
                if !storm.exists(&path) {
                    report.terrain.missing_textures.push(path);
                }
            }
        }
        if let Ok(skins) = SkinTable::load(&root.join("UI/war3skins.txt")) {
            report.skins.skins = skins.skins.clone();
            report.skins.sections = skins.sections.len();
            let entries = skins.texture_entries();
            report.skins.texture_entries = entries.len();
            for (_, _, path) in &entries {
                // war3skins names UI models with the `.mdl` extension while the
                // archives store `.mdx`; accept either.
                let exists = storm.exists(path)
                    || (path.to_ascii_lowercase().ends_with(".mdl")
                        && storm.exists(&format!("{}x", &path[..path.len() - 1])));
                if !exists {
                    report.skins.missing_textures.push(path.clone());
                }
            }
            report.fdf.decorate = audit_fdf_decorate(&root.join("UI"), &skins, "Human");
        }

        report.elapsed = started.elapsed();
        Ok(report)
    }

    /// Core asset/format correctness. Content that is absent from this RoC base
    /// install (TFT-only icons, UI models not extracted) is reported separately
    /// in `objects.icons.missing` / `skins.missing_textures` and does not mask a
    /// parser failure.
    pub fn ok(&self) -> bool {
        self.blp.probe_failed.is_empty()
            && self.blp.decode_failed.is_empty()
            && self.mdx.failed.is_empty()
            && self.slk.failed.is_empty()
            && self.txt.failed.is_empty()
            && self.fdf.failed.is_empty()
            && self.scripts.jass_failed.is_empty()
            && self.scripts.map_main_failed.is_empty()
            && self.ai.failed.is_empty()
            && self.maps.errors.is_empty()
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!(
            "\"root\":{},\"maps_root\":{},\"elapsed_ms\":{},",
            json_str(&self.root),
            json_str(&self.maps_root),
            self.elapsed.as_millis()
        ));
        out.push_str(&format!(
            "\"blp\":{{\"total\":{},\"probed\":{},\"decoded\":{},\"probe_failed\":{},\"decode_failed\":{},\"zero_mips\":{},\"pixels\":{},\"by_encoding\":{},\"by_alpha_bits\":{},\"dimensions\":{}}},",
            self.blp.total,
            self.blp.probed,
            self.blp.decoded,
            json_pairs(&self.blp.probe_failed),
            json_pairs(&self.blp.decode_failed),
            self.blp.zero_mips,
            self.blp.total_pixels,
            json_map(&self.blp.by_encoding),
            json_map_num(&self.blp.by_alpha_bits),
            json_map(&self.blp.dimensions)
        ));
        out.push_str(&format!(
            "\"mdx\":{{\"total\":{},\"chunk_scans\":{},\"parsed\":{},\"failed\":{},\"chunks\":{},\"sequences\":{},\"geosets\":{},\"materials\":{},\"textures\":{},\"geoset_animations\":{},\"bones\":{},\"attachments\":{},\"unknown_tags\":{}}},",
            self.mdx.total,
            self.mdx.chunk_scans,
            self.mdx.parsed,
            json_pairs(&self.mdx.failed),
            self.mdx.chunks,
            self.mdx.sequences,
            self.mdx.geosets,
            self.mdx.materials,
            self.mdx.textures,
            self.mdx.geoset_animations,
            self.mdx.bones,
            self.mdx.attachments,
            json_set(&self.mdx.unknown_tags)
        ));
        out.push_str(&format!(
            "\"slk\":{{\"total\":{},\"parsed\":{},\"rows\":{},\"columns\":{},\"failed\":{},\"per_sheet\":{}}},",
            self.slk.total,
            self.slk.parsed,
            self.slk.rows,
            self.slk.columns,
            json_pairs(&self.slk.failed),
            json_map_tuple(&self.slk.per_sheet)
        ));
        out.push_str(&format!(
            "\"txt\":{{\"total\":{},\"parsed\":{},\"sections\":{},\"entries\":{},\"failed\":{}}},",
            self.txt.total,
            self.txt.parsed,
            self.txt.sections,
            self.txt.entries,
            json_pairs(&self.txt.failed)
        ));
        out.push_str(&format!(
            "\"fdf\":{{\"total\":{},\"parsed\":{},\"nodes\":{},\"props\":{},\"failed\":{},\"decorate\":{{\"documents\":{},\"decorate_nodes\":{},\"texture_strings\":{},\"resolved_keys\":{}}}}},",
            self.fdf.total,
            self.fdf.parsed,
            self.fdf.nodes,
            self.fdf.props,
            json_pairs(&self.fdf.failed),
            self.fdf.decorate.documents,
            self.fdf.decorate.decorate_nodes,
            self.fdf.decorate.texture_strings,
            self.fdf.decorate.resolved_keys
        ));
        out.push_str(&format!(
            "\"scripts\":{{\"jass_files\":{},\"jass_parsed\":{},\"jass_failed\":{},\"functions\":{},\"natives\":{},\"globals\":{},\"map_scripts\":{},\"map_parsed\":{},\"map_main_ok\":{},\"map_main_failed\":{},\"steps\":{},\"unimplemented\":{}}},",
            self.scripts.jass_files,
            self.scripts.jass_parsed,
            json_pairs(&self.scripts.jass_failed),
            self.scripts.functions,
            self.scripts.natives,
            self.scripts.globals,
            self.scripts.map_scripts,
            self.scripts.map_parsed,
            self.scripts.map_main_ok,
            json_pairs(&self.scripts.map_main_failed),
            self.scripts.script_steps,
            json_map_counts(&self.scripts.unimplemented)
        ));
        out.push_str(&format!(
            "\"ai\":{{\"scripts\":{},\"parsed\":{},\"failed\":{},\"functions\":{},\"natives\":{},\"global_decls\":{},\"common_ai_functions\":{},\"init_runs\":{},\"init_failures\":{}}},",
            self.ai.scripts,
            self.ai.parsed,
            json_pairs(&self.ai.failed),
            self.ai.functions,
            self.ai.natives,
            self.ai.global_decls,
            self.ai.common_ai_functions,
            self.ai.init_runs,
            json_pairs(&self.ai.init_failures)
        ));
        out.push_str(&format!(
            "\"maps\":{{\"total\":{},\"opened\":{},\"errors\":{},\"inner_entries\":{},\"scripts\":{},\"terrain_tiles\":{},\"object_entries\":{},\"wts_entries\":{}}},",
            self.maps.total,
            self.maps.opened,
            json_pairs(&self.maps.errors),
            self.maps.inner_entries,
            self.maps.scripts,
            self.maps.terrain_tiles,
            self.maps.object_entries,
            self.maps.wts_entries
        ));
        out.push_str(&format!(
            "\"objects\":{{\"sheets\":{},\"objects\":{},\"counts\":{},\"metadata\":{},\"icon_paths\":{},\"unique_icon_paths\":{},\"icons\":{{\"references\":{},\"unique_paths\":{},\"resolved\":{},\"missing\":{},\"wrong_size\":{},\"probe_failed\":{}}}}},",
            self.objects.sheets,
            self.objects.objects,
            json_map(&self.objects.counts),
            self.objects.metadata,
            self.objects.icon_paths,
            self.objects.unique_icon_paths,
            self.objects.icons.references,
            self.objects.icons.unique_paths,
            self.objects.icons.resolved,
            json_str_vec(&self.objects.icons.missing),
            self.objects.icons.wrong_size.len(),
            self.objects.icons.probe_failed.len()
        ));
        out.push_str(&format!(
            "\"trigger\":{{\"categories\":{},\"types\":{},\"events\":{},\"conditions\":{},\"actions\":{},\"calls\":{},\"functions\":{},\"strings_resolved\":{},\"strings_missing\":{}}},",
            self.trigger.categories,
            self.trigger.types,
            self.trigger.events,
            self.trigger.conditions,
            self.trigger.actions,
            self.trigger.calls,
            self.trigger.functions,
            self.trigger.strings_resolved,
            json_str_vec(&self.trigger.strings_missing)
        ));
        out.push_str(&format!(
            "\"terrain\":{{\"tiles\":{},\"cliffs\":{},\"water\":{},\"weather\":{},\"textures\":{},\"missing\":{}}},",
            self.terrain.tiles,
            self.terrain.cliffs,
            self.terrain.water,
            self.terrain.weather,
            self.terrain.texture_count,
            json_str_vec(&self.terrain.missing_textures)
        ));
        out.push_str(&format!(
            "\"skins\":{{\"skins\":{},\"sections\":{},\"texture_entries\":{},\"missing\":{}}},",
            json_str_vec(&self.skins.skins),
            self.skins.sections,
            self.skins.texture_entries,
            json_str_vec(&self.skins.missing_textures)
        ));
        let archives: Vec<String> = self
            .archives
            .archives
            .iter()
            .map(|archive| {
                format!(
                    "{{\"label\":{},\"path\":{},\"opened\":{},\"listfile\":{},\"blocks\":{},\"error\":{}}}",
                    json_str(&archive.label),
                    json_str(&archive.path),
                    archive.opened,
                    archive.listfile_entries,
                    archive.block_count,
                    archive.error.as_ref().map_or_else(|| "null".to_string(), |err| json_str(err))
                )
            })
            .collect();
        out.push_str(&format!("\"archives\":[{}],", archives.join(",")));
        out.push_str(&format!("\"ok\":{}", self.ok()));
        out.push('}');
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# wc3-re scan report\n\n");
        out.push_str(&format!("- root: `{}`\n", self.root));
        out.push_str(&format!("- maps: `{}`\n", self.maps_root));
        out.push_str(&format!("- elapsed: {:.2}s\n\n", self.elapsed.as_secs_f64()));
        out.push_str("| subsystem | total | ok | failures | detail |\n|---|---:|---:|---:|---|\n");
        out.push_str(&format!(
            "| BLP decode | {} | {} | {} | {} encodings, {} mips-zero, {} pixels |\n",
            self.blp.total,
            self.blp.decoded,
            self.blp.probe_failed.len() + self.blp.decode_failed.len(),
            self.blp.by_encoding.len(),
            self.blp.zero_mips,
            self.blp.total_pixels
        ));
        out.push_str(&format!(
            "| MDX parse | {} | {} | {} | {} chunks, {} geosets, {} materials |\n",
            self.mdx.total,
            self.mdx.parsed,
            self.mdx.failed.len(),
            self.mdx.chunks,
            self.mdx.geosets,
            self.mdx.materials
        ));
        out.push_str(&format!(
            "| SLK tables | {} | {} | {} | {} rows, {} columns |\n",
            self.slk.total,
            self.slk.parsed,
            self.slk.failed.len(),
            self.slk.rows,
            self.slk.columns
        ));
        out.push_str(&format!(
            "| TXT tables | {} | {} | {} | {} sections, {} entries |\n",
            self.txt.total,
            self.txt.parsed,
            self.txt.failed.len(),
            self.txt.sections,
            self.txt.entries
        ));
        out.push_str(&format!(
            "| FDF documents | {} | {} | {} | {} nodes, {} props |\n",
            self.fdf.total,
            self.fdf.parsed,
            self.fdf.failed.len(),
            self.fdf.nodes,
            self.fdf.props
        ));
        out.push_str(&format!(
            "| JASS files | {} | {} | {} | {} functions, {} natives |\n",
            self.scripts.jass_files,
            self.scripts.jass_parsed,
            self.scripts.jass_failed.len(),
            self.scripts.functions,
            self.scripts.natives
        ));
        out.push_str(&format!(
            "| Map scripts | {} | {} | {} | {} main ok, {} steps |\n",
            self.scripts.map_scripts,
            self.scripts.map_parsed,
            self.scripts.map_main_failed.len(),
            self.scripts.map_main_ok,
            self.scripts.script_steps
        ));
        out.push_str(&format!(
            "| AI scripts | {} | {} | {} | {} functions, {} InitAI ok |\n",
            self.ai.scripts,
            self.ai.parsed,
            self.ai.failed.len(),
            self.ai.functions,
            self.ai.init_runs
        ));
        out.push_str(&format!(
            "| Maps | {} | {} | {} | {} inner files, {} wts strings |\n",
            self.maps.total,
            self.maps.opened,
            self.maps.errors.len(),
            self.maps.inner_entries,
            self.maps.wts_entries
        ));
        out.push_str(&format!(
            "| Object database | {} | {} | {} | {} metadata fields, {} icon refs |\n",
            self.objects.sheets,
            self.objects.objects,
            self.objects.icons.missing.len(),
            self.objects.metadata,
            self.objects.icon_paths
        ));
        out.push_str(&format!(
            "| TriggerData | {} | {} | {} | {} events, {} conditions, {} actions, {} calls, {} strings |\n",
            self.trigger.functions,
            self.trigger.strings_resolved,
            self.trigger.strings_missing.len(),
            self.trigger.events,
            self.trigger.conditions,
            self.trigger.actions,
            self.trigger.calls,
            self.trigger.functions
        ));
        out.push_str(&format!(
            "| TerrainArt | {} | {} | {} | {} textures |\n",
            self.terrain.tiles,
            self.terrain.tiles - self.terrain.missing_textures.len(),
            self.terrain.missing_textures.len(),
            self.terrain.texture_count
        ));
        out.push_str(&format!(
            "| Skins | {} | {} | {} | {} sections, {} texture entries |\n",
            self.skins.skins.len(),
            self.skins.sections,
            self.skins.missing_textures.len(),
            self.skins.sections,
            self.skins.texture_entries
        ));
        out.push_str(&format!("| Overall | | {} | | elapsed {:.2}s |\n\n", self.ok(), self.elapsed.as_secs_f64()));
        if !self.scripts.unimplemented.is_empty() {
            out.push_str("## Unimplemented native calls (map main)\n\n");
            let mut calls: Vec<_> = self.scripts.unimplemented.iter().collect();
            calls.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
            for (name, count) in calls.iter().take(30) {
                out.push_str(&format!("- `{name}` × {count}\n"));
            }
            out.push('\n');
        }
        if !self.archives.archives.is_empty() {
            out.push_str("## Archives\n\n");
            for archive in &self.archives.archives {
                out.push_str(&format!(
                    "- `{}`: opened={}, listfile={}, blocks={}, error={:?}\n",
                    archive.label, archive.opened, archive.listfile_entries, archive.block_count, archive.error
                ));
            }
        }
        out
    }
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => walk(&path, out),
                Ok(file_type) if file_type.is_file() => out.push(path),
                _ => {}
            }
        }
    }
    walk(root, &mut out);
    out.sort();
    out
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(ext))
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn json_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn json_pairs(pairs: &[(String, String)]) -> String {
    let items: Vec<String> = pairs
        .iter()
        .take(100)
        .map(|(name, err)| format!("[{},{}]", json_str(name), json_str(err)))
        .collect();
    format!("[{}]", items.join(","))
}

fn json_str_vec(values: &[String]) -> String {
    let items: Vec<String> = values.iter().take(100).map(|value| json_str(value)).collect();
    format!("[{}]", items.join(","))
}

fn json_set(values: &BTreeSet<String>) -> String {
    let items: Vec<String> = values.iter().map(|value| json_str(value)).collect();
    format!("[{}]", items.join(","))
}

fn json_map(map: &BTreeMap<String, usize>) -> String {
    let mut items = Vec::new();
    for (key, value) in map {
        items.push(format!("{}:{}", json_str(key), value));
    }
    format!("{{{}}}", items.join(","))
}

fn json_map_num(map: &BTreeMap<u32, usize>) -> String {
    let mut items = Vec::new();
    for (key, value) in map {
        items.push(format!("\"{key}\":{value}"));
    }
    format!("{{{}}}", items.join(","))
}

fn json_map_counts(map: &BTreeMap<String, u64>) -> String {
    let mut items = Vec::new();
    for (key, value) in map {
        items.push(format!("{}:{}", json_str(key), value));
    }
    format!("{{{}}}", items.join(","))
}

fn json_map_tuple(map: &BTreeMap<String, (usize, usize)>) -> String {
    let mut items = Vec::new();
    for (key, (a, b)) in map {
        items.push(format!("{}:[{},{}]", json_str(key), a, b));
    }
    format!("{{{}}}", items.join(","))
}
