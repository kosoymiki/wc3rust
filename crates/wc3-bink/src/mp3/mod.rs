//! MPEG-1 Layer III parser: frame header, side information and the bit
//! reservoir.
//!
//! Scope: MPEG-1 (ISO/IEC 11172-3) Layer III, 32/44.1/48 kHz, mono or stereo.
//! MPEG-2 LSF and Layers I/II are reported as [`BinkError::Unsupported`]
//! instead of being decoded wrongly; the WC3 cinematics are MPEG-1 Layer III
//! 44.1 kHz stereo (`auds` WAVE tag `0x0055`).

use crate::error::BinkError;

pub mod huffman;
pub mod synth;
pub mod tables;

/// A parsed 4-byte MPEG audio frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub mpeg1: bool,
    pub layer: u8,
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub padding: usize,
    /// 0 = stereo, 1 = joint stereo, 2 = dual channel, 3 = mono.
    pub mode: u8,
    pub mode_ext: u8,
    pub crc: bool,
}

impl Header {
    pub fn parse(h: &[u8]) -> Option<Self> {
        if h.len() < 4 || h[0] != 0xff || (h[1] & 0xe0) != 0xe0 {
            return None;
        }
        let mpeg1 = h[1] & 0x18 == 0x18;
        let version_bits = (h[1] >> 3) & 3;
        let layer_bits = (h[1] >> 1) & 3;
        if layer_bits == 0 || version_bits == 1 {
            return None; // reserved
        }
        let layer = 4 - layer_bits;
        let rate_idx = ((h[2] >> 2) & 3) as usize;
        let br_idx = (h[2] >> 4) as usize;
        if rate_idx == 3 || br_idx == 15 {
            return None;
        }
        // MPEG-1 bitrates (kb/s) for layers III, II, I; MPEG-2/2.5 only has
        // LSF layers I-III at half the rates, which we do not decode.
        const V1L3: [u32; 15] = [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ];
        const V1L2: [u32; 15] = [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
        ];
        const V1L1: [u32; 15] = [
            0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
        ];
        let table = match layer {
            3 => &V1L3,
            2 => &V1L2,
            _ => &V1L1,
        };
        let mut bitrate_kbps = table[br_idx];
        if !mpeg1 {
            bitrate_kbps /= 2;
        }
        let base_rate = [44100u32, 48000, 32000][rate_idx];
        let sample_rate = match version_bits {
            3 => base_rate,
            2 => base_rate / 2,
            _ => base_rate / 4,
        };
        Some(Self {
            mpeg1,
            layer,
            bitrate_kbps,
            sample_rate,
            padding: usize::from(h[2] & 2 != 0),
            mode: h[3] >> 6,
            mode_ext: (h[3] >> 4) & 3,
            crc: h[1] & 1 == 0,
        })
    }

    pub fn channels(&self) -> usize {
        if self.mode == 3 {
            1
        } else {
            2
        }
    }

    pub fn samples_per_frame(&self) -> usize {
        match self.layer {
            1 => 384,
            2 => 1152,
            _ => 1152,
        }
    }

    pub fn frame_bytes(&self) -> usize {
        if self.bitrate_kbps == 0 {
            return 0;
        }
        let slot = if self.layer == 1 { 4 } else { 1 };
        (self.samples_per_frame() / 8 * self.bitrate_kbps as usize * 1000 / self.sample_rate as usize)
            * slot
            + self.padding
    }

    pub fn is_ms_stereo(&self) -> bool {
        self.mode == 1 && self.mode_ext & 2 != 0
    }

    pub fn is_intensity_stereo(&self) -> bool {
        self.mode == 1 && self.mode_ext & 1 != 0
    }
}

/// Per-granule/channel side information (MPEG-1 Layer III).
#[derive(Debug, Clone, Copy)]
pub struct GranuleInfo {
    pub part_23_length: u32,
    pub big_values: u32,
    pub global_gain: u8,
    pub scalefac_compress: u8,
    pub block_type: u8,
    pub mixed_block_flag: bool,
    pub table_select: [u8; 3],
    pub region_count: [u8; 3],
    pub subblock_gain: [u8; 3],
    pub preflag: bool,
    pub scalefac_scale: bool,
    pub count1_table: bool,
    pub scfsi: u8,
    /// Set for the derived `long`/`short`/`mixed` band table selector.
    pub band_kind: BandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    Long,
    Short,
    Mixed,
}

