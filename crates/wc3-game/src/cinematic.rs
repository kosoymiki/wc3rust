//! Campaign movie playback: the CGI the engine plays between missions.
//!
//! The retail installs ship nine cinematics outside the data archives:
//! seven Reign of Chaos movies in `Movies\*.mpq` and two Frozen Throne trailers
//! in `trailers\*.blz`. Despite the extensions they are **RIFF/AVI** containers
//! carrying an MPEG-4 Part 2 (`BLZ0`) video stream and MPEG-1 Layer III audio at
//! 44.1 kHz stereo — measured on the operator's installer images (see
//! `analysis/media/BINK_INVENTORY.md`).
//!
//! Decoding is **our own code** (`crates/wc3-bink`): this module never shells
//! out to `ffmpeg` and never delegates to a platform codec. The split is:
//!
//! * [`MovieLibrary`] resolves one of the nine names to the extracted file on
//!   disk and reports `MOVIE_MISSING <name>` when the install does not carry it;
//! * [`MovieDecoder`] is the decoder contract: RGBA frames by index plus PCM
//!   audio, implemented by [`bink_decoder::OwnDecoder`] over
//!   `wc3_bink::BinkFile` behind the `movie_decoder` feature;
//! * [`CinematicPlayer`] is the deterministic state machine — fade in, play,
//!   fade out, skip tap target never smaller than [`SKIP_MIN_PX`] — pulling
//!   frames on its clock;
//! * [`Game::play_movie`] is the game-side entry point, and
//!   [`Game::cinematic_start`] is the one a campaign mission's `PlayCinematic`
//!   reaches: it decodes the movie on this thread, logs `MOVIE_PLAY`, and logs
//!   `MOVIE_MISSING <name>` instead of blocking when the install has no file.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use glam::Vec2;

/// The engine's Skip button is a fixed tap target; below this the touch layer
/// cannot distinguish an intentional skip from a mis-tap on this device.
pub const SKIP_MIN_PX: f32 = 126.0;

/// Fade-in duration when a movie starts, seconds.
pub const FADE_IN_S: f32 = 0.45;

/// Fade-out duration when a movie ends or is skipped, seconds.
pub const FADE_OUT_S: f32 = 0.35;

/// Ignore a tap during the first moments of the fade-in; the engine does the
/// same so the tap that started the movie cannot skip it.
pub const SKIP_ARM_DELAY_S: f32 = 0.30;

/// Nominal frame rate used until the caller supplies the container's real rate
/// (all shipped RoC movies are 289/12 ≈ 24.083 fps, the trailers 24 and 30).
pub const DEFAULT_MOVIE_FPS: f32 = 24.0;

/// The nine movie names the game's "Play movie" API accepts.
pub use wc3_formats::media::KNOWN_MOVIES;

/// Default search roots, in order: the operator's extracted ISO tree, then the
/// app's private data (where an embedded install would put them).
pub const DEFAULT_MOVIE_ROOTS: [&str; 3] = [
    "/storage/emulated/0/Download/wr3/iso/movies/Movies",
    "/storage/emulated/0/Download/wr3/iso/trailers/trailers",
    "/data/data/org.herofolends.p0/files/Movies",
];

/// Errors the movie layer reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovieError {
    /// The install does not carry the named movie. `Display` is exactly
    /// `MOVIE_MISSING <name>`, the contract the UI keys on.
    Missing(String),
    /// The file exists but cannot be read.
    Io(String),
    /// The container/bitstream is corrupt or unsupported.
    Decode(String),
    /// The decoder crate does not implement this part yet (honest, not a stub).
    Unsupported(String),
}

impl std::fmt::Display for MovieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "MOVIE_MISSING {name}"),
            Self::Io(message) => write!(f, "MOVIE_IO {message}"),
            Self::Decode(message) => write!(f, "MOVIE_DECODE {message}"),
            Self::Unsupported(message) => write!(f, "MOVIE_UNSUPPORTED {message}"),
        }
    }
}

impl std::error::Error for MovieError {}

thread_local! {
    /// The scripted CGI a campaign mission is playing on this thread. The player
    /// owns a `Box<dyn MovieDecoder>`, which is neither `Send` nor simulation
    /// state: it lives with the render thread and is dropped when the movie
    /// ends.
    static ACTIVE_MOVIE: RefCell<Option<CinematicPlayer>> = RefCell::new(None);
}

/// Device-independent viewport the scripted movie is composited into.
const MOVIE_VIEWPORT: Vec2 = Vec2::new(1280.0, 720.0);

impl MovieError {
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing(_))
    }
}

