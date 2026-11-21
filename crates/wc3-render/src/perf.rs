//! Frame instrumentation: CPU scopes, GPU timestamp queries, allocation traffic
//! counters and renderer state counters.
//!
//! The renderer hot path keeps every number here on the stack (no allocation),
//! so the counters can stay enabled in a release build and a bench run measures
//! the real renderer instead of a special instrumented copy.
//!
//! GPU timing uses `TIMESTAMP_QUERY` + `TIMESTAMP_QUERY_INSIDE_ENCODERS`. Both
//! are optional: [`GpuTimers::new`] returns `None` when the adapter does not
//! support them, and the bench then reports CPU time plus counters only.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ---------------------------------------------------------------- allocator

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static FREE_CALLS: AtomicU64 = AtomicU64::new(0);

/// Global allocator wrapper that counts allocation calls and requested bytes.
///
/// Install it in a *binary* (`#[global_allocator]`), never in a library, and
/// read it with [`alloc_traffic`]. Counting is two relaxed atomics per call:
/// cheap enough to leave on for a whole bench run, accurate enough to prove
/// that a frame path allocates or does not.
pub struct CountingAlloc;

// SAFETY: every method forwards to `System` with the same layout/contract.
unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        FREE_CALLS.fetch_add(1, Ordering::Relaxed);
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        // A realloc is new allocation traffic: the old block may be copied.
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        FREE_CALLS.fetch_add(1, Ordering::Relaxed);
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: CountingAlloc = CountingAlloc;

/// Allocator traffic since process start: `(calls, bytes, frees)`.
pub fn alloc_traffic() -> (u64, u64, u64) {
    (
        ALLOC_CALLS.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
        FREE_CALLS.load(Ordering::Relaxed),
    )
}

/// Resident set size in KiB, read from `/proc/self/status` (0 elsewhere).
pub fn rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().trim_end_matches(" kB").parse().unwrap_or(0);
        }
    }
    0
}

/// Peak resident set size in KiB (`VmHWM`), 0 elsewhere.
pub fn peak_rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest.trim().trim_end_matches(" kB").parse().unwrap_or(0);
        }
    }
    0
}

// ---------------------------------------------------------------- counters

/// Per-frame renderer work. All plain integers: copying this struct is free and
/// it is reset (not reallocated) every frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    /// `draw_indexed` + `draw` calls submitted.
    pub draws: u32,
    /// `set_pipeline` calls that actually changed the bound pipeline.
    pub pipeline_binds: u32,
    /// `set_bind_group(1, ..)` calls that actually changed the bound texture.
    pub texture_binds: u32,
    pub vertices: u64,
    pub indices: u64,
    /// Bytes written to GPU buffers this frame (`queue.write_buffer` payload).
    pub upload_bytes: u64,
    pub buffer_writes: u32,
    /// Times a dynamic buffer had to allocate a bigger GPU buffer.
    pub buffer_grows: u32,
    /// Bytes allocated on the CPU heap during the frame (see [`alloc_traffic`]).
    pub cpu_alloc_bytes: u64,
    pub cpu_alloc_calls: u64,
    /// Passes recorded (world, ui, ...).
    pub passes: u32,
    /// Wall time spent building/uploading the frame (CPU), microseconds.
    pub prepare_micros: u64,
    /// Wall time spent recording draw calls (CPU), microseconds.
    pub draw_micros: u64,
    /// Breakdown of `prepare`: build the draw list, upload, sort.
    pub build_micros: u64,
    pub upload_micros: u64,
    pub sort_micros: u64,
}

impl Counters {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn merge(&mut self, other: &Self) {
        self.draws += other.draws;
        self.pipeline_binds += other.pipeline_binds;
        self.texture_binds += other.texture_binds;
        self.vertices += other.vertices;
        self.indices += other.indices;
        self.upload_bytes += other.upload_bytes;
        self.buffer_writes += other.buffer_writes;
        self.buffer_grows += other.buffer_grows;
        self.cpu_alloc_bytes += other.cpu_alloc_bytes;
        self.cpu_alloc_calls += other.cpu_alloc_calls;
        self.passes += other.passes;
        self.prepare_micros += other.prepare_micros;
        self.draw_micros += other.draw_micros;
        self.build_micros += other.build_micros;
        self.upload_micros += other.upload_micros;
        self.sort_micros += other.sort_micros;
    }
}

