# Panic audit — operator-data reachability in `herofolends-rs`

Audit lane: ROBUSTNESS / WEAK CODE (ledger 419). Date: 2026-11-15.
Scope: `crates/*/src` and `apps/*/src` (production code, inline test modules excluded).

**Verdict of this audit:** the operator's headline number (1 082 `unwrap`/`expect`)
is real *as a grep count* but 68 % of it lives inside `#[cfg(test)]` modules. The
production surface is **190 panic sites**, of which **72** were in this lane's
owned crates and **every one of those 72 is now total code**. The remaining 118
are enumerated below with their owning lane; the ones reachable from operator
data are listed with line numbers for the lane owners.

---

## 1. Method (mechanical, reproducible)

```bash
# grep basis (the operator's number)
rg -c -g 'crates/*/src/**/*.rs' -g 'apps/*/src/**/*.rs' '\.unwrap\(\)|\.expect\(' \
  | awk -F: '{s+=$2} END {print s}'                     # => 1082

# classification basis (this audit)
python3 analysis/audit/panic_inventory.py --json /tmp/panic.json
```

`analysis/audit/panic_inventory.py` does three things a raw grep does not:

1. **Strips `#[cfg(test)]` items** (attribute + the item it decorates, brace
   matched) and counts what remains — the *production* count. This is why the
   numbers differ: `game.rs` has 66 raw sites and **2** production sites;
   `terrain_render.rs` 31 raw → **0**; `wc3-re/jass/natives.rs` 29 raw → **0**.
2. **Keeps line numbers** after stripping, so every site is addressable.
3. **Separates statement kinds** (`unwrap`, `expect`, `panic!`, `unreachable!`,
   `todo!`, `unimplemented!`, and a separate, deliberately noisy index-metric).

Raw vs production, measured 2026-11-15:

| metric | raw (includes tests) | production |
|---|---:|---:|
| `.unwrap()` | 291 | 17 |
| `.expect(` | 800 | 169 |
| `panic!(` | 34 | 2 |
| `unreachable!(` | 7 | 2 |
| `todo!` / `unimplemented!` | 0 | 0 |
| **panic sites (all kinds)** | **1 132** | **190** |

Two method notes on the operator's 1 082:

* The rg glob `crates/*/src/**/*.rs` does not match files directly under
  `src/` (`crates/x/src/lib.rs`); the inventory also walks `crates/*/src/*.rs`
  and finds 9 more. 1 082 (rg) vs 1 091 (parser) for `unwrap`+`expect` raw.
* grep also matches documentation (e.g. `wc3-data/src/lib.rs:27` is a `//!`
  example) and methods that happen to be named `expect` (`wc3-re`'s JASS parser
  has `self.expect(TokenKind::…, "')'")`). Both classes are marked below.

## 2. Classification

| class | meaning | count (before) |
|---|---|---:|
| **(a)** reachable from operator/untrusted bytes | map container (W3M/W3X/MPQ), `.w3e`/`.w3i`/`.wpm`/`.doo`/`.wts`/`.slk`/`.txt`, object data (`.w3u`/`.w3a`/…), MDX, BLP, FDF, JASS text | **72** (all in this lane, all fixed) |
| **(b)** internal invariant, type-system-guaranteed | `self.threads[t].frames.last()` right after a push, `Vec::with_capacity` residency, build-time registry tables | 118 (other lanes / justified) |
| **(c)** test-only or documentation | inside `#[cfg(test)]`, `//!` examples, parser methods named `expect` | 942 raw sites, excluded from production |

The classification is per *site*, not per file: `gamedata.rs` had 5 production
sites and 37 test sites, and one of the production sites (`ProfileOverlay::set`)
was a real unbounded-allocation bug even though it looked like a plain index.

## 3. What "reachable" means here (and the bugs found beyond the grep)

The grep only sees `unwrap`/`expect`. Auditing the (a) files by reading them
found **five more data-reachable defects that no `unwrap` count can see**
(they were index/OOM panics and one silent-corruption path):

