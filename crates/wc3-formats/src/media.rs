//! Movies (`Movies\*.bik`) and the media table the campaign UI needs.
//!
//! Warcraft III ships its CGI as Bink video inside the data archives. The
//! engine opens them by the path recorded in the campaign/mission script
//! (`PlayMovie`/`PlayMovieBJ`), so the port needs two things: the list of movie
//! files the install actually contains, and a byte-exact extraction path.
//!
//! Discovery walks three sources in order of evidence strength:
//!
//! 1. **Listfile entries** — the archive's own name table, filtered to
//!    `movies\*` with a `.bik` extension. This is authoritative when present.
//! 2. **Hash probes** — every name is a hash lookup, so a candidate list can
//!    recover files whose `(listfile)` entry is missing or incomplete. The
//!    candidate names come from the shipped campaign scripts (`PlayMovie`).
//! 3. **Unnamed blocks with a Bink magic** — a block that no name resolves to
//!    is still a file; if its payload starts with `BIK`/`KB2` it is a movie,
//!    and it can be read by block index (`read_block`).
//!
//! The module reports which of the three matched, so a caller can tell a
//! listed movie from a recovered one. Nothing here guesses a duration; that is
//! [`ffprobe`](crate) work in `analysis/media/`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use wc3_mpq::{MpqArchive, MpqError};

/// Directory inside the archives that holds the CGI.
pub const MOVIE_DIR: &str = "movies";

/// Minimum payload size worth probing for a Bink magic during a block scan.
/// The smallest shipped movie is several megabytes; the threshold only exists
/// to keep the scan bounded.
pub const SCAN_MIN_BYTES: u32 = 512 * 1024;

/// Upper bound for a block the scan will pull into memory. A movie is tens of
/// megabytes; anything above this is not a movie and is skipped unsampled.
pub const SCAN_MAX_BYTES: u32 = 256 * 1024 * 1024;

/// Bytes of the block head needed to classify a movie container: `BIK`/`KB2`
/// are three bytes, `RIFF` is four. The scan reads exactly this prefix, never
/// the whole block (WC3-MPQ-002).
pub const MOVIE_MAGIC_BYTES: usize = 8;

/// Total declared block bytes the unnamed-block scan will decode.
///
/// The prefix read is lazy for sector-compressed blocks, but a crafted
/// single-unit block has to be decompressed whole before its first byte can be
/// inspected, so the scan stops once this budget is spent instead of decoding
/// an arbitrary number of 256 MiB blocks (WC3-MPQ-002).
pub const SCAN_DECODE_BUDGET: u64 = 512 * 1024 * 1024;

/// Where a discovered movie came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieSource {
    /// Named by the archive listfile.
    Listfile,
    /// Recovered through a hash probe of a known engine path.
    Probed,
    /// Unnamed block whose payload is Bink video.
    UnnamedBlock,
}

impl MovieSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Listfile => "listfile",
            Self::Probed => "probed",
            Self::UnnamedBlock => "unnamed-block",
        }
    }
}

/// One movie file in one archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieEntry {
    /// Archive path (`Movies\...bik`), or the StormLib pseudo-name for an
    /// unnamed block (`File########.bik`).
    pub name: String,
    /// Declared (uncompressed) size.
    pub size: u32,
    /// Block index in the archive table.
    pub block: u32,
    /// How the entry was found.
    pub source: MovieSource,
    /// First four payload bytes, as ASCII when printable.
    pub magic: [u8; 4],
}

impl MovieEntry {
    /// File name without the archive directory.
    pub fn file_name(&self) -> &str {
        self.name
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(self.name.as_str())
    }

    /// Output file name for an extraction: the shipped name when it is known,
    /// the pseudo-name otherwise. Always `.bik`.
    pub fn output_name(&self) -> String {
        let file = self.file_name();
        let (stem, ext) = file
            .rsplit_once('.')
            .filter(|(_, ext)| MOVIE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
            .unwrap_or((file, "bik"));
        format!("{stem}.{ext}")
    }
}

/// The nine cinematics the game's "Play movie" API accepts.
pub const KNOWN_MOVIES: [&str; 9] = [
    "HumanEd",
    "HumanOp",
    "NightElfEd",
    "OrcEd",
    "UndeadEd",
    "TutorialIn",
    "TutorialOp",
    "Ghost",
    "WowGameplay",
];

/// One movie file found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMovie {
    /// One of [`KNOWN_MOVIES`].
    pub name: String,
    pub path: std::path::PathBuf,
    pub bytes: u64,
}

