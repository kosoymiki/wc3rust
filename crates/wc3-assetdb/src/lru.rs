//! Byte-budgeted LRU with refcount pinning.
//!
//! Assets are shared through `Arc`, so an entry that still has a live consumer
//! must not be dropped from the cache (the consumer would keep the memory
//! anyway and the cache would lose its index). Eviction therefore skips entries
//! whose `Arc::strong_count` is greater than one and retries them on a later
//! sweep. The list is an intrusive doubly-linked list over a slab: `get`,
//! `insert` and `remove` are O(1), and no allocation happens on `get`.

use std::collections::HashMap;
use std::sync::Arc;

struct Node<V> {
    key: String,
    value: Arc<V>,
    size: usize,
    prev: Option<usize>,
    next: Option<usize>,
}

/// Result of an eviction sweep, for stats and tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EvictionReport {
    pub evicted: usize,
    pub pinned_skipped: usize,
    pub bytes_freed: usize,
}

pub struct LruCache<V> {
    nodes: Vec<Option<Node<V>>>,
    free: Vec<usize>,
    map: HashMap<String, usize>,
    head: Option<usize>,
    tail: Option<usize>,
    byte_budget: usize,
    max_entries: usize,
    bytes_used: usize,
    evictions: u64,
    pinned_skips: u64,
}