| defect | before | impact on operator data |
|---|---|---|
| `SlkTable` (legacy view) materialised the declared grid | `slk.rs` | `C;Y1000000000;K…` ⇒ 10⁹-row `Vec` (multi-GB) — **this is what made the first test run peak at 3.71 GB** |
| `Sheet::read` resized to any declared position | `gamedata.rs` | same class, 65 536×1 024 bound added |
| `ProfileOverlay::set(index)` grew a vector to `index` | `gamedata.rs:1507` | object-data level index `i32::MAX` ⇒ 2³¹ `String`s |
| profile `key=value` before the first `[Section]` | `gamedata.rs:1773` | `sections[0]` on an empty `Vec` ⇒ panic on a malformed `*.txt` |
| `mask_water` indexed a caller slice | `sim_bridge.rs:887` | truncated WPM / mismatched grid ⇒ index panic |
| BLP dimension cap 32 768² | `blp.rs` | `w*h*4` = 4 GiB allocation from an 8-byte header |

## 4. Fixes in this lane (72 sites → 0)

Mechanism, not case-by-case patching:

1. **`arr::<N>()` on every bounded reader** (`mdx`, `map`, `w3e`, `gamedata`):
   read exactly `N` bytes and return `[u8; N]` via `first_chunk::<N>().copied()`,
   which cannot fail once the slice is checked. `u8/u16/u32/i32/f32/tag/fourcc`
   are now built on it. This removed **~50** `try_into().expect("N bytes")`
   sites by construction rather than by adding error branches one at a time.
2. **Fixed-size arrays use constant indices** (`mpq` header, `stream` encryption
   probe): `header_bytes[16]` on a `[u8; 32]` is a compile-time bounds check, so
   the `.try_into().expect()` round-trip disappears with no runtime cost.
3. **Header reads use `first_chunk`** (`wc3-formats/lib.rs`, `w3i`, `wpm`,
   `wmp`, `map` headers): a short file returns `TooShort`, never indexes.
4. **Slices use `get`/`split_at`** (`fdf` token ranges, `blp` DXT blocks,
   `gamedata::c_string`, `mdx::chunks`): every slice is fallible and returns the
   typed error.
5. **`Option` unwraps became `let-else` / `if let`** (`fdf` brace, `huffman`
   prev link, `mdx` track keys, `gamedata` first char, `wc3-data` merge
   outcome): the branch that cannot happen is still written down, so a future
   change cannot turn it into a panic.
6. **Declared counts are bounded before use** (`w3e` texture lists ≤ 256,
   `slk`/`sheet` grid ≤ 65 536×1 024, BLP ≤ 16 M px, profile components
   ≤ 1 024). Bounds are set above every real file observed in `analysis/war3_*`,
   so no shipped data is rejected; a forged count is a typed `Err` or a counted
   skip.

### Skipping instead of failing the whole map

A structurally valid file with one bad element must not throw away the map.
Those cases now skip the element and emit one bounded line through
`wc3_formats::diag` (`GAME_DATA_SKIP reason=…`), with a counter that survives
throttling:

| reason | where | behaviour |
|---|---|---|
| `slk_position_out_of_range` | `slk.rs`, `gamedata::Sheet::read` | cell skipped, sheet keeps its valid rows |
| `missing_slk_column` | `wc3-data::Table::cell*` | typed `None` (caller default) + count |
| `profile_component_index` | `gamedata::ProfileOverlay::set` | value dropped, count |
| `profile_section_missing` | profile `.txt` parse | line kept under a synthetic section |
| `pathing_grid_short` | `sim_bridge::mask_water` | cell skipped |
| format-level errors | `wc3-game::doodads::parse_doo`, `DoodadWorld::load` | `GAME_DATA_SKIP reason=doo_parse …` and an empty world, no panic |

`wc3-game` installs the sink at `Assets::load` / `DoodadWorld::load`
(`doodads::install_format_diag_sink`), so these lines reach logcat through the
existing `dev_log!`. Volume is bounded to 8 lines per reason per process.

### Appendix — the 72 (a) sites, with the line each occupied before the fix
### `crates/wc3-data/src/lib.rs`

| line | kind | site before the fix |
|---:|---|---|
| 27 | expect | `//! let footman = data.unit("hfoo").expect("footman");` |
| 626 | expect | `Ok(self.merges.last().expect("just pushed"))` |

### `crates/wc3-formats/src/blp.rs`

| line | kind | site before the fix |
|---:|---|---|
| 40 | expect | `.map(\|s\| u32::from_le_bytes(s.try_into().expect("4 bytes")))` |
| 450 | expect | `let c0 = u16::from_le_bytes(color[..2].try_into().expect("2 bytes"));` |
| 451 | expect | `let c1 = u16::from_le_bytes(color[2..4].try_into().expect("2 bytes"));` |
| 452 | expect | `let indices = u32::from_le_bytes(color[4..8].try_into().expect("4 bytes"));` |