/// CPU wall time of one scope, in milliseconds.
///
/// The scope holds a real borrow of the destination, so the compiler enforces
/// the lifetime the old `*mut f32` erased: a scope cannot outlive the `f32` it
/// writes on drop.
pub struct CpuScope<'a> {
    label: &'static str,
    start: Instant,
    out: &'a mut f32,
}

impl<'a> CpuScope<'a> {
    /// Times a scope and stores the elapsed milliseconds into `out`.
    pub fn new(label: &'static str, out: &'a mut f32) -> Self {
        Self {
            label,
            start: Instant::now(),
            out,
        }
    }

    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl Drop for CpuScope<'_> {
    fn drop(&mut self) {
        *self.out = self.start.elapsed().as_secs_f64() as f32 * 1000.0;
    }
}

// ------------------------------------------------------------------- GPU

/// How many timestamp slots a frame may use: start, world end, ui end.
pub const GPU_SLOTS_PER_FRAME: u32 = 8;
/// Resolve blocks are 256-byte aligned (`QUERY_RESOLVE_BUFFER_ALIGNMENT`), so the
/// readback buffer gives every frame its own 256-byte block.
const GPU_BLOCK: u64 = 256;

/// Batched GPU timestamp collection.
///
/// Timestamps are written into one query set per frame block and resolved into a
/// single readback buffer. The buffer is mapped once per batch, so a 120-frame
/// measurement costs one stall at the end instead of one per frame.
pub struct GpuTimers {
    device: wgpu::Device,
    query_set: wgpu::QuerySet,
    /// `QUERY_RESOLVE` destination; cannot be host-visible on its own.
    resolve: wgpu::Buffer,
    /// `MAP_READ` staging copy of `resolve`.
    readback: wgpu::Buffer,
    period_ns: f32,
    frames_per_batch: u32,
    frame_in_batch: u32,
    /// Milliseconds per frame for finished frames, in submission order.
    samples: Vec<f32>,
    open: bool,
    staging: Vec<u8>,
}

/// Handle for the timestamp block of one frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameSlot {
    pub frame: u32,
    pub base: u32,
}

