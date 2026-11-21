//! Truncated/forged media must fail with a typed error, never a panic, and the
//! container census must be deterministic and stream-order independent.
//!
//! Every fixture here is built in memory; the tests need neither the retail
//! movies nor `ffmpeg`, so they run in the default `cargo test` set.

use wc3_bink::avi::Avi;
use wc3_bink::mpeg4::Mpeg4Decoder;
use wc3_bink::mp3::Mp3Decoder;
use wc3_bink::{BinkError, BinkFile, PictureType};

// ---------------------------------------------------------------- fixtures

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + body.len() + 1);
    v.extend_from_slice(id);
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    if body.len() & 1 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(4 + body.len());
    inner.extend_from_slice(kind);
    inner.extend_from_slice(body);
    chunk(b"LIST", &inner)
}

fn stream_header(fcc: &[u8; 4], handler: &[u8; 4], length: u32) -> Vec<u8> {
    let mut h = vec![0u8; 56];
    h[0..4].copy_from_slice(fcc);
    h[4..8].copy_from_slice(handler);
    h[20..24].copy_from_slice(&41_554u32.to_le_bytes()); // dwScale
    h[24..28].copy_from_slice(&1_000_000u32.to_le_bytes()); // dwRate
    h[32..36].copy_from_slice(&length.to_le_bytes());
    h
}

fn strf_video(width: i32, height: i32) -> Vec<u8> {
    let mut s = vec![0u8; 40];
    s[0..4].copy_from_slice(&40u32.to_le_bytes());
    s[4..8].copy_from_slice(&width.to_le_bytes());
    s[8..12].copy_from_slice(&height.to_le_bytes());
    s[12..14].copy_from_slice(&1u16.to_le_bytes());
    s[14..16].copy_from_slice(&24u16.to_le_bytes());
    s[16..20].copy_from_slice(b"BLZ0");
    s
}

fn strf_audio() -> Vec<u8> {
    let mut s = vec![0u8; 18];
    s[0..2].copy_from_slice(&0x0055u16.to_le_bytes());
    s[2..4].copy_from_slice(&2u16.to_le_bytes());
    s[4..8].copy_from_slice(&44_100u32.to_le_bytes());
    s
}

fn strl(fcc: &[u8; 4], handler: &[u8; 4], strf: &[u8], length: u32) -> Vec<u8> {
    let mut body = chunk(b"strh", &stream_header(fcc, handler, length));
    body.extend(chunk(b"strf", strf));
    list(b"strl", &body)
}

/// Build a valid AVI with the stream lists in the given order.
///
/// `video_first` only changes the `strl` order — and AVI stream numbers follow
/// list order, so it also changes which fourcc prefix the chunks carry.
fn build_avi(video_first: bool, width: i32, height: i32, movi: &[u8], frames: u32) -> Vec<u8> {
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&41_554u32.to_le_bytes());
    avih[16..20].copy_from_slice(&frames.to_le_bytes());
    avih[32..36].copy_from_slice(&width.to_le_bytes());
    avih[36..40].copy_from_slice(&height.to_le_bytes());

    let vstrl = strl(b"vids", b"divx", &strf_video(width, height), frames);
    let astrl = strl(b"auds", b"MP3 ", &strf_audio(), frames);
    let mut hdrl = chunk(b"avih", &avih);
    if video_first {
        hdrl.extend(vstrl);
        hdrl.extend(astrl);
    } else {
        hdrl.extend(astrl);
        hdrl.extend(vstrl);
    }

    let mut body = Vec::new();
    body.extend_from_slice(b"AVI ");
    body.extend(list(b"hdrl", &hdrl));
    body.extend(list(b"movi", movi));

    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Two video chunks (`01dc` when audio is stream 0) and two audio chunks.
fn audio_first_movi() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(chunk(b"01dc", b"frame-one"));
    body.extend(chunk(b"00wb", b"audio"));
    body.extend(chunk(b"01dc", b"frame-two"));
    body.extend(chunk(b"00wb", b"audio"));
    body
}

/// Two video chunks (`00dc` when video is stream 0) and two audio chunks.
fn video_first_movi() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(chunk(b"00dc", b"frame-one"));
    body.extend(chunk(b"01wb", b"audio"));
    body.extend(chunk(b"00dc", b"frame-two"));
    body.extend(chunk(b"01wb", b"audio"));
    body
}

/// A minimal MPEG-1 Layer III frame that decodes to silence: 192 kb/s,
/// 44.1 kHz, joint stereo, 626 bytes, all side-info bits zero.
fn silent_mp3_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 626];
    frame[0..4].copy_from_slice(&[0xff, 0xfb, 0xb0, 0x64]);
    frame
}