/// An axis-aligned rectangle in device-independent pixels, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn width(&self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }

    pub fn height(&self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.x <= self.max.x
            && point.y <= self.max.y
    }
}

/// The decoder contract.
///
/// Implemented by [`bink_decoder::OwnDecoder`] over `wc3_bink::BinkFile`
/// (`movie_decoder` feature). Object-safe on purpose: the player owns
/// `Box<dyn MovieDecoder>`, so a real decoder and [`TestPatternDecoder`] are
/// interchangeable without touching the state machine.
///
/// `frame` returns RGBA8, `size().0 * size().1 * 4` bytes, top-left origin.
/// `audio` appends interleaved signed 16-bit PCM at `sample_rate()`; a movie
/// with no audio leaves `out` untouched and returns `Ok(())`.
pub trait MovieDecoder {
    fn size(&self) -> (u32, u32);
    fn frame(&mut self, index: usize) -> Result<Vec<u8>, MovieError>;
    fn sample_rate(&self) -> u32;
    fn audio(&mut self, out: &mut Vec<i16>) -> Result<(), MovieError>;
}

/// A decoded frame handed to the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub pts_ms: u64,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, pts_ms: u64, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
            pts_ms,
        }
    }

    /// Bytes per row, for `wgpu::TexelCopyBufferLayout`.
    pub fn stride(&self) -> u32 {
        self.width * 4
    }

    /// A solid-colour frame; used by [`TestPatternDecoder`].
    pub fn solid(width: u32, height: u32, pts_ms: u64, rgba: [u8; 4]) -> Self {
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
        Self::new(width, height, pts_ms, pixels)
    }
}

/// GPU texture that holds the current movie frame.
///
/// The renderer-side half of the playback path: the decoder produces RGBA8,
/// this owns an `Rgba8UnormSrgb` texture of the frame size and re-uploads in
/// place. A size change reallocates; the caller samples `view` with a
/// full-screen quad.
pub struct VideoSurfaceTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl VideoSurfaceTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cinematic-frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// Uploads `frame`, reallocating when the dimensions changed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &VideoFrame) {
        if frame.width != self.width || frame.height != self.height {
            *self = Self::new(device, frame.width, frame.height);
        }
        debug_assert_eq!(
            frame.rgba.len(),
            (frame.width as usize) * (frame.height as usize) * 4,
            "VideoFrame buffer must be width*height*4 RGBA bytes"
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.stride()),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Deterministic stand-in decoder: solid frames, a two-tone PCM block.
///
/// Tests the fade/skip/frame-clock behaviour on the real state machine without
/// shipping movie bytes into the repository.
#[derive(Debug, Clone)]
pub struct TestPatternDecoder {
    pub width: u32,
    pub height: u32,
    pub frames: usize,
    pub fps: f32,
    pub audio_rate: u32,
    pcm: Vec<i16>,
}

impl TestPatternDecoder {
    pub fn new(width: u32, height: u32, frames: usize, fps: f32) -> Self {
        let audio_rate = 22_050;
        let samples_per_frame = (audio_rate as f32 / fps.max(1.0)).round() as usize;
        Self {
            width,
            height,
            frames,
            fps,
            audio_rate,
            pcm: (0..samples_per_frame * 2)
                .map(|sample| if (sample / 50) % 2 == 0 { 8_000 } else { -8_000 })
                .collect(),
        }
    }
}

impl MovieDecoder for TestPatternDecoder {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame(&mut self, index: usize) -> Result<Vec<u8>, MovieError> {
        if index >= self.frames {
            return Err(MovieError::Decode(format!(
                "frame {index} past the end ({} frames)",
                self.frames
            )));
        }
        let shade = (index % 256) as u8;
        Ok(VideoFrame::solid(self.width, self.height, 0, [shade, 40, 200, 255]).rgba)
    }

    fn sample_rate(&self) -> u32 {
        self.audio_rate
    }

    fn audio(&mut self, out: &mut Vec<i16>) -> Result<(), MovieError> {
        out.extend_from_slice(&self.pcm);
        Ok(())
    }
}

/// One movie on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieFile {
    /// One of [`KNOWN_MOVIES`] (`HumanEd`).
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// `(width, height)` from the container header, when read.
    pub size: Option<(u32, u32)>,
    /// Duration in seconds, when read from the container.
    pub duration_s: Option<f32>,
}

/// The movies reachable from the configured roots.
#[derive(Debug, Clone, Default)]
pub struct MovieLibrary {
    pub roots: Vec<PathBuf>,
    pub movies: Vec<MovieFile>,
    /// Known names with no file under any root.
    pub missing: Vec<String>,
    /// Directories that could not be read, with the reason.
    pub errors: Vec<String>,
}

