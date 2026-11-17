//! Byte-level manifest of every file in the operator's MPQ archives.
//!
//! The manifest is the Phase-0 evidence base for the full WC3 port: for each
//! listfile entry it records the archive, name, uncompressed size, SHA-256 of
//! the extracted payload, detected format, MPQ codec chain and the parser that
//! consumes it. Unnamed blocks are reported explicitly (`blocks - listfile`)
//! instead of being silently dropped.
//!
//! Scope (default): the canonical TFT install archives plus the split base
//! archives under `analysis/war3_full3`. Duplicate mirror trees
//! (`analysis/war3_full`, `analysis/war3_full2`) are not rescanned; identical
//! archives are detected by SHA-256 and reported via `duplicate_of`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wc3_mpq::MpqArchive;

use crate::error::{ReError, Result};

#[derive(Debug, Clone, Default)]
pub struct ManifestEntry {
    pub archive: String,
    pub name: String,
    pub size: u32,
    pub compressed_size: u32,
    pub sha256: String,
    pub format: String,
    pub codec: String,
    pub parser: String,
    pub extractor: String,
    pub encrypted: bool,
    pub single_unit: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveSummary {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub duplicate_of: Option<String>,
    pub listfile: usize,
    pub blocks: usize,
    pub named: usize,
    pub unnamed: usize,
    pub read_ok: usize,
    pub read_failed: usize,
    pub payload_bytes: u64,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ByteManifest {
    pub generated_unix: u64,
    pub archives: Vec<ArchiveSummary>,
    pub total_files: usize,
    pub total_payload_bytes: u64,
    pub total_read_failed: usize,
    pub by_format: BTreeMap<String, usize>,
    pub by_codec: BTreeMap<String, usize>,
    pub by_parser: BTreeMap<String, usize>,
}

impl ByteManifest {
    pub fn ok(&self) -> bool {
        self.total_files > 0 && self.total_read_failed == 0
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Detect the file format from magic bytes first, extension second.
/// Returns `(format, parser)`.
pub fn detect_format(name: &str, bytes: &[u8]) -> (&'static str, &'static str) {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if bytes.starts_with(b"BLP1") || bytes.starts_with(b"BLP2") {
        return ("BLP", "wc3-re::blp");
    }
    if bytes.starts_with(b"MDLX") {
        return ("MDX", "wc3-re::mdx");
    }
    if bytes.starts_with(b"WTG!") {
        return ("WTG", "wc3-re::cutscenes");
    }
    if bytes.starts_with(b"HM3W") {
        return ("W3M", "wc3-formats::map");
    }
    if bytes.starts_with(b"MPQ\x1a") {
        return ("MPQ", "wc3-mpq");
    }
    if bytes.starts_with(b"RIFF") {
        return ("WAV", "audio");
    }
    if bytes.starts_with(b"OggS") {
        return ("OGG", "audio");
    }
    if bytes.starts_with(b"ID3") {
        return ("MP3", "audio");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return ("ZIP", "raw");
    }
    if bytes.starts_with(b"MZ") {
        return ("PE", "raw");
    }
    match ext.as_str() {
        "blp" => ("BLP", "wc3-re::blp"),
        "mdx" => ("MDX", "wc3-re::mdx"),
        "mdl" => ("MDL", "wc3-re::mdx"),
        "slk" => ("SLK", "wc3-formats::slk"),
        "txt" | "ini" => ("TXT", "wc3-re::txt"),
        "fdf" => ("FDF", "wc3-formats::fdf"),
        "toc" => ("TOC", "wc3-formats::fdf"),
        "w3e" => ("W3E", "wc3-formats::w3e"),
        "w3i" => ("W3I", "wc3-formats::w3i"),
        "wpm" => ("WPM", "wc3-formats::wpm"),
        "w3u" | "w3t" | "w3a" | "w3b" | "w3d" | "w3h" | "w3q" => {
            ("OBJDATA", "wc3-formats::gamedata")
        }
        "w3c" | "w3r" | "w3s" | "wtg" | "wct" => ("MAPDATA", "wc3-re::cutscenes"),
        "w3m" | "w3x" | "w3n" => ("MAP", "wc3-formats::map"),
        "wav" => ("WAV", "audio"),
        "mp3" => ("MP3", "audio"),
        "ogg" => ("OGG", "audio"),
        "ttf" | "otf" => ("FONT", "raw"),
        "tga" | "dds" | "png" | "jpg" => ("IMAGE", "raw"),
        "dll" | "exe" => ("PE", "raw"),
        "pld" => ("PLD", "raw"),
        "ai" | "j" => ("JASS", "wc3-re::jass"),
        "wts" => ("WTS", "wc3-re::wts"),
        "mpq" => ("MPQ", "wc3-mpq"),
        _ => ("BIN", "raw"),
    }
}

fn format_label(format: &str) -> &'static str {
    match format {
        "BLP" => "BLP image",
        "MDX" => "MDX model",
        "MDL" => "MDL model",
        "SLK" => "SLK table",
        "TXT" => "text table",
        "FDF" => "FDF UI",
        "TOC" => "TOC UI",
        "W3E" => "terrain",
        "W3I" => "map info",
        "WPM" => "pathing",
        "OBJDATA" => "object data",
        "MAPDATA" => "map data",
        "MAP" => "map archive",
        "WAV" => "WAV audio",
        "MP3" => "MP3 audio",
        "OGG" => "OGG audio",
        "FONT" => "font",
        "IMAGE" => "image",
        "PE" => "PE binary",
        "PLD" => "credits data",
        "JASS" => "JASS script",
        "WTS" => "string table",
        "MPQ" => "MPQ archive",
        _ => "binary",
    }
}

/// Default archive set: canonical TFT install MPQs + split base archives.
/// Stub files (<4 KiB, e.g. the placeholder install under `agent-assets`) are
/// skipped because they are not readable archives.
pub fn default_archives(root: &Path) -> Vec<PathBuf> {
    let analysis = root.join("analysis");
    let mut archives = Vec::new();
    for name in [
        "tft_files_Files100_War3x.mpq",
        "tft_files_Files100_War3xlocal.mpq",
    ] {
        let path = analysis.join(name);
        if path.is_file() {
            archives.push(path);
        }
    }
    let corpus = analysis.join("war3_full3");
    if let Ok(entries) = fs::read_dir(&corpus) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_mpq = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mpq"));
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            if is_mpq && size >= 4096 {
                archives.push(path);
            }
        }
    }
    archives.sort();
    archives.dedup();
    archives
}

fn archive_kind(path: &Path) -> String {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if name.starts_with("tft_files_") {
        "install".to_string()
    } else if name.starts_with("War3") {
        "install-split".to_string()
    } else {
        "corpus-split".to_string()
    }
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Build the manifest. Every listfile entry is read and hashed.
pub fn build(root: &Path, archives: &[PathBuf]) -> Result<ByteManifest> {
    if archives.is_empty() {
        return Err(ReError::Unsupported(
            "manifest: no MPQ archives selected".into(),
        ));
    }
    let mut manifest = ByteManifest {
        generated_unix: unix_now(),
        ..ByteManifest::default()
    };
    let mut seen_archives: BTreeMap<String, String> = BTreeMap::new();
    for path in archives {
        let archive = MpqArchive::open(path)
            .map_err(|err| ReError::Parse(format!("manifest {}: {err}", path.display())))?;
        let archive_sha = sha256_file(path)?;
        let archive_rel = relative_to(root, path);
        let duplicate_of = seen_archives.get(&archive_sha).cloned();
        seen_archives
            .entry(archive_sha.clone())
            .or_insert_with(|| archive_rel.clone());

        let mut summary = ArchiveSummary {
            path: archive_rel.clone(),
            kind: archive_kind(path),
            size_bytes: fs::metadata(path)?.len(),
            sha256: archive_sha,
            duplicate_of,
            listfile: archive.listfile().len(),
            blocks: archive.block_count(),
            named: 0,
            unnamed: 0,
            ..ArchiveSummary::default()
        };
        let names: Vec<String> = archive
            .files()
            .map(|(name, _)| name.to_string())
            .collect();
        summary.named = names.len();
        summary.unnamed = summary.blocks.saturating_sub(summary.listfile);

        for name in names {
            let Some((_, meta)) = archive.meta(&name) else {
                continue;
            };
            let (format, parser, codec, error) = match archive.read_with_info(&name) {
                Ok((bytes, usage)) => {
                    let (format, parser) = detect_format(&name, &bytes);
                    let sha = sha256_bytes(&bytes);
                    summary.payload_bytes += bytes.len() as u64;
                    summary.read_ok += 1;
                    let entry = ManifestEntry {
                        archive: archive_rel.clone(),
                        name: name.clone(),
                        size: meta.size,
                        compressed_size: meta.compressed_size,
                        sha256: sha,
                        format: format.to_string(),
                        codec: usage.describe(),
                        parser: parser.to_string(),
                        extractor: format!("wc3-mpq[{}] + {}", usage.describe(), parser),
                        encrypted: meta.is_encrypted(),
                        single_unit: meta.is_single_unit(),
                        error: None,
                    };
                    manifest.total_files += 1;
                    manifest.total_payload_bytes += u64::from(meta.size);
                    *manifest
                        .by_format
                        .entry(format_label(format).to_string())
                        .or_insert(0) += 1;
                    *manifest.by_codec.entry(entry.codec.clone()).or_insert(0) += 1;
                    *manifest.by_parser.entry(parser.to_string()).or_insert(0) += 1;
                    summary.entries.push(entry);
                    continue;
                }
                Err(err) => {
                    summary.read_failed += 1;
                    manifest.total_read_failed += 1;
                    let (format, parser) = ("UNREADABLE", "none");
                    (
                        format,
                        parser,
                        "unknown".to_string(),
                        Some(err.to_string()),
                    )
                }
            };
            // Unreadable entry: still recorded with block metadata.
            summary.entries.push(ManifestEntry {
                archive: archive_rel.clone(),
                name,
                size: meta.size,
                compressed_size: meta.compressed_size,
                sha256: String::new(),
                format: format.to_string(),
                codec,
                parser: parser.to_string(),
                extractor: "unreadable".to_string(),
                encrypted: meta.is_encrypted(),
                single_unit: meta.is_single_unit(),
                error,
            });
        }
        summary.entries.sort_by(|a, b| a.name.cmp(&b.name));
        manifest.archives.push(summary);
    }
    manifest.archives.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(manifest)
}

pub(crate) fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn json_str(text: &str) -> String {
    format!("\"{}\"", esc(text))
}

fn json_map(map: &BTreeMap<String, usize>) -> String {
    let body = map
        .iter()
        .map(|(key, value)| format!("{}:{}", json_str(key), value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

impl ByteManifest {
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"schema\":\"wc3-byte-manifest-v1\",\"generated_unix\":{},\"ok\":{},\"total_files\":{},\"total_payload_bytes\":{},\"total_read_failed\":{},\"by_format\":{},\"by_codec\":{},\"by_parser\":{},\"archives\":[",
            self.generated_unix,
            self.ok(),
            self.total_files,
            self.total_payload_bytes,
            self.total_read_failed,
            json_map(&self.by_format),
            json_map(&self.by_codec),
            json_map(&self.by_parser)
        );
        for (index, archive) in self.archives.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"path\":{},\"kind\":{},\"size_bytes\":{},\"sha256\":{},\"duplicate_of\":{},\"listfile\":{},\"blocks\":{},\"named\":{},\"unnamed\":{},\"read_ok\":{},\"read_failed\":{},\"payload_bytes\":{},\"files\":[",
                json_str(&archive.path),
                json_str(&archive.kind),
                archive.size_bytes,
                json_str(&archive.sha256),
                archive
                    .duplicate_of
                    .as_deref()
                    .map_or_else(|| "null".to_string(), json_str),
                archive.listfile,
                archive.blocks,
                archive.named,
                archive.unnamed,
                archive.read_ok,
                archive.read_failed,
                archive.payload_bytes
            );
            for (file_index, file) in archive.entries.iter().enumerate() {
                if file_index > 0 {
                    out.push(',');
                }
                let _ = write!(
                    out,
                    "{{\"name\":{},\"size\":{},\"compressed_size\":{},\"sha256\":{},\"format\":{},\"codec\":{},\"parser\":{},\"extractor\":{},\"encrypted\":{},\"single_unit\":{},\"error\":{}}}",
                    json_str(&file.name),
                    file.size,
                    file.compressed_size,
                    json_str(&file.sha256),
                    json_str(&file.format),
                    json_str(&file.codec),
                    json_str(&file.parser),
                    json_str(&file.extractor),
                    file.encrypted,
                    file.single_unit,
                    file.error.as_deref().map_or_else(|| "null".to_string(), json_str)
                );
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# MPQ BYTE MANIFEST — every listfile file");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Generated (unix): {} · **ok={}** · files **{}** · payload **{} B** · read failures **{}**",
            self.generated_unix,
            self.ok(),
            self.total_files,
            self.total_payload_bytes,
            self.total_read_failed
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Each row is a real listfile entry read through `wc3-mpq`; SHA-256 is over the extracted (decompressed) payload. Unnamed blocks are reported per archive as `blocks - listfile`."
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "## Archives");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| archive | kind | size | sha256 | listfile | blocks | named | unnamed | read ok | read fail | payload | duplicate of |"
        );
        let _ = writeln!(out, "|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---|");
        for archive in &self.archives {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | `{}` | {} | {} | {} | {} | {} | {} | {} | {} |",
                archive.path,
                archive.kind,
                archive.size_bytes,
                archive.sha256,
                archive.listfile,
                archive.blocks,
                archive.named,
                archive.unnamed,
                archive.read_ok,
                archive.read_failed,
                archive.payload_bytes,
                archive.duplicate_of.as_deref().unwrap_or("—")
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Formats");
        let _ = writeln!(out);
        let _ = writeln!(out, "| format | files |");
        let _ = writeln!(out, "|---|---:|");
        for (format, count) in &self.by_format {
            let _ = writeln!(out, "| {} | {} |", format, count);
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## MPQ codecs (observed sector tags)");
        let _ = writeln!(out);
        let _ = writeln!(out, "| codec | files |");
        let _ = writeln!(out, "|---|---:|");
        for (codec, count) in &self.by_codec {
            let _ = writeln!(out, "| {} | {} |", codec, count);
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Parsers");
        let _ = writeln!(out);
        let _ = writeln!(out, "| parser | files |");
        let _ = writeln!(out, "|---|---:|");
        for (parser, count) in &self.by_parser {
            let _ = writeln!(out, "| `{}` | {} |", parser, count);
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "## Entries");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| archive | name | size | sha256 | format | codec | extractor |"
        );
        let _ = writeln!(out, "|---|---|---:|---|---|---|---|");
        for archive in &self.archives {
            for file in &archive.entries {
                let sha = if file.sha256.is_empty() {
                    file.error.as_deref().unwrap_or("—")
                } else {
                    &file.sha256
                };
                let _ = writeln!(
                    out,
                    "| `{}` | `{}` | {} | `{}` | {} | {} | {} |",
                    file.archive,
                    file.name,
                    file.size,
                    sha,
                    file.format,
                    file.codec,
                    file.extractor
                );
            }
        }
        out
    }
}

/// Write the JSON + Markdown manifest artifacts.
pub fn write_artifacts(manifest: &ByteManifest, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = md.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(json, manifest.to_json())?;
    fs::write(md, manifest.to_markdown())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn detects_formats_from_magic_and_extension() {
        assert_eq!(detect_format("x.bin", b"BLP1xxxx"), ("BLP", "wc3-re::blp"));
        assert_eq!(detect_format("x.bin", b"MDLXxxxx"), ("MDX", "wc3-re::mdx"));
        assert_eq!(detect_format("x.bin", b"WTG!xx"), ("WTG", "wc3-re::cutscenes"));
        assert_eq!(detect_format("x.slk", b"ID;PWXL"), ("SLK", "wc3-formats::slk"));
        assert_eq!(detect_format("x.unknown", b"zz"), ("BIN", "raw"));
    }

    #[test]
    fn json_escapes_control_and_quotes() {
        assert_eq!(esc("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn manifests_a_real_small_archive() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("analysis/war3_full3/A.mpq");
        if !path.is_file() {
            eprintln!("skip: {} missing", path.display());
            return;
        }
        let manifest = build(&root, &[path]).expect("manifest");
        assert_eq!(manifest.archives.len(), 1);
        assert!(manifest.total_files > 0, "files {}", manifest.total_files);
        assert_eq!(manifest.total_read_failed, 0);
        for file in &manifest.archives[0].entries {
            assert_eq!(file.sha256.len(), 64, "sha for {}", file.name);
        }
        let json = manifest.to_json();
        assert!(json.contains("\"schema\":\"wc3-byte-manifest-v1\""));
        assert!(json.contains("\"files\":["));
        assert!(manifest.to_markdown().contains("MPQ BYTE MANIFEST"));
    }
}