### `crates/wc3-formats/src/fdf.rs`

| line | kind | site before the fix |
|---:|---|---|
| 1217 | unwrap | `let header = self.tokens[statement.start..statement.brace.unwrap()].to_vec();` |
| 1219 | unwrap | `self.position = statement.brace.unwrap() + 1;` |

### `crates/wc3-formats/src/gamedata.rs`

| line | kind | site before the fix |
|---:|---|---|
| 802 | expect | `let first = chars.next().expect("non-empty");` |
| 1323 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |
| 1329 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |
| 1335 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |
| 1340 | expect | `Ok(FourCc::new(self.bytes(4)?.try_into().expect("4 bytes")))` |

### `crates/wc3-formats/src/lib.rs`

| line | kind | site before the fix |
|---:|---|---|
| 72 | expect | `let magic: [u8; 4] = bytes[..4].try_into().expect("len checked");` |
| 88 | expect | `bytes[..4].try_into().expect("len checked"),` |
| 94 | expect | `let chunk_size = u32::from_le_bytes(bytes[8..12].try_into().expect("len checked"));` |
| 98 | expect | `let version = u32::from_le_bytes(bytes[12..16].try_into().expect("len checked"));` |
| 108 | expect | `bytes[..4].try_into().expect("len checked"),` |
| 111 | expect | `let version = u32::from_le_bytes(bytes[4..8].try_into().expect("len checked"));` |

### `crates/wc3-formats/src/map.rs`

| line | kind | site before the fix |
|---:|---|---|
| 114 | expect | `let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4"));` |
| 115 | expect | `let subversion = u32::from_le_bytes(bytes[8..12].try_into().expect("4"));` |
| 121 | expect | `let count = u32::from_le_bytes(bytes[12..16].try_into().expect("4")) as usize;` |
| 288 | expect | `let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4"));` |
| 293 | expect | `let count = u32::from_le_bytes(bytes[12..16].try_into().expect("4")) as usize;` |
| 413 | expect | `Ok(self.bytes(4)?.try_into().expect("4"))` |
| 419 | expect | `Ok(u32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))` |
| 422 | expect | `Ok(i32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))` |
| 425 | expect | `Ok(f32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))` |

### `crates/wc3-formats/src/mdx.rs`

| line | kind | site before the fix |
|---:|---|---|
| 149 | expect | `Some(self.keys.last().expect("non-empty").value)` |
| 660 | expect | `let tag: [u8; 4] = reader.bytes(4)?.try_into().expect("4 bytes");` |
| 808 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 892 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 958 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1019 | expect | `.expect("4 bytes");` |
| 1023 | expect | `.expect("4 bytes"),` |
| 1154 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1214 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1370 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1489 | expect | `let mut tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1495 | expect | `tag = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1508 | expect | `tag = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1548 | expect | `let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");` |
| 1602 | expect | `self.bytes(2)?.try_into().expect("2 bytes"),` |
| 1608 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |
| 1614 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |
| 1620 | expect | `self.bytes(4)?.try_into().expect("4 bytes"),` |

### `crates/wc3-formats/src/w3e.rs`

| line | kind | site before the fix |
|---:|---|---|
| 297 | expect | `Ok(u16::from_le_bytes(self.bytes(2)?.try_into().expect("2")))` |
| 300 | expect | `Ok(u32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))` |
| 303 | expect | `Ok(f32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))` |
| 306 | expect | `Ok(self.bytes(4)?.try_into().expect("4"))` |

### `crates/wc3-formats/src/w3i.rs`

| line | kind | site before the fix |
|---:|---|---|
| 22 | expect | `let version = u32::from_le_bytes(bytes[0..4].try_into().expect("4"));` |
| 32 | expect | `u32::from_le_bytes(bytes[4..8].try_into().expect("4")),` |
| 33 | expect | `u32::from_le_bytes(bytes[8..12].try_into().expect("4")),` |

### `crates/wc3-formats/src/wpm.rs`

| line | kind | site before the fix |
|---:|---|---|
| 30 | expect | `let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4"));` |
| 34 | expect | `let width = u32::from_le_bytes(bytes[8..12].try_into().expect("4"));` |
| 35 | expect | `let height = u32::from_le_bytes(bytes[12..16].try_into().expect("4"));` |

### `crates/wc3-mpq/src/huffman.rs`