impl MovieLibrary {
    /// Scans `roots` (depth ≤ 3) for the nine known movie files.
    ///
    /// The directory walk lives in `wc3_formats::media` so the archive-based
    /// scanner and the directory resolver share one definition of the nine
    /// names; this wrapper adds the per-file metadata the player needs.
    pub fn scan(roots: &[PathBuf]) -> Self {
        let (found, missing, errors) = wc3_formats::media::scan_movie_dirs(roots);
        Self {
            roots: roots.to_vec(),
            movies: found
                .into_iter()
                .map(|movie| MovieFile {
                    name: movie.name,
                    path: movie.path,
                    bytes: movie.bytes,
                    size: None,
                    duration_s: None,
                })
                .collect(),
            missing,
            errors,
        }
    }

    /// Scan roots from `HEROFOLENDS_MOVIES` (colon-separated) followed by
    /// [`DEFAULT_MOVIE_ROOTS`].
    pub fn installed() -> Self {
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(list) = std::env::var_os("HEROFOLENDS_MOVIES") {
            for part in std::env::split_paths(&list) {
                roots.push(part);
            }
        }
        for root in DEFAULT_MOVIE_ROOTS {
            let path = PathBuf::from(root);
            if !roots.iter().any(|known| known == &path) {
                roots.push(path);
            }
        }
        Self::scan(&roots)
    }

    pub fn find(&self, name: &str) -> Option<&MovieFile> {
        let wanted = name.trim();
        let stem = wanted
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(wanted)
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or_else(|| wanted.rsplit(['\\', '/']).next().unwrap_or(wanted));
        self.movies
            .iter()
            .find(|movie| movie.name.eq_ignore_ascii_case(stem) || movie.name.eq_ignore_ascii_case(wanted))
    }

    /// The nine names with a boolean saying whether the file is present.
    pub fn availability(&self) -> Vec<(&'static str, bool)> {
        KNOWN_MOVIES
            .iter()
            .map(|name| (*name, self.find(name).is_some()))
            .collect()
    }
}

/// Process-wide library, scanned on first use.
pub fn movies() -> &'static MovieLibrary {
    static LIBRARY: OnceLock<MovieLibrary> = OnceLock::new();
    LIBRARY.get_or_init(MovieLibrary::installed)
}

/// Where the player is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CinematicState {
    Idle,
    FadeIn,
    Playing,
    Skipping,
    FadeOut,
    Finished,
}

impl CinematicState {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle | Self::Finished)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::FadeIn => "fade-in",
            Self::Playing => "playing",
            Self::Skipping => "skipping",
            Self::FadeOut => "fade-out",
            Self::Finished => "finished",
        }
    }
}

/// A tap that landed on the skip target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkipTap {
    pub position: Vec2,
}

/// What the renderer/audio mixer needs for this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoSurface {
    pub title: String,
    pub uri: PathBuf,
    pub opacity: f32,
    pub frame_size: Option<(u32, u32)>,
    pub skip_rect: Rect,
    pub audio_rate: u32,
}

/// The deterministic movie player.
pub struct CinematicPlayer {
    pub movie: MovieFile,
    decoder: Option<Box<dyn MovieDecoder>>,
    state: CinematicState,
    elapsed: f32,
    fade_elapsed: f32,
    duration_s: Option<f32>,
    opacity: f32,
    skipped: bool,
    viewport: Vec2,
    frame_rate: f32,
    decoded_frames: usize,
    current_frame: Option<VideoFrame>,
    frame_clock: f32,
    audio_samples: usize,
    audio_error: Option<String>,
}

impl std::fmt::Debug for CinematicPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CinematicPlayer")
            .field("movie", &self.movie.name)
            .field("state", &self.state)
            .field("elapsed", &self.elapsed)
            .field("decoded_frames", &self.decoded_frames)
            .field("skipped", &self.skipped)
            .finish()
    }
}

impl CinematicPlayer {
    pub fn new(movie: MovieFile) -> Self {
        let duration_s = movie.duration_s;
        Self {
            movie,
            decoder: None,
            state: CinematicState::Idle,
            elapsed: 0.0,
            fade_elapsed: 0.0,
            duration_s,
            opacity: 0.0,
            skipped: false,
            viewport: Vec2::new(1920.0, 1080.0),
            frame_rate: DEFAULT_MOVIE_FPS,
            decoded_frames: 0,
            current_frame: None,
            frame_clock: 0.0,
            audio_samples: 0,
            audio_error: None,
        }
    }