/// MSB-first bit reader with an explicit bit limit, mirroring the reservoir
/// boundary semantics of the reference decoder.
pub struct Bits<'a> {
    data: &'a [u8],
    pub pos: usize,
    pub limit: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            limit: data.len() * 8,
        }
    }

    /// Next `n` bits (n <= 32), zero-filled past the limit.
    #[inline]
    pub fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let mut out: u32 = 0;
        for i in 0..n as usize {
            let bit = self.pos + i;
            let b = if bit < self.limit {
                (self.data[bit >> 3] >> (7 - (bit & 7))) & 1
            } else {
                0
            };
            out = (out << 1) | u32::from(b);
        }
        out
    }

    #[inline]
    pub fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    #[inline]
    pub fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.skip(n);
        v
    }

    pub fn over_limit(&self) -> bool {
        self.pos > self.limit
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

/// Read MPEG-1 Layer III side information. Returns `main_data_begin`.
pub fn read_side_info(
    br: &mut Bits<'_>,
    gr: &mut [GranuleInfo; 4],
    hdr: &Header,
) -> Result<usize, BinkError> {
    if !hdr.mpeg1 || hdr.layer != 3 {
        return Err(BinkError::Unsupported("mp3: only MPEG-1 Layer III"));
    }
    let nch = hdr.channels();
    let main_data_begin = br.get(9) as usize;
    // private_bits: 5 for mono, 3 for stereo.
    br.skip(if nch == 1 { 5 } else { 3 });
    // scfsi: 4 bits per channel, granule 1 only.
    let mut scfsi = [0u8; 2];
    for ch in 0..nch {
        scfsi[ch] = br.get(4) as u8;
    }
    let mut part_23_sum = 0u32;
    for grp in 0..2 {
        for ch in 0..nch {
            let g = &mut gr[grp * nch + ch];
            g.part_23_length = br.get(12);
            part_23_sum += g.part_23_length;
            g.big_values = br.get(9);
            if g.big_values > 288 {
                return Err(BinkError::Corrupt("mp3: big_values > 288"));
            }
            g.global_gain = br.get(8) as u8;
            g.scalefac_compress = br.get(4) as u8;
            g.band_kind = BandKind::Long;
            g.scfsi = if grp == 1 { scfsi[ch] } else { 0 };
            if br.get(1) == 1 {
                g.block_type = br.get(2) as u8;
                if g.block_type == 0 {
                    return Err(BinkError::Corrupt("mp3: window switching with block_type 0"));
                }
                g.mixed_block_flag = br.get(1) == 1;
                g.region_count = [7, 255, 255];
                if g.block_type == 2 {
                    if g.mixed_block_flag {
                        g.band_kind = BandKind::Mixed;
                    } else {
                        g.region_count[0] = 8;
                        g.band_kind = BandKind::Short;
                    }
                } else if g.mixed_block_flag {
                    return Err(BinkError::Corrupt("mp3: mixed block on a non-short block"));
                }
                let tables = br.get(10) << 5;
                for i in 0..3 {
                    g.subblock_gain[i] = br.get(3) as u8;
                }
                g.table_select = [
                    (tables >> 10) as u8,
                    ((tables >> 5) & 31) as u8,
                    (tables & 31) as u8,
                ];
            } else {
                g.block_type = 0;
                g.mixed_block_flag = false;
                let tables = br.get(15);
                g.region_count = [br.get(4) as u8, br.get(3) as u8, 255];
                g.subblock_gain = [0; 3];
                g.table_select = [
                    (tables >> 10) as u8,
                    ((tables >> 5) & 31) as u8,
                    (tables & 31) as u8,
                ];
            }
            g.preflag = br.get(1) == 1;
            g.scalefac_scale = br.get(1) == 1;
            g.count1_table = br.get(1) == 1;
        }
    }
    if part_23_sum as usize + br.pos > br.limit + main_data_begin * 8 {
        return Err(BinkError::Corrupt("mp3: side info exceeds the bit reservoir"));
    }
    Ok(main_data_begin)
}


mod granule;

pub use granule::process_granule;

/// Frames of reservoir history kept between MPEG audio frames.
const MAX_BITRESERVOIR_BYTES: usize = 511;
/// Scratch capacity for one frame's payload plus the reservoir.
const MAX_MAINDATA_BYTES: usize = MAX_BITRESERVOIR_BYTES + 2304;
/// MPEG-1 Layer III granules per frame.
const GRANULES: usize = 2;

fn empty_granule() -> GranuleInfo {
    GranuleInfo {
        part_23_length: 0,
        big_values: 0,
        global_gain: 0,
        scalefac_compress: 0,
        block_type: 0,
        mixed_block_flag: false,
        table_select: [0; 3],
        region_count: [0; 3],
        subblock_gain: [0; 3],
        preflag: false,
        scalefac_scale: false,
        count1_table: false,
        scfsi: 0,
        band_kind: BandKind::Long,
    }
}

/// Streaming MPEG-1 Layer III decoder.
///
/// Feed it one elementary-stream frame at a time; it keeps the bit reservoir,
/// the overlap buffers and the polyphase history across calls, so granule 1 of
/// a frame can reference main data that arrived with an earlier frame.
pub struct Mp3Decoder {
    /// Previous frame header (kept for diagnostics/resync).
    header: [u8; 4],
    reservoir: Vec<u8>,
    maindata: Vec<u8>,
    maindata_len: usize,
    gr_info: [GranuleInfo; 4],
    grbuf: [f32; 1152],
    scf: [f32; 40],
    syn: [f32; 2112],
    ist_pos: [[u8; 39]; 2],
    overlap: [[f32; 288]; 2],
    qmf_state: [f32; 960],
    /// Output samples produced by the frame decoded last.
    pub frame_samples: usize,
    /// Count of frames decoded so far (diagnostics).
    pub frames_decoded: usize,
    pub sample_rate: u32,
    pub channels: usize,
}

impl Default for Mp3Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Mp3Decoder {
    pub fn new() -> Self {
        Self {
            header: [0; 4],
            reservoir: Vec::with_capacity(MAX_BITRESERVOIR_BYTES),
            maindata: vec![0u8; MAX_MAINDATA_BYTES],
            maindata_len: 0,
            gr_info: [empty_granule(); 4],
            grbuf: [0.0; 1152],
            scf: [0.0; 40],
            syn: [0.0; 2112],
            ist_pos: [[0; 39]; 2],
            overlap: [[0.0; 288]; 2],
            qmf_state: [0.0; 960],
            frame_samples: 0,
            frames_decoded: 0,
            sample_rate: 0,
            channels: 0,
        }
    }

    /// Drop all history (bit reservoir, overlap, QMF state).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Decode one MPEG audio frame from the front of `data`.
    ///
    /// Returns the number of bytes consumed. `out` receives interleaved i16
    /// PCM (1152 samples per channel for MPEG-1 Layer III). A frame whose
    /// side info needs reservoir data that has not arrived yet decodes to
    /// silence (that is what the reference decoder does too).
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<i16>) -> Result<usize, BinkError> {
        let (offset, frame_bytes) = self.find_frame(data)?;
        let hdr = Header::parse(&data[offset..offset + 4])
            .ok_or(BinkError::Corrupt("mp3: header vanished while searching"))?;
        if !hdr.mpeg1 {
            return Err(BinkError::Unsupported("mp3: only MPEG-1 is decoded"));
        }
        if hdr.layer != 3 {
            return Err(BinkError::Unsupported("mp3: only Layer III is decoded"));
        }
        self.header.copy_from_slice(&data[offset..offset + 4]);
        self.sample_rate = hdr.sample_rate;
        self.channels = hdr.channels();

        let nch = hdr.channels();
        let mut bs = Bits::new(&data[offset + 4..offset + frame_bytes]);
        if hdr.crc {
            bs.get(16);
        }
        let main_data_begin = read_side_info(&mut bs, &mut self.gr_info, &hdr)?;
        self.ist_pos = [[0; 39]; 2];

        let have_reservoir = self.restore_reservoir(&bs, main_data_begin);
        if std::env::var_os("H2_MP3_TRACE").is_some() {
            eprintln!(
                "frame hdr={:02x?} mdb={main_data_begin} reserv={} have={have_reservoir} side_bits={} g0=(len={} bv={} gain={} sfc={})",
                &data[offset..offset + 4],
                self.reservoir.len(),
                bs.pos,
                self.gr_info[0].part_23_length,
                self.gr_info[0].big_values,
                self.gr_info[0].global_gain,
                self.gr_info[0].scalefac_compress
            );
        }
        let maindata_len = self.maindata.len();
        let mut pcm = vec![0i16; 576 * nch * GRANULES];
        let mut produced = 0usize;
        if have_reservoir {
            // The main-data buffer is borrowed by the bit reader while the
            // granule decoders run; take it out of `self` for the duration.
            let maindata = std::mem::take(&mut self.maindata);
            let mut gbs = Bits::new(&maindata[..maindata_len]);
            let result = (|| -> Result<usize, BinkError> {
                let mut produced = 0usize;
                for igr in 0..GRANULES {
                    self.grbuf = [0.0; 1152];
                    self.decode_granule(&mut gbs, igr, &hdr)?;
                    let at = igr * 576 * nch;
                    synth::synth_granule(
                        &mut self.qmf_state,
                        &mut self.grbuf,
                        18,
                        nch,
                        &mut pcm[at..],
                        &mut self.syn,
                    );
                    produced += 576 * nch;
                }
                Ok(produced)
            })();
            self.maindata = maindata;
            produced = result?;
        }
        self.frame_samples = produced;
        self.frames_decoded += 1;
        if std::env::var("H2_MP3_DUMP_FRAME").ok().and_then(|v| v.parse().ok())
            == Some(self.frames_decoded.wrapping_sub(1))
        {
            eprintln!(
                "dump frame {} mdb={main_data_begin} scf={:?}",
                self.frames_decoded - 1,
                &self.scf[..8]
            );
        }
        out.extend_from_slice(&pcm[..produced]);
        self.save_reservoir(&bs);
        Ok(offset + frame_bytes)
    }

    /// (granule, channel) decode into `self.grbuf`.
    fn decode_granule(
        &mut self,
        bs: &mut Bits<'_>,
        igr: usize,
        hdr: &Header,
    ) -> Result<(), BinkError> {
        let nch = hdr.channels();
        let base = igr * nch;
        let mut infos = [empty_granule(); 2];
        infos[..nch].copy_from_slice(&self.gr_info[base..base + nch]);

        for ch in 0..nch {
            let g = infos[ch];
            let (sfbtab, _n_long, _n_short) = huffman::bands(hdr, &g);
            let limit = bs.pos + g.part_23_length as usize;
            huffman::decode_scalefactors(hdr, &g, bs, &mut self.ist_pos[ch], &mut self.scf)?;
            huffman::decode_huffman(
                &mut self.grbuf[ch * 576..(ch + 1) * 576],
                bs,
                &g,
                &self.scf,
                sfbtab,
                limit,
            )?;
            if std::env::var("H2_MP3_DUMP_FRAME").ok().and_then(|v| v.parse().ok())
                == Some(self.frames_decoded)
            {
                let nz = self.grbuf[ch * 576..(ch + 1) * 576]
                    .iter()
                    .filter(|v| **v != 0.0)
                    .count();
                eprintln!(
                    "  igr{igr} ch{ch} scf{:?} huff{:?} nz={nz} pos={} limit={limit}",
                    &self.scf[..8],
                    &self.grbuf[ch * 576..ch * 576 + 8],
                    bs.pos
                );
            }
        }

        if std::env::var_os("H2_MP3_TRACE2").is_some() {
            let nz = self.grbuf.iter().filter(|v| **v != 0.0).count();
            let mx = self.grbuf.iter().fold(0f32, |a, v| a.max(v.abs()));
            let scf_max = self.scf.iter().fold(0f32, |a, v| a.max(*v));
            eprintln!("  gr igr ch={nch} nonzero={nz}/1152 max={mx:.3} scf0={:.4} scfmax={scf_max:.4}", self.scf[0]);
        }
        if nch == 2 {
            let (left, right) = self.grbuf.split_at_mut(576);
            if hdr.is_intensity_stereo() {
                let (sfbtab, _nl, _ns) = huffman::bands(hdr, &infos[0]);
                let mut ist = self.ist_pos[1];
                granule::intensity_stereo(left, right, &mut ist, &infos[0], hdr, sfbtab);
                self.ist_pos[1] = ist;
            } else if hdr.is_ms_stereo() {
                granule::midside_stereo(left, right, 576);
            }
        }

        if std::env::var("H2_MP3_DUMP_FRAME").ok().and_then(|v| v.parse().ok())
            == Some(self.frames_decoded)
        {
            let nz0 = self.grbuf[..576].iter().filter(|v| **v != 0.0).count();
            let nz1 = self.grbuf[576..].iter().filter(|v| **v != 0.0).count();
            eprintln!(
                "  pre-imdct grbuf[0..8]={:?} nz={nz0}/{nz1}",
                &self.grbuf[..8]
            );
        }
        let (sfbtab, _nl, _ns) = huffman::bands(hdr, &infos[0]);
        let mut scratch = [0f32; 576];
        let overlap = &mut self.overlap;
        let grbuf = &mut self.grbuf;
        granule::process_granule(grbuf, overlap, &mut scratch, &infos, nch, hdr, sfbtab);
        if std::env::var("H2_MP3_DUMP_FRAME").ok().and_then(|v| v.parse().ok())
            == Some(self.frames_decoded)
        {
            eprintln!("  post-imdct grbuf[0..8]={:?}", &self.grbuf[..8]);
        }
        Ok(())
    }

    fn restore_reservoir(&mut self, bs: &Bits<'_>, main_data_begin: usize) -> bool {
        let frame_bytes = (bs.limit - bs.pos) / 8;
        let bytes_have = self.reservoir.len().min(main_data_begin);
        let start = self.reservoir.len().saturating_sub(main_data_begin);
        self.maindata[..bytes_have].copy_from_slice(&self.reservoir[start..start + bytes_have]);
        let from = bs.pos / 8;
        let body = &bs.data()[from..from + frame_bytes];
        self.maindata[bytes_have..bytes_have + frame_bytes].copy_from_slice(body);
        self.maindata_len = bytes_have + frame_bytes;
        self.reservoir.len() >= main_data_begin
    }

    fn save_reservoir(&mut self, bs: &Bits<'_>) {
        let mut pos = (bs.pos + 7) / 8;
        let available = self.maindata_len;
        let mut remains = available.saturating_sub(pos);
        if remains > MAX_BITRESERVOIR_BYTES {
            // Keep the *last* 511 bytes before the next frame's payload.
            pos += remains - MAX_BITRESERVOIR_BYTES;
            remains = MAX_BITRESERVOIR_BYTES;
        }
        self.reservoir.clear();
        if remains > 0 {
            self.reservoir
                .extend_from_slice(&self.maindata[pos..pos + remains]);
        }
    }

    fn find_frame(&self, data: &[u8]) -> Result<(usize, usize), BinkError> {
        if data.len() < 4 {
            return Err(BinkError::Corrupt("mp3: short buffer"));
        }
        let mut i = 0usize;
        while i + 4 <= data.len() {
            if let Some(hdr) = Header::parse(&data[i..]) {
                if hdr.bitrate_kbps == 0 {
                    return Err(BinkError::Unsupported("mp3: free-format frames"));
                }
                let frame_bytes = hdr.frame_bytes();
                if frame_bytes >= 4 && i + frame_bytes <= data.len() {
                    let next = i + frame_bytes;
                    let consistent = next + 4 > data.len()
                        || Header::parse(&data[next..])
                            .map(|h| {
                                h.bitrate_kbps == hdr.bitrate_kbps
                                    && h.sample_rate == hdr.sample_rate
                            })
                            .unwrap_or(false);
                    if consistent {
                        return Ok((i, frame_bytes));
                    }
                }
            }
            i += 1;
        }
        Err(BinkError::Corrupt("mp3: no frame sync"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parsing_matches_the_streamed_movies() {
        // First frame header of HumanEd.mpq audio: MPEG-1 Layer III, 192 kb/s,
        // 44.1 kHz, joint stereo.
        let h = [0xffu8, 0xfb, 0xb0, 0x64];
        let hdr = Header::parse(&h).expect("valid header");
        assert!(hdr.mpeg1);
        assert_eq!(hdr.layer, 3);
        assert_eq!(hdr.bitrate_kbps, 192);
        assert_eq!(hdr.sample_rate, 44100);
        assert_eq!(hdr.channels(), 2);
        assert!(hdr.is_ms_stereo() || hdr.is_intensity_stereo());
        assert_eq!(hdr.frame_bytes(), 626);
    }

    #[test]
    fn reserved_fields_are_rejected() {
        assert!(Header::parse(&[0xff, 0xfb, 0xf0, 0x00]).is_none()); // bitrate 15
        assert!(Header::parse(&[0xff, 0xfb, 0xbc, 0x00]).is_none()); // rate idx 3
        assert!(Header::parse(&[0xff, 0xe9, 0xb0, 0x00]).is_none()); // layer 0
        assert!(Header::parse(&[0xff, 0x00, 0x00, 0x00]).is_none());
    }

    #[test]
    fn bit_reader_is_msb_first_and_zero_pads() {
        let mut br = Bits::new(&[0b1010_1100, 0b0011_0000]);
        assert_eq!(br.get(3), 0b101);
        assert_eq!(br.get(5), 0b01100);
        assert_eq!(br.get(4), 0b0011);
        assert_eq!(br.get(8), 0); // past the limit
        br.pos = 0;
        br.limit = 5;
        // Only five bits are inside the limit; the rest is zero-filled.
        assert_eq!(br.peek(8), 0b1010_1000);
        br.pos = 4;
        assert_eq!(br.peek(8), 0b1000_0000);
    }
}