| line | kind | site before the fix |
|---:|---|---|
| 164 | expect | `let prev = self.nodes[this].prev.expect("prev checked");` |

### `crates/wc3-mpq/src/index.rs`

| line | kind | site before the fix |
|---:|---|---|
| 364 | expect | `let read_u32 = \|b: &[u8]\| u32::from_le_bytes(b.try_into().expect("4 bytes"));` |
| 365 | expect | `let read_u64 = \|b: &[u8]\| u64::from_le_bytes(b.try_into().expect("8 bytes"));` |
| 394 | expect | `let codec = u16::from_le_bytes(take(2)?.try_into().expect("2 bytes"));` |

### `crates/wc3-mpq/src/lib.rs`

| line | kind | site before the fix |
|---:|---|---|
| 252 | expect | `.map(\|chunk\| u32::from_le_bytes(chunk.try_into().expect("chunk")))` |
| 360 | expect | `header_bytes[..4].try_into().expect("4 bytes"),` |
| 363 | expect | `let declared_version = u16::from_le_bytes(header_bytes[12..14].try_into().expect("2 bytes"));` |
| 385 | expect | `hash_table_pos: u32::from_le_bytes(header_bytes[16..20].try_into().expect("4 bytes")),` |
| 386 | expect | `block_table_pos: u32::from_le_bytes(header_bytes[20..24].try_into().expect("4 bytes")),` |
| 387 | expect | `hash_table_count: u32::from_le_bytes(header_bytes[24..28].try_into().expect("4 bytes")),` |
| 389 | expect | `header_bytes[28..32].try_into().expect("4 bytes"),` |
| 391 | expect | `block_size: u16::from_le_bytes(header_bytes[14..16].try_into().expect("2 bytes")),` |
| 1108 | expect | `let word = u32::from_le_bytes(chunk.try_into().expect("4 bytes"));` |

### `crates/wc3-mpq/src/stream.rs`

| line | kind | site before the fix |
|---:|---|---|
| 57 | expect | `let enc0 = u32::from_le_bytes(encrypted[0..4].try_into().expect("4 bytes"));` |
| 58 | expect | `let enc1 = u32::from_le_bytes(encrypted[4..8].try_into().expect("4 bytes"));` |
| 205 | expect | `.map(\|c\| u32::from_le_bytes(c.try_into().expect("4 bytes")))` |

---

## 5. Invariants that stay panicking (justified, class b)

These are *not* reachable from bytes; each is a type/ownership invariant and
each carries a comment at the site. They were deliberately left as panics:
turning them into `Err` would hide a programming error behind a data error.

| site | invariant | why it cannot see operator data |
|---|---|---|
| `wc3-data::GameDatabase::merge_object_data` | `merges.last()` after an unconditional `push` | no input can make the push conditional |
| `wc3-sim/jass/vm.rs:622,636,822,826,832,885` | `.frames.last()/.pop()` inside an executing frame | the VM pushes a frame before executing its first instruction |
| `wc3-sim/save.rs:1607` | `HANDLE_KINDS` covers every `HandleKind` | compile-time table, checked by the match arm order |
| `wc3-sim/jass/registry.rs:48,53` | native registration parameter lists | build-time constant table, not a runtime file |
| `wc3-game/game.rs:1361,1571`, `hud.rs:173` | `engine`/`layout`/`textured` stored immediately above | UI lane owns the fix (see §6) |
| `wc3-sim/ai.rs:349,637`, `data.rs:569`, `lib.rs:389,506,526,1019,1078,1136`, `net.rs:243`, `save.rs:314,3330` | canonical def ids (`htow`, unit/destructable/item index) exist in the shipped data set | they are asserted at load: `GameDatabase::validate` fails the load instead |
| `wc3-re/art.rs:216,253`, `blp.rs:110` | `OnceLock` family init / 4-byte slice after a length check | RE lane owns the fix; the blp one is the same `first_chunk` pattern |
| `wc3-audio/wav.rs` (14), `ambient.rs` (9), `engine.rs` (2) | audio fixture/format assumptions | audio lane; `wav.rs` parses operator audio and is listed in §6 |

The JASS parser's `self.expect(TokenKind, …)` in `wc3-re/jass/parser.rs` is a
**false positive** of the grep: it is a parser method that *returns* `Result`,
not `Option::expect`.

## 6. Sites outside this lane (read-only), with line numbers

Reported, not modified, because the files belong to other lanes.