    /// Attaches the decoder.
    pub fn with_decoder(mut self, decoder: Box<dyn MovieDecoder>) -> Self {
        self.decoder = Some(decoder);
        self
    }

    /// Sets the presentation rate (the container's real rate, when known).
    pub fn with_frame_rate(mut self, fps: f32) -> Self {
        if fps.is_finite() && fps > 0.0 {
            self.frame_rate = fps;
        }
        self
    }

    /// Begins playback: fade-in starts on the next [`Self::update`].
    pub fn start(mut self) -> Self {
        if self.state == CinematicState::Idle {
            self.state = CinematicState::FadeIn;
            self.fade_elapsed = 0.0;
            self.opacity = 0.0;
        }
        self
    }

    pub fn state(&self) -> CinematicState {
        self.state
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    pub fn duration(&self) -> Option<f32> {
        self.duration_s
    }

    /// Frames decoded and displayed so far.
    pub fn decoded_frames(&self) -> usize {
        self.decoded_frames
    }

    pub fn current_frame(&self) -> Option<&VideoFrame> {
        self.current_frame.as_ref()
    }

    /// PCM samples handed to the mixer so far.
    pub fn audio_samples(&self) -> usize {
        self.audio_samples
    }

    /// Set when the decoder reported it cannot produce audio (e.g. the audio
    /// codec is not landed yet); playback stays visual instead of lying.
    pub fn audio_error(&self) -> Option<&str> {
        self.audio_error.as_deref()
    }

    pub fn set_duration(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.duration_s = Some(seconds);
        }
    }

    pub fn was_skipped(&self) -> bool {
        self.skipped
    }

    /// The skip target for a viewport in device-independent pixels.
    ///
    /// Lower-right corner, inset by `max(24 px, 2 %)`, at least
    /// [`SKIP_MIN_PX`] on both axes — the WC3 Skip button is a fixed square
    /// there. The contract holds even on a pathologically small viewport.
    pub fn skip_rect(&self, viewport: Vec2) -> Rect {
        let short = viewport.x.min(viewport.y).max(SKIP_MIN_PX);
        let size = SKIP_MIN_PX.max(short * 0.18).min(short * 0.42);
        let margin = (short * 0.02).max(24.0);
        let max = Vec2::new(viewport.x - margin, viewport.y - margin);
        let min = Vec2::new((max.x - size).max(0.0), (max.y - size).max(0.0));
        let min = Vec2::new(
            min.x.min(max.x - SKIP_MIN_PX),
            min.y.min(max.y - SKIP_MIN_PX),
        );
        Rect::new(min.max(Vec2::ZERO), max)
    }

    /// Requests a stop; the player fades out instead of cutting.
    pub fn stop(&mut self) {
        match self.state {
            CinematicState::Idle => self.state = CinematicState::Finished,
            CinematicState::FadeIn | CinematicState::Playing => {
                self.state = CinematicState::FadeOut;
                self.fade_elapsed = 0.0;
            }
            _ => {}
        }
    }

    /// The decoder reached the end of the file.
    pub fn finish(&mut self) {
        if matches!(self.state, CinematicState::FadeIn | CinematicState::Playing) {
            self.state = CinematicState::FadeOut;
            self.fade_elapsed = 0.0;
        }
    }

    /// Advances the state machine and pulls the frame the clock calls for.
    ///
    /// `tap` is a touch released this frame in device-independent pixels.
    /// Returns `true` when the player became [`CinematicState::Finished`].
    pub fn update(&mut self, dt: f32, tap: Option<SkipTap>, viewport: Vec2) -> bool {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        self.elapsed += dt;
        self.viewport = viewport;
        let armed = self.elapsed >= SKIP_ARM_DELAY_S;
        if let Some(tap) = tap {
            if armed
                && matches!(self.state, CinematicState::FadeIn | CinematicState::Playing)
                && self.skip_rect(viewport).contains(tap.position)
            {
                self.skipped = true;
                self.state = CinematicState::Skipping;
                self.fade_elapsed = 0.0;
            }
        }
        match self.state {
            CinematicState::Idle | CinematicState::Finished => false,
            CinematicState::FadeIn => {
                self.advance_frames(dt);
                self.fade_elapsed += dt;
                self.opacity = (self.fade_elapsed / FADE_IN_S).min(1.0);
                if self.opacity >= 1.0 {
                    self.state = CinematicState::Playing;
                }
                self.check_duration();
                self.state == CinematicState::Finished
            }
            CinematicState::Playing => {
                self.advance_frames(dt);
                self.opacity = 1.0;
                self.check_duration();
                self.state == CinematicState::Finished
            }
            CinematicState::Skipping => {
                self.fade_elapsed += dt;
                let t = (self.fade_elapsed / FADE_OUT_S).min(1.0);
                self.opacity = 1.0 - t;
                if t >= 1.0 {
                    self.state = CinematicState::FadeOut;
                    self.fade_elapsed = 0.0;
                }
                false
            }
            CinematicState::FadeOut => {
                self.fade_elapsed += dt;
                let t = (self.fade_elapsed / FADE_OUT_S).min(1.0);
                self.opacity = 1.0 - t;
                if t >= 1.0 {
                    self.opacity = 0.0;
                    self.state = CinematicState::Finished;
                    return true;
                }
                false
            }
        }
    }

