//! Per-tileset ambience: the retail day/night ambient score.
//!
//! The chain is shipped data end to end:
//!
//! ```text
//! war3map.w3e tileset byte ('L')
//!   -> UI/WorldEditData.txt [DayAmbience]/[NightAmbience]   L=LordaeronSummerDay
//!   -> UI/SoundInfo/MIDISounds.slk                           Sound\Ambient\LordaeronSummer\LordaeronSummerDay.mid
//!                                                                 + LordaeronSummer.dls
//!   -> DLS instrument bank + MIDI score -> PCM loop
//! ```
//!
//! Retail plays the `.mid` through the `.dls` sample bank that ships next to
//! it; this module implements that path (RIFF/DLS Level 2 instrument regions
//! plus a Standard MIDI File sequencer) so a tileset is not silent. The
//! rendered loop is plain [`Sound`] PCM, so the mixer's 3D/ducking/statistics
//! path applies unchanged.

use std::collections::BTreeMap;

use crate::soundinfo::parse_slk;
use crate::{AudioError, Sound};

/// Retail table of ambient MIDI themes and their DLS banks.
pub const MIDI_SOUNDS_TABLE: &str = "UI\\SoundInfo\\MIDISounds.slk";
/// Retail tileset -> ambient theme map.
pub const WORLD_EDIT_DATA: &str = "UI\\WorldEditData.txt";

/// Sample rate the sequencer renders at (the banks are 11.025/22.05 kHz).
pub const AMBIENT_SAMPLE_RATE: u32 = 22_050;
/// Upper bound on a rendered loop (retail themes are ~2 minutes).
pub const MAX_AMBIENT_SECONDS: f32 = 180.0;

/// One `MIDISounds.slk` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientTheme {
    pub name: String,
    /// `DirectoryBase` (`Sound\Ambient\LordaeronSummer`).
    pub directory: String,
    /// `MIDIFileName` (`LordaeronSummerDay.mid`).
    pub midi: String,
    /// `DLSFileName` (`LordaeronSummer.dls`).
    pub dls: String,
    /// `Volume` column normalised to 0..=1 (the SLK stores 0..127).
    pub volume: f32,
    pub priority: u32,
}

impl AmbientTheme {
    /// Archive path of the MIDI score.
    pub fn midi_path(&self) -> String {
        join_path(&self.directory, &self.midi)
    }

    /// Archive path of the instrument bank.
    pub fn dls_path(&self) -> String {
        join_path(&self.directory, &self.dls)
    }
}

/// `MIDISounds.slk` themes plus the `WorldEditData.txt` tileset mapping.
#[derive(Debug, Clone, Default)]
pub struct AmbientTable {
    themes: BTreeMap<String, AmbientTheme>,
    day: BTreeMap<char, String>,
    night: BTreeMap<char, String>,
}

impl AmbientTable {
    /// Parse `UI/SoundInfo/MIDISounds.slk`.
    pub fn parse_midi_sounds(bytes: &[u8]) -> Result<Self, AudioError> {
        let rows = parse_slk(bytes)?;
        let mut table = Self::default();
        for row in rows.iter().skip(1) {
            let Some(name) = row.get(&1).map(|value| value.trim()).filter(|v| !v.is_empty()) else {
                continue;
            };
            if name.eq_ignore_ascii_case("SoundLabel") {
                continue;
            }
            let directory = row.get(&2).cloned().unwrap_or_default();
            let midi = row.get(&3).cloned().unwrap_or_default();
            if midi.is_empty() || !midi.to_ascii_lowercase().ends_with(".mid") {
                continue;
            }
            let dls = row.get(&4).cloned().unwrap_or_default();
            let volume = row
                .get(&5)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .map(|value| (value / 127.0).clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let priority = row
                .get(&6)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .unwrap_or(0.0) as u32;
            let theme = AmbientTheme {
                name: name.to_string(),
                directory,
                midi,
                dls,
                volume,
                priority,
            };
            table.themes.insert(name.to_ascii_lowercase(), theme);
        }
        Ok(table)
    }

    /// Parse the `[DayAmbience]` / `[NightAmbience]` sections of
    /// `UI/WorldEditData.txt` (`L=LordaeronSummerDay`). Returns the rows added.
    pub fn parse_world_edit_data(&mut self, bytes: &[u8]) -> usize {
        let text = String::from_utf8_lossy(bytes);
        let mut section = String::new();
        let mut added = 0usize;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line.starts_with('[') {
                section = line
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_ascii_lowercase();
                continue;
            }
            let night = match section.as_str() {
                "dayambience" => false,
                "nightambience" => true,
                _ => continue,
            };
            let Some((letter, name)) = line.split_once('=') else {
                continue;
            };
            let Some(letter) = letter.trim().chars().next() else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let map = if night { &mut self.night } else { &mut self.day };
            map.insert(letter.to_ascii_uppercase(), name.to_string());
            added += 1;
        }
        added
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }

    pub fn theme(&self, name: &str) -> Option<&AmbientTheme> {
        self.themes.get(&name.to_ascii_lowercase())
    }

    /// Theme for a W3E tileset letter (`'L'`) and time of day.
    pub fn theme_for_tileset(&self, tileset: char, night: bool) -> Option<&AmbientTheme> {
        let letter = tileset.to_ascii_uppercase();
        let name = if night {
            self.night.get(&letter)
        } else {
            self.day.get(&letter)
        }?;
        self.theme(name)
    }

    /// Ambient theme name for a tileset letter, day or night.
    pub fn theme_name_for_tileset(&self, tileset: char, night: bool) -> Option<&str> {
        let letter = tileset.to_ascii_uppercase();
        let name = if night {
            self.night.get(&letter)
        } else {
            self.day.get(&letter)
        }?;
        Some(name.as_str())
    }

    /// Tileset letters with a day theme, sorted.
    pub fn day_letters(&self) -> Vec<char> {
        self.day.keys().copied().collect()
    }

    pub fn night_letters(&self) -> Vec<char> {
        self.night.keys().copied().collect()
    }

    pub fn themes(&self) -> impl Iterator<Item = &AmbientTheme> {
        self.themes.values()
    }
}

fn join_path(directory: &str, file: &str) -> String {
    if file.contains('\\') || file.contains('/') {
        return file.replace('/', "\\");
    }
    let directory = directory.replace('/', "\\");
    if directory.is_empty() {
        file.to_string()
    } else if directory.ends_with('\\') {
        format!("{}{}", directory, file)
    } else {
        format!("{}\\{}", directory, file)
    }
}

// ---------------------------------------------------------------------------
// RIFF/DLS instrument bank
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct DlsSample {
    pub sample_rate: u32,
    pub channels: u16,
    /// Mono PCM in `[-1, 1]`.
    pub samples: Vec<f32>,
    /// `wsmp.usUnityNote` (MIDI note the sample plays at its own rate).
    pub unity_note: u8,
    /// `wsmp.sFineTune` in cents.
    pub fine_tune: i16,
    /// Loop window in sample frames, when the bank declares one.
    pub loop_start: u32,
    pub loop_end: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DlsRegion {
    pub program: u8,
    pub bank: u8,
    pub key_low: u8,
    pub key_high: u8,
    pub velocity_low: u8,
    pub velocity_high: u8,
    /// Index into [`DlsBank::samples`].
    pub wave: usize,
    pub unity_note: u8,
    pub fine_tune: i16,
}

/// DLS Level 2 bank: PCM samples plus the program/key regions that use them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DlsBank {
    pub samples: Vec<DlsSample>,
    pub regions: Vec<DlsRegion>,
}

