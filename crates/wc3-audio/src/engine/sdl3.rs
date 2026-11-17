//! SDL3 audio backend, loaded at runtime with `dlopen` so the crate does not
//! need a build-time link against SDL.
//!
//! The app already links SDL3 for window/input, so `libSDL3.so` is mapped into
//! the process and `dlopen` finds it. Termux has `$PREFIX/lib/libSDL3.so`.
//! Usage follows the SDL3 wiki (`SDL_OpenAudioDeviceStream`, accessed
//! 2026-11-12): open a stream for the default playback device with an f32
//! spec, push blocks with `SDL_PutAudioStreamData`, and resume the device once
//! (`SDL_ResumeAudioStreamDevice`) because a stream opened this way starts
//! paused.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use super::{AudioBackend, AudioConfig};
use crate::AudioError;

const SDL_INIT_AUDIO: u32 = 0x0000_0010;
const SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK: u32 = 0xFFFF_FFFF;
const SDL_AUDIO_F32: u16 = 0x8120;

#[repr(C)]
struct SdlAudioSpec {
    format: u16,
    _pad: u16,
    channels: c_int,
    freq: c_int,
}

type SdlInitSubSystem = unsafe extern "C" fn(u32) -> c_int;
type SdlOpenAudioDeviceStream =
    unsafe extern "C" fn(u32, *const SdlAudioSpec, *mut c_void, *mut c_void) -> *mut c_void;
type SdlPutAudioStreamData = unsafe extern "C" fn(*mut c_void, *const c_void, c_int) -> c_int;
type SdlGetAudioStreamQueued = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlClearAudioStream = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlPauseAudioStreamDevice = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlResumeAudioStreamDevice = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlDestroyAudioStream = unsafe extern "C" fn(*mut c_void);
type SdlGetError = unsafe extern "C" fn() -> *const c_char;
type SdlQuitSubSystem = unsafe extern "C" fn(u32);

struct Api {
    handle: *mut c_void,
    init_subsystem: SdlInitSubSystem,
    open_stream: SdlOpenAudioDeviceStream,
    put_data: SdlPutAudioStreamData,
    /// Optional: SDL >= 3.1.x exposes the stream queue depth. When absent the
    /// engine falls back to wall-clock pacing (old behaviour).
    get_queued: Option<SdlGetAudioStreamQueued>,
    /// Optional: drops queued PCM before destroy (a stalled device otherwise
    /// makes `SDL_DestroyAudioStream` wait forever).
    clear: Option<SdlClearAudioStream>,
    pause_device: Option<SdlPauseAudioStreamDevice>,
    resume: SdlResumeAudioStreamDevice,
    destroy_stream: SdlDestroyAudioStream,
    get_error: SdlGetError,
    quit_subsystem: SdlQuitSubSystem,
}