    /// Decodes as many frames as the elapsed time calls for.
    fn advance_frames(&mut self, dt: f32) {
        let Some(decoder) = self.decoder.as_mut() else {
            return;
        };
        let interval = 1.0 / self.frame_rate.max(1.0);
        self.frame_clock += dt;
        let (width, height) = decoder.size();
        let mut ended = false;
        let mut pcm = Vec::new();
        while self.frame_clock >= interval {
            match decoder.frame(self.decoded_frames) {
                Ok(rgba) => {
                    let expected = (width as usize) * (height as usize) * 4;
                    if rgba.len() != expected {
                        self.audio_error = Some(format!(
                            "decoder returned {} bytes, expected {expected}",
                            rgba.len()
                        ));
                        ended = true;
                        break;
                    }
                    let pts_ms = ((self.decoded_frames as f32) * 1000.0 / self.frame_rate)
                        .round() as u64;
                    self.current_frame = Some(VideoFrame::new(width, height, pts_ms, rgba));
                    self.decoded_frames += 1;
                    self.frame_clock -= interval;
                    pcm.clear();
                    if let Err(err) = decoder.audio(&mut pcm) {
                        // Video keeps playing; the mixer gap is reported, not
                        // hidden.
                        self.audio_error = Some(err.to_string());
                    } else {
                        self.audio_samples += pcm.len();
                    }
                }
                Err(MovieError::Decode(_)) => {
                    // End of stream: the trait has no frame count, so the first
                    // out-of-range frame is the natural end signal.
                    ended = true;
                    break;
                }
                Err(err) => {
                    self.audio_error = Some(err.to_string());
                    ended = true;
                    break;
                }
            }
        }
        if ended {
            self.finish();
        }
    }

    fn check_duration(&mut self) {
        let Some(duration) = self.duration_s else {
            return;
        };
        if self.elapsed >= duration && self.state.is_active() {
            self.state = CinematicState::FadeOut;
            self.fade_elapsed = 0.0;
        }
    }

    /// What the renderer and audio mixer consume this frame.
    pub fn surface(&self, viewport: Vec2) -> VideoSurface {
        VideoSurface {
            title: self.movie.name.clone(),
            uri: self.movie.path.clone(),
            opacity: self.opacity,
            frame_size: self
                .current_frame
                .as_ref()
                .map(|frame| (frame.width, frame.height)),
            skip_rect: self.skip_rect(viewport),
            audio_rate: self
                .decoder
                .as_ref()
                .map(|decoder| decoder.sample_rate())
                .unwrap_or(0),
        }
    }

    /// The skip target of the last update.
    pub fn last_skip_rect(&self) -> Rect {
        self.skip_rect(self.viewport)
    }
}

/// Adapter over the project's own decoder (`crates/wc3-bink`).
///
/// Enabled by the `movie_decoder` feature. Video and container handling are
/// complete there; the MP3 audio decoder is still landing, so [`MovieDecoder::audio`]
/// returns [`MovieError::Unsupported`] in the meantime and the player reports
/// it via `CinematicPlayer::audio_error` rather than pretending silence is the
/// film's soundtrack.
#[cfg(feature = "movie_decoder")]
pub mod bink_decoder {
    use std::path::Path;

    use super::{MovieDecoder, MovieError};

    pub struct OwnDecoder {
        file: wc3_bink::BinkFile,
        /// Interleaved-sample cursor into the decoded track: `audio()` hands the
        /// mixer the samples for the frame that is playing.
        cursor: usize,
        audio_error: Option<String>,
    }

    impl OwnDecoder {
        pub fn open(path: &Path) -> Result<Self, MovieError> {
            let file = wc3_bink::BinkFile::open(path).map_err(|err| MovieError::Io(err.to_string()))?;
            Ok(Self {
                file,
                cursor: 0,
                audio_error: None,
            })
        }

        /// Container frame rate `(num, den)`, for the player's clock.
        pub fn fps(&self) -> f32 {
            let (num, den) = self.file.fps();
            if den == 0 {
                super::DEFAULT_MOVIE_FPS
            } else {
                num as f32 / den as f32
            }
        }

