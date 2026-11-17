//! Resident-set management: byte budgets, LRU eviction, texture residency plan.
//!
//! `PARITY_MATRIX.md` rows #1/#2 require every `UnitDef` to render its own MDX
//! (542 models in the operator's install) while staying inside a phone memory
//! budget. A renderer cannot keep every model's GPU buffers resident, so it needs
//! a resident set with an exact byte budget, LRU eviction and a re-residency path.
//!
//! * [`ResidentSet`] is the policy: one entry per key, exact byte accounting,
//!   eviction of the least recently used entry before an insert would exceed the
//!   budget. It is generic over the payload, so the renderer stores GPU buffers
//!   and the tests store byte counts — the policy is the same code.
//! * [`ResidentModel`] is the renderer payload: one vertex + one index buffer per
//!   model plus its geoset ranges.
//! * [`TexturePlan`] answers the texture half: how many textures of a given size
//!   fit as RGBA8 vs ASTC, which decides whether the whole install's textures can
//!   stay resident.
//!
//! Integration into `MeshRenderer` (model id -> resident payload) is a small
//! patch, kept in `analysis/tech-lead/merge-gpu.md` so it can land when the crate
//! has a single writer (see the coordination note there).

use std::collections::HashMap;

use crate::texture::{CompressedFormat, CompressionSupport};

/// Per-frame residency counters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResidencyStats {
    pub resident: usize,
    pub bytes: u64,
    pub budget_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub loads: u64,
    pub evictions: u64,
    /// Requests refused because a single entry is larger than the budget.
    pub rejected: u64,
}

impl ResidencyStats {
    pub fn utilisation(&self) -> f32 {
        if self.budget_bytes == 0 {
            0.0
        } else {
            self.bytes as f32 / self.budget_bytes as f32
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "resident={} bytes={} budget={} ({:.0}%) loads={} evictions={} hits={} misses={} rejected={}",
            self.resident,
            self.bytes,
            self.budget_bytes,
            self.utilisation() * 100.0,
            self.loads,
            self.evictions,
            self.hits,
            self.misses,
            self.rejected
        )
    }
}

struct Entry<T> {
    payload: T,
    bytes: u64,
    last_used: u64,
}

/// LRU resident set with a hard byte budget.
///
/// Invariant: `stats.bytes <= budget_bytes` after every operation (an entry that
/// cannot fit on its own is refused rather than allowed to break the budget).
pub struct ResidentSet<T> {
    entries: HashMap<u32, Entry<T>>,
    clock: u64,
    budget_bytes: u64,
    stats: ResidencyStats,
}

impl<T> ResidentSet<T> {
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            budget_bytes,
            stats: ResidencyStats {
                budget_bytes,
                ..ResidencyStats::default()
            },
        }
    }

    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Change the budget; call [`enforce_budget`](Self::enforce_budget) to apply.
    pub fn set_budget(&mut self, budget_bytes: u64) {
        self.budget_bytes = budget_bytes;
        self.stats.budget_bytes = budget_bytes;
    }

    pub fn stats(&self) -> ResidencyStats {
        self.stats
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> u64 {
        self.stats.bytes
    }

    pub fn contains(&self, key: u32) -> bool {
        self.entries.contains_key(&key)
    }

    /// Advance the frame clock once per frame.
    pub fn begin_frame(&mut self) {
        self.clock = self.clock.wrapping_add(1);
    }

    /// Look `key` up and mark it most recently used.
    pub fn get(&mut self, key: u32) -> Option<&T> {
        let clock = self.clock;
        match self.entries.get_mut(&key) {
            Some(entry) => {
                entry.last_used = clock;
                self.stats.hits += 1;
                Some(&entry.payload)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Insert (or replace) an entry, evicting LRU entries first if needed.
    ///
    /// Returns `false` when the entry cannot fit the budget at all; the set is
    /// unchanged in that case.
    pub fn insert(&mut self, key: u32, payload: T, bytes: u64) -> bool {
        if bytes > self.budget_bytes {
            self.stats.rejected += 1;
            return false;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.stats.bytes -= old.bytes;
        }
        while self.stats.bytes + bytes > self.budget_bytes {
            if self.evict_one().is_none() {
                self.stats.rejected += 1;
                return false;
            }
        }
        let clock = self.clock;
        self.entries.insert(
            key,
            Entry {
                payload,
                bytes,
                last_used: clock,
            },
        );
        self.stats.bytes += bytes;
        self.stats.loads += 1;
        self.stats.resident = self.entries.len();
        true
    }

    /// Evict the least recently used entry and return its key.
    pub fn evict_one(&mut self) -> Option<u32> {
        let key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)?;
        let entry = self.entries.remove(&key)?;
        self.stats.bytes -= entry.bytes;
        self.stats.evictions += 1;
        self.stats.resident = self.entries.len();
        Some(key)
    }

    /// Evict until the resident bytes fit the budget. Returns the eviction count.
    pub fn enforce_budget(&mut self) -> usize {
        let mut evicted = 0;
        while self.stats.bytes > self.budget_bytes {
            if self.evict_one().is_none() {
                break;
            }
            evicted += 1;
        }
        evicted
    }

    /// Drop everything (used when the model set changes).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.bytes = 0;
        self.stats.resident = 0;
    }
}

/// GPU geometry of one resident model.
pub struct ResidentModel {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    /// `(index_start, index_count)` per geoset.
    pub geosets: Vec<(u32, u32)>,
    /// Bytes held by the two buffers (the budget unit).
    pub bytes: u64,
}

impl ResidentModel {
    /// Upload one model's already-packed vertex/index streams.
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[u8],
        indices: &[u8],
        geosets: Vec<(u32, u32)>,
    ) -> Self {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cache-model-vertices"),
            size: vertices.len().max(4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            queue.write_buffer(&vertex_buffer, 0, vertices);
        }
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cache-model-indices"),
            size: indices.len().max(4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !indices.is_empty() {
            queue.write_buffer(&index_buffer, 0, indices);
        }
        Self {
            bytes: vertex_buffer.size() + index_buffer.size(),
            vertices: vertex_buffer,
            indices: index_buffer,
            geosets,
        }
    }
}

/// Texture residency plan for one asset set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexturePlan {
    pub textures: usize,
    pub rgba_bytes: u64,
    pub astc6_bytes: u64,
    pub astc8_bytes: u64,
    pub budget_bytes: u64,
}