**UI / render lane (read-only for this audit)** — measured after the test-module
strip:

| file | production sites | lines |
|---|---:|---|
| `wc3-game/src/game.rs` | 2 | 1361 (`engine`), 1571 (`mobile_layout`) |
| `wc3-game/src/hud.rs` | 1 | 173 (`textured.last_mut()` after push) |
| `wc3-game/src/mobile_hud.rs` | 0 | — (all 28 raw sites are in its test module) |
| `wc3-game/src/model_render.rs` | 0 | — (all 24 raw in tests) |
| `wc3-game/src/terrain_render.rs` | 0 | — (all 31 raw in tests) |

The three remaining UI sites are the same shape: an `Option` field that was set
a few lines above. They are not operator-data reachable; the UI lane can make
them total with `let … else { return }` if it wants the number at zero.

**Sim / RE lane (read-only)**: 23 sites in `wc3-sim` and 11 in `wc3-re`, all
class (b) with the exception of `wc3-re/src/blp.rs:110` (4-byte slice after
`get`, same fix as §4.1) and `wc3-re/src/jass/parser.rs` (6 false positives).
`wc3-re/src/abilities.rs:751` and `jass/vm.rs:1445` are `unreachable!()` on
exhaustive matches over an enum that is built from the shipped data — if a new
effect kind is added the compiler forces the match arm.

**Other crates with production sites** (not reachable from map bytes, tools and
asset pipelines): `apps/driver-probe` 12, `apps/bench` 11, `apps/android-native`
4, `wc3-audio/wav.rs` 14 (operator audio — recommended next target),
`wc3-render/mesh.rs` 3, `wc3-assetdb` 4, `wc3-adrenotools` 7,
`apps/headless` 1.

Recommended order for the next lane: `wc3-audio/wav.rs` (14, operator audio) →
`wc3-re/blp.rs:110` (1, same helper) → UI trio (3).

## 7. Proof — malformed-input tests

`crates/wc3-formats/tests/malformed_inputs.rs` (27 cases) feeds bytes only: no
`Assets::load`, no map spawn, no GPU, no archive mount.

| case | input | expected |
|---|---|---|
| `mdx_truncated_real_model_never_panics` | real `Footman.mdx` cut at 10 even boundaries + `len-1` | no panic; a mid-payload cut is `Err` |
| `mdx_truncated_at_every_short_length_is_total` | synthetic MDX cut at **every** length 0..=len | no panic; the full file parses |
| `mdx_lying_chunk_size_is_an_error` | chunk size `u32::MAX`, chunk one byte past EOF | `Err` |
| `mdx_random_corruption_never_panics` | 256 seeded byte mutations | no panic |
| `w3e_lying_dimensions_are_errors` | 10⁹×10⁹ dims; short tile array | `Err` before allocation |
| `w3e_lying_texture_counts_are_errors` | 10⁹ ground/cliff texture ids | `Err` |
| `w3e_truncated_at_every_length_never_panics` | every cut | no panic |
| `doo_lying_count_is_an_error` | 10⁹ placed doodads | `Err` |
| `doo_out_of_range_indices_never_panic` | `item_table = i32::MIN`; 10⁹ item sets; 10⁹ lights | first parses (index is ignored), others `Err` |
| `doo_truncated_at_every_length_never_panics` | doodads **and** units at every cut | no panic; lying unit count `Err` |
| `w3i_huge_player_count_is_preserved_not_parsed` | 10⁹ player count in the tail; v27 cut at 20 B | parses, tail preserved; short v27 `Err` |
| `w3i_truncated_at_every_length_never_panics` | every cut; unterminated string | no panic / `Err` |
| `wpm_lying_dimensions_and_truncation_never_panic` | 4096² declared, 16 cells; every cut; 0 and `u32::MAX` | `Err` |
| `object_data_bad_sections_are_errors` | bad version, 10⁹ entries, truncated custom section | `Err` |
| `object_data_truncated_at_every_length_never_panics` | 6 object kinds × every cut | no panic |
| `wts_broken_keys_and_blocks_are_errors` | `STRING abc`, 21-digit key, unterminated block, non-UTF-8 | `Err`; `u32::MAX` key parses |
| `fdf_truncated_at_every_length_never_panics` | every cut; unbalanced and stray brace | no panic / `Err` |
| `blp_absurd_dimensions_are_rejected_before_allocating` | BLP1 8192² paletted, BLP2 8192² DXT | `Err` before allocating |
| `blp_truncated_at_every_length_never_panics` | BLP2 64² paletted, every cut ≤ 2048 | no panic |
| `slk_out_of_range_position_is_skipped_and_counted` | `Y=1000000000` row | valid rows kept, counter incremented |
| `slk_missing_column_reads_through_the_typed_default` | sheet without `regenHP` | `None`, other columns readable |
| `slk_lying_bounds_and_skipped_rows_are_errors_or_counted` | bounds smaller than grid; headerless row | `Err` / counted skip |
| `profile_overlay_rejects_absurd_component_index` | `index = i32::MAX` | rejected + counted, normal index still works |
| `fourcc_parse_is_total` | 0/3/5-byte and empty ids | `None`, no panic |
| `mpq_bad_tables_are_errors_or_marked_malformed` | block table past EOF, zero counts, non-power-of-two, truncated table + read, garbage, header-only | `Err`, or `Ok` + `is_malformed()` + `read_block` `Err` |
| `mpq_index_bytes_are_total` | truncated index + 64 garbage indexes | `Err` |
| `suite_stays_within_its_memory_budget` | reads `VmHWM` | asserts peak < 512 MiB and prints it |