impl Api {
    fn load() -> Result<Self, AudioError> {
        unsafe {
            let mut handle = std::ptr::null_mut();
            for name in ["libSDL3.so.0", "libSDL3.so", "SDL3"] {
                let Ok(cname) = CString::new(name) else {
                    continue;
                };
                handle = libc::dlopen(cname.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
                if !handle.is_null() {
                    break;
                }
            }
            if handle.is_null() {
                return Err(AudioError::Backend(
                    "libSDL3.so not found (install SDL3 or bundle it in the APK)".into(),
                ));
            }
            let sym = |name: &str| -> Result<*mut c_void, AudioError> {
                let cname =
                    CString::new(name).map_err(|_| AudioError::Backend("bad symbol".into()))?;
                let ptr = libc::dlsym(handle, cname.as_ptr());
                if ptr.is_null() {
                    Err(AudioError::Backend(format!("missing SDL3 symbol {name}")))
                } else {
                    Ok(ptr)
                }
            };
            let sym_opt = |name: &str| -> Option<*mut c_void> {
                let cname = CString::new(name).ok()?;
                let ptr = libc::dlsym(handle, cname.as_ptr());
                (!ptr.is_null()).then_some(ptr)
            };
            Ok(Self {
                handle,
                init_subsystem: std::mem::transmute(sym("SDL_InitSubSystem")?),
                open_stream: std::mem::transmute(sym("SDL_OpenAudioDeviceStream")?),
                put_data: std::mem::transmute(sym("SDL_PutAudioStreamData")?),
                get_queued: sym_opt("SDL_GetAudioStreamQueued")
                    .map(|ptr| std::mem::transmute::<*mut c_void, SdlGetAudioStreamQueued>(ptr)),
                clear: sym_opt("SDL_ClearAudioStream")
                    .map(|ptr| std::mem::transmute::<*mut c_void, SdlClearAudioStream>(ptr)),
                pause_device: sym_opt("SDL_PauseAudioStreamDevice")
                    .map(|ptr| std::mem::transmute::<*mut c_void, SdlPauseAudioStreamDevice>(ptr)),
                resume: std::mem::transmute(sym("SDL_ResumeAudioStreamDevice")?),
                destroy_stream: std::mem::transmute(sym("SDL_DestroyAudioStream")?),
                get_error: std::mem::transmute(sym("SDL_GetError")?),
                quit_subsystem: std::mem::transmute(sym("SDL_QuitSubSystem")?),
            })
        }
    }

    fn error(&self) -> String {
        unsafe {
            let ptr = (self.get_error)();
            if ptr.is_null() {
                "unknown SDL3 error".into()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                // Do not SDL_Quit: the application owns the SDL lifecycle for
                // video/input. Only release our reference.
                libc::dlclose(self.handle);
                self.handle = std::ptr::null_mut();
            }
        }
    }
}

pub struct Sdl3Backend {
    api: Api,
    stream: *mut c_void,
    /// Bytes of f32 samples per millisecond, for queue-depth reporting.
    bytes_per_ms: f64,
    /// Target latency advertised to the engine (`AudioConfig::effective_buffer_ms`).
    latency_ms: u32,
    /// Queue depth above which `write` refuses to push (non-blocking contract).
    max_queued_ms: u32,
    closed: bool,
}

// The backend is moved into the render thread and only touched there.
unsafe impl Send for Sdl3Backend {}

impl std::fmt::Debug for Sdl3Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sdl3Backend")
            .field("closed", &self.closed)
            .finish()
    }
}

impl Sdl3Backend {
    pub fn open(config: &AudioConfig) -> Result<Self, AudioError> {
        let api = Api::load()?;
        unsafe {
            if (api.init_subsystem)(SDL_INIT_AUDIO) == 0 {
                let err = api.error();
                return Err(AudioError::Backend(format!(
                    "SDL_InitSubSystem(audio): {err}"
                )));
            }
            let spec = SdlAudioSpec {
                format: SDL_AUDIO_F32,
                _pad: 0,
                channels: config.channels as c_int,
                freq: config.sample_rate as c_int,
            };
            let stream = (api.open_stream)(
                SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
                &spec,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if stream.is_null() {
                let err = api.error();
                (api.quit_subsystem)(SDL_INIT_AUDIO);
                return Err(AudioError::Backend(format!(
                    "SDL_OpenAudioDeviceStream: {err}"
                )));
            }
            // Streams opened this way start paused; resume before the first block.
            if (api.resume)(stream) == 0 {
                let err = api.error();
                (api.destroy_stream)(stream);
                (api.quit_subsystem)(SDL_INIT_AUDIO);
                return Err(AudioError::Backend(format!(
                    "SDL_ResumeAudioStreamDevice: {err}"
                )));
            }
            Ok(Self {
                api,
                stream,
                bytes_per_ms: config.sample_rate as f64
                    * config.channels as f64
                    * std::mem::size_of::<f32>() as f64
                    / 1000.0,
                latency_ms: config.effective_buffer_ms(),
                max_queued_ms: config.effective_buffer_ms().saturating_add(50),
                closed: false,
            })
        }
    }

    /// Description of the loaded library, for diagnostics.
    pub fn describe() -> Result<String, AudioError> {
        let api = Api::load()?;
        let rc = unsafe { (api.init_subsystem)(SDL_INIT_AUDIO) };
        unsafe { (api.quit_subsystem)(SDL_INIT_AUDIO) };
        Ok(if rc != 0 {
            "SDL3 audio subsystem available".to_string()
        } else {
            format!("SDL3 loaded but audio init failed: {}", api.error())
        })
    }
}

/// Largest byte count handed to one `SDL_PutAudioStreamData` call.
///
/// SDL takes a `c_int` byte count; a caller-supplied block larger than this is
/// split into [`chunk_lengths`] so no single FFI call can approach the ABI
/// width or park the device queue for a long transfer. 1 MiB is 128x the stock
/// mixer block (1024 frames x 2 channels x 4 bytes = 8 KiB), so the normal
/// render path issues exactly one call per block.
pub const MAX_PUT_BYTES: usize = 1 << 20;

/// Bytes for `samples` interleaved `f32` samples, checked for the SDL ABI.
///
/// `samples.len() * size_of::<f32>()` is caller-controlled arithmetic and the
/// result is narrowed to `c_int` for FFI: both steps are explicit so an
/// oversized slice returns [`AudioError::Backend`] *before* the unsafe call
/// instead of wrapping or truncating (AUD-020).
fn put_bytes(samples: usize) -> Result<c_int, AudioError> {
    let bytes = samples
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| {
            AudioError::Backend(format!(
                "audio block of {samples} samples overflows the byte count"
            ))
        })?;
    c_int::try_from(bytes).map_err(|_| {
        AudioError::Backend(format!(
            "audio block of {bytes} bytes exceeds the c_int FFI limit ({})",
            c_int::MAX
        ))
    })
}

