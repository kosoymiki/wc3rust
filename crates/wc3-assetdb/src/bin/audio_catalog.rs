//! Validate every sound in the archive and check the retail sound tables.
//!
//! Reports exact counts for:
//! * `.wav` — probe + full decode, codec histogram, durations, peaks, silent files
//! * `.mp3` — header/frame probe for all files, full decode for a bounded sample
//! * sound tables — every `UI/SoundInfo/*.slk` definition and whether every
//!   referenced file exists in the archive
//! * unit coverage — how many unit display names resolve to an ack sound
//! * music themes — `musictheme` rows from `UI/TriggerData.txt` and their files
//!
//! Usage: `audio_catalog [--mpq PATH] [--index PATH] [--out DIR] [--limit N]`

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use wc3_assetdb::audio_provider::load_sound_tables;
use wc3_assetdb::{AssetDb, Resolution};
use wc3_audio::mp3::{decode_mp3_bounded, probe_mp3};
use wc3_audio::wav::{decode_wav, probe_wav};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into());
    let mut index: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("analysis/agent-assets");
    let mut limit = usize::MAX;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mpq" => {
                i += 1;
                mpq = PathBuf::from(&args[i]);
            }
            "--index" => {
                i += 1;
                index = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            "--limit" => {
                i += 1;
                limit = args[i].parse()?;
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
        i += 1;
    }

    let (mut db, used_index) = match &index {
        Some(path) => AssetDb::open_mpq_best(&mpq, path)?,
        None => (AssetDb::open_mpq(&mpq)?, false),
    };
    println!(
        "provider files={} index={used_index}",
        db.file_count()
    );

    let files = db.files();
    let mut wav_rows = String::from("name\tcodec\tchannels\tsample_rate\tbits\tframes\tduration_ms\tpeak\tsilent\tok\terror\n");
    let mut mp3_rows = String::from("name\tlayer\tchannels\tsample_rate\tframes\tduration_ms\tbitrate_kbps\tvbr_tag\tok\terror\n");
    let mut wav_files = 0usize;
    let mut wav_ok = 0usize;
    let mut wav_fail = 0usize;
    let mut wav_bytes = 0u64;
    let mut wav_samples = 0u64;
    let mut wav_frames = 0u64;
    let mut wav_silent = 0usize;
    let mut wav_duration_ms = 0u64;
    let mut codecs: BTreeMap<String, usize> = BTreeMap::new();
    let mut rates: BTreeMap<u32, usize> = BTreeMap::new();
    let mut channels_hist: BTreeMap<u16, usize> = BTreeMap::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    let mut mp3_files = 0usize;
    let mut mp3_ok = 0usize;
    let mut mp3_fail = 0usize;
    let mut mp3_bytes = 0u64;
    let mut mp3_duration_ms = 0u64;
    let mut mp3_decoded = 0usize;
    let mut mp3_decode_mismatch = 0usize;

    let scan_start = std::time::Instant::now();
    let mut scanned = 0usize;
    for entry in &files {
        if scanned >= limit {
            break;
        }
        let lower = entry.name.to_ascii_lowercase();
        if lower.ends_with(".wav") {
            scanned += 1;
            wav_files += 1;
            wav_bytes += entry.size;
            let bytes = match db.read_uncached(&entry.name) {
                Ok(bytes) => bytes,
                Err(err) => {
                    wav_fail += 1;
                    failures.push((entry.name.clone(), err.to_string()));
                    let _ = writeln!(wav_rows, "{}\t-\t0\t0\t0\t0\t0\t0\tfalse\tfalse\t{}", entry.name, err);
                    continue;
                }
            };
            let info = match probe_wav(&bytes) {
                Ok(info) => info,
                Err(err) => {
                    wav_fail += 1;
                    failures.push((entry.name.clone(), err.to_string()));
                    let _ = writeln!(wav_rows, "{}\t-\t0\t0\t0\t0\t0\t0\tfalse\tfalse\t{}", entry.name, err);
                    continue;
                }
            };
            *codecs.entry(info.codec.name().to_string()).or_default() += 1;
            *rates.entry(info.sample_rate).or_default() += 1;
            *channels_hist.entry(info.channels).or_default() += 1;
            match decode_wav(&bytes) {
                Ok(sound) => {
                    wav_ok += 1;
                    wav_samples += sound.samples.len() as u64;
                    wav_frames += sound.frames() as u64;
                    wav_duration_ms += sound.duration_ms();
                    let peak = sound.peak();
                    if sound.is_silent() {
                        wav_silent += 1;
                    }
                    let _ = writeln!(
                        wav_rows,
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.4}\t{}\ttrue\t",
                        entry.name,
                        info.codec.name(),
                        info.channels,
                        info.sample_rate,
                        info.bits_per_sample,
                        sound.frames(),
                        sound.duration_ms(),
                        peak,
                        sound.is_silent()
                    );
                }
                Err(err) => {
                    wav_fail += 1;
                    failures.push((entry.name.clone(), err.to_string()));
                    let _ = writeln!(wav_rows, "{}\t{}\t0\t0\t0\t0\t0\t0\tfalse\tfalse\t{}", entry.name, info.codec.name(), err);
                }
            }
        } else if lower.ends_with(".mp3") {
            scanned += 1;
            mp3_files += 1;
            mp3_bytes += entry.size;
            let bytes = match db.read_uncached(&entry.name) {
                Ok(bytes) => bytes,
                Err(err) => {
                    mp3_fail += 1;
                    failures.push((entry.name.clone(), err.to_string()));
                    continue;
                }
            };
            match probe_mp3(&bytes) {
                Ok(info) => {
                    mp3_ok += 1;
                    mp3_duration_ms += info.duration_ms;
                    let _ = writeln!(
                        mp3_rows,
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\ttrue\t",
                        entry.name,
                        info.layer,
                        info.channels,
                        info.sample_rate,
                        info.frames,
                        info.duration_ms,
                        info.bitrate_avg_kbps,
                        info.has_vbr_tag
                    );
                    // Decode a bounded sample fully and compare durations.
                    if mp3_decoded < 40 && entry.size < 600_000 {
                        mp3_decoded += 1;
                        match decode_mp3_bounded(&bytes, 48_000 * 120) {
                            Ok((sound, truncated)) => {
                                let decoded_ms = sound.duration_ms();
                                let expect = if truncated {
                                    decoded_ms
                                } else {
                                    info.duration_ms
                                };
                                let delta = decoded_ms.abs_diff(expect);
                                if !truncated && delta > expect / 20 + 50 {
                                    mp3_decode_mismatch += 1;
                                    failures.push((
                                        entry.name.clone(),
                                        format!("duration probe {expect} ms vs decode {decoded_ms} ms"),
                                    ));
                                }
                            }
                            Err(err) => {
                                mp3_fail += 1;
                                failures.push((entry.name.clone(), err.to_string()));
                            }
                        }
                    }
                }
                Err(err) => {
                    mp3_fail += 1;
                    failures.push((entry.name.clone(), err.to_string()));
                    let _ = writeln!(mp3_rows, "{}\t-\t0\t0\t0\t0\t0\tfalse\tfalse\t{}", entry.name, err);
                }
            }
        }
    }
    let scan_secs = scan_start.elapsed().as_secs_f64();

    // ---- Sound tables and coverage -------------------------------------------------
    let table_start = std::time::Instant::now();
    let (sounds, units, tables) = load_sound_tables(&mut db);
    let table_secs = table_start.elapsed().as_secs_f64();
    let mut table_defs = 0usize;
    let mut table_sound_paths = 0usize;
    let mut sound_paths_missing = 0usize;
    let mut defs_playable = 0usize;
    let mut defs_unplayable = 0usize;
    let mut covering_units = 0usize;
    let mut units_with_ack = 0usize;
    let mut units_with_attack = 0usize;
    let mut units_playable = 0usize;
    let mut missing_by_top: BTreeMap<String, usize> = BTreeMap::new();
    let mut listfile_resolved = 0usize;
    let mut ambiguous = 0usize;
    for def in sounds.iter() {
        table_defs += 1;
        let mut any = false;
        for path in def.paths() {
            table_sound_paths += 1;
            match db.resolve_asset(&path) {
                Resolution::Exact { .. } => {
                    any = true;
                    continue;
                }
                Resolution::ListfileMatch { .. } => {
                    // Retail table path is stale; the listfile has the real one.
                    any = true;
                    listfile_resolved += 1;
                    continue;
                }
                Resolution::Ambiguous { .. } => {
                    ambiguous += 1;
                }
                Resolution::Absent { .. } => {}
            }
            sound_paths_missing += 1;
            let top = path
                .split('\\')
                .next()
                .unwrap_or("(root)")
                .to_string();
            *missing_by_top.entry(top).or_default() += 1;
            if failures.len() < 500 {
                failures.push((path, "referenced by sound table but absent from every mounted layer".into()));
            }
        }
        if any {
            defs_playable += 1;
        } else {
            defs_unplayable += 1;
        }
    }
    for (unit_id, name) in unit_iter(&units) {
        let _ = unit_id;
        covering_units += 1;
        if let Some(def) = sounds.unit_ack(&name) {
            units_with_ack += 1;
            if def.paths().iter().any(|path| db.contains(path)) {
                units_playable += 1;
            }
        }
        if sounds.unit_attack(&name).is_some() {
            units_with_attack += 1;
        }
    }

    // Music themes from TriggerData.txt
    let mut themes = 0usize;
    let mut themes_missing = 0usize;
    let mut theme_names: Vec<String> = Vec::new();
    if let Ok(bytes) = db.read_uncached("UI\\TriggerData.txt") {
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if !value.trim_start().starts_with("musictheme,") {
                continue;
            }
            let Some(start) = value.find('`') else { continue };
            let Some(end) = value[start + 1..].find('`') else { continue };
            let path = value[start + 1..start + 1 + end].replace("\\\\", "\\");
            themes += 1;
            theme_names.push(key.trim().to_string());
            if !db.contains(&path) {
                themes_missing += 1;
                failures.push((path.to_string(), "music theme file missing".into()));
            }
        }
    }

    // ---- Report -------------------------------------------------------------------
    std::fs::create_dir_all(&out_dir)?;
    std::fs::write(out_dir.join("audio-wav-catalog.tsv"), &wav_rows)?;
    std::fs::write(out_dir.join("audio-mp3-catalog.tsv"), &mp3_rows)?;

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"provider_files\": {},\n", db.file_count()));
    json.push_str(&format!("  \"index_used\": {used_index},\n"));
    json.push_str("  \"wav\": {\n");
    json.push_str(&format!("    \"files\": {wav_files},\n"));
    json.push_str(&format!("    \"decoded_ok\": {wav_ok},\n"));
    json.push_str(&format!("    \"failures\": {wav_fail},\n"));
    json.push_str(&format!("    \"source_bytes\": {wav_bytes},\n"));
    json.push_str(&format!("    \"decoded_samples\": {wav_samples},\n"));
    json.push_str(&format!("    \"decoded_frames\": {wav_frames},\n"));
    json.push_str(&format!("    \"total_duration_ms\": {wav_duration_ms},\n"));
    json.push_str(&format!("    \"silent_files\": {wav_silent},\n"));
    json.push_str("    \"codecs\": {");
    for (n, (codec, count)) in codecs.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        let _ = write!(json, "\"{codec}\": {count}");
    }
    json.push_str("},\n    \"sample_rates\": {");
    for (n, (rate, count)) in rates.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        let _ = write!(json, "\"{rate}\": {count}");
    }
    json.push_str("},\n    \"channels\": {");
    for (n, (ch, count)) in channels_hist.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        let _ = write!(json, "\"{ch}\": {count}");
    }
    json.push_str("}\n  },\n");
    json.push_str("  \"mp3\": {\n");
    json.push_str(&format!("    \"files\": {mp3_files},\n"));
    json.push_str(&format!("    \"probed_ok\": {mp3_ok},\n"));
    json.push_str(&format!("    \"probe_failures\": {mp3_fail},\n"));
    json.push_str(&format!("    \"source_bytes\": {mp3_bytes},\n"));
    json.push_str(&format!("    \"total_duration_ms\": {mp3_duration_ms},\n"));
    json.push_str(&format!("    \"fully_decoded_sample\": {mp3_decoded},\n"));
    json.push_str(&format!("    \"decode_duration_mismatches\": {mp3_decode_mismatch}\n"));
    json.push_str("  },\n");
    json.push_str("  \"sound_tables\": {\n");
    json.push_str(&format!("    \"tables_loaded\": {tables},\n"));
    json.push_str(&format!("    \"definitions\": {table_defs},\n"));
    json.push_str(&format!("    \"definitions_playable\": {defs_playable},\n"));
    json.push_str(&format!("    \"definitions_unplayable\": {defs_unplayable},\n"));
    json.push_str(&format!("    \"referenced_paths\": {table_sound_paths},\n"));
    json.push_str(&format!("    \"missing_paths\": {sound_paths_missing},\n"));
    json.push_str(&format!("    \"listfile_resolved_paths\": {listfile_resolved},\n"));
    json.push_str(&format!("    \"ambiguous_paths\": {ambiguous},\n"));
    json.push_str("    \"missing_by_top\": {");
    for (n, (top, count)) in missing_by_top.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        let _ = write!(json, "\"{}\": {}", top.replace('\\', "\\\\"), count);
    }
    json.push_str("}\n  },\n");
    json.push_str("  \"units\": {\n");
    json.push_str(&format!("    \"with_names\": {covering_units},\n"));
    json.push_str(&format!("    \"with_ack_sound\": {units_with_ack},\n"));
    json.push_str(&format!("    \"with_attack_sound\": {units_with_attack},\n"));
    json.push_str(&format!("    \"playable_from_this_archive\": {units_playable}\n"));
    json.push_str("  },\n");
    json.push_str(&format!("  \"music_themes\": {{\"declared\": {themes}, \"missing\": {themes_missing}}},\n"));
    json.push_str(&format!("  \"music_theme_names\": {},\n", theme_names.len()));
    json.push_str(&format!("  \"scan_seconds\": {scan_secs:.3},\n"));
    json.push_str(&format!("  \"table_seconds\": {table_secs:.3},\n"));
    json.push_str(&format!("  \"failures\": [\n"));
    for (n, (name, detail)) in failures.iter().take(500).enumerate() {
        let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"detail\": \"{}\"}}{}\n",
            escape(name),
            escape(detail),
            if n + 1 == failures.len().min(500) { "" } else { "," }
        ));
    }
    json.push_str("  ]\n}\n");
    std::fs::write(out_dir.join("audio-catalog.json"), &json)?;

    let mut md = String::new();
    md.push_str("# Audio catalog\n\n");
    let _ = writeln!(md, "Scanned {scanned} audio files in {scan_secs:.2} s (tables {table_secs:.2} s).\n");
    md.push_str("## WAV\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(md, "| files | {wav_files} |");
    let _ = writeln!(md, "| decoded OK | {wav_ok} |");
    let _ = writeln!(md, "| failures | {wav_fail} |");
    let _ = writeln!(md, "| source bytes | {wav_bytes} |");
    let _ = writeln!(md, "| decoded frames | {wav_frames} |");
    let _ = writeln!(md, "| total duration | {:.1} s |", wav_duration_ms as f64 / 1000.0);
    let _ = writeln!(md, "| silent files | {wav_silent} |");
    let _ = writeln!(md, "| codecs | {:?} |", codecs);
    let _ = writeln!(md, "| sample rates | {:?} |", rates);
    md.push_str("\n## MP3\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(md, "| files | {mp3_files} |");
    let _ = writeln!(md, "| probed OK | {mp3_ok} |");
    let _ = writeln!(md, "| probe failures | {mp3_fail} |");
    let _ = writeln!(md, "| source bytes | {mp3_bytes} |");
    let _ = writeln!(md, "| total duration | {:.1} s |", mp3_duration_ms as f64 / 1000.0);
    let _ = writeln!(md, "| fully decoded sample | {mp3_decoded} |");
    let _ = writeln!(md, "| duration mismatches | {mp3_decode_mismatch} |");
    md.push_str("\n## Sound tables\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(md, "| tables loaded | {tables} |");
    let _ = writeln!(md, "| definitions | {table_defs} |");
    let _ = writeln!(md, "| definitions playable from this archive | {defs_playable} |");
    let _ = writeln!(md, "| definitions needing expansion data | {defs_unplayable} |");
    let _ = writeln!(md, "| referenced paths | {table_sound_paths} |");
    let _ = writeln!(md, "| listfile-resolved paths (stale table dir) | {listfile_resolved} |");
    let _ = writeln!(md, "| ambiguous paths | {ambiguous} |");
    let _ = writeln!(md, "| absent paths | {sound_paths_missing} |");
    let _ = writeln!(md, "| missing by top-level dir | {missing_by_top:?} |");
    md.push_str("\n## Units and music\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(md, "| units with names | {covering_units} |");
    let _ = writeln!(md, "| units with ack sound | {units_with_ack} |");
    let _ = writeln!(md, "| units with attack sound | {units_with_attack} |");
    let _ = writeln!(md, "| units playable from this archive | {units_playable} |");
    let _ = writeln!(md, "| music themes | {themes} |");
    let _ = writeln!(md, "| themes missing files | {themes_missing} |");
    if !failures.is_empty() {
        md.push_str(&format!("\n## Failures ({}, first 30)\n\n", failures.len()));
        for (name, detail) in failures.iter().take(30) {
            let _ = writeln!(md, "- `{name}`: {detail}");
        }
    }
    std::fs::write(out_dir.join("audio-catalog.md"), &md)?;
    println!("{md}");
    println!("report_dir={}", out_dir.display());

    // Sound-table paths that are absent from War3.mpq are expansion (TFT)
    // assets, not decode failures; they are reported per top-level directory so
    // coverage is explicit instead of silently accepted.
    if wav_fail > 0 || mp3_fail > 0 || mp3_decode_mismatch > 0 {
        return Err(format!(
            "audio catalog failures: wav={wav_fail} mp3={mp3_fail} duration_mismatch={mp3_decode_mismatch}"
        )
        .into());
    }
    Ok(())
}

fn unit_iter(units: &wc3_audio::UnitNameMap) -> Vec<(String, String)> {
    // UnitNameMap does not expose its map; reconstruct via the known unit ids is
    // not possible, so use the public accessor for the ids we care about.
    let mut out = Vec::new();
    for id in units.ids() {
        if let Some(name) = units.name(&id) {
            out.push((id, name.to_string()));
        }
    }
    out
}
