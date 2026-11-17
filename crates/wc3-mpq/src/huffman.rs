//! WC3 MPQ Huffman decoder (adaptive Huffman, War3Net/StormLib algorithm).
//!
//! Ported from War3Net `HuffmanCoding.cs` (the canonical C# implementation of
//! StormLib's `huff.cpp` decoder). The tree is stored in an arena with integer
//! links instead of C# object references so the parent/child/next/prev cycles
//! do not leak.

#[derive(Debug)]
pub struct HuffmanError;

const HUFFMAN_PRIME: [&[u8]; 9] = [
    &[
    10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
    ],
    &[
    84, 22, 22, 13, 12, 8, 6, 5, 6, 5, 6, 3, 4, 4, 3, 5,
    14, 11, 20, 19, 19, 9, 11, 6, 5, 4, 3, 2, 3, 2, 2, 2,
    13, 7, 9, 6, 6, 4, 3, 2, 4, 3, 3, 3, 3, 3, 2, 2,
    9, 6, 4, 4, 4, 4, 3, 2, 3, 2, 2, 2, 2, 3, 2, 4,
    8, 3, 4, 7, 9, 5, 3, 3, 3, 3, 2, 2, 2, 3, 2, 2,
    3, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 1, 2, 2,
    6, 10, 8, 8, 6, 7, 4, 3, 4, 4, 2, 2, 4, 2, 3, 3,
    4, 3, 7, 7, 9, 6, 4, 3, 3, 2, 1, 2, 2, 2, 2, 2,
    10, 2, 2, 3, 2, 2, 1, 1, 2, 2, 2, 6, 3, 5, 2, 3,
    2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 3, 1, 1, 1,
    2, 1, 1, 1, 1, 1, 1, 2, 4, 4, 4, 7, 9, 8, 12, 2,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 3,
    4, 1, 2, 4, 5, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1,
    4, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 1, 1, 1, 1, 1, 1, 1, 3, 1, 1, 1, 1, 1, 1, 1,
    2, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 2, 2, 2, 6, 75,
    ],
    &[
    0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 39, 0, 0, 35, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    255, 1, 1, 1, 1, 1, 1, 1, 2, 2, 1, 1, 6, 14, 16, 4,
    6, 8, 5, 4, 4, 3, 3, 2, 2, 3, 3, 1, 1, 2, 1, 1,
    1, 4, 2, 4, 2, 2, 2, 1, 1, 4, 1, 1, 2, 3, 3, 2,
    3, 1, 3, 6, 4, 1, 1, 1, 1, 1, 1, 2, 1, 2, 1, 1,
    1, 41, 7, 22, 18, 64, 10, 10, 17, 37, 1, 3, 23, 16, 38, 42,
    16, 1, 35, 35, 47, 16, 6, 7, 2, 9, 1, 1, 1, 1, 1,
    ],
    &[
    255, 11, 7, 5, 11, 2, 2, 2, 6, 2, 2, 1, 4, 2, 1, 3,
    9, 1, 1, 1, 3, 4, 1, 1, 2, 1, 1, 1, 2, 1, 1, 1,
    5, 1, 1, 1, 13, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 1, 1, 3, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1,
    10, 4, 2, 1, 6, 3, 2, 1, 1, 1, 1, 1, 3, 1, 1, 1,
    5, 2, 3, 4, 3, 3, 3, 2, 1, 1, 1, 2, 1, 2, 3, 3,
    1, 3, 1, 1, 2, 5, 1, 1, 4, 3, 5, 1, 3, 1, 3, 3,
    2, 1, 4, 3, 10, 6, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 1, 10, 2, 5, 1, 1, 2, 7, 2, 23, 1, 5, 1, 1,
    14, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    6, 2, 1, 4, 5, 1, 1, 2, 1, 1, 1, 1, 2, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 7, 1, 1, 2, 1, 1, 1, 1,
    2, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 17,
    ],
    &[
    255, 251, 152, 154, 132, 133, 99, 100, 62, 62, 34, 34, 19, 19, 24, 23,
    ],
    &[
    255, 241, 157, 158, 154, 155, 154, 151, 147, 147, 140, 142, 134, 136, 128, 130,
    124, 124, 114, 115, 105, 107, 95, 96, 85, 86, 74, 75, 64, 65, 55, 55,
    47, 47, 39, 39, 33, 33, 27, 28, 23, 23, 19, 19, 16, 16, 13, 13,
    11, 11, 9, 9, 8, 8, 7, 7, 6, 5, 5, 4, 4, 4, 25, 24,
    ],
    &[
    195, 203, 245, 65, 255, 123, 247, 33, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    191, 204, 242, 64, 253, 124, 247, 34, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    122, 70,
    ],
    &[
    195, 217, 239, 61, 249, 124, 233, 30, 253, 171, 241, 44, 252, 91, 254, 23,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    189, 217, 236, 61, 245, 125, 232, 29, 251, 174, 240, 44, 251, 92, 255, 24,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    112, 108,
    ],
    &[
    186, 197, 218, 51, 227, 109, 216, 24, 229, 148, 218, 35, 223, 74, 209, 16,
    238, 175, 228, 44, 234, 90, 222, 21, 244, 135, 233, 33, 246, 67, 252, 18,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    176, 199, 216, 51, 227, 107, 214, 24, 231, 149, 216, 35, 219, 73, 208, 17,
    233, 178, 226, 43, 232, 92, 221, 21, 241, 135, 231, 32, 247, 68, 255, 19,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    95, 158,
    ]
];