/// Sample counts of the individual FFI transfers for a `samples`-long block.
///
/// At most [`MAX_PUT_BYTES`] per transfer; a zero-length block yields no
/// transfers. Pure arithmetic so the split can be tested without a device.
fn chunk_lengths(samples: usize) -> impl Iterator<Item = usize> {
    let chunk = MAX_PUT_BYTES / std::mem::size_of::<f32>();
    (0..samples)
        .step_by(chunk)
        .map(move |start| (samples - start).min(chunk))
}

impl AudioBackend for Sdl3Backend {
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError> {
        if self.closed || self.stream.is_null() {
            return Err(AudioError::Backend("SDL3 stream is closed".into()));
        }
        if samples.is_empty() {
            return Ok(0);
        }
        // Non-blocking contract: `SDL_PutAudioStreamData` blocks when the
        // stream buffer is full (device paused/suspended), and a blocked write
        // also blocks `Drop`'s join. Keep the queue at the pacing target and
        // skip this block instead; `write` returns 0 and the mixer sleeps.
        if self.max_queued_ms > 0
            && let Some(queued) = self.buffered_ms()
            && queued >= self.max_queued_ms
        {
            return Ok(0);
        }
        let mut written = 0usize;
        for len in chunk_lengths(samples.len()) {
            let chunk = &samples[written..written + len];
            let bytes = put_bytes(chunk.len())?;
            super::shutdown_log("sdl_put_begin");
            let ok =
                unsafe { (self.api.put_data)(self.stream, chunk.as_ptr() as *const c_void, bytes) };
            super::shutdown_log("sdl_put_end");
            if ok == 0 {
                return Err(AudioError::Backend(format!(
                    "SDL_PutAudioStreamData: {} (after {written} of {} samples)",
                    self.api.error(),
                    samples.len()
                )));
            }
            written += len;
        }
        Ok(written)
    }

    fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    /// Bytes queued in the SDL stream, converted to milliseconds of playback.
    ///
    /// This is the signal the render thread paces against: keeping it near the
    /// configured target keeps the device fed across scheduler jitter.
    fn buffered_ms(&self) -> Option<u32> {
        let get_queued = self.api.get_queued?;
        if self.closed || self.stream.is_null() {
            return None;
        }
        let bytes = unsafe { get_queued(self.stream) };
        if bytes <= 0 || self.bytes_per_ms <= 0.0 {
            return Some(0);
        }
        Some((bytes as f64 / self.bytes_per_ms).round() as u32)
    }

