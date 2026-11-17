//! IMA ADPCM decompressor used by WC3 MPQ WAV sectors (codecs 0x40/0x41/0x80/0x81).
//! Ported from War3Net `AdpcmCompression.cs` (StormLib `adpcm.cpp`).
//!
//! The sector payload is untrusted archive data, so the header is validated
//! before it is used: the channel count must be 1 or 2, the header must be long
//! enough for the initial sample of every channel, and the shift byte must be
//! inside the range the format can produce. A malformed header is reported as
//! [`MpqError::Corrupt`] instead of yielding an empty or partially decoded
//! buffer.

use crate::MpqError;

const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14,
    16, 17, 19, 21, 23, 25, 28, 31,
    34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143,
    157, 173, 190, 209, 230, 253, 279, 307,
    337, 371, 408, 449, 494, 544, 598, 658,
    724, 796, 876, 963, 1060, 1166, 1282, 1411,
    1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024,
    3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484,
    7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899,
    15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

const INDEX_TABLE: [i32; 32] = [
    -1, 0, -1, 4, -1, 2, -1, 6,
    -1, 1, -1, 5, -1, 3, -1, 7,
    -1, 1, -1, 5, -1, 3, -1, 7,
    -1, 2, -1, 4, -1, 6, -1, 8,
];

/// Channel counts the format defines (mono/stereo sectors).
pub const MAX_CHANNELS: usize = 2;

/// Largest shift the format can produce.
///
/// The header stores the shift as one byte, but the step table tops out at
/// 32767 (15 significant bits), so a shift of 16 or more makes every delta
/// zero: the encoder never emits it, and `step >> shift` for `shift >= 32`
/// is a shift-overflow panic in a debug build (process abort in release).
/// Every legal value is therefore `0..=15`.
pub const MAX_SHIFT: u8 = 15;

/// Minimum header length for `channels`: a skipped byte, the shift byte, then
/// one little-endian `i16` initial sample per channel.
pub const fn header_len(channels: usize) -> usize {
    2 + 2 * channels
}