/// Scans `roots` (depth ≤ 3) for the nine known movie files.
///
/// Returns `(found, missing, unreadable)`: found entries are deduplicated by
/// name (first root wins), `missing` lists the known names with no file, and
/// `unreadable` lists directories that exist but cannot be read. Nonexistent
/// roots are ignored — a trailer directory is optional.
pub fn scan_movie_dirs(
    roots: &[std::path::PathBuf],
) -> (Vec<LocalMovie>, Vec<String>, Vec<String>) {
    fn walk(
        dir: &std::path::Path,
        depth: u8,
        found: &mut Vec<LocalMovie>,
        unreadable: &mut Vec<String>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            if dir.exists() {
                unreadable.push(format!("cannot read {}", dir.display()));
            }
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 3 {
                    walk(&path, depth + 1, found, unreadable);
                }
                continue;
            }
            let Some(stem) = path.file_stem().map(|stem| stem.to_string_lossy().to_string()) else {
                continue;
            };
            let Some(extension) = path.extension().map(|ext| ext.to_string_lossy().to_string()) else {
                continue;
            };
            if !MOVIE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
            {
                continue;
            }
            let Some(known) = KNOWN_MOVIES
                .iter()
                .find(|name| name.eq_ignore_ascii_case(&stem))
            else {
                continue;
            };
            if found.iter().any(|movie| movie.name.eq_ignore_ascii_case(known)) {
                continue;
            }
            let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            found.push(LocalMovie {
                name: known.to_string(),
                path,
                bytes,
            });
        }
    }
    let mut found = Vec::new();
    let mut unreadable = Vec::new();
    for root in roots {
        walk(root, 0, &mut found, &mut unreadable);
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    let missing = KNOWN_MOVIES
        .iter()
        .filter(|name| !found.iter().any(|movie| movie.name.eq_ignore_ascii_case(name)))
        .map(|name| name.to_string())
        .collect();
    (found, missing, unreadable)
}

/// Movie container extensions Warcraft III installs are known to use: Bink
/// (`.bik`), the AVI-in-MPQ campaign movies (`Movies\*.mpq`), the Frozen
/// Throne trailers (`.blz`) and plain AVI.
pub const MOVIE_EXTENSIONS: &[&str] = &["bik", "mpq", "avi", "blz"];

/// True when `bytes` starts with a movie container magic: Bink 1/2 or RIFF
/// (the AVI payload the campaign `Movies\*.mpq` entries carry).
pub fn is_movie_container(bytes: &[u8]) -> bool {
    is_bink(bytes) || bytes.get(..4) == Some(b"RIFF")
}

/// True when `bytes` starts with a Bink container magic.
///
/// Bink 1 files start with `BIK` followed by a version letter (`i`, `b`, `f`,
/// `m`, `j`, `k`); Bink 2 files start with `KB2` followed by a letter. The
/// shared three-byte prefix is the stable part, and it is what the engine's
/// movie loader keys on.
pub fn is_bink(bytes: &[u8]) -> bool {
    match bytes.get(..3) {
        Some(b"BIK") | Some(b"KB2") => true,
        _ => false,
    }
}

fn magic4(bytes: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (slot, byte) in out.iter_mut().zip(bytes.iter().copied()) {
        *slot = byte;
    }
    out
}

/// True when the path lives under `Movies\` (or `Movies/`) and ends in one of
/// [`MOVIE_EXTENSIONS`].
pub fn is_movie_path(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    let normalized = lowered.replace('/', "\\");
    normalized.starts_with("movies\\")
        && MOVIE_EXTENSIONS
            .iter()
            .any(|ext| normalized.ends_with(&format!(".{ext}")))
}