impl GpuTimers {
    /// `None` when the adapter lacks timestamp queries: the bench degrades to CPU
    /// timing and says so, instead of reporting invented GPU numbers.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, frames_per_batch: u32) -> Option<Self> {
        let required = wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !device.features().contains(required) {
            return None;
        }
        let frames_per_batch = frames_per_batch.clamp(1, 4096);
        let count = frames_per_batch * GPU_SLOTS_PER_FRAME;
        // WebGPU's portable minimum is 4096 timestamps per set; stay inside it.
        if count == 0 || count > 4096 {
            return None;
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("perf-timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count,
        });
        let size = frames_per_batch as u64 * GPU_BLOCK;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perf-timestamps-resolve"),
            size,
            usage: wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::QUERY_RESOLVE,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perf-timestamps-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Some(Self {
            device: device.clone(),
            query_set,
            resolve,
            readback,
            period_ns: queue.get_timestamp_period(),
            frames_per_batch,
            frame_in_batch: 0,
            samples: Vec::with_capacity(frames_per_batch as usize),
            open: false,
            staging: Vec::new(),
        })
    }

    pub fn period_ns(&self) -> f32 {
        self.period_ns
    }

    /// True once the current batch is full and must be [`flush`](Self::flush)ed.
    pub fn batch_full(&self) -> bool {
        self.frame_in_batch >= self.frames_per_batch
    }

    pub fn frames_in_batch(&self) -> u32 {
        self.frame_in_batch
    }

    /// Reserve the timestamp block for the next frame.
    pub fn open_frame(&mut self, queue: &wgpu::Queue) -> FrameSlot {
        debug_assert!(!self.open, "previous frame was not resolved");
        let frame = self.frame_in_batch;
        self.frame_in_batch += 1;
        self.open = true;
        let base = frame * GPU_SLOTS_PER_FRAME;
        // Submission-order marker before the frame's own command buffers.
        let mut encoder = self
            .query_set_encoder(&format!("perf-open-{frame}"));
        encoder.write_timestamp(&self.query_set, base);
        queue.submit(Some(encoder.finish()));
        FrameSlot { frame, base }
    }

    /// Write a mid-frame timestamp (call inside the encoder that records passes).
    pub fn mark(&self, encoder: &mut wgpu::CommandEncoder, slot: FrameSlot, index: u32) {
        if index < GPU_SLOTS_PER_FRAME {
            encoder.write_timestamp(&self.query_set, slot.base + index);
        }
    }

    /// Close the frame: end timestamp + resolve, submitted in order after the
    /// frame's own command buffers.
    pub fn close_frame(&mut self, queue: &wgpu::Queue, slot: FrameSlot) {
        let offset = slot.frame as u64 * GPU_BLOCK;
        let mut encoder = self.query_set_encoder(&format!("perf-close-{}", slot.frame));
        encoder.write_timestamp(&self.query_set, slot.base + 1);
        encoder.resolve_query_set(
            &self.query_set,
            slot.base..slot.base + 2,
            &self.resolve,
            offset,
        );
        queue.submit(Some(encoder.finish()));
        self.open = false;
    }

    fn query_set_encoder(&self, label: &str) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(label),
            })
    }

    /// Map the batch, read every finished frame and return their GPU milliseconds.
    pub fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<f32> {
        if self.frame_in_batch == 0 {
            return Vec::new();
        }
        let frames = self.frame_in_batch;
        let bytes = (frames as u64 * GPU_BLOCK) as usize;
        let mut copy = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("perf-timestamps-copy"),
            });
        copy.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, bytes as u64);
        queue.submit(Some(copy.finish()));
        let slice = self.readback.slice(..bytes as u64);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        if receiver.recv().map(|r| r.is_ok()).unwrap_or(false) {
            if let Ok(mapped) = slice.get_mapped_range() {
                self.staging.clear();
                self.staging.extend_from_slice(&mapped);
            }
            self.samples.clear();
            for frame in 0..frames as usize {
                let base = frame * GPU_BLOCK as usize;
                let start = read_u64(&self.staging, base);
                let end = read_u64(&self.staging, base + 8);
                let ns = end.saturating_sub(start) as f64 * self.period_ns as f64;
                self.samples.push((ns / 1.0e6) as f32);
            }
        }
        self.readback.unmap();
        self.frame_in_batch = 0;
        std::mem::take(&mut self.samples)
    }

    /// Flush any partial batch (end of run).
    pub fn finish(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<f32> {
        self.flush(device, queue)
    }
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut raw = [0u8; 8];
    if offset + 8 <= bytes.len() {
        raw.copy_from_slice(&bytes[offset..offset + 8]);
    }
    u64::from_le_bytes(raw)
}

// ---------------------------------------------------------------- statistics

/// Summary of a metric series.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub count: usize,
    pub mean: f32,
    pub min: f32,
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