fn mp3_avi() -> Vec<u8> {
    let mut movi = chunk(b"00dc", b"not-a-decodable-video-frame");
    movi.extend(chunk(b"01wb", &silent_mp3_frame()));
    build_avi(true, 800, 368, &movi, 1)
}

/// Hand-built VOL header for 800x368, matching the shipped profile.
fn vol_bits() -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut push = |v: u32, n: u32| {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    };
    push(0, 1); // random_accessible_vol
    push(0, 8); // vo_type
    push(0, 1); // is_object_layer_identifier
    push(1, 4); // aspect_ratio_info 1:1
    push(0, 1); // vol_control_parameters
    push(0, 2); // shape = rect
    push(1, 1); // marker
    push(30_000, 16); // time_increment_resolution
    push(1, 1); // marker
    push(0, 1); // fixed_vop_rate
    push(1, 1); // marker
    push(800, 13); // width
    push(1, 1); // marker
    push(368, 13); // height
    push(1, 1); // marker
    push(0, 1); // progressive
    push(1, 1); // obmc_disable
    push(0, 1); // sprite_usage
    push(0, 1); // not_8_bit
    push(0, 1); // quant_type
    push(1, 1); // complexity estimation disabled
    push(0, 1); // resync_marker_disabled -> enabled
    push(0, 1); // data_partitioning
    push(0, 1); // scalability
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut out = vec![0u8; bits.len() / 8];
    for (i, b) in bits.iter().enumerate() {
        if *b {
            out[i / 8] |= 1 << (7 - (i % 8));
        }
    }
    out
}

/// I-VOP header after the `00 00 01 B6` start code, followed by zero
/// macroblock bits (the decoder conceals what it cannot read).
fn i_vop_payload() -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut push = |v: u32, n: u32| {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    };
    push(0, 2); // coding_type = I
    push(0, 1); // modulo_time_base terminator
    push(1, 1); // marker
    push(0, 15); // time_increment (time_increment_bits = 15)
    push(1, 1); // marker
    push(1, 1); // vop_coded
    push(0, 3); // intra_dc_threshold index
    push(5, 5); // qscale
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut out = vec![0x00, 0x00, 0x01, 0xB6];
    out.extend(std::iter::repeat(0u8).take(bits.len() / 8));
    for (i, b) in bits.iter().enumerate() {
        if *b {
            out[4 + i / 8] |= 1 << (7 - (i % 8));
        }
    }
    out
}

fn mpeg4_payload() -> Vec<u8> {
    let mut out = vec![0x00, 0x00, 0x01, 0x20];
    out.extend(vol_bits());
    out.extend(i_vop_payload());
    out
}

/// Deterministic xorshift so a failure is reproducible from the seed alone.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn byte(&mut self) -> u8 {
        self.next_u64() as u8
    }

    fn fill(&mut self, data: &mut [u8]) {
        for b in data {
            *b = self.byte();
        }
    }
}

// ------------------------------------------------------------------- tests

/// AVI stream numbers follow `strl` list order, not stream type. A file that
/// lists the audio stream first must still report its video frames.
#[test]
fn media_avi_census_ignores_stream_list_order() {
    let bytes = build_avi(false, 800, 368, &audio_first_movi(), 2);
    let avi = Avi::from_bytes(bytes).expect("parse audio-first AVI");
    assert_eq!(avi.video_frame_count(), 2, "video stream is stream 1 here");
    assert_eq!(avi.audio_chunk_count(), 2);
    assert_eq!(avi.chunks().len(), 4);
    assert_eq!(avi.video_frame(0), Some(&b"frame-one"[..]));
    assert_eq!(avi.video_frame(1), Some(&b"frame-two"[..]));
}

/// The same bytes must produce the same census on every parse, and the census
/// numbers must agree with each other.
#[test]
fn media_avi_census_is_deterministic() {
    let bytes = build_avi(true, 800, 368, &video_first_movi(), 2);
    let first = Avi::from_bytes(bytes.clone()).expect("parse");
    let second = Avi::from_bytes(bytes).expect("parse again");
    for avi in [&first, &second] {
        assert_eq!(avi.video_frame_count(), 2);
        assert_eq!(avi.audio_chunk_count(), 2);
        assert_eq!(avi.chunks().len(), 4);
        assert_eq!(avi.video().width, 800);
        assert_eq!(avi.video().height, 368);
        assert_eq!(avi.video().total_frames, 2);
        assert!(avi.idx1().is_empty());
    }
    assert_eq!(
        first.video_frames().collect::<Vec<_>>(),
        second.video_frames().collect::<Vec<_>>()
    );
}