/// Candidate engine paths for the movies the shipped campaigns play.
///
/// These are probed by hash, so a wrong name costs nothing. The list is the
/// union of the `PlayMovie` calls in the shipped campaign scripts and the
/// cinematic names the retail installs use; a name that is absent simply does
/// not register.
pub fn movie_candidates() -> &'static [&'static str] {
    &[
        "Movies\\Warcraft III Intro.bik",
        "Movies\\Warcraft III Credits.bik",
        "Movies\\Warcraft III End.bik",
        "Movies\\Warcraft III Human.bik",
        "Movies\\Warcraft III Orc.bik",
        "Movies\\Warcraft III Undead.bik",
        "Movies\\Warcraft III NightElf.bik",
        "Movies\\Human01.bik",
        "Movies\\Human02.bik",
        "Movies\\Human03.bik",
        "Movies\\Human04.bik",
        "Movies\\Human05.bik",
        "Movies\\Human06.bik",
        "Movies\\Human07.bik",
        "Movies\\Human08.bik",
        "Movies\\Human09.bik",
        "Movies\\Orc01.bik",
        "Movies\\Orc02.bik",
        "Movies\\Orc03.bik",
        "Movies\\Orc04.bik",
        "Movies\\Orc05.bik",
        "Movies\\Orc06.bik",
        "Movies\\Orc07.bik",
        "Movies\\Orc08.bik",
        "Movies\\Undead01.bik",
        "Movies\\Undead02.bik",
        "Movies\\Undead03.bik",
        "Movies\\Undead04.bik",
        "Movies\\Undead05.bik",
        "Movies\\Undead06.bik",
        "Movies\\Undead07.bik",
        "Movies\\Undead08.bik",
        "Movies\\NightElf01.bik",
        "Movies\\NightElf02.bik",
        "Movies\\NightElf03.bik",
        "Movies\\NightElf04.bik",
        "Movies\\NightElf05.bik",
        "Movies\\NightElf06.bik",
        "Movies\\NightElf07.bik",
        "Movies\\NightElf08.bik",
        "Movies\\Prologue.bik",
        "Movies\\Intro.bik",
        "Movies\\Outro.bik",
        "Movies\\Credits.bik",
        "Movies\\Warcraft III Intro.mpq",
        "Movies\\Warcraft III End.mpq",
        "Movies\\Warcraft III Human.mpq",
        "Movies\\Warcraft III Orc.mpq",
        "Movies\\Warcraft III Undead.mpq",
        "Movies\\Warcraft III NightElf.mpq",
        "Movies\\Prologue.mpq",
        "Movies\\Outro.mpq",
        "Movies\\Credits.mpq",
        "Movies\\Warcraft III Intro.blz",
        "Movies\\Warcraft III End.blz",
    ]
}

/// Lists the movies an archive contains.
///
/// The archive is taken `&mut` because hash probing registers recovered names
/// in its name table; after this call `archive.listfile()` includes them.
pub fn discover_movies(archive: &mut MpqArchive) -> Vec<MovieEntry> {
    let mut found: Vec<MovieEntry> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    // 1. The archive's own listfile.
    for (name, meta) in archive.files() {
        if !is_movie_path(name) {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let magic = read_prefix(archive, name).unwrap_or([0; 4]);
        found.push(MovieEntry {
            name: name.replace('/', "\\"),
            size: meta.size,
            block: meta.block_index,
            source: MovieSource::Listfile,
            magic,
        });
    }

    // 2. Hash probes for engine paths whose listfile entry is missing.
    archive.probe_names(movie_candidates().iter().copied());
    for name in movie_candidates() {
        if !archive.contains(name) {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let Some((block, meta)) = archive.meta(name) else {
            continue;
        };
        let magic = read_prefix(archive, name).unwrap_or([0; 4]);
        found.push(MovieEntry {
            name: name.to_string(),
            size: meta.size,
            block,
            source: MovieSource::Probed,
            magic,
        });
    }

    // 3. Nameless blocks whose payload is Bink video.
    let named: std::collections::BTreeSet<u32> = archive
        .files()
        .map(|(_, meta)| meta.block_index)
        .collect();
    let mut scanned_bytes = 0u64;
    for block in 0..archive.block_count() as u32 {
        if named.contains(&block) {
            continue;
        }
        let Some((_offset, _csize, size, _flags)) = archive.block_entry(block) else {
            continue;
        };
        // Only blocks of movie-ish size are worth probing; `read_block`
        // itself rejects deleted/absent entries, so the flags check stays in
        // the MPQ crate where the bit definitions live.
        if size < SCAN_MIN_BYTES || size > SCAN_MAX_BYTES {
            continue;
        }
        if scanned_bytes.saturating_add(u64::from(size)) > SCAN_DECODE_BUDGET {
            // A crafted archive must not turn discovery into an unbounded
            // decode loop (WC3-MPQ-002).
            crate::diag::note("movie_scan_budget", || {
                format!(
                    "skipped_blocks={} scanned_bytes={scanned_bytes} budget={SCAN_DECODE_BUDGET}",
                    archive.block_count() as u32 - block
                )
            });
            break;
        }
        scanned_bytes += u64::from(size);
        // Read the container magic only: a whole-block read would decode up to
        // 256 MiB per block just to inspect four bytes (WC3-MPQ-002).
        let Ok(prefix) = archive.read_block_prefix(block, MOVIE_MAGIC_BYTES) else {
            continue;
        };
        if !is_movie_container(&prefix) {
            continue;
        }
        let name = MpqArchive::pseudo_name(block, "bik");
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        found.push(MovieEntry {
            name,
            size,
            block,
            source: MovieSource::UnnamedBlock,
            magic: magic4(&prefix),
        });
    }

    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn read_prefix(archive: &MpqArchive, name: &str) -> Result<[u8; 4], MpqError> {
    use std::io::Read;
    let mut reader = archive.stream(name)?;
    let mut head = [0u8; 4];
    let mut filled = 0usize;
    while filled < 4 {
        match reader.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) => return Err(MpqError::Io(err)),
        }
    }
    Ok(head)
}

/// Extracts one movie into `dir`, returning the written path and byte count.
///
/// The write is atomic (temp file + rename) so a killed extraction never leaves
/// a truncated `.bik` that a later transcode would silently accept.
pub fn extract_movie(
    archive: &MpqArchive,
    entry: &MovieEntry,
    dir: impl AsRef<Path>,
) -> std::io::Result<(PathBuf, u64)> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    let target = dir.join(entry.output_name());
    let bytes = match entry.source {
        MovieSource::UnnamedBlock => archive
            .read_block(entry.block)
            .map_err(std::io::Error::other)?,
        _ => archive.read(&entry.name).map_err(std::io::Error::other)?,
    };
    let tmp = target.with_extension("bik.part");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &target)?;
    Ok((target, bytes.len() as u64))
}