impl<V> Default for LruCache<V> {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl<V> LruCache<V> {
    pub fn new(byte_budget: usize, max_entries: usize) -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            map: HashMap::new(),
            head: None,
            tail: None,
            byte_budget,
            max_entries,
            bytes_used: 0,
            evictions: 0,
            pinned_skips: 0,
        }
    }

    pub fn with_budget(byte_budget: usize) -> Self {
        Self::new(byte_budget, usize::MAX)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn bytes_used(&self) -> usize {
        self.bytes_used
    }

    pub fn byte_budget(&self) -> usize {
        self.byte_budget
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    pub fn pinned_skips(&self) -> u64 {
        self.pinned_skips
    }

    pub fn set_byte_budget(&mut self, budget: usize) -> EvictionReport {
        self.byte_budget = budget;
        self.evict_to_fit(0)
    }

    pub fn set_max_entries(&mut self, max: usize) -> EvictionReport {
        self.max_entries = max;
        self.evict_to_fit(0)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn peek(&self, key: &str) -> Option<Arc<V>> {
        let slot = *self.map.get(key)?;
        self.nodes[slot].as_ref().map(|node| node.value.clone())
    }

    /// Look up and promote to most-recently-used.
    pub fn get(&mut self, key: &str) -> Option<Arc<V>> {
        let slot = *self.map.get(key)?;
        self.unlink(slot);
        self.push_front(slot);
        self.nodes[slot].as_ref().map(|node| node.value.clone())
    }

    /// Insert or replace. Returns the evictions performed to stay in budget.
    pub fn insert(&mut self, key: String, value: Arc<V>, size: usize) -> EvictionReport {
        if let Some(&slot) = self.map.get(&key) {
            self.unlink(slot);
            if let Some(node) = self.nodes[slot].as_mut() {
                self.bytes_used = self.bytes_used.saturating_sub(node.size);
                node.value = value;
                node.size = size;
                self.bytes_used += size;
            }
            self.push_front(slot);
            return self.evict_to_fit(0);
        }
        let slot = self.alloc_slot(Node {
            key: key.clone(),
            value,
            size,
            prev: None,
            next: None,
        });
        self.map.insert(key, slot);
        self.push_front(slot);
        self.bytes_used += size;
        self.evict_to_fit(0)
    }

    pub fn remove(&mut self, key: &str) -> Option<Arc<V>> {
        let slot = self.map.remove(key)?;
        self.unlink(slot);
        let node = self.nodes[slot].take()?;
        self.bytes_used = self.bytes_used.saturating_sub(node.size);
        self.free.push(slot);
        Some(node.value)
    }

    pub fn clear(&mut self) {
        for slot in 0..self.nodes.len() {
            if let Some(node) = self.nodes[slot].take() {
                self.bytes_used = self.bytes_used.saturating_sub(node.size);
            }
        }
        self.free = (0..self.nodes.len()).collect();
        self.free.reverse();
        self.map.clear();
        self.head = None;
        self.tail = None;
    }

    /// Evict from the cold end until `reserve` more bytes fit and the entry cap
    /// holds. Pinned entries are moved to the hot end and retried next sweep.
    pub fn evict_to_fit(&mut self, reserve: usize) -> EvictionReport {
        let mut report = EvictionReport::default();
        loop {
            let fits_bytes = self.bytes_used + reserve <= self.byte_budget;
            let fits_entries = self.len() <= self.max_entries;
            if fits_bytes && fits_entries {
                break;
            }
            let Some(tail) = self.tail else {
                break;
            };
            let pinned = self.nodes[tail]
                .as_ref()
                .map(|node| Arc::strong_count(&node.value) > 1)
                .unwrap_or(false);
            if pinned {
                self.pinned_skips += 1;
                report.pinned_skipped += 1;
                self.unlink(tail);
                self.push_front(tail);
                // Everything is pinned: stop instead of spinning.
                if report.pinned_skipped >= self.len() {
                    break;
                }
                continue;
            }
            let Some(node) = self.nodes[tail].as_ref() else {
                break;
            };
            let key = node.key.clone();
            let size = node.size;
            self.map.remove(&key);
            self.unlink(tail);
            if let Some(node) = self.nodes[tail].take() {
                self.bytes_used = self.bytes_used.saturating_sub(node.size);
            }
            self.free.push(tail);
            self.evictions += 1;
            report.evicted += 1;
            report.bytes_freed += size;
        }
        report
    }

    /// Keys from most to least recently used (bounded by `limit`).
    pub fn keys_mru(&self, limit: usize) -> Vec<&str> {
        let mut out = Vec::with_capacity(limit.min(self.len()));
        let mut cursor = self.head;
        while let Some(slot) = cursor {
            if out.len() >= limit {
                break;
            }
            let Some(node) = self.nodes[slot].as_ref() else {
                break;
            };
            out.push(node.key.as_str());
            cursor = node.next;
        }
        out
    }

    fn alloc_slot(&mut self, node: Node<V>) -> usize {
        if let Some(slot) = self.free.pop() {
            self.nodes[slot] = Some(node);
            slot
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        }
    }

    fn unlink(&mut self, slot: usize) {
        let Some(node) = self.nodes[slot].as_ref() else {
            return;
        };
        let prev = node.prev;
        let next = node.next;
        if let Some(prev) = prev {
            if let Some(prev_node) = self.nodes[prev].as_mut() {
                prev_node.next = next;
            }
        } else {
            self.head = next;
        }
        if let Some(next) = next {
            if let Some(next_node) = self.nodes[next].as_mut() {
                next_node.prev = prev;
            }
        } else {
            self.tail = prev;
        }
        if let Some(node) = self.nodes[slot].as_mut() {
            node.prev = None;
            node.next = None;
        }
    }

    fn push_front(&mut self, slot: usize) {
        let old_head = self.head;
        if let Some(node) = self.nodes[slot].as_mut() {
            node.prev = None;
            node.next = old_head;
        }
        if let Some(old_head) = old_head {
            if let Some(node) = self.nodes[old_head].as_mut() {
                node.prev = Some(slot);
            }
        } else {
            self.tail = Some(slot);
        }
        self.head = Some(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used_first() {
        let mut cache: LruCache<u32> = LruCache::new(10, usize::MAX);
        cache.insert("a".into(), Arc::new(1), 4);
        cache.insert("b".into(), Arc::new(2), 4);
        assert!(cache.get("a").is_some(), "a becomes most recent");
        let report = cache.insert("c".into(), Arc::new(3), 4);
        assert_eq!(report.evicted, 1);
        assert!(cache.contains("a"), "a was promoted");
        assert!(!cache.contains("b"), "b was coldest");
        assert!(cache.contains("c"));
        assert_eq!(cache.bytes_used(), 8);
    }

    #[test]
    fn pinned_entries_survive_eviction_and_are_retried() {
        let mut cache: LruCache<String> = LruCache::new(4, usize::MAX);
        cache.insert("keep".into(), Arc::new("x".to_string()), 4);
        let pinned = cache.get("keep").expect("present");
        // The only resident entry is pinned, so the new entry is the one that
        // gets dropped: the cache must not evict memory a consumer still holds.
        let report = cache.insert("new".into(), Arc::new("z".to_string()), 4);
        assert_eq!(report.pinned_skipped, 1);
        assert!(cache.contains("keep"), "pinned entry survives");
        assert!(!cache.contains("new"), "new entry is evicted instead");
        drop(pinned);
        cache.insert("next".into(), Arc::new("w".to_string()), 4);
        assert!(!cache.contains("keep"), "unpinned entry is now evictable");
        assert!(cache.bytes_used() <= 4);
    }

    #[test]
    fn entry_cap_is_enforced() {
        let mut cache: LruCache<u32> = LruCache::new(usize::MAX, 2);
        cache.insert("a".into(), Arc::new(1), 1);
        cache.insert("b".into(), Arc::new(2), 1);
        cache.insert("c".into(), Arc::new(3), 1);
        assert_eq!(cache.len(), 2);
        assert!(!cache.contains("a"));
        assert_eq!(cache.keys_mru(5), vec!["c", "b"]);
    }

    #[test]
    fn replace_updates_budget_accounting() {
        let mut cache: LruCache<Vec<u8>> = LruCache::new(100, 16);
        cache.insert("a".into(), Arc::new(vec![0; 10]), 10);
        cache.insert("a".into(), Arc::new(vec![0; 30]), 30);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes_used(), 30);
        cache.remove("a");
        assert_eq!(cache.bytes_used(), 0);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn many_operations_keep_list_consistent() {
        let mut cache: LruCache<u32> = LruCache::new(1000, 64);
        for i in 0..200 {
            cache.insert(format!("k{i}"), Arc::new(i), 8);
            if i % 3 == 0 {
                cache.get(&format!("k{}", i / 2));
            }
            if i % 7 == 0 {
                cache.remove(&format!("k{}", i.saturating_sub(5)));
            }
        }
        assert!(cache.len() <= 64);
        assert!(cache.bytes_used() <= 1000);
        // Keys walked from the head must be a permutation of the map keys.
        let keys = cache.keys_mru(1000);
        assert_eq!(keys.len(), cache.len());
        for key in keys {
            assert!(cache.contains(key));
        }
    }
}