impl Stats {
    pub fn from_samples(samples: &[f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let pick = |q: f32| -> f32 {
            let index = ((sorted.len() - 1) as f32 * q).round() as usize;
            sorted[index.min(sorted.len() - 1)]
        };
        let sum: f32 = sorted.iter().sum();
        Self {
            count: sorted.len(),
            mean: sum / sorted.len() as f32,
            min: sorted[0],
            p50: pick(0.50),
            p95: pick(0.95),
            p99: pick(0.99),
            max: sorted[sorted.len() - 1],
        }
    }

    /// Budget verdict for a frame-time series: how much of the frame budget is
    /// used at p95, and whether the 1% worst frames still fit.
    pub fn budget(&self, budget_ms: f32) -> BudgetVerdict {
        BudgetVerdict {
            budget_ms,
            p95_used_pct: if budget_ms > 0.0 {
                self.p95 / budget_ms * 100.0
            } else {
                0.0
            },
            p99_used_pct: if budget_ms > 0.0 {
                self.p99 / budget_ms * 100.0
            } else {
                0.0
            },
            headroom_ms: budget_ms - self.p99,
            max_fps: if self.max > 0.0 { 1000.0 / self.max } else { 0.0 },
            sustained_fps: if self.p95 > 0.0 { 1000.0 / self.p95 } else { 0.0 },
        }
    }

    /// Values without a key, for embedding in a larger JSON object.
    pub fn values_json(&self) -> String {
        format!(
            "{{\"n\":{},\"mean\":{:.3},\"min\":{:.3},\"p50\":{:.3},\"p95\":{:.3},\"p99\":{:.3},\"max\":{:.3}}}",
            self.count, self.mean, self.min, self.p50, self.p95, self.p99, self.max
        )
    }

    pub fn to_json(&self, key: &str) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "\"{key}\":{{\"n\":{},\"mean\":{:.3},\"min\":{:.3},\"p50\":{:.3},\"p95\":{:.3},\"p99\":{:.3},\"max\":{:.3}}}",
            self.count, self.mean, self.min, self.p50, self.p95, self.p99, self.max
        );
        out
    }
}

/// Frame-budget arithmetic for a target refresh rate.
#[derive(Debug, Default, Clone, Copy)]
pub struct BudgetVerdict {
    pub budget_ms: f32,
    pub p95_used_pct: f32,
    pub p99_used_pct: f32,
    pub headroom_ms: f32,
    pub max_fps: f32,
    pub sustained_fps: f32,
}

impl BudgetVerdict {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"budget_ms\":{:.3},\"p95_used_pct\":{:.1},\"p99_used_pct\":{:.1},\"headroom_ms\":{:.3},\"max_fps\":{:.1},\"sustained_fps\":{:.1}}}",
            self.budget_ms, self.p95_used_pct, self.p99_used_pct, self.headroom_ms, self.max_fps,
            self.sustained_fps
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_percentiles_and_budget() {
        let samples: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        let stats = Stats::from_samples(&samples);
        assert_eq!(stats.count, 100);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 100.0);
        assert_eq!(stats.p50, 51.0);
        // Nearest-rank percentile over 100 samples: the 99th value.
        assert_eq!(stats.p99, 99.0);
        let verdict = stats.budget(16.6);
        assert!(verdict.p95_used_pct > 570.0 && verdict.p95_used_pct < 580.0);
        assert!(verdict.headroom_ms < 0.0);
    }

    #[test]
    fn counters_merge() {
        let mut a = Counters {
            draws: 1,
            vertices: 3,
            ..Counters::default()
        };
        let b = Counters {
            draws: 2,
            upload_bytes: 64,
            ..Counters::default()
        };
        a.merge(&b);
        assert_eq!(a.draws, 3);
        assert_eq!(a.vertices, 3);
        assert_eq!(a.upload_bytes, 64);
    }

    #[test]
    fn alloc_counter_tracks_traffic() {
        let (calls, bytes, _) = alloc_traffic();
        let v: Vec<u8> = vec![7u8; 4096];
        let (calls2, bytes2, _) = alloc_traffic();
        assert!(calls2 > calls);
        assert!(bytes2 >= bytes + 4096);
        assert_eq!(v.len(), 4096);
    }

    #[test]
    fn cpu_scope_records_ms() {
        let mut ms = 0.0f32;
        {
            let _scope = CpuScope::new("test", &mut ms);
        }
        assert!(ms >= 0.0);
        assert!(ms < 1000.0);
    }
}