    fn name(&self) -> &'static str {
        "sdl3"
    }

    fn close(&mut self) {
        if !self.closed && !self.stream.is_null() {
            // `SDL_DestroyAudioStream` must run on the thread that opened the
            // stream: called from the render thread it blocked indefinitely in
            // SDL's device/mainloop teardown (measured `sdl_destroy_begin`
            // without `sdl_destroy_end`, 200 ms join timeout), while the same
            // call from the opening thread returned in 0 ms. The engine hands
            // the backend back before joining (see `AudioEngine::stop_render_thread`).
            // Destroy waits for queued PCM to drain; a stalled/suspended
            // device leaves the queue full and turns teardown into a hang
            // (measured `sdl_destroy_begin` with no end). Drop the queue first.
            if let Some(pause) = self.api.pause_device {
                super::shutdown_log("sdl_pause_begin");
                unsafe {
                    let _ = pause(self.stream);
                }
                super::shutdown_log("sdl_pause_end");
            }
            if let Some(clear) = self.api.clear {
                super::shutdown_log("sdl_clear_begin");
                unsafe {
                    let _ = clear(self.stream);
                }
                super::shutdown_log("sdl_clear_end");
            }
            super::shutdown_log("sdl_destroy_begin");
            unsafe {
                (self.api.destroy_stream)(self.stream);
            }
            super::shutdown_log("sdl_destroy_end");
            self.stream = std::ptr::null_mut();
            self.closed = true;
            super::shutdown_log("sdl_quit_subsystem_begin");
            unsafe {
                (self.api.quit_subsystem)(SDL_INIT_AUDIO);
            }
            super::shutdown_log("sdl_quit_subsystem_end");
        }
    }
}

impl Drop for Sdl3Backend {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stock mixer block: 1024 frames x 2 channels (`AudioConfig::default`).
    const STOCK_BLOCK_SAMPLES: usize = 2048;

    #[test]
    fn zero_samples_need_zero_bytes() {
        assert_eq!(put_bytes(0).expect("0 bytes always fit"), 0);
        assert_eq!(
            chunk_lengths(0).count(),
            0,
            "an empty block issues no FFI call"
        );
    }

    #[test]
    fn a_stock_mixer_block_matches_the_sdl_byte_count() {
        // 2048 f32 samples = 8192 bytes, one transfer well inside c_int.
        assert_eq!(put_bytes(STOCK_BLOCK_SAMPLES).expect("stock block"), 8192);
        assert_eq!(
            chunk_lengths(STOCK_BLOCK_SAMPLES).collect::<Vec<_>>(),
            vec![2048]
        );
    }

    #[test]
    fn the_c_int_boundary_is_accepted_and_the_next_sample_is_rejected() {
        // 536_870_911 samples * 4 = 2_147_483_644 <= c_int::MAX.
        let boundary_samples = (c_int::MAX as usize) / std::mem::size_of::<f32>();
        assert_eq!(boundary_samples, 536_870_911);
        assert_eq!(
            put_bytes(boundary_samples).expect("the largest byte count c_int can carry"),
            2_147_483_644
        );
        // One more sample crosses c_int::MAX (2_147_483_648 bytes).
        let err = put_bytes(boundary_samples + 1).expect_err("c_int cannot carry 2^31 bytes");
        assert!(
            err.to_string().contains("c_int FFI limit"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn a_sample_count_that_overflows_the_byte_multiplication_is_an_error() {
        // No allocation: the helper only multiplies the length.
        let err = put_bytes(usize::MAX).expect_err("usize::MAX samples cannot be sized");
        assert!(
            err.to_string().contains("overflows the byte count"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn blocks_above_the_documented_maximum_are_split_not_truncated() {
        let chunk = MAX_PUT_BYTES / std::mem::size_of::<f32>();
        assert_eq!(chunk, 262_144);
        assert_eq!(chunk_lengths(chunk).collect::<Vec<_>>(), vec![chunk]);
        assert_eq!(chunk_lengths(chunk + 1).collect::<Vec<_>>(), vec![chunk, 1]);
        assert_eq!(
            chunk_lengths(3 * chunk + 1).collect::<Vec<_>>(),
            vec![chunk, chunk, chunk, 1]
        );
        // Every transfer is representable in c_int, which is the point of the
        // split: the maximum chunk passes the same check as the stock block.
        assert!(put_bytes(chunk).is_ok());
        assert_eq!(
            chunk_lengths(3 * chunk + 1).sum::<usize>(),
            3 * chunk + 1,
            "splitting must not drop samples"
        );
    }
}