impl DlsBank {
    pub fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"DLS " {
            return Err(AudioError::BadMagic);
        }
        let riff_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let end = (8 + riff_len).min(bytes.len());
        let mut ptbl: Option<(usize, usize)> = None;
        let mut wvpl: Option<(usize, usize)> = None;
        let mut lins: Option<(usize, usize)> = None;
        for chunk in Chunks::new(bytes, 12, end) {
            match &chunk.id {
                b"ptbl" => ptbl = Some((chunk.data_start, chunk.data_end)),
                b"LIST" => match chunk.list_type() {
                    Some(tag) if tag == *b"wvpl" => {
                        wvpl = Some((chunk.children_start(), chunk.data_end))
                    }
                    Some(tag) if tag == *b"lins" => {
                        lins = Some((chunk.children_start(), chunk.data_end))
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let Some((lins_start, lins_end)) = lins else {
            return Err(AudioError::Corrupt("DLS has no instrument list"));
        };
        let Some((wvpl_start, wvpl_end)) = wvpl else {
            return Err(AudioError::Corrupt("DLS has no wave pool"));
        };
        let mut bank = Self::default();
        // Wave pool: ptbl offsets are relative to the byte after the `wvpl`
        // fourcc (verified against the shipped tables: offset 0 is the first
        // `LIST wave`).
        if let Some((ptbl_start, ptbl_end)) = ptbl {
            let data = &bytes[ptbl_start..ptbl_end];
            let offsets = ptbl_offsets(data);
            for offset in offsets {
                let start = wvpl_start.saturating_add(offset as usize);
                if start + 8 > wvpl_end {
                    bank.samples.push(DlsSample::empty());
                    continue;
                }
                bank.samples
                    .push(parse_wave(bytes, start, wvpl_end).unwrap_or_else(DlsSample::empty));
            }
        }
        // Instrument list: each `LIST ins ` holds `insh` + `LIST lrgn`.
        for instrument in Chunks::new(bytes, lins_start, lins_end) {
            if instrument.id != *b"LIST" || instrument.list_type() != Some(*b"ins ") {
                continue;
            }
            let mut program = 0u8;
            let mut bank_msb = 0u8;
            let mut regions: Vec<DlsRegion> = Vec::new();
            for part in Chunks::new(bytes, instrument.children_start(), instrument.data_end) {
                if part.id == *b"insh" {
                    let data = part.data(bytes);
                    if data.len() >= 12 {
                        bank_msb = ((read_u32(data, 4) >> 8) & 0x7f) as u8;
                        program = (read_u32(data, 8) & 0x7f) as u8;
                    }
                } else if part.id == *b"LIST" && part.list_type() == Some(*b"lrgn") {
                    for region in Chunks::new(bytes, part.children_start(), part.data_end) {
                        if region.list_type() != Some(*b"rgn ") {
                            continue;
                        }
                        let Some(parsed) = parse_region(bytes, &region) else {
                            continue;
                        };
                        regions.push(DlsRegion {
                            program,
                            bank: bank_msb,
                            ..parsed
                        });
                    }
                }
            }
            bank.regions.extend(regions);
        }
        if bank.samples.is_empty() || bank.regions.is_empty() {
            return Err(AudioError::Corrupt("DLS bank has no usable instrument"));
        }
        Ok(bank)
    }

    /// First region covering `program`/`key` (DLS key splits are disjoint).
    pub fn region_for(&self, program: u8, key: u8) -> Option<&DlsRegion> {
        self.regions.iter().find(|region| {
            region.program == program
                && key >= region.key_low
                && key <= region.key_high
                && region.wave < self.samples.len()
        })
    }
}

impl DlsSample {
    fn empty() -> Self {
        Self {
            sample_rate: AMBIENT_SAMPLE_RATE,
            channels: 1,
            samples: Vec::new(),
            unity_note: 60,
            fine_tune: 0,
            loop_start: 0,
            loop_end: 0,
        }
    }
}

/// `ptbl` offset table.
///
/// The shipped banks write `u32 8` (the entry size of the cue table header)
/// followed by the offset count and the offsets; the fallback keeps a bank that
/// writes the bare count working.
fn ptbl_offsets(data: &[u8]) -> Vec<u32> {
    if data.len() < 8 {
        return Vec::new();
    }
    let declared = read_u32(data, 4) as usize;
    let (count, start) = if declared > 0 && 8 + declared * 4 <= data.len() {
        (declared, 8)
    } else {
        (read_u32(data, 0) as usize, 4)
    };
    (0..count)
        .filter_map(|index| data.get(start + index * 4..start + index * 4 + 4))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

fn parse_wave(bytes: &[u8], list_start: usize, limit: usize) -> Option<DlsSample> {
    let header = Chunks::new(bytes, list_start, limit).next()?;
    if header.id != *b"LIST" || header.list_type() != Some(*b"wave") {
        return None;
    }
    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut pcm: Option<&[u8]> = None;
    let mut wsmp: Option<(u8, i16, u32, u32)> = None;
    for part in Chunks::new(bytes, header.children_start(), header.data_end) {
        match &part.id {
            b"fmt " => {
                let data = part.data(bytes);
                if data.len() >= 16 {
                    format = Some((
                        read_u16(data, 0),
                        read_u16(data, 2),
                        read_u32(data, 4),
                        read_u16(data, 14),
                    ));
                }
            }
            b"data" => pcm = Some(part.data(bytes)),
            b"wsmp" => {
                let data = part.data(bytes);
                if data.len() >= 20 {
                    let unity = read_u16(data, 4) as u8;
                    let fine = read_u16(data, 6) as i16;
                    let loops = read_u32(data, 16);
                    let mut loop_start = 0u32;
                    let mut loop_end = 0u32;
                    // DLS WSMP loop entry: cbSize, type, start, length (16 bytes).
                    if loops > 0 && data.len() >= 20 + 16 {
                        let entry = &data[20..];
                        loop_start = read_u32(entry, 8);
                        let length = read_u32(entry, 12);
                        loop_end = loop_start.saturating_add(length);
                    }
                    wsmp = Some((unity, fine, loop_start, loop_end));
                }
            }
            _ => {}
        }
    }
    let (tag, channels, sample_rate, bits) = format?;
    let data = pcm?;
    if tag != 1 || channels == 0 || channels > 2 {
        return None;
    }
    let mut samples = Vec::with_capacity(data.len() / (bits as usize / 8).max(1) / channels as usize);
    match bits {
        8 => {
            for frame in data.chunks_exact(channels as usize) {
                let sum: f32 = frame.iter().map(|byte| (*byte as f32 - 128.0) / 128.0).sum();
                samples.push(sum / channels as f32);
            }
        }
        16 => {
            for frame in data.chunks_exact(2 * channels as usize) {
                let mut sum = 0.0f32;
                for channel in 0..channels as usize {
                    let value = i16::from_le_bytes(
                        frame[channel * 2..channel * 2 + 2].try_into().unwrap(),
                    );
                    sum += value as f32 / 32768.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        _ => return None,
    }
    let (unity_note, fine_tune, loop_start, loop_end) = wsmp.unwrap_or((60, 0, 0, 0));
    let loop_end = loop_end.min(samples.len() as u32);
    let looping = loop_end > loop_start + 8;
    Some(DlsSample {
        sample_rate: sample_rate.max(1),
        channels,
        samples,
        unity_note,
        fine_tune,
        loop_start: if looping { loop_start } else { 0 },
        loop_end: if looping { loop_end } else { 0 },
    })
}

fn parse_region(bytes: &[u8], region: &Chunk<'_>) -> Option<DlsRegion> {
    let mut keys = None;
    let mut wsmp: Option<(u8, i16)> = None;
    let mut wave = None;
    for part in Chunks::new(bytes, region.children_start(), region.data_end) {
        match &part.id {
            b"rgnh" => {
                let data = part.data(bytes);
                if data.len() >= 12 {
                    keys = Some((
                        read_u16(data, 0) as u8,
                        read_u16(data, 2) as u8,
                        read_u16(data, 4) as u8,
                        read_u16(data, 6) as u8,
                    ));
                }
            }
            b"wsmp" => {
                let data = part.data(bytes);
                if data.len() >= 8 {
                    wsmp = Some((read_u16(data, 4) as u8, read_u16(data, 6) as i16));
                }
            }
            b"wlnk" => {
                let data = part.data(bytes);
                if data.len() >= 12 {
                    wave = Some(read_u32(data, 8) as usize);
                }
            }
            _ => {}
        }
    }
    let (key_low, key_high, velocity_low, velocity_high) = keys?;
    let (unity_note, fine_tune) = wsmp?;
    Some(DlsRegion {
        program: 0,
        bank: 0,
        key_low,
        key_high,
        velocity_low,
        velocity_high,
        wave: wave?,
        unity_note,
        fine_tune,
    })
}

// ---------------------------------------------------------------------------
// Standard MIDI File
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiNote {
    pub start_tick: u32,
    pub end_tick: u32,
    pub program: u8,
    pub key: u8,
    pub velocity: u8,
}

#[derive(Debug, Clone, Default)]
pub struct MidiSong {
    pub division: u16,
    pub notes: Vec<MidiNote>,
    pub end_tick: u32,
    /// `(absolute tick, microseconds per quarter note)` from meta 0x51.
    pub tempo_events: Vec<(u32, u32)>,
}

impl MidiSong {
    pub fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        if bytes.len() < 14 || &bytes[..4] != b"MThd" {
            return Err(AudioError::BadMagic);
        }
        let header_len = read_be_u32(bytes, 4) as usize;
        let division = read_be_u16(bytes, 12);
        if division == 0 || division & 0x8000 != 0 {
            return Err(AudioError::Unsupported("SMPTE MIDI timing"));
        }
        let mut pos = 8 + header_len;
        let mut song = MidiSong {
            division,
            ..Default::default()
        };
        let mut tempo_events: Vec<(u32, u32)> = Vec::new();
        while pos + 8 <= bytes.len() {
            if &bytes[pos..pos + 4] != b"MTrk" {
                break;
            }
            let track_len = read_be_u32(bytes, pos + 4) as usize;
            let track_start = pos + 8;
            let track_end = (track_start + track_len).min(bytes.len());
            if track_start > track_end {
                break;
            }
            parse_track(bytes, track_start, track_end, &mut song, &mut tempo_events)?;
            pos = track_start + track_len;
            if track_len % 2 == 1 {
                pos += 1;
            }
        }
        if song.notes.is_empty() {
            return Err(AudioError::Corrupt("MIDI has no notes"));
        }
        song.end_tick = song.notes.iter().map(|note| note.end_tick).max().unwrap_or(0);
        song.tempo_events = tempo_events;
        Ok(song)
    }

    /// Length in seconds (tempo map applied).
    pub fn seconds(&self) -> f32 {
        let mut seconds = 0.0f64;
        let mut last_tick = 0u32;
        let mut tempo = 500_000u32;
        for (tick, value) in &self.tempo_events {
            if *tick > self.end_tick {
                break;
            }
            let tick = (*tick).max(last_tick);
            seconds += ticks_to_seconds(tick - last_tick, tempo, self.division);
            last_tick = tick;
            tempo = *value;
        }
        seconds += ticks_to_seconds(self.end_tick.saturating_sub(last_tick), tempo, self.division);
        seconds as f32
    }

    /// Tick -> seconds using the tempo map.
    pub fn tick_seconds(&self, tick: u32) -> f32 {
        let mut seconds = 0.0f64;
        let mut last_tick = 0u32;
        let mut tempo = 500_000u32;
        for (at, value) in &self.tempo_events {
            if *at >= tick {
                break;
            }
            let at = (*at).max(last_tick);
            seconds += ticks_to_seconds(at - last_tick, tempo, self.division);
            last_tick = at;
            tempo = *value;
        }
        seconds += ticks_to_seconds(tick.saturating_sub(last_tick), tempo, self.division);
        seconds as f32
    }
}

fn ticks_to_seconds(ticks: u32, tempo_us_per_quarter: u32, division: u16) -> f64 {
    if division == 0 {
        return 0.0;
    }
    ticks as f64 * tempo_us_per_quarter as f64 / (division as f64 * 1_000_000.0)
}

fn parse_track(
    bytes: &[u8],
    start: usize,
    end: usize,
    song: &mut MidiSong,
    tempos: &mut Vec<(u32, u32)>,
) -> Result<(), AudioError> {
    let mut pos = start;
    let mut tick = 0u32;
    let mut status: Option<u8> = None;
    let mut program = 0u8;
    let mut active: Vec<((u8, u8), (u32, u8, u8))> = Vec::new();
    while pos < end {
        let (delta, next) = read_vlq(bytes, pos, end)?;
        pos = next;
        tick = tick.saturating_add(delta);
        let byte = *bytes.get(pos).ok_or(AudioError::Corrupt("MIDI event truncated"))?;
        if byte & 0x80 != 0 {
            status = Some(byte);
            pos += 1;
        }
        let status = status.ok_or(AudioError::Corrupt("MIDI running status missing"))?;
        match status {
            0xff => {
                let kind = *bytes.get(pos).ok_or(AudioError::Corrupt("MIDI meta"))?;
                pos += 1;
                let (len, next) = read_vlq(bytes, pos, end)?;
                let len = len as usize;
                pos = next;
                let data_end = pos.checked_add(len).ok_or(AudioError::Corrupt("MIDI meta"))?;
                if data_end > end {
                    return Err(AudioError::Corrupt("MIDI meta truncated"));
                }
                if kind == 0x51 && len >= 3 {
                    let tempo = ((bytes[pos] as u32) << 16)
                        | ((bytes[pos + 1] as u32) << 8)
                        | bytes[pos + 2] as u32;
                    tempos.push((tick, tempo.max(1)));
                }
                pos = data_end;
            }
            0xf0 | 0xf7 => {
                let (len, next) = read_vlq(bytes, pos, end)?;
                pos = next + len as usize;
            }
            _ => {
                let high = status & 0xf0;
                let channel = status & 0x0f;
                let data_len = if high == 0xc0 || high == 0xd0 { 1 } else { 2 };
                if pos + data_len > end {
                    return Err(AudioError::Corrupt("MIDI event truncated"));
                }
                let a = bytes[pos];
                let b = if data_len == 2 { Some(bytes[pos + 1]) } else { None };
                pos += data_len;
                match high {
                    0x90 => {
                        let key = a & 0x7f;
                        let velocity = b.unwrap_or(0) & 0x7f;
                        if velocity == 0 {
                            close_note(&mut active, &mut song.notes, channel, key, tick);
                        } else {
                            active.retain(|((ch, k), _)| !(*ch == channel && *k == key));
                            active.push(((channel, key), (tick, velocity, program)));
                        }
                    }
                    0x80 => {
                        let key = a & 0x7f;
                        close_note(&mut active, &mut song.notes, channel, key, tick);
                    }
                    0xc0 => program = a & 0x7f,
                    _ => {}
                }
            }
        }
    }
    for ((channel, key), (start, velocity, program)) in active {
        let _ = (channel, key);
        song.notes.push(MidiNote {
            start_tick: start,
            end_tick: tick.max(start + 1),
            program,
            key,
            velocity,
        });
    }
    Ok(())
}

fn close_note(
    active: &mut Vec<((u8, u8), (u32, u8, u8))>,
    notes: &mut Vec<MidiNote>,
    channel: u8,
    key: u8,
    tick: u32,
) {
    if let Some(index) = active
        .iter()
        .position(|((ch, k), _)| *ch == channel && *k == key)
    {
        let (_, (start, velocity, program)) = active.remove(index);
        notes.push(MidiNote {
            start_tick: start,
            end_tick: tick.max(start + 1),
            program,
            key,
            velocity,
        });
    }
}

// ---------------------------------------------------------------------------
// Sequencer -> PCM
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AmbientRender {
    pub sound: Sound,
    pub notes: usize,
    pub seconds: f32,
    pub peak: f32,
}

/// Render a MIDI score through a DLS bank into a loopable mono PCM buffer.
///
/// The result is normalised to a 0.5 peak so the category/ducking gains stay in
/// control of the final level; a score that renders silence returns the buffer
/// with the measured peak so callers can report the data gap honestly.
pub fn render_ambient(
    midi: &MidiSong,
    bank: &DlsBank,
    max_seconds: f32,
) -> Option<AmbientRender> {
    let seconds = midi.seconds().min(max_seconds.max(1.0));
    let frames = (seconds * AMBIENT_SAMPLE_RATE as f32).ceil() as usize;
    if frames == 0 {
        return None;
    }
    let mut out = vec![0.0f32; frames];
    let out_rate = AMBIENT_SAMPLE_RATE as f64;
    let mut rendered = 0usize;
    let attack = (0.005 * out_rate) as usize;
    let release = (0.040 * out_rate) as usize;
    for note in &midi.notes {
        let start = midi.tick_seconds(note.start_tick);
        let end = midi.tick_seconds(note.end_tick);
        if start >= seconds {
            continue;
        }
        let Some(region) = bank.region_for(note.program, note.key) else {
            continue;
        };
        let Some(sample) = bank.samples.get(region.wave) else {
            continue;
        };
        if sample.samples.len() < 2 {
            continue;
        }
        let semitones = note.key as f32 - region.unity_note as f32
            + region.fine_tune as f32 / 100.0
            + sample.fine_tune as f32 / 100.0;
        let step = sample.sample_rate as f64 / out_rate * 2f64.powf(semitones as f64 / 12.0);
        if !step.is_finite() || step <= 0.0 {
            continue;
        }
        let first = (start * AMBIENT_SAMPLE_RATE as f32).floor().max(0.0) as usize;
        let last = ((end.min(seconds) * AMBIENT_SAMPLE_RATE as f32).ceil() as usize)
            .min(frames)
            .max(first);
        if last <= first {
            continue;
        }
        let gain = (note.velocity as f32 / 127.0).clamp(0.0, 1.0) * 0.5;
        let looping = sample.loop_end > sample.loop_start + 8;
        let loop_start = sample.loop_start as f64;
        let loop_len = (sample.loop_end - sample.loop_start) as f64;
        let mut phase = 0.0f64;
        for frame in first..last {
            let mut index = phase as usize;
            if looping && phase >= sample.loop_end as f64 {
                phase = loop_start + (phase - loop_start) % loop_len;
                index = phase as usize;
            }
            if index + 1 >= sample.samples.len() {
                if looping {
                    phase = loop_start;
                    index = loop_start as usize;
                } else {
                    break;
                }
            }
            let frac = phase - phase.floor();
            let a = sample.samples[index];
            let b = sample.samples.get(index + 1).copied().unwrap_or(a);
            let mut value = a + (b - a) * frac as f32;
            // Short attack/release ramps: the banks are raw sample loops.
            let into = frame - first;
            if into < attack {
                value *= into as f32 / attack as f32;
            }
            let left = last - frame;
            if left < release {
                value *= left as f32 / release as f32;
            }
            out[frame] += value * gain;
            phase += step;
            rendered += 1;
        }
    }
    let peak = out.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if peak > 1e-4 {
        let scale = 0.5 / peak;
        for sample in &mut out {
            *sample *= scale;
        }
    }
    Some(AmbientRender {
        sound: Sound {
            sample_rate: AMBIENT_SAMPLE_RATE,
            channels: 1,
            samples: out,
        },
        notes: rendered,
        seconds,
        peak,
    })
}

// ---------------------------------------------------------------------------
// RIFF helpers
// ---------------------------------------------------------------------------

struct Chunks<'a> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
}

#[derive(Debug)]
struct Chunk<'a> {
    id: [u8; 4],
    data_start: usize,
    data_end: usize,
    bytes: &'a [u8],
}

impl<'a> Chunk<'a> {
    fn data(&self, bytes: &'a [u8]) -> &'a [u8] {
        let _ = self.bytes;
        &bytes[self.data_start..self.data_end]
    }

    fn list_type(&self) -> Option<[u8; 4]> {
        if self.id != *b"LIST" && self.id != *b"RIFF" {
            return None;
        }
        let bytes = &self.bytes[self.data_start..self.data_end];
        bytes.get(..4).map(|slice| slice.try_into().unwrap())
    }

    fn children_start(&self) -> usize {
        self.data_start + 4
    }
}

impl<'a> Chunks<'a> {
    fn new(bytes: &'a [u8], start: usize, end: usize) -> Self {
        Self {
            bytes,
            pos: start,
            end: end.min(bytes.len()),
        }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos + 8 <= self.end {
            let id: [u8; 4] = self.bytes[self.pos..self.pos + 4].try_into().unwrap();
            let size = read_u32(self.bytes, self.pos + 4) as usize;
            let data_start = self.pos + 8;
            let Some(data_end) = data_start.checked_add(size) else {
                self.pos = self.end;
                return None;
            };
            if data_end > self.end {
                self.pos = self.end;
                return None;
            }
            let chunk = Chunk {
                id,
                data_start,
                data_end,
                bytes: self.bytes,
            };
            self.pos = data_end + (size & 1);
            return Some(chunk);
        }
        None
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    bytes
        .get(offset..offset + 2)
        .map(|slice| u16::from_le_bytes(slice.try_into().unwrap()))
        .unwrap_or(0)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes(slice.try_into().unwrap()))
        .unwrap_or(0)
}

fn read_be_u16(bytes: &[u8], offset: usize) -> u16 {
    bytes
        .get(offset..offset + 2)
        .map(|slice| u16::from_be_bytes(slice.try_into().unwrap()))
        .unwrap_or(0)
}

fn read_be_u32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_be_bytes(slice.try_into().unwrap()))
        .unwrap_or(0)
}

fn read_vlq(bytes: &[u8], start: usize, end: usize) -> Result<(u32, usize), AudioError> {
    let mut value = 0u32;
    let mut pos = start;
    for _ in 0..4 {
        let byte = *bytes
            .get(pos)
            .ok_or(AudioError::Corrupt("MIDI variable-length value truncated"))?;
        if pos >= end {
            return Err(AudioError::Corrupt("MIDI variable-length value truncated"));
        }
        pos += 1;
        value = (value << 7) | (byte & 0x7f) as u32;
        if byte & 0x80 == 0 {
            return Ok((value, pos));
        }
    }
    Err(AudioError::Corrupt("MIDI variable-length value too long"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ambient_dir() -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Sound/Ambient");
        path.is_dir().then_some(path)
    }

    fn table() -> Option<AmbientTable> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/UI/SoundInfo/MIDISounds.slk");
        let bytes = std::fs::read(path).ok()?;
        AmbientTable::parse_midi_sounds(&bytes).ok()
    }

    #[test]
    fn midi_sounds_table_parses_every_theme() {
        let Some(table) = table() else {
            return;
        };
        assert!(table.len() >= 20, "themes={}", table.len());
        let day = table.theme("LordaeronSummerDay").expect("LordaeronSummerDay");
        assert_eq!(day.midi, "LordaeronSummerDay.mid");
        assert_eq!(day.dls, "LordaeronSummer.dls");
        assert!(day.midi_path().starts_with("Sound\\Ambient\\"));
        assert!(day.volume > 0.5, "volume {}", day.volume);
    }

    #[test]
    fn world_edit_maps_every_tileset_letter() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/UI/WorldEditData.txt");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Some(mut table) = table() else {
            return;
        };
        let added = table.parse_world_edit_data(&bytes);
        assert!(added >= 24, "ambient rows: {added}");
        assert_eq!(table.theme_name_for_tileset('L', false), Some("LordaeronSummerDay"));
        assert_eq!(table.theme_name_for_tileset('L', true), Some("LordaeronSummerNight"));
        assert_eq!(table.theme_name_for_tileset('A', false), Some("AshenvaleDay"));
        assert_eq!(table.theme_name_for_tileset('C', false), Some("FelwoodDay"));
        assert_eq!(table.theme_name_for_tileset('Y', false), Some("CityScapeDay"));
        // Justice (W3E tileset 'L') resolves to the shipped LordaeronSummer clips.
        let summer = table
            .theme_for_tileset('L', false)
            .expect("LordaeronSummer theme");
        assert_eq!(summer.midi_path(), "Sound\\Ambient\\LordaeronSummer\\LordaeronSummerDay.mid");
        assert_eq!(summer.dls_path(), "Sound\\Ambient\\LordaeronSummer\\LordaeronSummer.dls");
        // Every letter WorldEditData names must resolve to a MIDISounds row.
        let mut unresolved = Vec::new();
        for (night, letters) in [(false, table.day_letters()), (true, table.night_letters())] {
            for letter in letters {
                if table.theme_for_tileset(letter, night).is_none() {
                    unresolved.push((letter, night));
                }
            }
        }
        assert!(unresolved.is_empty(), "unresolved tileset themes: {unresolved:?}");
    }

    #[test]
    fn every_midi_sounds_theme_has_its_mid_and_dls() {
        let Some(table) = table() else {
            return;
        };
        let install = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        let mut missing = Vec::new();
        for theme in table.themes() {
            for path in [theme.midi_path(), theme.dls_path()] {
                let file = install.join(path.replace('\\', "/"));
                if !file.is_file() {
                    missing.push(format!("{}: {path}", theme.name));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "unresolved ambient clips ({} themes): {missing:#?}",
            table.len()
        );
        println!("AMBIENT_CLIPS themes={} unresolved=0", table.len());
    }

    #[test]
    fn dls_bank_parses_the_shipped_lordaeron_summer_bank() {        let Some(root) = ambient_dir() else {
            return;
        };
        let bytes = std::fs::read(root.join("LordaeronSummer/LordaeronSummer.dls")).expect("dls");
        let bank = DlsBank::parse(&bytes).expect("parse dls");
        assert!(bank.samples.len() >= 8, "samples={}", bank.samples.len());
        assert!(bank.regions.len() >= 20, "regions={}", bank.regions.len());
        assert!(bank.regions.iter().any(|region| region.program == 15));
        let sample = bank.samples.iter().find(|s| !s.samples.is_empty()).expect("pcm");
        assert!(sample.sample_rate > 8000, "rate {}", sample.sample_rate);
        assert!(!sample.samples.is_empty());
    }

    #[test]
    fn midi_song_parses_the_lordaeron_summer_day_score() {
        let Some(root) = ambient_dir() else {
            return;
        };
        let bytes = std::fs::read(root.join("LordaeronSummer/LordaeronSummerDay.mid")).expect("mid");
        let song = MidiSong::parse(&bytes).expect("parse midi");
        assert!(song.notes.len() > 60, "notes={}", song.notes.len());
        assert!(song.seconds() > 60.0, "seconds {}", song.seconds());
        assert!(song.end_tick > 100_000, "end tick {}", song.end_tick);
        assert!(!song.tempo_events.is_empty(), "tempo changes: {}", song.tempo_events.len());
    }
}