        pub fn duration_secs(&self) -> f64 {
            self.file.duration_secs()
        }
    }

    impl MovieDecoder for OwnDecoder {
        fn size(&self) -> (u32, u32) {
            self.file.size()
        }

        fn frame(&mut self, index: usize) -> Result<Vec<u8>, MovieError> {
            let frame = self
                .file
                .decode_frame(index)
                .map_err(|err| MovieError::Decode(err.to_string()))?;
            let rgba = self.file.frame_rgba(index);
            let expected = frame.width * frame.height * 4;
            if rgba.len() != expected {
                return Err(MovieError::Decode(format!(
                    "frame {index}: {} RGBA bytes, expected {expected}",
                    rgba.len()
                )));
            }
            Ok(rgba.to_vec())
        }

        fn sample_rate(&self) -> u32 {
            self.file
                .audio_info()
                .map(|audio| audio.sample_rate)
                .unwrap_or(0)
        }

        fn audio(&mut self, out: &mut Vec<i16>) -> Result<(), MovieError> {
            // The AVI carries MPEG-1 Layer III chunks; `wc3_bink` decodes the
            // whole track once (`BinkFile::audio_pcm`, cached) and this hands the
            // mixer one frame's worth of interleaved samples per call.
            if self.file.audio_info().is_none() {
                return Ok(());
            }
            let channels = self
                .file
                .audio_info()
                .map(|audio| audio.channels.max(1) as usize)
                .unwrap_or(2);
            let rate = self.file.audio_sample_rate().max(1) as f32;
            let per_frame = ((rate / self.fps().max(1.0)).round() as usize).max(1) * channels;
            let pcm = match self.file.audio_pcm() {
                Ok(pcm) => pcm,
                Err(err) => {
                    let message = err.to_string();
                    self.audio_error = Some(message.clone());
                    return Err(MovieError::Decode(message));
                }
            };
            if self.cursor >= pcm.len() {
                return Ok(());
            }
            let end = (self.cursor + per_frame).min(pcm.len());
            out.extend_from_slice(&pcm[self.cursor..end]);
            self.cursor = end;
            Ok(())
        }
    }
}

impl crate::Game {
    /// Starts the named movie: one of the nine
    /// [`KNOWN_MOVIES`] (`"HumanEd"`, `"Ghost"`, …).
    ///
    /// The file is resolved from `HEROFOLENDS_MOVIES` (colon-separated roots)
    /// and the standard install paths. A clean install without the movie files
    /// returns `Err(MovieError::Missing(name))`, whose `Display` is
    /// `MOVIE_MISSING <name>` — there is no fake playback.
    pub fn play_movie(&mut self, name: &str) -> Result<CinematicPlayer, MovieError> {
        let library = movies();
        let movie = library
            .find(name)
            .cloned()
            .ok_or_else(|| MovieError::Missing(name.trim().to_string()))?;
        Ok(CinematicPlayer::new(movie).start())
    }

    /// Starts a movie with the project's own decoder attached (`movie_decoder`
    /// feature): container + MPEG-4 video from `wc3-bink`, real frame rate from
    /// the header, audio as far as the decoder crate implements it.
    #[cfg(feature = "movie_decoder")]
    pub fn play_movie_decoded(&mut self, name: &str) -> Result<CinematicPlayer, MovieError> {
        let library = movies();
        let movie = library
            .find(name)
            .cloned()
            .ok_or_else(|| MovieError::Missing(name.trim().to_string()))?;
        let decoder = bink_decoder::OwnDecoder::open(&movie.path)?;
        let fps = decoder.fps();
        let duration = decoder.duration_secs() as f32;
        let mut player = CinematicPlayer::new(movie)
            .with_decoder(Box::new(decoder))
            .with_frame_rate(fps)
            .start();
        if duration.is_finite() && duration > 0.0 {
            player.set_duration(duration);
        }
        Ok(player)
    }

    /// The nine movie names with presence flags.
    pub fn available_movies(&self) -> Vec<(&'static str, bool)> {
        movies().availability()
    }