/// With two video streams the first list owns the census: counting both would
/// make a stream-order change silently double the frame count.
#[test]
fn media_avi_duplicate_video_streams_use_the_first() {
    let mut hdrl = {
        let mut avih = vec![0u8; 56];
        avih[16..20].copy_from_slice(&2u32.to_le_bytes());
        avih[32..36].copy_from_slice(&800u32.to_le_bytes());
        avih[36..40].copy_from_slice(&368u32.to_le_bytes());
        chunk(b"avih", &avih)
    };
    hdrl.extend(strl(b"vids", b"divx", &strf_video(800, 368), 1));
    hdrl.extend(strl(b"vids", b"divx", &strf_video(800, 368), 1));
    let mut movi = chunk(b"00dc", b"first");
    movi.extend(chunk(b"01dc", b"second"));
    let mut body = Vec::new();
    body.extend_from_slice(b"AVI ");
    body.extend(list(b"hdrl", &hdrl));
    body.extend(list(b"movi", &movi));
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);

    let avi = Avi::from_bytes(out).expect("parse");
    assert_eq!(avi.video_frame_count(), 1);
    assert_eq!(avi.video_frame(0), Some(&b"first"[..]));
}

/// Every truncation, every single-byte corruption and random buffers must
/// return a typed error or a valid parse — never panic.
#[test]
fn media_avi_truncation_and_forgery_never_panic() {
    let good = build_avi(true, 800, 368, &audio_first_movi(), 2);
    for len in 0..good.len() {
        let _ = Avi::from_bytes(good[..len].to_vec());
    }

    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for _ in 0..256 {
        let mut data = good.clone();
        for _ in 0..4 {
            let idx = (rng.next_u64() as usize) % data.len();
            data[idx] = rng.byte();
        }
        let _ = Avi::from_bytes(data);
    }
    for len in 0..128 {
        let mut data = vec![0u8; len];
        rng.fill(&mut data);
        let _ = Avi::from_bytes(data);
    }
}

/// Untrusted `strf` dimensions must be refused before they size the frame
/// ring; a 65 535 x 65 535 header must not allocate three 17 GB buffers.
#[test]
fn media_bink_rejects_oversized_picture_headers() {
    let movi = chunk(b"00dc", b"x");
    let bytes = build_avi(true, 65_535, 65_535, &movi, 1);
    match BinkFile::from_bytes(bytes) {
        Err(BinkError::Corrupt(_)) => {}
        Err(err) => panic!("expected a corrupt error, got {err}"),
        Ok(_) => panic!("oversized picture header was accepted"),
    }
}

/// Out-of-range and undecodable frame requests are typed errors, and a valid
/// container with a garbage payload never reaches the frame ring.
#[test]
fn media_bink_decode_frame_errors_are_typed() {
    let movi = chunk(b"00dc", b"not mpeg4");
    let bytes = build_avi(true, 800, 368, &movi, 1);
    let mut file = BinkFile::from_bytes(bytes).expect("container parses");
    assert_eq!(file.frame_count(), 1);
    assert!(matches!(
        file.decode_frame(1),
        Err(BinkError::NoSuchFrame(1))
    ));
    match file.decode_frame(0) {
        Err(BinkError::Corrupt(_)) | Err(BinkError::Unsupported(_)) => {}
        Err(err) => panic!("expected a decode error, got {err}"),
        Ok(frame) => panic!("garbage payload decoded to {frame:?}"),
    }
}

/// The MPEG-4 decoder must return a typed error or conceal a damaged frame —
/// never panic — for truncations and random payloads.
#[test]
fn media_mpeg4_decoder_forgery_never_panics() {
    let payload = mpeg4_payload();
    for len in 0..payload.len() {
        let mut decoder = Mpeg4Decoder::new();
        let _ = decoder.decode(&payload[..len]);
    }

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for len in 0..256 {
        let mut data = vec![0u8; len];
        rng.fill(&mut data);
        let mut decoder = Mpeg4Decoder::new();
        let _ = decoder.decode(&data);
    }
    for _ in 0..64 {
        let mut data = payload.clone();
        for _ in 0..8 {
            let idx = (rng.next_u64() as usize) % data.len();
            data[idx] = rng.byte();
        }
        let mut decoder = Mpeg4Decoder::new();
        let _ = decoder.decode(&data);
    }
}

