//! Per-frame scratch storage that does not allocate.
//!
//! Every renderer pass needs temporary CPU storage (skinned vertices, index
//! ranges, draw lists) and a dynamic GPU buffer for the result. The baseline
//! renderer allocated fresh `Vec`s and uploaded them every frame; this module
//! keeps the `Vec` capacity and the GPU buffer across frames and only grows on a
//! new high-water mark.
//!
//! The types here are deliberately dumb: no interior mutability, no global
//! state, no `unsafe`. They are sized for "clear, fill, upload, draw" per frame.

/// Reusable vector that keeps its capacity for the next frame.
#[derive(Debug)]
pub struct Scratch<T> {
    items: Vec<T>,
    /// Largest length ever recorded: the value a budget check cares about.
    high_water: usize,
}

impl<T> Default for Scratch<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Scratch<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            high_water: 0,
        }
    }

    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Drop the logical length but keep the allocation.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn high_water(&self) -> usize {
        self.high_water
    }

    pub fn capacity(&self) -> usize {
        self.items.capacity()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items
    }

    pub fn push(&mut self, value: T) {
        self.items.push(value);
        self.note_len();
    }

    /// Reserve up-front from a known upper bound (scene planning) so the fill
    /// loop never reallocates.
    pub fn reserve(&mut self, additional: usize) {
        if self.items.capacity() < self.items.len() + additional {
            self.items.reserve(additional);
        }
    }

    fn note_len(&mut self) {
        if self.items.len() > self.high_water {
            self.high_water = self.items.len();
        }
    }
}

impl<T: Copy> Scratch<T> {
    pub fn extend_from_slice(&mut self, values: &[T]) {
        self.items.extend_from_slice(values);
        self.note_len();
    }

    /// Grow to `len` filled with `value` (used for fixed-size scratch like a
    /// per-unit matrix palette).
    pub fn resize(&mut self, len: usize, value: T) {
        self.items.resize(len, value);
        self.note_len();
    }
}

/// A GPU buffer that is rewritten every frame.
///
/// Grows on a new high-water mark (never shrinks), so the steady state is one
/// `queue.write_buffer` per frame and no allocation at all.
pub struct DynamicBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
    high_water: u64,
    pub writes: u32,
    pub bytes: u64,
    pub grows: u32,
}

impl DynamicBuffer {
    pub fn new(
        device: &wgpu::Device,
        bytes: u64,
        usage: wgpu::BufferUsages,
        label: &'static str,
    ) -> Self {
        let capacity = bytes.max(256);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity,
            usage,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity,
            usage,
            label,
            high_water: 0,
            writes: 0,
            bytes: 0,
            grows: 0,
        }
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn high_water(&self) -> u64 {
        self.high_water
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Reset the per-frame counters (call once per frame).
    pub fn begin_frame(&mut self) {
        self.writes = 0;
        self.bytes = 0;
        self.grows = 0;
    }

    /// Upload `data` at offset 0. Returns false when the payload was empty.
    pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }
        self.ensure_capacity(device, data.len() as u64);
        queue.write_buffer(&self.buffer, 0, data);
        self.writes += 1;
        self.bytes += data.len() as u64;
        self.high_water = self.high_water.max(data.len() as u64);
        true
    }

    /// Grow to fit `needed`, doubling from the current high-water mark so the
    /// steady state reaches zero growth after a couple of frames.
    pub fn ensure_capacity(&mut self, device: &wgpu::Device, needed: u64) {
        if needed <= self.capacity {
            return;
        }
        let mut capacity = self.capacity.max(4096);
        let target = (self.high_water.max(needed) * 3 / 2).max(needed);
        while capacity < target {
            capacity *= 2;
        }
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: capacity,
            usage: self.usage,
            mapped_at_creation: false,
        });
        self.capacity = capacity;
        self.grows += 1;
    }
}

/// Dynamic uniform buffer with per-draw 256-byte-aligned slots.
///
/// Replaces "one vertex colour per duplicated vertex" with "one 96-byte uniform
/// per draw": the layer's world matrix and tint live here, so a geoset with
/// three material layers uploads its skinned vertices once instead of three
/// times.
pub struct UniformRing {
    buffer: wgpu::Buffer,
    capacity: u64,
    alignment: u64,
    slot_size: u64,
    cursor: u64,
    high_water: u64,
    pub writes: u32,
    pub bytes: u64,
    pub grows: u32,
}

impl UniformRing {
    pub fn new(device: &wgpu::Device, slot_size: u64, slots: u64, label: &'static str) -> Self {
        let alignment = device.limits().min_uniform_buffer_offset_alignment.max(16) as u64;
        let slot_size = slot_size.max(16).div_ceil(alignment) * alignment;
        let capacity = (slot_size * slots.max(1)).max(4096);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity,
            alignment,
            slot_size,
            cursor: 0,
            high_water: 0,
            writes: 0,
            bytes: 0,
            grows: 0,
        }
    }

    pub fn slot_stride(&self) -> u64 {
        self.slot_size
    }

    pub fn alignment(&self) -> u64 {
        self.alignment
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn begin_frame(&mut self) {
        self.cursor = 0;
        self.writes = 0;
        self.bytes = 0;
        self.grows = 0;
    }

    pub fn used_slots(&self) -> u64 {
        self.cursor
    }

    /// Append one slot and return its dynamic offset.
    pub fn push(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
    ) -> u32 {
        debug_assert!(data.len() as u64 <= self.slot_size);
        let offset = self.cursor * self.slot_size;
        self.ensure_capacity(device, offset + self.slot_size);
        queue.write_buffer(&self.buffer, offset, data);
        self.cursor += 1;
        self.writes += 1;
        self.bytes += data.len() as u64;
        self.high_water = self.high_water.max(self.cursor);
        offset as u32
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, needed: u64) {
        if needed <= self.capacity {
            return;
        }
        let mut capacity = self.capacity.max(4096);
        let target = (needed * 3 / 2).max(needed);
        while capacity < target {
            capacity *= 2;
        }
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("perf-uniform-ring"),
            size: capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = capacity;
        self.grows += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_keeps_capacity_across_clear() {
        let mut scratch: Scratch<u32> = Scratch::with_capacity(16);
        for i in 0..16 {
            scratch.push(i);
        }
        let capacity = scratch.capacity();
        assert!(capacity >= 16);
        assert_eq!(scratch.high_water(), 16);
        scratch.clear();
        assert_eq!(scratch.len(), 0);
        assert_eq!(scratch.capacity(), capacity);
        for i in 0..8 {
            scratch.push(i);
        }
        assert_eq!(scratch.high_water(), 16);
    }

    #[test]
    fn scratch_reserve_is_stable() {
        let mut scratch: Scratch<u64> = Scratch::new();
        scratch.reserve(1024);
        let capacity = scratch.capacity();
        for i in 0..1024 {
            scratch.push(i);
        }
        assert_eq!(scratch.capacity(), capacity, "no growth after reserve");
    }
}