    /// Starts the movie a campaign mission asked for (`PlayCinematic`) on this
    /// thread and decodes it with the project's own codec.
    ///
    /// A name the install does not carry logs exactly `MOVIE_MISSING <name>` and
    /// returns the typed error: the mission keeps running, the cutscene is
    /// simply absent. A decode failure logs `MOVIE_DECODE …` the same way.
    pub fn cinematic_start(&mut self, name: &str) -> Result<(), MovieError> {
        match self.open_scripted_movie(name) {
            Ok(player) => {
                let duration = player.duration();
                let path = player.movie.path.clone();
                let bytes = player.movie.bytes;
                let label = player.movie.name.clone();
                crate::dev_log!(
                    "MOVIE_PLAY name={} duration={:.2} bytes={} path={}",
                    label,
                    duration.unwrap_or(0.0),
                    bytes,
                    path.display()
                );
                ACTIVE_MOVIE.with(|slot| *slot.borrow_mut() = Some(player));
                Ok(())
            }
            Err(error) => {
                crate::dev_log!("{error}");
                Err(error)
            }
        }
    }

    /// `play_movie_decoded` with the build's decoder configuration: the own
    /// codec when the `movie_decoder` feature is on, the container-only player
    /// otherwise.
    #[cfg(feature = "movie_decoder")]
    fn open_scripted_movie(&mut self, name: &str) -> Result<CinematicPlayer, MovieError> {
        self.play_movie_decoded(name)
    }

    #[cfg(not(feature = "movie_decoder"))]
    fn open_scripted_movie(&mut self, name: &str) -> Result<CinematicPlayer, MovieError> {
        self.play_movie(name)
    }