/// A complete I-VOP decodes through the macroblock path and reports the VOL
/// dimensions; the zero macroblock bits take the concealment path, which must
/// be counted rather than panic.
#[test]
fn media_mpeg4_i_vop_decode_path_runs() {
    let payload = mpeg4_payload();
    let mut decoder = Mpeg4Decoder::new();
    let kind = decoder.decode(&payload).expect("valid I-VOP header");
    assert_eq!(kind, PictureType::I);
    let vol = decoder.vol().expect("VOL parsed");
    assert_eq!((vol.width, vol.height), (800, 368));
    let picture = decoder.picture();
    assert_eq!((picture.width, picture.height), (800, 368));
    // The frame is either fully decoded or partly concealed; both are typed.
    let _ = (decoder.concealed_macroblocks(), decoder.last_error());
}

/// The synthetic Layer III frame decodes to 1152 stereo samples per channel of
/// silence; two independent decoders produce the same PCM.
#[test]
fn media_mp3_silent_frame_decodes_deterministically() {
    let bytes = mp3_avi();
    let mut first = BinkFile::from_bytes(bytes.clone()).expect("container");
    let mut second = BinkFile::from_bytes(bytes).expect("container");
    let first_pcm = first.audio_pcm().expect("mp3 decode").to_vec();
    let second_pcm = second.audio_pcm().expect("mp3 decode").to_vec();
    assert_eq!(first_pcm.len(), 2 * 1152, "stereo, one frame");
    assert_eq!(first_pcm, second_pcm);
    assert!(first_pcm.iter().all(|&s| s == 0), "silent frame stays silent");
}

/// Build a VOP whose header is valid and whose macroblock payload is `tail`.
/// Priming the decoder with a full access unit supplies the VOL and reference
/// picture, so the random tail reaches the coefficient/motion decoders.
fn vop_with_tail(coding_type: u32, tail: &[u8]) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut push = |v: u32, n: u32| {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    };
    push(coding_type, 2);
    push(0, 1); // modulo_time_base terminator
    push(1, 1); // marker
    push(0, 15); // time_increment
    push(1, 1); // marker
    push(1, 1); // vop_coded
    if coding_type == 1 {
        push(1, 1); // rounding
    }
    push(0, 3); // intra_dc_threshold index
    push(5, 5); // qscale
    if coding_type != 0 {
        push(1, 3); // f_code
    }
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut out = vec![0x00, 0x00, 0x01, 0xB6];
    out.resize(4 + bits.len() / 8, 0);
    for (i, b) in bits.iter().enumerate() {
        if *b {
            out[4 + i / 8] |= 1 << (7 - (i % 8));
        }
    }
    out.extend_from_slice(tail);
    out
}

/// Once a VOL and a reference picture exist, random macroblock tails must be
/// decoded, concealed or rejected — never panic.
#[test]
fn media_mpeg4_macroblock_tails_never_panic() {
    let mut decoder = Mpeg4Decoder::new();
    decoder.decode(&mpeg4_payload()).expect("priming I-VOP");

    let mut rng = Rng(0xDEAD_BEEF_CAFE_1234);
    for coding_type in [0u32, 1] {
        for len in 0..48usize {
            let tail: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
            let _ = decoder.decode(&vop_with_tail(coding_type, &tail));
        }
        for _ in 0..64 {
            let mut tail = vec![0u8; 32];
            rng.fill(&mut tail);
            let _ = decoder.decode(&vop_with_tail(coding_type, &tail));
        }
    }
}

/// Forged MP3 data (truncated frames, random bytes, mutated frames) must be a
/// typed error or silence, never a panic. Exercises header search, side info,
/// bit reservoir and Huffman decode.
#[test]
fn media_mp3_decoder_forgery_never_panics() {
    let frame = silent_mp3_frame();
    for len in 0..frame.len() {
        let mut decoder = Mp3Decoder::new();
        let mut out = Vec::new();
        let _ = decoder.decode(&frame[..len], &mut out);
    }

    let mut rng = Rng(0xC0FF_EE00_1234_5678);
    for len in 0..256 {
        let mut data = vec![0u8; len];
        rng.fill(&mut data);
        let mut decoder = Mp3Decoder::new();
        let mut out = Vec::new();
        let _ = decoder.decode(&data, &mut out);
    }
    for _ in 0..128 {
        let mut data = frame.clone();
        // Keep the sync word so the frame is found, corrupt everything else.
        for idx in 4..data.len() {
            data[idx] = rng.byte();
        }
        let mut decoder = Mp3Decoder::new();
        let mut out = Vec::new();
        let _ = decoder.decode(&data, &mut out);
    }
}
