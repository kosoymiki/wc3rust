# MDX geoset LOD: measurement and falsification

Owner: layer DATA + FORMATS (ledger 394). Date: 2026-11-15.
Trigger: operator-visible artifact — units drawn as doubled/overlapping bodies with
detached shoulder plates and missing legs (screenshot
`Screenshot_2026-11-15-01-54-33-19_c8c86ae9e2087f7215f6d83bf6c221e0.jpg`).
Hypothesis to test: the MDX geoset chunk's LOD index was discarded by the parser,
so every detail level of the same body is drawn at once.

## Verdict

**The hypothesis is not the cause on this install.** Three measured facts:

1. **The shipped models have no LOD field at all.** All 2048 `.mdx` files in the
   installed `War3.mpq` are MDX **version 800** (`VERS` chunk). The geoset
   `lod`/`lod_name` fields only exist from version 900 on, so every geoset of every
   shipped model has `lod = 0`, and each model has exactly **one** LOD group.
2. **The parser refuses non-classic models outright** —
   `crates/wc3-formats/src/mdx.rs:988-991` returns
   `Unsupported("MDX version != 800 (classic only)")` (and the material reader
   rejects them again at `mdx.rs:1324`). A v900+ geoset LOD value can therefore
   never reach a consumer of this parser today; it is not silently dropped, the
   file is rejected.
3. **Filtering by the lowest LOD changes nothing here**: with a single group the
   "draw only LOD 0" filter keeps every geoset.

Evidence commands (run 2026-11-15):

```text
cargo run -p wc3-formats --example mdx_lod_scan --offline
archive .../War3.mpq: 10523 listfile entries, 2048 .mdx
versions = {800: 2048}
models without the lod field (version <= 800) = {800: 2048}
models with the lod field (version > 800) = 0, geosets = 0
models with more than one LOD group = 0
read_failed=0 parse_failed=0
```

```text
cargo test -p wc3-formats --test mdx_lod -- --nocapture
```

## What was added anyway (additive, no behaviour change)

The LOD value is real data in v900+ files (1.30+/Reforged-era models), and a
future parser will need it. `crates/wc3-formats/src/mdx.rs`:

| Item | Meaning |
|---|---|
| `Geoset::lod: u32` | the geoset's detail level (`0` for v800 files, which have no such field) |
| `Geoset::lod_name: String` | the 80-byte name from the chunk (`""` for v800) |
| `Geoset::drawn_at(lod) -> bool` | `self.lod == lod`, for a renderer's filter |
| `Geoset::lowest_lod(&[Geoset]) -> Option<u32>` | the level a plain renderer should draw |

Proof of the parse branch (`crates/wc3-formats/src/mdx.rs` unit test
`geoset_lod_fields_follow_the_file_version`): a synthetic **v901** geoset read on
the geoset reader returns `lod = 1` and `lod_name = "LOD1"` from the right
offsets, while the same bytes at **v800** return `lod = 0` / empty name — i.e. the
field is read exactly where the format puts it, and *not* invented for classic
files. `tests/mdx_lod.rs` pins the public contract (v901 rejected, v800 reports 0)
and the archive facts.

## Measured geoset inventory (what the renderer is actually given)

`cargo run -p wc3-formats --example mdx_geoset_probe --offline`:

```text
units\orc\Peon\Peon.mdx: version=800 geosets=5 materials=3 bones=20 helpers=16 pivots=55 geoset_anims=5
  geoset[0] lod=0 mat=0 sel=0 verts=220 tris=212 gndx_max=22 groups=[1,2,2,3,2,1,2,1,1,2,2,1,2,1,2,1,2,1,2,1,2,1,1]
  geoset[1] lod=0 mat=1 sel=0 verts=58  tris=46  gndx_max=9  groups=[3,2,2,1,2,2,1,2,1,2]
  geoset[2] lod=0 mat=0 sel=0 verts=38  tris=46  gndx_max=4  groups=[2,2,2,1,1]
  geoset[3] lod=0 mat=0 sel=0 verts=120 tris=48  gndx_max=0  groups=[1]
  geoset[4] lod=0 mat=2 sel=0 verts=45  tris=20  gndx_max=0  groups=[1]
  materials: [(0, 1 layer), (1, 2 layers), (2, 1 layer)]
  max MATS index = 3 (bones-1 = 19)
```

Two structural facts a renderer must respect, and the ones worth checking first:

1. **Geosets outnumber materials, and they are grouped:** material 0 owns geosets
   {0, 2, 3}, material 1 owns {1}, material 2 owns {4}. A draw loop written as
   `for material { for geoset in all_geosets }` (without the
   `geoset.material_id == material` test) draws the body three times — the exact
   "two bodies + a duplicated pauldron" signature. With `materials = 3` and
   `geosets = 5` a missing filter yields 15 draws instead of 5.
2. **The vertex→bone mapping is the `GNDX` → `MTGC`/`MATS` chain.** `matrix_groups`
   is the *per-group matrix count* (23 groups for the Peon body, 1-3 matrices
   each), `matrix_indices` is the flat bone list. A consumer that treats
   `matrix_groups` values as bone indices (instead of as counts/offsets) binds the
   mesh to the wrong nodes — the mesh then hangs in the air with parts detached.
   `wc3-anim` already indexes nodes by `object_id`
   (`crates/wc3-anim/src/lib.rs:372-386`, "otherwise maps limb vertices to the
   wrong node and the mesh hangs in the air"), so that half is covered; the
   renderer must consume `GeosetSkin` rather than the raw arrays to inherit it.

What the formats layer cannot see (and therefore does not claim): whether the
render layer draws a model twice (e.g. model + portrait instance, or a bind-pose
pass plus a skinned pass), or spawns two sim entities at the same position. Those
live in `wc3-render` / `wc3-game` / `wc3-sim` and need their own evidence.

## Tools left in place

| Tool | Use |
|---|---|
| `cargo run -p wc3-formats --example mdx_lod_scan` | archive-wide MDX version / LOD-group census |
| `cargo run -p wc3-formats --example mdx_lod -- <model>` | per-geoset LOD table for named models |
| `cargo run -p wc3-formats --example mdx_geoset_probe -- <model>` | geoset/material/bone/skinning inventory for a suspected model |
| `cargo test -p wc3-formats --test mdx_lod` | the contract + archive pins |