/// Decode a Blizzard/IMA ADPCM sector into interleaved PCM i16 LE.
///
/// `data` starts with the ADPCM header (a skipped byte, the shift byte, then
/// one initial little-endian i16 per channel). Output is bounded by the input:
/// at most one `i16` per payload byte plus the header samples, so a decoded
/// sector can never exceed `2 * data.len() + header_len(channels)` bytes.
///
/// Returns [`MpqError::Corrupt`] for an unusable channel count, a truncated
/// header, or a shift outside [`MAX_SHIFT`].
pub fn decompress(data: &[u8], channels: usize) -> Result<Vec<u8>, MpqError> {
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(MpqError::Corrupt("ADPCM channel count must be 1 or 2"));
    }
    let header = header_len(channels);
    if data.len() < header {
        return Err(MpqError::Corrupt("ADPCM header truncated"));
    }
    let shift = data[1];
    if shift > MAX_SHIFT {
        return Err(MpqError::Corrupt("ADPCM shift out of range"));
    }
    let shift = shift as u32;
    let mut out = Vec::with_capacity(data.len().saturating_mul(2));
    let mut step_index = [0x2c_i32; 2];
    let mut predicted = [0i32; 2];
    let mut position = 2usize;
    for channel in 0..channels {
        let sample = i16::from_le_bytes([data[position], data[position + 1]]);
        position += 2;
        predicted[channel] = sample as i32;
        out.extend_from_slice(&sample.to_le_bytes());
    }
    let mut channel = channels - 1;
    while position < data.len() {
        let value = data[position];
        position += 1;
        if channels == 2 {
            channel = 1 - channel;
        }
        if value & 0x80 != 0 {
            match value & 0x7f {
                0 => {
                    if step_index[channel] != 0 {
                        step_index[channel] -= 1;
                    }
                    out.extend_from_slice(&(predicted[channel] as i16).to_le_bytes());
                }
                1 => {
                    step_index[channel] += 8;
                    if step_index[channel] > 0x58 {
                        step_index[channel] = 0x58;
                    }
                    if channels == 2 {
                        channel = 1 - channel;
                    }
                }
                2 => {}
                _ => {
                    step_index[channel] -= 8;
                    if step_index[channel] < 0 {
                        step_index[channel] = 0;
                    }
                    if channels == 2 {
                        channel = 1 - channel;
                    }
                }
            }
        } else {
            let step = STEP_TABLE[step_index[channel] as usize];
            let mut delta = step >> shift;
            if value & 0x01 != 0 { delta += step; }
            if value & 0x02 != 0 { delta += step >> 1; }
            if value & 0x04 != 0 { delta += step >> 2; }
            if value & 0x08 != 0 { delta += step >> 3; }
            if value & 0x10 != 0 { delta += step >> 4; }
            if value & 0x20 != 0 { delta += step >> 5; }
            let mut sample = predicted[channel];
            if value & 0x40 != 0 {
                sample -= delta;
                if sample <= i16::MIN as i32 {
                    sample = i16::MIN as i32;
                }
            } else {
                sample += delta;
                if sample >= i16::MAX as i32 {
                    sample = i16::MAX as i32;
                }
            }
            predicted[channel] = sample;
            out.extend_from_slice(&(sample as i16).to_le_bytes());
            step_index[channel] += INDEX_TABLE[(value & 0x1f) as usize];
            step_index[channel] = step_index[channel].clamp(0, 0x58);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Header for `channels` with the given shift and zero initial samples,
    /// followed by `payload` bytes.
    fn sector(channels: usize, shift: u8, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![0u8; header_len(channels)];
        data[1] = shift;
        data.extend_from_slice(payload);
        data
    }

    #[test]
    fn every_legal_shift_decodes() {
        for channels in [1usize, 2] {
            for shift in 0..=MAX_SHIFT {
                let data = sector(channels, shift, &[0x11, 0x42, 0x7f, 0x80, 0x00, 0x3c]);
                let out = decompress(&data, channels).unwrap_or_else(|err| {
                    panic!("channels={channels} shift={shift} rejected: {err}")
                });
                // One header sample per channel, then one sample per payload byte.
                let samples = channels + 6;
                assert_eq!(out.len(), samples * 2, "channels={channels} shift={shift}");
                assert_eq!(out.len() % 2, 0);
            }
        }
        // The maximum legal shift is accepted and produces the declared shape.
        let max = sector(1, MAX_SHIFT, &[0x11, 0x42]);
        assert_eq!(decompress(&max, 1).unwrap().len(), 2 * (1 + 2));
    }

    #[test]
    fn out_of_range_shift_is_rejected() {
        for shift in [16u8, 31, 32, 255] {
            let data = sector(1, shift, &[0x11, 0x42]);
            assert!(
                matches!(decompress(&data, 1), Err(MpqError::Corrupt(_))),
                "shift={shift} must be rejected"
            );
        }
    }

    #[test]
    fn empty_and_short_headers_are_rejected() {
        for data in [&[][..], &[0u8][..], &[0u8, 0][..], &[0u8, 0, 0][..]] {
            assert!(
                matches!(decompress(data, 1), Err(MpqError::Corrupt(_))),
                "{} bytes must not decode",
                data.len()
            );
        }
        // Stereo needs the second initial sample: 4 bytes are still short.
        assert!(matches!(
            decompress(&[0u8, 0, 0, 0], 2),
            Err(MpqError::Corrupt(_))
        ));
        assert!(decompress(&[0u8; 6], 2).is_ok());
    }

    #[test]
    fn illegal_channel_counts_are_rejected() {
        for channels in [0usize, 3, 8] {
            assert!(matches!(
                decompress(&[0u8; 32], channels),
                Err(MpqError::Corrupt(_))
            ));
        }
    }

    #[test]
    fn mono_and_stereo_shapes_differ() {
        let payload = [0x11u8, 0x22, 0x33, 0x44];
        let mono = decompress(&sector(1, 0, &payload), 1).unwrap();
        let stereo = decompress(&sector(2, 0, &payload), 2).unwrap();
        // One i16 per payload byte, plus one header sample per channel.
        assert_eq!(mono.len(), 2 + 2 * payload.len());
        assert_eq!(stereo.len(), 4 + 2 * payload.len());
    }

    #[test]
    fn output_is_bounded_by_the_input() {
        // Worst case for the decoder: every payload byte contributes one
        // sample. Nothing here may allocate more than 2 * len + 4.
        let data = sector(2, 0, &[0x00; 8192]);
        let out = decompress(&data, 2).unwrap();
        assert!(out.len() <= 2 * data.len() + header_len(2));
    }
}