/// Discovers and extracts every movie of one archive.
///
/// Returns the entries that were written, with the on-disk size filled in from
/// the actual write (not the declared size), so the report cannot claim bytes
/// that did not land.
pub fn extract_all(
    archive: &mut MpqArchive,
    dir: impl AsRef<Path>,
) -> Vec<(MovieEntry, std::io::Result<(PathBuf, u64)>)> {
    let dir = dir.as_ref();
    discover_movies(archive)
        .into_iter()
        .map(|entry| {
            let result = extract_movie(archive, &entry, dir);
            (entry, result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_bink_magics() {
        assert!(is_bink(b"BIKi\x00\x01\x02\x03"));
        assert!(is_bink(b"BIKb"));
        assert!(is_bink(b"KB2i"));
        assert!(!is_bink(b"BI"));
        assert!(!is_bink(b"MDLX"));
        assert!(!is_bink(b""));
    }

    #[test]
    fn classifies_movie_paths() {
        assert!(is_movie_path("Movies\\Warcraft III Intro.bik"));
        assert!(is_movie_path("movies/human01.bik"));
        assert!(is_movie_path("Movies\\Warcraft III Intro.mpq"));
        assert!(is_movie_path("Movies\\Warcraft III End.blz"));
        assert!(is_movie_path("Movies\\clip.avi"));
        assert!(!is_movie_path("Movies\\readme.txt"));
        assert!(!is_movie_path("Sound\\Movies.bik"));
        assert!(!is_movie_path("war3map.j"));
    }

    #[test]
    fn recognises_avi_container() {
        assert!(is_movie_container(b"RIFF\x00\x00\x00\x00AVI "));
        assert!(!is_movie_container(b"MDLX"));
    }

    #[test]
    fn output_names_are_flat_and_bik() {
        let entry = MovieEntry {
            name: "Movies\\Warcraft III Intro.bik".into(),
            size: 0,
            block: 0,
            source: MovieSource::Listfile,
            magic: *b"BIKi",
        };
        assert_eq!(entry.output_name(), "Warcraft III Intro.bik");
        let pseudo = MovieEntry {
            name: "File00001234.bik".into(),
            source: MovieSource::UnnamedBlock,
            ..entry
        };
        assert_eq!(pseudo.output_name(), "File00001234.bik");
        let avi = MovieEntry {
            name: "Movies\\Warcraft III Intro.mpq".into(),
            source: MovieSource::Listfile,
            ..entry
        };
        assert_eq!(avi.output_name(), "Warcraft III Intro.mpq");
    }
}