    /// A scripted movie is on screen.
    pub fn cinematic_active(&self) -> bool {
        ACTIVE_MOVIE.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|player| player.state().is_active())
        })
    }

    /// Advances the scripted movie and returns the frame it is showing.
    pub fn cinematic_frame(&mut self, dt: f32) -> Option<VideoFrame> {
        ACTIVE_MOVIE.with(|cell| {
            let mut guard = cell.borrow_mut();
            let (finished, frame, name) = {
                let Some(player) = guard.as_mut() else {
                    return None;
                };
                let finished = player.update(dt, None, MOVIE_VIEWPORT);
                (
                    finished,
                    player.current_frame().cloned(),
                    player.movie.name.clone(),
                )
            };
            if finished {
                *guard = None;
                crate::dev_log!("MOVIE_END name={name}");
            }
            frame
        })
    }

    /// What the renderer should draw for the running movie.
    pub fn cinematic_surface(&self) -> Option<VideoSurface> {
        ACTIVE_MOVIE.with(|slot| slot.borrow().as_ref().map(|player| player.surface(MOVIE_VIEWPORT)))
    }

    /// Ends the scripted movie now (Skip tap, mission end).
    pub fn cinematic_stop(&mut self) {
        ACTIVE_MOVIE.with(|slot| {
            if let Some(player) = slot.borrow_mut().take() {
                crate::dev_log!("MOVIE_SKIP name={}", player.movie.name);
            }
        });
    }

    /// Camera/scene events the in-game map engine recorded so far.
    pub fn engine_camera_events(&self) -> &[wc3_re::jass::vm::CameraEvent] {
        self.engine
            .as_ref()
            .and_then(|engine| engine.runtime())
            .map(|runtime| runtime.world.camera_events.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> MovieFile {
        MovieFile {
            name: "HumanEd".into(),
            path: PathBuf::from("/nonexistent/HumanEd.mpq"),
            bytes: 20_172_800,
            size: Some((800, 368)),
            duration_s: None,
        }
    }

    #[test]
    fn cinematic_fade_in_then_play() {
        let mut player = CinematicPlayer::new(movie()).start();
        assert_eq!(player.state(), CinematicState::FadeIn);
        let mut finished = false;
        for _ in 0..60 {
            finished |= player.update(1.0 / 60.0, None, Vec2::new(1920.0, 1080.0));
        }
        assert!(!finished);
        assert_eq!(player.state(), CinematicState::Playing);
        assert!((player.opacity() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn cinematic_skip_target_is_at_least_126px() {
        let player = CinematicPlayer::new(movie());
        for viewport in [
            Vec2::new(2400.0, 1080.0),
            Vec2::new(1080.0, 2400.0),
            Vec2::new(800.0, 600.0),
            Vec2::new(320.0, 240.0),
        ] {
            let rect = player.skip_rect(viewport);
            assert!(
                rect.width() >= SKIP_MIN_PX && rect.height() >= SKIP_MIN_PX,
                "viewport {viewport:?} rect {rect:?}"
            );
            assert!(rect.max.x <= viewport.x + 1e-3 && rect.max.y <= viewport.y + 1e-3);
        }
    }

    #[test]
    fn cinematic_skip_tap_inside_target_fades_out() {
        let viewport = Vec2::new(1920.0, 1080.0);
        let mut player = CinematicPlayer::new(movie()).start();
        for _ in 0..30 {
            player.update(1.0 / 60.0, None, viewport);
        }
        let rect = player.skip_rect(viewport);
        let centre = (rect.min + rect.max) * 0.5;
        let finished = player.update(1.0 / 60.0, Some(SkipTap { position: centre }), viewport);
        assert!(!finished);
        assert!(player.was_skipped());
        assert_eq!(player.state(), CinematicState::Skipping);
        let mut done = false;
        for _ in 0..120 {
            done |= player.update(1.0 / 60.0, None, viewport);
        }
        assert!(done);
        assert_eq!(player.state(), CinematicState::Finished);
    }

    #[test]
    fn cinematic_tap_outside_target_does_not_skip() {
        let viewport = Vec2::new(1920.0, 1080.0);
        let mut player = CinematicPlayer::new(movie()).start();
        for _ in 0..60 {
            player.update(1.0 / 60.0, None, viewport);
        }
        player.update(
            1.0 / 60.0,
            Some(SkipTap {
                position: Vec2::new(40.0, 40.0),
            }),
            viewport,
        );
        assert!(!player.was_skipped());
        assert_eq!(player.state(), CinematicState::Playing);
    }

    #[test]
    fn cinematic_duration_end_triggers_fade_out() {
        let viewport = Vec2::new(1920.0, 1080.0);
        let mut player = CinematicPlayer::new(movie()).start();
        player.set_duration(1.0);
        let mut done = false;
        for _ in 0..180 {
            done |= player.update(1.0 / 60.0, None, viewport);
            if done {
                break;
            }
        }
        assert!(done, "a one-second movie must finish within three seconds");
        assert!(!player.was_skipped());
    }

    #[test]
    fn cinematic_decoder_drives_frames_and_audio() {
        let viewport = Vec2::new(1920.0, 1080.0);
        let decoder = TestPatternDecoder::new(320, 200, 60, 24.0);
        let mut player = CinematicPlayer::new(movie())
            .with_decoder(Box::new(decoder))
            .with_frame_rate(24.0)
            .start();
        for _ in 0..60 {
            player.update(1.0 / 60.0, None, viewport);
        }
        assert!(
            (23..=24).contains(&player.decoded_frames()),
            "decoded {} frames in one second at 24 fps",
            player.decoded_frames()
        );
        assert!(player.audio_samples() > 0, "PCM must reach the mixer");
        let frame = player.current_frame().expect("frame decoded");
        assert_eq!((frame.width, frame.height), (320, 200));
        assert_eq!(frame.rgba.len(), 320 * 200 * 4);
        assert_eq!(frame.stride(), 320 * 4);
        let surface = player.surface(viewport);
        assert_eq!(surface.frame_size, Some((320, 200)));
        assert!(surface.skip_rect.width() >= SKIP_MIN_PX);
    }

    #[test]
    fn cinematic_decoder_end_of_stream_finishes() {
        let viewport = Vec2::new(1920.0, 1080.0);
        let decoder = TestPatternDecoder::new(64, 64, 5, 24.0);
        let mut player = CinematicPlayer::new(movie())
            .with_decoder(Box::new(decoder))
            .with_frame_rate(24.0)
            .start();
        let mut done = false;
        for _ in 0..120 {
            done |= player.update(1.0 / 60.0, None, viewport);
            if done {
                break;
            }
        }
        assert!(done, "five frames must finish inside two seconds");
        assert_eq!(player.decoded_frames(), 5);
    }

    #[test]
    fn cinematic_library_resolves_known_names_and_reports_missing() {
        let dir = std::env::temp_dir().join(format!("h2-movies-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("HumanEd.mpq"), b"RIFF").expect("write");
        std::fs::write(dir.join("Ghost.blz"), b"RIFF").expect("write");
        std::fs::write(dir.join("unrelated.mpq"), b"RIFF").expect("write");
        let library = MovieLibrary::scan(&[dir.clone()]);
        assert!(library.find("HumanEd").is_some());
        assert!(library.find("humanEd.mpq").is_some());
        assert!(library.find("Ghost").is_some());
        assert!(library.find("OrcEd").is_none());
        let missing = &library.missing;
        assert!(missing.iter().any(|name| name == "OrcEd"));
        assert!(!missing.iter().any(|name| name == "HumanEd"));
        let availability = library.availability();
        assert_eq!(availability.len(), KNOWN_MOVIES.len());
        assert!(availability
            .iter()
            .any(|(name, present)| *name == "Ghost" && *present));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cinematic_missing_movie_displays_contract_line() {
        let error = MovieError::Missing("OrcEd".into());
        assert_eq!(error.to_string(), "MOVIE_MISSING OrcEd");
        assert!(error.is_missing());
    }
}