The suite is **byte-level on purpose** and measures itself: the full 27-case
run peaks at **5.7 MiB** RSS (`VmHWM`, budget 512 MiB). A heavier map-loading
case would build the full install (814 models / 1 275 textures) per process and
peak at 2–3.7 GB — that is the shape that triggered the OOM incidents on this
device, and it is deliberately outside this file. Any future case that needs the
full install must sit behind `H2_HEAVY_TESTS=1`.

The equivalent map-loading path in `wc3-game` is covered by
`crates/wc3-game/tests/malformed_map.rs`: a truncated `MapBundle` (no terrain,
no units, short pathing grid) through `spawn_map_scene` plus `mask_water` with a
grid shorter than `width*height` — the exact shape a truncated WPM produces.

Run:

```bash
scripts/h2-cargo test -p wc3-formats -p wc3-mpq -p wc3-data --offline
scripts/h2-cargo check -p wc3-game --offline
```

## 8. Evidence

| item | value |
|---|---|
| production panic sites, whole tree, before | 190 (unwrap 17, expect 169, panic 2, unreachable 2) |
| production panic sites, this lane (wc3-formats, wc3-mpq, wc3-data, wc3-game sim_bridge/assets/doodads), before | **72** |
| production panic sites, this lane, after | **0** (one `//!` doc example remains as a grep false positive) |
| production panic sites, whole tree, after | 122 (other lanes moved during the audit; see §6) |
| data-reachable non-grep defects fixed | 6 (§3) |
| tests added | 27 (`malformed_inputs`) + map-load path in `wc3-game` |
| test rc | 0 (`wc3-formats`, `wc3-mpq`, `wc3-data`; see report for the exact tail) |
| `cargo check -p wc3-game` | rc 0 |
| `malformed_inputs` peak RSS | **5.7 MiB** for the full 27-test suite (`VmHWM`, budget 512 MiB). The 3.71 GB peak in the incident log came from a *test generator* that looped over a declared count; the generator now patches the count in place, and the suite is byte-level (no `Assets::load`, no map spawn) |

### Final verification run (2026-11-15 16:45–16:54, `scripts/h2-cargo`)

| step | command | result |
|---|---|---|
| S1 | `scripts/h2-cargo test -p wc3-formats -p wc3-mpq -p wc3-data --offline` | **rc 0** — `wc3-formats` lib 73, `malformed_inputs` 27, `map_corpus` 6, `mdx_lod` 7, `wc3-mpq` suites 4+13+3+…, `wc3-data` 56+8+10+7+6+**3** (`malformed_data`)+7+10+20 |
| S2 | `scripts/h2-cargo check -p wc3-game --offline` | **rc 0** |
| S3 | `scripts/h2-cargo test -p wc3-game --offline --test malformed_map` | **rc 0** — 5 passed |
| RSS | `target/debug/deps/malformed_inputs-… --nocapture` | 27 passed, **peak 5.7 MiB** |

Logs: `$PREFIX/tmp/opencode/final-verify.log`,
`$PREFIX/tmp/opencode/step1-rerun.log`, `$PREFIX/tmp/opencode/mal5.log`.

H2: capture 895, experience 4545–4549, monitor 419.