impl TexturePlan {
    pub fn new(textures: usize, width: u32, height: u32, budget_bytes: u64) -> Self {
        Self {
            textures,
            rgba_bytes: crate::texture::mip_bytes(width, height, 32),
            astc6_bytes: crate::texture::astc_chain_bytes(width, height, 6, 6),
            astc8_bytes: crate::texture::astc_chain_bytes(width, height, 8, 8),
            budget_bytes,
        }
    }

    pub fn total_rgba(&self) -> u64 {
        self.textures as u64 * self.rgba_bytes
    }

    pub fn total_astc6(&self) -> u64 {
        self.textures as u64 * self.astc6_bytes
    }

    pub fn total_astc8(&self) -> u64 {
        self.textures as u64 * self.astc8_bytes
    }

    pub fn fits(&self, per_texture: u64) -> usize {
        if per_texture == 0 {
            0
        } else {
            (self.budget_bytes / per_texture) as usize
        }
    }

    /// Smallest format that keeps every texture resident.
    pub fn recommended(&self) -> Option<&'static str> {
        if self.total_rgba() <= self.budget_bytes {
            Some("rgba8")
        } else if self.total_astc6() <= self.budget_bytes {
            Some("astc6x6")
        } else if self.total_astc8() <= self.budget_bytes {
            Some("astc8x8")
        } else {
            None
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "textures={} rgba_total_mb={:.1} astc6_total_mb={:.1} astc8_total_mb={:.1} budget_mb={:.1} fits_rgba={} fits_astc6={} fits_astc8={}",
            self.textures,
            self.total_rgba() as f64 / 1e6,
            self.total_astc6() as f64 / 1e6,
            self.total_astc8() as f64 / 1e6,
            self.budget_bytes as f64 / 1e6,
            self.fits(self.rgba_bytes),
            self.fits(self.astc6_bytes),
            self.fits(self.astc8_bytes)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_bytes(set: &mut ResidentSet<u64>, key: u32, bytes: u64) -> bool {
        set.insert(key, bytes, bytes)
    }

    /// The install's model count: 542 MDX (PARITY_MATRIX R1/R2).
    #[test]
    fn lru_holds_542_models_inside_a_budget() {
        let per_model = 64 * 1024u64; // 64 KB of packed geometry
        let budget = 100 * per_model; // 100 models resident
        let mut set = ResidentSet::new(budget);
        for key in 0..542u32 {
            set.begin_frame();
            assert!(insert_bytes(&mut set, key, per_model), "insert {key}");
            assert!(set.bytes() <= budget, "budget respected after {key}");
        }
        let stats = set.stats();
        assert_eq!(stats.resident, 100, "exactly the budget's worth stays resident");
        assert_eq!(stats.resident, set.len());
        assert_eq!(stats.evictions, 542 - 100);
        assert_eq!(stats.bytes, budget);
        // Keys 0..442 were evicted in order; the last 100 are resident.
        assert!(!set.contains(0));
        assert!(!set.contains(441));
        for key in 442..542u32 {
            assert!(set.contains(key), "recent key {key} stays");
        }
    }

    #[test]
    fn touching_keeps_an_entry_alive() {
        let per_model = 1024u64;
        let mut set = ResidentSet::new(3 * per_model);
        for key in 0..3u32 {
            set.begin_frame();
            insert_bytes(&mut set, key, per_model);
        }
        set.begin_frame();
        assert!(set.get(0).is_some(), "touch 0");
        set.begin_frame();
        insert_bytes(&mut set, 3, per_model);
        assert!(set.contains(0), "touched entry survives");
        assert!(!set.contains(1), "the LRU entry is evicted");
        assert!(set.contains(3));
    }

    #[test]
    fn an_entry_larger_than_the_budget_is_refused() {
        let mut set = ResidentSet::new(1024);
        assert!(!insert_bytes(&mut set, 7, 2048));
        assert_eq!(set.len(), 0);
        assert_eq!(set.stats().rejected, 1);
        assert_eq!(set.bytes(), 0);
    }

    #[test]
    fn lowering_the_budget_evicts() {
        let mut set = ResidentSet::new(4 * 1024);
        for key in 0..4u32 {
            set.begin_frame();
            insert_bytes(&mut set, key, 1024);
        }
        assert_eq!(set.len(), 4);
        set.set_budget(2 * 1024);
        assert_eq!(set.enforce_budget(), 2);
        assert_eq!(set.len(), 2);
        assert_eq!(set.bytes(), 2048);
    }

    #[test]
    fn replacing_a_key_does_not_double_count() {
        let mut set = ResidentSet::new(4096);
        insert_bytes(&mut set, 1, 1024);
        insert_bytes(&mut set, 1, 2048);
        assert_eq!(set.len(), 1);
        assert_eq!(set.bytes(), 2048);
    }

    /// ASTC path status and fallback (p2): on this adapter ASTC is present, so
    /// the first choice is ASTC 6x6; without it the chain must degrade to ETC2,
    /// then to uncompressed RGBA8, and the caller must still get an answer.
    #[test]
    fn astc_fallback_chain() {
        let with_astc = CompressionSupport {
            astc_ldr: true,
            astc_hdr: false,
            etc2: true,
            bc: false,
        };
        assert_eq!(with_astc.best_color(), Some(CompressedFormat::Astc6x6));
        assert_eq!(with_astc.best_small(), Some(CompressedFormat::Astc8x8));

        let etc2_only = CompressionSupport {
            astc_ldr: false,
            astc_hdr: false,
            etc2: true,
            bc: false,
        };
        assert_eq!(etc2_only.best_color(), Some(CompressedFormat::Etc2Rgba8));
        assert_eq!(etc2_only.best_small(), Some(CompressedFormat::Etc2Rgb));

        let none = CompressionSupport::default();
        assert_eq!(none.best_color(), None);
        assert_eq!(none.best_small(), None);
        assert!(!none.any());

        // The uncompressed fallback stays valid on every adapter: a texture the
        // store cannot compress is uploaded as RGBA8 (TextureImage::fit path).
        assert_eq!(CompressedFormat::Astc6x6.accepts(258, 258), true);
        assert_eq!(CompressedFormat::Astc6x6.accepts(256, 256), false);
    }

    #[test]
    fn texture_plan_picks_astc_when_rgba_does_not_fit() {
        // Measured chain sizes for one 256x256 texture: RGBA8 + mips 349 524 B,
        // ASTC 6x6 40 096 B, ASTC 8x8 21 888 B. 542 textures therefore need
        // 189.4 MB / 21.7 MB / 11.9 MB respectively (see tests/cache.rs, which
        // asserts the RGBA figure against a real upload).
        let plan = TexturePlan::new(542, 256, 256, 64 * 1024 * 1024);
        assert_eq!(plan.rgba_bytes, 349_524);
        assert_eq!(plan.astc6_bytes, 40_096);
        assert_eq!(plan.astc8_bytes, 21_888);
        assert!(plan.total_rgba() > plan.budget_bytes, "RGBA8 does not fit 64 MB");
        assert!(plan.total_astc6() <= plan.budget_bytes, "ASTC 6x6 fits");
        assert_eq!(plan.recommended(), Some("astc6x6"));

        let tight = TexturePlan::new(542, 256, 256, 16 * 1024 * 1024);
        assert_eq!(tight.recommended(), Some("astc8x8"), "11.9 MB of ASTC 8x8 fits");
        let tiny = TexturePlan::new(542, 256, 256, 8 * 1024 * 1024);
        assert_eq!(tiny.recommended(), None, "8 MB cannot hold 542 textures");

        let roomy = TexturePlan::new(542, 256, 256, 512 * 1024 * 1024);
        assert_eq!(roomy.recommended(), Some("rgba8"));
        assert!(plan.fits(plan.astc6_bytes) >= 542);
    }
}