struct Node {
    value: i32,
    weight: i32,
    next: Option<usize>,
    prev: Option<usize>,
    parent: Option<usize>,
    child0: Option<usize>,
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn new_node(&mut self, value: i32, weight: i32) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node {
            value,
            weight,
            next: None,
            prev: None,
            parent: None,
            child0: None,
        });
        index
    }

    /// Port of War3Net LinkedNode.Insert; returns the lower-weight list end.
    fn insert(&mut self, this: usize, other: usize) -> usize {
        if self.nodes[other].weight <= self.nodes[this].weight {
            let next = self.nodes[this].next;
            if let Some(next) = next {
                self.nodes[next].prev = Some(other);
                self.nodes[other].next = Some(next);
            }
            self.nodes[this].next = Some(other);
            self.nodes[other].prev = Some(this);
            other
        } else {
            if let Some(prev) = self.nodes[this].prev {
                self.insert(prev, other);
            } else {
                self.nodes[other].prev = None;
                self.nodes[this].prev = Some(other);
                self.nodes[other].next = Some(this);
            }
            this
        }
    }

    fn build_list(prime: &[u8]) -> (Self, usize) {
        let mut tree = Self { nodes: Vec::with_capacity(512) };
        let mut root = tree.new_node(256, 1);
        let n257 = tree.new_node(257, 1);
        root = tree.insert(root, n257);
        for (value, &weight) in prime.iter().enumerate() {
            if weight != 0 {
                let node = tree.new_node(value as i32, weight as i32);
                root = tree.insert(root, node);
            }
        }
        (tree, root)
    }

    fn build_tree(&mut self, tail: usize) -> usize {
        let mut current = Some(tail);
        while let Some(cur) = current {
            let child0 = cur;
            let Some(child1) = self.nodes[cur].prev else {
                return cur;
            };
            let weight = self.nodes[child0].weight + self.nodes[child1].weight;
            let parent = self.new_node(0, weight);
            self.nodes[parent].child0 = Some(child0);
            self.nodes[child0].parent = Some(parent);
            self.nodes[child1].parent = Some(parent);
            self.insert(cur, parent);
            let prev = self.nodes[cur].prev;
            current = prev.and_then(|prev| self.nodes[prev].prev);
        }
        tail
    }

    fn insert_node(&mut self, tail: usize, decompressed: i32) -> usize {
        let parent = tail;
        let result = self.nodes[tail].prev.unwrap_or(tail);
        let temp = self.new_node(self.nodes[parent].value, self.nodes[parent].weight);
        self.nodes[temp].parent = Some(parent);
        let newnode = self.new_node(decompressed, 0);
        self.nodes[newnode].parent = Some(parent);
        self.nodes[parent].child0 = Some(newnode);
        self.nodes[tail].next = Some(temp);
        self.nodes[temp].prev = Some(tail);
        self.nodes[newnode].prev = Some(temp);
        self.nodes[temp].next = Some(newnode);
        self.adjust_tree(newnode);
        self.adjust_tree(newnode);
        result
    }

    fn adjust_tree(&mut self, new_node: usize) {
        let mut current = Some(new_node);
        let mut guard = 0usize;
        while let Some(cur) = current {
            guard += 1;
            if guard > 200_000 {
                return;
            }
            self.nodes[cur].weight += 1;
            let mut insertpoint = cur;
            let mut insert_prev = None;
            loop {
                let Some(prev) = self.nodes[insertpoint].prev else {
                    break;
                };
                if self.nodes[prev].weight >= self.nodes[cur].weight {
                    insert_prev = Some(prev);
                    break;
                }
                insertpoint = prev;
                insert_prev = Some(prev);
            }
            if insertpoint == cur {
                current = self.nodes[cur].parent;
                continue;
            }
            let Some(prev) = insert_prev else {
                current = self.nodes[cur].parent;
                continue;
            };
            // remove insertpoint
            let ip_prev = self.nodes[insertpoint].prev;
            let ip_next = self.nodes[insertpoint].next;
            if let Some(p) = ip_prev {
                self.nodes[p].next = ip_next;
            }
            if let Some(n) = ip_next {
                self.nodes[n].prev = ip_prev;
            }
            // insert insertpoint after current
            let cur_next = self.nodes[cur].next;
            self.nodes[insertpoint].next = cur_next;
            self.nodes[insertpoint].prev = Some(cur);
            if let Some(n) = cur_next {
                self.nodes[n].prev = Some(insertpoint);
            }
            self.nodes[cur].next = Some(insertpoint);
            // remove current
            let cur_prev = self.nodes[cur].prev;
            if let Some(p) = cur_prev {
                self.nodes[p].next = self.nodes[cur].next;
            }
            let cur_next2 = self.nodes[cur].next;
            if let Some(n) = cur_next2 {
                self.nodes[n].prev = cur_prev;
            }
            // insert current after prev
            let temp = self.nodes[prev].next;
            self.nodes[cur].next = temp;
            self.nodes[cur].prev = Some(prev);
            if let Some(t) = temp {
                self.nodes[t].prev = Some(cur);
            }
            self.nodes[prev].next = Some(cur);
            // parent/child links
            let currentparent = self.nodes[cur].parent;
            let insertparent = self.nodes[insertpoint].parent;
            if let Some(cp) = currentparent {
                if self.nodes[cp].child0 == Some(cur) {
                    self.nodes[cp].child0 = Some(insertpoint);
                }
            }
            if let (Some(cp), Some(ip)) = (currentparent, insertparent) {
                if cp != ip && self.nodes[ip].child0 == Some(insertpoint) {
                    self.nodes[ip].child0 = Some(cur);
                }
            }
            self.nodes[cur].parent = insertparent;
            self.nodes[insertpoint].parent = currentparent;
            current = self.nodes[cur].parent;
        }
    }

    fn decode(&self, head: usize, bits: &mut Bits<'_>) -> Result<i32, HuffmanError> {
        let mut node = head;
        while let Some(child0) = self.nodes[node].child0 {
            let bit = bits.read(1).ok_or(HuffmanError)?;
            node = if bit == 0 {
                child0
            } else {
                self.nodes[child0].prev.ok_or(HuffmanError)?
            };
        }
        Ok(self.nodes[node].value)
    }
}

struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    current: u32,
    bit_count: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0, current: 0, bit_count: 0 }
    }

    fn ensure(&mut self, count: u32) -> bool {
        if count <= self.bit_count {
            return true;
        }
        if self.position >= self.data.len() {
            return false;
        }
        self.current |= (self.data[self.position] as u32) << self.bit_count;
        self.position += 1;
        self.bit_count += 8;
        true
    }

    fn read(&mut self, count: u32) -> Option<u32> {
        if count > 16 || !self.ensure(count) {
            return None;
        }
        let mask = 0xFFFFu32 >> (16 - count);
        let value = self.current & mask;
        self.current >>= count;
        self.bit_count -= count;
        Some(value)
    }
}

/// Decompress a Huffman stream. `data[0]` is the WC3 Huffman compression type
/// (1..=8); the remaining bytes are the adaptive-Huffman bit stream.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, HuffmanError> {
    let comptype = *data.first().ok_or(HuffmanError)? as usize;
    if comptype == 0 || comptype >= HUFFMAN_PRIME.len() {
        return Err(HuffmanError);
    }
    let (mut tree, mut tail) = Tree::build_list(HUFFMAN_PRIME[comptype]);
    let head = tree.build_tree(tail);
    let mut bits = Bits::new(&data[1..]);
    let mut out = Vec::new();
    loop {
        let value = tree.decode(head, &mut bits)?;
        match value {
            256 => return Ok(out),
            257 => {
                let byte = bits.read(8).ok_or(HuffmanError)? as u8;
                out.push(byte);
                tail = tree.insert_node(tail, byte as i32);
            }
            other if (0..=255).contains(&other) => out.push(other as u8),
            _ => return Err(HuffmanError),
        }
        if out.len() > 8 * 1024 * 1024 {
            return Err(HuffmanError);
        }
    }
}
