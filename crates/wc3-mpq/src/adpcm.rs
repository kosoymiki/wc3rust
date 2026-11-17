//! IMA ADPCM decompressor used by WC3 MPQ WAV sectors (codecs 0x40/0x41/0x80/0x81).
//! Ported from War3Net `AdpcmCompression.cs` (StormLib `adpcm.cpp`).

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

/// `data` starts with the ADPCM header (a skipped byte, the shift byte, then
/// one initial little-endian i16 per channel). Output is interleaved PCM i16 LE.
pub fn decompress(data: &[u8], channels: usize) -> Vec<u8> {
    let mut out = Vec::new();
    if data.len() < 2 {
        return out;
    }
    let shift = data[1] as u32;
    let mut step_index = [0x2c_i32; 2];
    let mut predicted = [0i32; 2];
    let mut position = 2usize;
    for channel in 0..channels.min(2) {
        if position + 2 > data.len() {
            return out;
        }
        let sample = i16::from_le_bytes([data[position], data[position + 1]]);
        position += 2;
        predicted[channel] = sample as i32;
        out.extend_from_slice(&sample.to_le_bytes());
    }
    let mut channel = channels.saturating_sub(1).min(1);
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
    out
}
