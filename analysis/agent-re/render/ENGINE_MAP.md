# ENGINE_MAP — render / animation pipeline of the WC3 engine (TFT 1.27 + RoC)

Ledger **405** · agent `r2-render` · 2026-11-15 · deliverable for the render layer (ledgers 395/402).
Report-only: no `src/` edits were made.

| | |
|---|---|
| primary binary | `analysis/tft_files_Files100_Game.dll` — sha256 `d1a80c89597e08e00715dac19a5e906c58b2a351285b7e2ae94bbc5c73f4ad2e`, PE32 i386, image base `0x6f000000`, 28 001 r2 functions |
| cross-check | `analysis/game_files/Files100_Game.dll` (RoC) — same dispatch shape |
| tools | radare2 6.2.6 + r2ghidra (`pdg`), custom `afl`+immediate-xref call graph (`tools/xref.py`), RTTI/PE scanner (`tools/pe_rtti.py`), geoset replayer (`tools/mdx_geoset_check.py`) |
| method | targeted `s <addr>; af; pdg`, `rabin2 -zz` strings, 4-byte immediate scans for 4CC tags and struct displacements, call-graph walk over `Game.dll.tft.afl.txt`; **no `aaa`** (28 k functions) |
| raw evidence | `analysis/agent-re/render/raw/**` (every `pdg` used below is saved there) |

Labels: **observed** = read directly in disassembly/decompilation or measured in files; **inferred** = follows from observed data but the consuming code was not located; **unknown** = not established.

---

## 0. Reproduction

```bash
D=analysis/tft_files_Files100_Game.dll
A=analysis/agent-re/r2/Game.dll.tft.afl.txt
# decompile one function (r2ghidra)
r2 -q -e bin.relocs.apply=true -c 'e scr.color=0; s 0x6f3484e0; af; pdg' $D
# find who references a string / a VA (no aaa needed)
python3 analysis/agent-re/render/tools/xref.py $D $A str '.?AVCRibbonEmitter@@'
python3 analysis/agent-re/render/tools/xref.py $D $A callers 0x6f0d3fe0
# replay the engine's geoset reader over real models
python3 analysis/agent-re/render/tools/mdx_geoset_check.py analysis/TownHall.mdx analysis/Farm.mdx
```

Two facts about this binary that shape everything below (observed):

* The renderer is **OpenGL 1.1 fixed-function** on the GL path (`glVertexPointer/glNormalPointer/glColorPointer/glTexCoordPointer/glDrawElements/glLoadMatrixf`, imports at `0x6f6921a0..`), with a **Direct3D 8** twin implementation (`SetRenderState` wrappers in `0x6f0d2a50..0x6f0d3fe0`). Both backends are driven from the same parameter block, so the numeric contracts below are backend-independent.
* All geometry paths are reached through **device vtables** (45-entry table at `0x6f698340`, referenced only from `0x6f0cbd72`/`0x6f0cbdcd`). Direct call-graph walking of the concrete draw functions therefore returns zero callers — this is why the analysis below anchors on chunk tags, struct displacements and the vtable slot numbers instead.

---

## 1. Model (MDX) consumption

### 1.1 Chunk dispatch — the complete engine-side tag table

`fcn.6f32bcf0` is the binary MDX reader's dispatcher (`ECX` = 4CC read little-endian, args `(model, stream)`). Decompiled line:

```c
if (in_ECX == 0x534f4547) { uVar1 = fcn.6f3484e0(arg_ch,arg_10h); return uVar1; }   // "GEOS"
...
(**(*arg_10h + 0xc))(arg_10h,1,0x6f789df8);   // unknown tag -> error string 0x6f789df8
```

Handlers recovered (full table in `constants.json → mdx_chunk_dispatch`):

| tag | u32 | handler | tag | u32 | handler |
|---|---|---|---|---|---|
| `VERS` | `0x53524556` | `0x6f3505c0` | `BONE` | `0x454e4f42` | `0x6f3447f0` |
| `MODL` | `0x4c444f4d` | `0x6f34fa50` | `LITE` | `0x4554494c` | `0x6f342200` |
| `SEQS` | `0x53514553` | `0x6f34e9e0` | `HELP` | `0x504c4548` | `0x6f3408e0` |
| `GLBS` | `0x53424c47` | `0x6f34ec40` | `ATCH` | `0x48435441` | `0x6f33fba0` |
| `MTLS` | `0x534c544d` | `0x6f34c540` | `PIVT` | `0x54564950` | `0x6f33f170` |
| `TEXS` | `0x53584554` | `0x6f34dbe0` | `PREM` | `0x4d455250` | `0x6f33df10` |
| `TXAN` | `0x4e415854` | `0x6f34a890` | `PRE2` | `0x32455250` | `0x6f33ae90` |
| `GEOS` | `0x534f4547` | `0x6f3484e0` | `RIBB` | `0x42424952` | `0x6f3365e0` |
| `GEOA` | `0x414f4547` | `0x6f347d20` | `EVTS` | `0x53545645` | `0x6f331b00` |
| `CAMS` | `0x534d4143` | `0x6f333b40` | `CLID` | `0x44494c43` | `0x6f330a30` |

`VERS` (observed, `0x6f3505c0`):

```c
if (in_EDX == 4) { uVar1 = read(); *(arg_8h + 0x3a8) = uVar1; return 1; }
(**(*arg_ch + 0xc))(arg_ch,2,"Invalid VERX section detected in model.\n");
```

→ the version lives at **model+0x3a8**, is exactly 4 bytes, and **no geometry/hierarchy reader in this binary reads it** (measured: the displacement `+0x3a8` occurs in 21 functions, all in the anim/export area `0x6f0ed6..0x6f2213`; none of the chunk readers `0x6f3484e0/0x6f348640/0x6f348ac0/0x6f348d00/0x6f3488a0/0x6f348720` touch it).

### 1.2 LOD — nothing filters on it (item 1, confirmed)

* The geoset record reader `fcn.6f348640` reads, in order: `u32, u32, u32, f32, vec3, vec3` and then the bounds array. There is **no version branch and no `lod`/`lod_name` read anywhere in the GEOS path**.
* `fcn.6f348ac0` (the 8 fixed sub-sections) reads `PVTX` as **u16 unconditionally** (`iVar5 = *arg_ch + uVar4 * 2;`), i.e. no `version >= 900` widening exists in the engine.
* Consequence: for a v800 file the engine reads exactly the bytes our parser reads for v800 — `matId, selGroup, selFlags, radius, min, max, nBounds, bounds[]`. The `lod`/`lod_name` pair in `crates/wc3-formats/src/mdx.rs:1477` is a v900+ construct that this binary never executes.

Verdict: **engine = v800 geometry, no LOD, no per-geoset LOD filter.** A renderer that drops or reorders geosets by a "LOD" field is diverging from the engine (and for v800 input the field does not exist).

### 1.3 Geoset record — byte-exact contract

`fcn.6f3484e0` (GEOS; 200-byte runtime struct per geoset, growable array at model+`0x1bac`/`0x1bb0`/`0x1bb4`):

```c
uVar2 = in_EDX / 200 + *(arg_8h + 0x1bb0);      // 200-byte HGEOSET per record
...
puVar3 = *(arg_8h + 0x1bb0) * 200 + *(arg_8h + 0x1bb4);
```

`fcn.6f348640` (one record):

```c
arg_8h_00 = read(); var_4h = 4;
iVar1 = fcn.6f348ac0(arg_8h,&var_4h);            // 8 fixed sections
uVar2 = read(); *(in_EDX + 0x90) = uVar2;        // material id
uVar2 = read(); *(in_EDX + 0xc0) = uVar2;
uVar2 = read(); *(in_EDX + 0xc4) = uVar2;
fVar3 = readf(); *(in_EDX + 0xac) = fVar3;       // radius
read3f(in_EDX + 0x94); read3f(in_EDX + 0xa0);    // bbox min / max
var_4h += 0x18; fcn.6f348990(&var_4h);           // nBounds * 28 bytes
iVar1 = fcn.6f348720(...);                       // UVAS / skip-unknown loop
```

`fcn.6f348ac0` element widths (observed, the `switch` over the static table at `0x6f794610`):

| section | width | reader | note |
|---|---|---|---|
| `VRTX`, `NRMS` | 12 B | `0x6f344f70` | float3 arrays |
| `PTYP` | 4 B | `0x6f348d00` | `if (tag != 0x50595450) "Invalid primitives type section in Geoset.\n"` |
| `PCNT` | 4 B | case 2 | u32 array |
| `PVTX` | **2 B** | case 3 | `iVar5 = *arg_ch + uVar4 * 2` |
| `GNDX` | **1 B** | case 4 | `sub.War3.exe_Ordinal_234` |
| `MTGC` | 4 B | case 2 | per-group matrix **count** |
| `MATS` | 4 B | case 2 | flattened node indices |

`fcn.6f348990` (bounds): reads `u32 n`, then per entry `f32 radius; vec3 min; vec3 max;` = **28 bytes**.
`fcn.6f348720` (tail): `UVAS` → `u32 count`, then `count × (tag 'UVBS', u32 count, count*8 B)`; any other tag is skipped by its own size field.

Validation (observed, `tools/mdx_geoset_check.py` replaying exactly the above): **62/62 geosets parsed byte-exact** and each record landed exactly on its `inclusiveSize` boundary in `leaves.mdx` (2), `Farm.mdx` (14), `TownHall.mdx` (36), `peasant.mdx` (6), `Archer.mdx` (4). `inclusiveSize` **counts its own 4 bytes** (`leaves.mdx`: records of 432 bytes, `432 = 4 + 428`).

Per-model constant found while validating: `nBounds` (the 28-byte entry count) is **constant across all geosets of a model and equals the sequence count** — TownHall 14/14, Farm 5/5, Archer 13/13. That is the **per-sequence geoset extent table** (`sequence_bounds` in our parser), and the sentinel `min = +FLT_MAX, max = -FLT_MAX` (bytes `ee ff 7f 7f` / `ee ff 7f ff`) marks "geoset not present in this sequence".

### 1.4 Material binding and hidden geosets

| item | engine | confidence |
|---|---|---|
| geoset → material | `geoset+0x90` = material index read from the record; file data confirms values stay inside the material count (TownHall matIds 0..8, Archer 0..2) | observed (reader) + inferred (binding code not located — the draw path receives the material through the device call) |
| hidden per sequence | per-sequence extent table with `±FLT_MAX` sentinel (§1.3) | observed (data) / inferred (the skip check itself was not located) |
| hidden by animation | geoset animation (GEOA) alpha track reaches 0 (§3.3) | observed (struct/defaults) |
| helper geosets | every node object (bone *and* helper) owns a pivot in `PIVT`; `MATS` indices address that shared node space (§2.2). Nothing in the geoset record distinguishes "helper" geometry — a helper is a bone with `flags` and no deformation of its own | observed (PIVT reader + node space) / inferred (semantics of helper) |

---

## 2. Skinning — the contract (item 2, highest priority)

### 2.1 The draw-time contract, exactly as the engine executes it

Device entry `fcn.6f0d3fe0` (`SetGeosetState`, 13 arguments) sets a block of globals that the draw path reads back:

```c
*0x6f7cad04 = arg_ch;    // vertex cursor
*0x6f7cad24 = arg_18h;   // normal stride
*0x6f7cacf4 = arg_20h;
*0x6f7cad28 = arg_8h;    // vertex count
*0x6f7cad10 = arg_24h;   // matrix-index cursor
*0x6f7cad2c = arg_10h;   // vertex stride
*0x6f7cad00 = arg_28h;   // matrix-index stride
*0x6f7cad30 = arg_30h;   // uv array base
*0x6f7cad48 = arg_14h;   // normal cursor
*0x6f7cacf8 = arg_1ch;   // colour cursor
*0x6f7cad08 = arg_2ch;   // uv0
*0x6f7cad0c = arg_34h;   // uv1
*0x6f7cad34 = arg_38h;
```

It also lazily sizes six scratch arrays on the device object — proof of what the skinned path writes back:

```c
*(in_ECX + 0xb88) = arg_8h;   // count of 12-byte positions at +0xb8c
*(in_ECX + 0xb94) = arg_8h;   // count of 12-byte normals   at +0xb98
*(in_ECX + 0xba0) = arg_8h;   // count of 4-byte values    at +0xba4
*(in_ECX + 0xbac) = arg_8h;   // count of 8-byte values    at +0xbb0
*(in_ECX + 0xbb8) = arg_8h;   // count of 8-byte values    at +0xbbc
```

`fcn.6f0d3bc0` then performs the actual skinning (quoted verbatim, trimmed):

```c
if (*(param_1 + 0x224) == 0) {                    // static geoset
    glVertexPointer(3,0x1406,*0x6f7cad2c,*0x6f7cad04);
    glNormalPointer(0x1406,*0x6f7cad24,*0x6f7cad48);
} else if (*(param_1 + 0x224) == 1) {             // skinned geoset
    glVertexPointer(3,0x1406,0xc,*(param_1 + 0xb8c));
    glNormalPointer(0x1406,0xc,*(param_1 + 0xb98));
    do {
        iVar18  = **0x6f7cad10 * 0x30;            // index * 48
        pfVar15 = *(param_1 + 0x19c) + iVar18;    // palette + index*48
        ...
        *pfVar16     = *pfVar15 * **0x6f7cad04 + pfVar15[6]*(*0x6f7cad04)[2]
                     + pfVar15[3]*(*0x6f7cad04)[1] + pfVar15[9];
        pfVar16[1]   = pfVar15[1]*W[0] + pfVar15[7]*W[2] + pfVar15[4]*W[1] + pfVar15[10];
        pfVar16[2]   = pfVar15[2]*W[0] + pfVar15[8]*W[2] + pfVar15[5]*W[1] + pfVar15[11];
        // normals: same basis, translation column dropped
        *pfVar16n    = pfVar15[0]*N[0] + pfVar15[3]*N[1] + pfVar15[6]*N[2];
        *0x6f7cad04 += *0x6f7cad2c;               // vertex cursor += vertex stride
        *0x6f7cad48 += *0x6f7cad24;               // normal cursor += normal stride
        *0x6f7cad10 += *0x6f7cad00;               // index cursor  += index stride
    } while (++var_4h != *0x6f7cad28);
}
```

**Contract (numeric, citable):**

1. Matrix = **48 bytes = 12 floats**, layout `col0[0..2], col1[3..5], col2[6..8], translation[9..11]`; position = `M·v` with translation, normal = `M3x3·n` **without** translation. Output arrays are 12 bytes/vertex.
2. Per-vertex skin index is a **u32 read through a strided cursor** (`**0x6f7cad10`), stride = `arg_28h`. The palette is `*(object+0x19c)` and the slot is `index` (no extra scaling beyond `0x30`).
3. `object+0x224` = 0/1 selects **static vs skinned**; `object+0x228` (0..5, `switch` at `0x6f0d3e80`) selects the vertex layout = 2 colour variants × {0,1,2} texture-coordinate sets, driving `glEnable/DisableClientState(GL_TEXTURE_COORD_ARRAY 0x8078)` and `glTexCoordPointer(2, GL_FLOAT, …)`.
4. The model→world transform is **not** part of the palette: it is applied by `glLoadMatrixf` (`fcn.6f0d4970`, device slot at `0x6f698340[20]`), i.e. palette matrices are **model-space, absolute** (pivot-inclusive), and the vertex buffer fed in is also absolute model-space (the static path feeds `*0x6f7cad04` = the raw vertex array).

### 2.2 Matrix groups — the resolution rule (proven, with measurements)

Group container `CMatrixGroup` is **20 bytes** and appended by `fcn.6f032c50`:

```c
piVar3 = in_ECX[2] + in_ECX[1] * 0x14;      // stride 0x14
in_ECX[1] = in_ECX[1] + 1;
*piVar3 = arg_8h;      // first MATS index
piVar3[1] = arg_ch;    // count of matrices in the group
piVar3[2] = uVar1;     // group slot index (returned)
piVar3[3] = -1; piVar3[4] = -1;              // dedup-tree child links
in_ECX[4] = in_ECX[4] + arg_ch;              // running total of matrices
```

`fcn.6f0337d0` builds extra groups for **contiguous ranges** of the group list (segment tree over `firstIndex/count`), and `fcn.6f032d10` canonicalises them, so a vertex's group can be a single group *or* an aggregate of several. `fcn.6f033070` constructs the per-geoset `HGEOSET`/`HGEOSETDATA` runtime objects that consume this.

Measured on real assets (`tools/mdx_geoset_check.py` + per-geoset histogram):

| model / geoset | verts | GNDX | MTGC (n) | Σ MTGC | MATS | max(GNDX) | distinct GNDX | max group size |
|---|---|---|---|---|---|---|---|---|
| TownHall g0 | 177 | 177 | 21 | **31** | 31 | **20** | **21** | 2 |
| Archer g0 | 331 | 331 | 56 | **101** | 101 | **55** | **56** | 3 |
| Archer g1 | 73 | 73 | 19 | **45** | 45 | **18** | **19** | **5** |
| Farm g0 | 165 | 165 | 3 | **3** | 3 | **2** | **3** | 1 |

Two invariants hold in **every** geoset measured (62 geosets):
* `max(GNDX) == MTGC_count - 1` and `distinct(GNDX) == MTGC_count` → **GNDX indexes the group array**;
* `Σ MTGC == MATS_count` → **MTGC holds per-group matrix counts**, `MATS` is the flattened list; group *g* owns `MATS[prefix(g) … prefix(g)+MTGC[g]-1]`.

Group sizes above 1 are not a corner case: 21 of TownHall's groups have 2 matrices, Archer has groups of 5.

### 2.3 How our renderer compares

| # | engine contract | our code | verdict |
|---|---|---|---|
| 2.1 | palette = per-node model-space matrices, 48 B, col0..col2 + translation in col3 | `crates/wc3-render/src/mesh.rs:325` `palette: array<mat4x4<f32>>` addressed as `palette[base + bone]`; `mesh.rs:44` comments "`unit.nodes.len()` handed to the palette" (all nodes, not bones-only) | **MATCH** (column-major mat4 with translation in column 3 ≡ engine's 3x4 with translation at floats 9..11) |
| 2.2 | normals transformed without translation | `mesh.rs:333-336` `(matrix * vec4<f32>(input.normal.xyz, 0.0)).xyz` | **MATCH** |
| 2.3 | GNDX → group → MATS range with prefix sum | `crates/wc3-anim/src/lib.rs:694-715` (`offsets` = prefix sum of `matrix_groups`, `group = vertex_groups[vertex]`, `bone = matrix_indices[group_offset + i]`) | **MATCH** |
| 2.4 | multi-matrix groups exist with sizes up to **5**; per-vertex influence budget in engine = whole group | `wc3-anim/src/lib.rs:694` `from_geoset_with_limit`, GPU upload caps at **4** and renormalises, `mesh.rs:325-360` shader has exactly 4 slots | **DIFF (small)** — groups of 5 lose one matrix and get renormalised; positions stay in the group's convex hull, so this shifts vertices by ≤ ~20 % of a 5-way average, not a detachment. Fix = raise the budget to 5 (or fold the engine's group average on the CPU) |
| 2.5 | palette index space = node ids shared by bones **and** helpers (`PIVT` order) | `mesh.rs:44` uses `unit.nodes.len()` | **MATCH** (needs a regression test, see T-2) |
| 2.6 | model→world applied once, after skinning (`glLoadMatrixf` slot) | `mesh.rs:8` "…bone palettes", vertex shader applies only the palette; the world matrix is a separate uniform (verified by the shader having no world matrix multiply) | **MATCH** |

### 2.4 Root-cause analysis for "doubled bodies / detached plates" (evidence-bounded)

Nothing in the contract above *requires* a doubled mesh, so the defect is in how our pipeline instantiates the contract. These are the three places where the engine's numbers are specific and easy to get wrong; each has a named test:

1. **Pivot double-application (best candidate for detached plates).** The engine's palette holds *absolute* model-space matrices and the vertex buffer is absolute model-space (§2.1 item 4). If our node matrix is composed as `parent * T(pivot_i - pivot_parent) * R * S` **and** the vertex stream has already been rebased by subtracting a bind pivot (or the palette matrix is built as `T(-pivot) * R * T(pivot)` relative to a *different* pivot), every vertex shifts by `pivot_i − pivot_ref` — per-plate offsets, i.e. exactly "detached plates". Test T-1 verifies this numerically on one model rather than by eye.
2. **Influence truncation at 4** (§2.3 row 2.4) — smooth-joint vertices bend wrongly; visible as seam tearing, never as a rigid offset.
3. **Index-space mismatch**: if a node list is rebuilt bones-only while `MATS` indexes the shared node space (`PIVT` order, §1.4), each bone resolves to the wrong matrix — parts fly to another joint ("doubled body" when two nodes collapse onto one matrix). Test T-2.

The two candidate defects are cheap to separate: T-1 is a pure-math check on one geoset with zero skinning influence ambiguity (all groups of size 1), T-2 is a table check.

---

## 3. Animation

### 3.1 Sequences (`SEQS`, `fcn.6f34e9e0`)

```c
arg_8h_00 = in_EDX / 0x84;                                   // count = chunkSize / 132
if (in_EDX % 0x84 != 0) → "Invalid SEQX section detected in model -- nonintegral number of sequences.\n"
...
sub.War3.exe_Ordinal_265(puVar4 + 0xf,0x50);                 // name[80] at +0x3c
*puVar4 = readU32();   puVar4[1] = readU32();                // interval begin/end
puVar4[2] = readF32(); puVar4[3] = readU32();                // moveSpeed, flags
puVar4[4] = readF32(); puVar4[5] = readU32();                // rarity, syncPoint
read3f(puVar4 + 8); read3f(puVar4 + 0xb);                    // extent min / max
puVar4[0xe] = readF32();                                     // trailing radius
puVar4 = puVar4 + 0x23;                                      // 35 dwords = 140 B in memory
```

**Record = 132 bytes** (80 name + 4·6 scalars + 24 extents + 4 radius); in-memory 140 B. The count is derived from the chunk size, *not* stored — our parser does the same (`crates/wc3-formats/src/mdx.rs`, `while r.remaining() >= 132`) and `Extent::read` includes the radius first (`mdx.rs:184-197`), so records line up. Verified on `TownHall.mdx`: 1848 B = 14 records, names `Birth`, `Stand`, `Stand Work`, … at stride 132.

Sequence selection by name is a hash lookup (`ANIM`, `ANIMHASH`, `CAnimNameHash`, `CAnimList` RTTI classes are present); the *name → index* table build was not decompiled (gap G-2), but the identity of `Stand/Walk/Attack/Death/Birth/Decay/Spell/Sit` as the MDL names is the file contract and is what the tables key on.

### 3.2 Interpolation and global sequences

* Interpolation enum: the engine carries the three names as strings — `Linear` (`0x6f7895e4`), `Hermite` (`0x6f789660`), `Bezier` (`0x6f78986c`) — and the canonical order is `0 = Linear, 1 = Hermite, 2 = Bezier` (inferred from order; the enum has exactly three cases — every track struct initialises its type to `2`, and the track readers take the value verbatim).
* **Default interpolation type is 2 for every track kind**: BVN defaults in the bone reader (`func_0x6f330ca0(2); func_0x6f330cd0(2); func_0x6f330d00(2)` → translation/rotation/scaling), layer defaults (`puVar5[0xf] = 2`, and `+0x1c`/`+0x3c`), geoset-anim defaults (`puVar3[4] = 2; puVar3[0xb] = 2; puVar3[0x12] = 2`).
* **Global sequence default is `-1`** (no global sequence): layer `+0x20`/`+0x40`, geoset anim `+0x14`/`0x1c`/`0x24` slots initialised to `-1`; `GLBS` holds u32 durations (`count = chunkSize / 4`, `"Invalid GLBX section ... nonintegral number of global sequences.\n"`), array at model+`0xc78`/`0xc7c`/`0xc80`.
* Key layouts: `KMTA`/`KMTF` 8 B per key, widened to 16 B when the track's interpolation type is > 1 (`iVar4 = 8; if (bVar9) iVar4 = 0x10;` with `bVar9 = 1 < *(arg_ch + 0x3c)`).

### 3.3 Geoset animation (`GEOA`, `fcn.6f347d20`)

Runtime struct **96 bytes**, array at model+`0x21f8`/`0x21fc`/`0x2200`. Defaults written at load (verbatim):

```c
puVar3[6] = 0x3f800000;   // 1.0f
puVar3[4] = 2;            // interpolation type (alpha)
puVar3[5] = 0xffffffff;   // global sequence = -1
puVar3[0xb] = 2; puVar3[0xc] = 0xffffffff;
puVar3[0x12] = 2; puVar3[0x13] = 0xffffffff;
puVar3[0x14] = 0x3f800000; puVar3[0x15] = 0x3f800000; puVar3[0x16] = 0x3f800000;
```

→ alpha defaults to **1.0**, colour to **1,1,1**, three independent tracks (alpha / colour / geoset-id), each with `interp = 2` and `globalSeq = -1`. A geoset is hidden either by its per-sequence extent sentinel (§1.3) or by an alpha track reaching 0.

### 3.4 Event objects and attachment points

Node space is uniform: `PIVT` supplies one `C3Vector` (12 B) per node object in file order, and `EVTS`/`ATCH`/`LITE`/`CAMS`/`CLID` are arrays in the same model object (observed field map from `fcn.6f02b820`):

| array | capacity | count | pointer | type string |
|---|---|---|---|---|
| pivots | `+0x4204` | `+0x4208` | `+0x420c` | `.?AVC3Vector@NTempest@@` |
| camera sections | `+0x4544` | `+0x4548` | `+0x454c` | `.?AUMDLCAMERASECTION@@` |
| event objects | `+0x4624` | `+0x4628` | `+0x462c` | `.?AUMDLEVENTSECTION@@` |
| particle emitters 2 | `+0x4c10` | `+0x4c14` | `+0x4c18` | `.?AUMDLPARTICLEEMITTER2@@` |
| collision shapes | `+0x545c` | `+0x5460` | `+0x5464` | `.?AUMDLCOLLISIONSHAPE@@` |
| ribbon emitters | `+0x55e8` | `+0x55ec` | `+0x55f0` | `.?AUMDLRIBBONEMITTER@@` |
| sequences | `+0x3ac` | `+0x3b0` | `+0x3b4` | — |
| global sequences | `+0xc78` | `+0xc7c` | `+0xc80` | — |
| materials | `+0xdc0` | `+0xdc4` | `+0xdc8` | — |
| bones | `+0x2610` | `+0x2614` | `+0x2618` | 172 B each |
| geoset anims | `+0x21f8` | `+0x21fc` | `+0x2200` | 96 B each |

Attachment/event *positions* come from the node's pivot, so a renderer that resolves attachment points by a separate table (or by bone-local transforms) will place them wrong in exactly the same way as §2.4-1.

---

## 4. Material and render state (item 4, highest priority)

### 4.1 Layer / material structs (observed)

`fcn.6f34c540` (MTLS): material = **164 bytes**, `+0x00` layers capacity, `+0x04` layers count, `+0x08` layers pointer, `+0x9c` priority plane, `+0xa0` flags.

`fcn.6f34c7c0` (layer): **72 bytes**, read order exactly

```c
uVar2 = readU32();            // inclusiveSize
*arg_ch       = readU32();    // +0x00 filter mode
arg_ch[0x04]  = readU32();    // +0x04 shading flags
arg_ch[0x08]  = readU32();    // +0x08 texture id
arg_ch[0x24]  = readU32();    // +0x24 texture animation id
arg_ch[0x28]  = readU32();    // +0x28 coord id
arg_ch[0x44]  = readF32();    // +0x44 alpha  (default 1.0f, set by the MTLS pre-init)
var_4h += 0x18;
// then tagged KMTA (alpha keys, 8 or 16 B/key) and KMTF (texture-id keys, 8 B/key)
```

Layer defaults from `fcn.6f34c610`: `+0x20 = -1`, `+0x24 = -1`, `+0x3c = 2`, `+0x40 = -1`, `+0x44 = 1.0f`.

### 4.2 Blend modes — the numeric table (the answer to "doubled bodies" via wrong state is *not* here, but the plate colours/tests are)

Both backends are driven from two parallel tables; GL (`fcn.6f0cdf20`):

```c
if (param_1[0xe8] == 0) { glDisable(...); glDisable(0xbc0); }      // 0xbc0 = GL_ALPHA_TEST
else {
    glEnable(0xbe2);                                               // GL_BLEND
    glBlendFunc(tbl_src[param_1[0xe8]], tbl_dst[param_1[0xe8]]);
    glEnable(0xbc0);                                               // GL_ALPHA_TEST
    glAlphaFunc(0x206, fcn.6f0c99a0(param_1[0xe8]) * 0.003921568859368563);
}
```

D3D twin (`fcn.6f0d30e0`):

```c
if (param_1[0xe8] == 0) { SetRenderState(0x1b,0); SetRenderState(0xf,0); return; }   // ALPHABLENDENABLE, ALPHATESTENABLE
SetRenderState(0x1b,1);
SetRenderState(0x13, tbl_src[mode]);   // SRCBLEND
SetRenderState(0x14, tbl_dst[mode]);   // DESTBLEND
SetRenderState(0xf,1);
SetRenderState(0x19,7);               // ALPHAFUNC = D3DCMP_GREATEREQUAL
SetRenderState(0x18, fcn.6f0c99a0(mode));   // ALPHAREF
```

Tables (dumped from `0x6f767254`, `0x6f76726c`, `0x6f76784c`, `0x6f767864`, `0x6f766648`):

| mode | GL src | GL dst | D3D src | D3D dst | alpha ref | effective GL threshold | meaning |
|---|---|---|---|---|---|---|---|
| 0 | `ONE` | `ZERO` | ONE | ZERO | 255 | — (blend+test off) | opaque |
| 1 | `SRC_ALPHA` | `ONE_MINUS_SRC_ALPHA` | SRCALPHA | INVSRCALPHA | **192** | **0.7529** | transparent (hard alpha cut + blend) |
| 2 | `SRC_ALPHA` | `ONE_MINUS_SRC_ALPHA` | SRCALPHA | INVSRCALPHA | **4** | **0.0157** | blend |
| 3 | `SRC_ALPHA` | `ONE` | SRCALPHA | ONE | 4 | 0.0157 | additive |
| 4 | `DST_COLOR` | `ZERO` | DESTCOLOR | ZERO | 4 | 0.0157 | modulate |
| 5 | `DST_COLOR` | `SRC_COLOR` | DESTCOLOR | SRCCOLOR | 4 | 0.0157 | modulate 2x |

`glAlphaFunc` uses **`GL_GEQUAL` (0x206)**, ref scaled by `1/255` (float at `0x6f699890` = 0.003921568859368563); D3D uses `GREATEREQUAL`. (GL enum check: `0x206` is `GL_GEQUAL`; `0x200..0x207` = NEVER/LESS/EQUAL/LEQUAL/GREATER/NOTEQUAL/GEQUAL/ALWAYS.)

**Engine mode index range is 0..5.** The MDX layer filter mode range is 0..6 (1 transparent, 2 blend, 3 additive, 4 add-alpha, 5 modulate, 6 modulate2x). Therefore *some* fold must exist between the file value and the table index; the fold is done before the device call (the device never sees a value > 5). Two readings are compatible with the tables — mode == filterMode for 1..3 and either `4→3, 5→4, 6→5` (AddAlpha folded into additive) or `4→4, 5→5, 6→?` (Modulate folded into modulate2x) — and I could not locate the fold (gap G-3). What *is* proven: the six (src,dst,ref) tuples above, the `0x206`/`7` compare, and the 192/4 split. The disambiguating test is T-4.

Texture environment (`fcn.6f0cdf20`, per texture unit, up to 2):

```c
glTexEnvi(0x2300, 0x2200, (*piVar7 != 1) + 0x2100);   // GL_TEXTURE_ENV_MODE = GL_MODULATE (0x2100) unless layerMode == 1 -> GL_DECAL (0x2101)
```

D3D twin adds the stage configuration that the GL path does not re-state (observed in `fcn.6f0d30e0`, `fcn.6f0d37c0(unit, state, value)`):

* `TEXCOORDINDEX(11) := texture unit index` → **coord set == unit** (set 0 for unit 0, set 1 for unit 1);
* `ADDRESSU(13)`/`ADDRESSV(14)` from bits 2/3 of the texture flags, table `0x6f7678ac = [CLAMP, WRAP, WRAP]`;
* `MAGF(16)/MINF(17)/MIPF(18)` from `(flags & 3) * 12 + 0x6f76787c`;
* `COLOROP(1)`/`ALPHAOP(4)` from `0x6f7678b4 = [DISABLE, MODULATE, BLENDTEXTUREALPHA, ADD]` and `0x6f7678c8 = [DISABLE, MODULATE, SELECTARG2, SELECTARG2]`, indexed by the layer's *texture mode* field;
* the GL path's `glTexEnvi` DECAL case is the analogue of `BLENDTEXTUREALPHA` for team-colour layers.

State caching (`fcn.6f0cdf20` head): the flush is skipped unless `device[0x148] != 0` or the 6-dword state block at `device+0x38c` differs; after the flush the block is copied to `device+0x1508`. A renderer that re-applies state per geoset is functionally correct but never matches the engine's draw-call count.

### 4.3 Our renderer vs the blend/alpha contract

| # | engine | our code | verdict |
|---|---|---|---|
| 4.1 | alpha test ref = **192/255 = 0.7529** for device mode 1 and **4/255 = 0.0157** for modes 2..5, compare `>=` (which file filter mode maps to device mode 1 is gap G-3; the 192-vs-4 split itself is proven) | `crates/wc3-render/src/mesh.rs:417-421` `if (alpha < 0.5) { discard; }` → threshold **0.5** for every mode | **DIFF (numeric)** — a 0.5 cut is not any engine mode: vs mode 1 it *keeps* alpha 0.5..0.7529 that the engine culls (halo/fringe pixels the engine discards), vs modes 2..5 it *drops* alpha 0.0157..0.5 that the engine keeps (dim glow/fade pixels lost) |
| 4.2 | filter mode 1 = *blend + alpha test*, filter 2 = *blend + ~no test* | `mesh.rs:445-454` `from_blend`: `1 => Cutout`, `2 => Blend`; cutout has **no blending** and writes depth | **DIFF** — transparent layers lose their soft alpha blend in our pipeline |
| 4.3 | mode 4 = modulate (`DST_COLOR/ZERO`), mode 5 = modulate2x (`DST_COLOR/SRC_COLOR`) | `mesh.rs:449` `3 \| 4 => Additive`, `5 => Modulate`, `6 => Modulate2x` | **DIFF** — filter mode 4 renders additive instead of modulate; 5/6 shifted by one against the engine's table |
| 4.4 | modulate pair = `(DST_COLOR, ZERO)`; modulate2x = `(DST_COLOR, SRC_COLOR)` | `mesh.rs:652-675` modulate = `(Zero, Src)` **reversed factors**; modulate2x = `(Dst, Src)` ✓ | **DIFF (reversed pair)** — `(src=Zero, dst=Src)` is the same *product* as `(src=Src, dst=Zero)` only when the shader output is the texture; with a tinted shader output the two differ, and `wgpu::BlendFactor::Zero` as *source* factor zeroes the incoming fragment before adding `dst*Src` — i.e. it computes `dst*src` only if the shader multiplies by hand. Test T-5 pins this numerically |
| 4.5 | additive = `(SRC_ALPHA, ONE)`, alpha channel `(ONE, ONE)` | `mesh.rs:637-651` colour `(SrcAlpha, One)` ✓ | **MATCH** |
| 4.6 | two texture units, coord set == unit, second unit = DECAL/MODULATE | `emitters.rs:494` uses layer 0 only for ribbons; mesh path: one texture per draw (checked by the pipeline layout having a single texture) | **DIFF/limitation** — two-layer materials (team colour + diffuse) cannot be reproduced in one draw; our second layer is dropped or drawn as a second pass. This is the most likely cause of "plates missing their team colour" |

---

## 5. Particles and ribbons

* `PRE2` (`fcn.6f33ae90`): runtime emitter = **528 bytes**; array at model+`0x4c10`/`0x4c14`/`0x4c18`. Body reader `fcn.6f33afd0` starts with the generic object (`fcn.6f32ee40`) then reads scalars that land `0x1c` (28 B) apart — each animated scalar is a 28-byte block (`value`, key array, interp type, global sequence), which is why the raw offsets look sparse:

```c
fVar7 = readF32(); *(in_EDX + 0xa4)  = fVar7;   ...
fVar7 = readF32(); *(in_EDX + 0xc0)  = fVar7;
fVar7 = readF32(); *(in_EDX + 0xdc)  = fVar7;
fVar7 = readF32(); *(in_EDX + 0xf8)  = fVar7;
fVar7 = readF32(); *(in_EDX + 0x114) = fVar7;
fVar7 = readF32(); *(in_EDX + 0x118) = fVar7;
fVar7 = readF32(); *(in_EDX + 0x150) = fVar7;
fVar7 = readF32(); *(in_EDX + 0x134) = fVar7;
uVar4 = readU32(); *(in_EDX + 0x16c) = uVar4;   // four u32: rows/cols/filter/head-tail family
uVar4 = readU32(); *(in_EDX + 0x170) = uVar4;
uVar4 = readU32(); *(in_EDX + 0x174) = uVar4;
uVar4 = readU32(); *(in_EDX + 0x178) = uVar4;
fVar7 = readF32(); *(in_EDX + 0x17c) = fVar7;   // then 0x180, 0x18c, 0x188, 0x184
```

Field *names* (speed/variation/latitude/gravity/lifespan/decay/emissionRate/width/length) are the MDL names in the same order but the pairing to these offsets was not completed (gap G-1) — **no numeric emitter default is asserted here**, because the engine's defaults are set in the same reader and I have only the first half of it decompiled.

* `RIBB` (`fcn.6f3365e0`): the reader's error strings enumerate the exact sub-records the engine expects — visibility keys, colour, height above, height below, alpha, texture-slot keys, generic object — which is the ribbon record contract:

```
"Error reading RibbonEmitter."
"Error reading visibility keys portion of RibbonEmitter."
"Error reading color portion of RibbonEmitter."
"Error reading height above portion of RibbonEmitter."
"Error reading height below of RibbonEmitter."
"Error reading alpha portion of RibbonEmitter."
"Error reading texture slot keys portion of RibbonEmitter."
"Error reading gen object portion of RibbonEmitter."
```

* Runtime classes present (observed RTTI strings, `analysis/agent-re/render/raw/rtti_classes.txt`): `CParticle`, `CParticle2`, `CParticleEmitter`, `CPlaneParticleEmitter`, `CSortableParticleRecord`, `CRibbonEmitter`, `CRibbonVertex`.
* Our code (`crates/wc3-render/src/emitters.rs`) implements 4-influence-free instanced emitters and takes a **ribbon's** blend from the first layer of its material (`emitters.rs:494-494`) — consistent with the engine's material-driven blending, but the layer→mode fold of §4.2 applies to it too.

---

## 6. Terrain and water — not covered in this pass

No numeric terrain/water contract is asserted. What exists to anchor the next pass:

| anchor | value |
|---|---|
| RTTI classes | `TerrainGeosetLayer`, `CliffGeoset`, `CombinedCliffGeoset`, `CombinedCliffPrim`, `CliffPrim`, `CliffTexture`, `RegionGeoset`, `WEGeoset`, `WaterLevelInfo`, `TerrainDeformation`, `TerrainGridModel`, `TerrainWaterModel`, `ShoreWaveModel`, `ShadowTexture`, `TerrainImageTextureHash`, `TerrainTileHash` |
| strings | `"Terrain geoset cache: %d del, %d load, mem - %d used / %d alloc (%.1f%%)"` @ `0x6f79da10`, `"Geoset Cache Statistics"` @ `0x6f79ce84`, `"Geoset cache"` @ `0x6f79d554` |
| measured so far | water/terrain build their own geosets (`CliffGeoset`, `CombinedCliffGeoset`, `ShoreWaveModel`) and go through the same device call as models — so §2/§4 apply to them unchanged |

Concrete next step: xref the `TerrainGeosetLayer`/`CliffGeoset` allocation sites (`func_0x6f37bd76(size, ".?AUTerrainGeosetLayer@@", …)`) and decompile the layer-mask/alpha and 5-bit variation selection from there; the pointer comparison in `fcn.6f60c160` (which touches geoset `+0x74`/`+0x84`) is a promising start.

### 6.1 Closed 2026-11-16 (LEDGER 441): water border, shoreline, cliff faces

Measured against the Game.dll anchors and the original data:

| contract | engine anchor | our implementation |
|---|---|---|
| water is per-corner W3E surface + Water.slk `height` offset, `Smin/Smax/Dmin/Dmax` depth ramp with authored alpha | `TerrainWaterModel`, `WaterLevelInfo`, HiveWE `data/shaders/water.vert` | `terrain_geometry::water_depth_color` + `terrain_render::water_ramp`; per-corner level `final_water_height` |
| water animation is `numTex=45` frames stepped at `texRate` fps | Water.slk `numTex`/`texRate`; `Water00.blp`..`Water44.blp` | `texture_2d_array` + `fs_water` frame = `floor(time*texRate) % frames` |
| shoreline is authored shore geometry (`Shoreline`, `ShorelineWave`, inside/outside corners) | `ShoreWaveModel`; Water.slk `shoreDir`/`shoreSFile`/`shore*Var` | `terrain_geometry::shoreline` band + `fs_shore` sampling `Textures\Shoreline1.blp` (binding 7) on every W3E water-flag transition |
| cliffs are separate geosets (`CliffGeoset`, `CombinedCliffGeoset`, `CliffPrim`) with the map's cliff texture; the ground cell under the transition is replaced | RTTI anchors §6; HiveWE `CliffMesh`/`update_cliff_meshes` | `terrain_geometry::cliff_blocks` flat plateau + vertical faces; map cliff BLPs from `war3map.w3e` cliff list via `CliffTypes.slk` as array layers 8..; quadrant UV (`variation % 4`, `/ 4`) |
| the original client never shows past the camera bounds; the mobile camera can zoom out | camera-bounds clamp | `terrain_geometry::water_border` 8-tile deep-water frame with a 3-row alpha fade |

Evidence: `analysis/agent-re/render/TERRAIN-WATER-441.md`,
`analysis/agent-qa/terrain/{before,after}-justice-map-edge-2414x1080.png`,
`crates/wc3-render/tests/terrain.rs` (`render_terrain_water_shoreline`,
`render_terrain_cliff_face`), `crates/wc3-render/examples/terrain_edge.rs`.


---

## 7. Comparison matrix (verdicts + proposed tests)

| # | subsystem | engine contract | address / command | our file:line | verdict | proposed test |
|---|---|---|---|---|---|---|
| T-1 | skinning · pivot | palette = absolute model-space matrices; vertices absolute; world applied once by `glLoadMatrixf` | `0x6f0d3bc0`, `0x6f0d4970` | `crates/wc3-render/src/mesh.rs:325-360` (shader has no world multiply) | verify | take one model with a single-bone geoset, compute `Σ wᵢ Mᵢ v` on the CPU from our palette and compare against the engine's own transform for 20 vertices; tolerance 1e-4. A constant offset per node = pivot double-application |
| T-2 | skinning · index space | `MATS` indexes the shared node space created by `PIVT` (bones *and* helpers) | `0x6f33f170`, `0x6f032c50` | `mesh.rs:44`, `wc3-anim/src/lib.rs:694` | verify | assert `max(MATS) < nodes.len()` for `TownHall/Farm/Archer/peasant`; assert our palette is built from `nodes` (not `bones`), and that `nodes[i].pivot == PIVT[i]` |
| T-3 | skinning · group size | group sizes up to 5 enumerated; Σ MTGC == MATS count; GNDX max == MTGC−1 | `0x6f032c50`, measurements §2.2 | `wc3-anim/src/lib.rs:694` (limit 4) | **DIFF** | unit test: for every geoset of the 4 fixtures assert the two invariants, then assert `max_influences >= 5` when the file has a 5-matrix group (Archer g1) |
| T-4 | material · filter fold | 6 device modes, exact `(src,dst,ref)` triples | `0x6f0cdf20` + tables | `mesh.rs:445-454` | **DIFF** | build a 7-layer test model (one layer per filterMode 0..6), render, and compare against the engine's `glBlendFunc` trace; the fold is proven when layer 4 reproduces `(DST_COLOR, ZERO)` |
| T-5 | material · modulate pair | `(DST_COLOR, ZERO)` for modulate; `(DST_COLOR, SRC_COLOR)` for modulate2x | tables | `mesh.rs:652-675` | **DIFF** | shader-level test: render a 50 % grey quad with tint `(0.5,0.5,0.5,1)` over a white target; engine result 0.5, our current pipeline result depends on the shader's own multiply → assert equality with the engine tuple |
| T-6 | material · alpha test | threshold = 192/255 for transparent, 4/255 otherwise, compare `>=` | `0x6f0c99a0`, `0x6f699890` | `mesh.rs:417-421` (0.5) | **DIFF** | render a 0.5..1.0 alpha ramp texture through filter mode 1 and assert the first visible texel is at 0.753, not 0.5 |
| T-7 | material · two layers | 2 texture units, coord set == unit, second unit DECAL/MODULATE | `0x6f0cdf20`, `0x6f0d30e0` | `emitters.rs:494` (layer 0 only), mesh path single texture | **DIFF** | Footman material (layer0 team colour blend 0, layer1 diffuse blend 2): assert the drawn result contains the team tint *and* the diffuse texture |
| T-8 | model · LOD | no LOD field, no version branch in the geometry path | `0x6f348640`, `0x6f348ac0` | `crates/wc3-formats/src/mdx.rs:1477` (v>800 only) | **MATCH** for v800; unused for shipped assets | parser test: assert a v800 geoset consumes exactly the record and that no `lod` field is read |
| T-9 | model · sequence records | 132-byte records, count = size/132, trailing radius | `0x6f34e9e0` | `mdx.rs` `while r.remaining() >= 132`, `Extent` includes radius | **MATCH** | parser test on `TownHall.mdx`: 14 sequences, names `Birth/Stand/Stand Work/…`, and `SEQS` size % 132 == 0 |
| T-10 | model · per-sequence extents | geoset `nBounds` == sequence count, sentinel `±FLT_MAX` marks "absent" | `0x6f348990`, measurements §1.3 | `mdx.rs:184-206` `Extent::is_empty` | **MATCH** | assert `geoset.sequence_bounds.len() == model.sequences.len()` for the 4 fixtures and that Archer's arrow geoset is empty in `Stand` |
| T-11 | animation · track defaults | interp default 2, globalSeq default −1, alpha default 1.0, colour default (1,1,1) | `0x6f3447f0`, `0x6f347d20`, `0x6f34c7c0` | `wc3-formats/src/mdx.rs` (Option tracks) | verify | assert that a track without keys evaluates to the default and not to 0 (a 0 default hides every geoset) |
| T-12 | animation · global sequences | `GLBS` u32 ms durations; `-1` = unbound | `0x6f34ec40` | `mdx.rs` global_sequence | MATCH | assert a layer with `globalSeq = -1` is time-driven by the sequence, and one with an index loops modulo its duration |

---

## 8. Gaps (no silent omissions)

| id | gap | why it is not closed | exact next step |
|---|---|---|---|
| G-1 | PRE2 field-name ↔ offset mapping and emitter defaults (gravity, latitude, spawn rate) | only the first ~90 lines of `fcn.6f33afd0` were decompiled before the budget ran out; the scalars are 28 B apart, so names cannot be assigned from offsets alone | `pdg` the rest of `fcn.6f33afd0` (0x6f33afd0–0x6f33bc00) and map each 28-byte block to the MDL field order |
| G-2 | sequence-name → sequence-index selection (`Stand`, `Walk`, `Attack`, …) | the hash table build is in the `ANIM`/`CAnimNameHash` area; not decompiled | xref `.?AUCAnimNameHash@@` (`0x6f7568xx`) and decompile the insert/lookup pair |
| G-3 | filterMode (0..6) → device mode (0..5) fold | the fold happens above the device call and was not located | set a hardware breakpoint on `device+0x3a0` (blend field) and step the layer draw; or find the switch that reads `layer+0x00` |
| G-4 | terrain/water numbers | not started (§6) | see §6 anchors |
| G-5 | "hidden geoset" skip check | the sentinel data is proven, the branch that consumes it is not | xref `geoset+0xb4`/`+0xb8` consumers |
| G-6 | team colour/glow replacement | not decompiled; the two-layer material is the mechanism | see T-7 |

---

## 9. Evidence index

| file | content |
|---|---|
| `raw/tft.strings.txt` | `rabin2 -zz` of TFT `Game.dll` (129 273 strings) |
| `raw/rtti_classes.txt` | 1 743 engine class/struct names recovered from the binary |
| `raw/tft.vtables.txt` | RTTI scan output (`tools/pe_rtti.py`; empty for this binary — its type names are a string blob, not MSVC descriptors) |
| `raw/mdx_top_parse.pdg.txt` | `fcn.6f32bcf0` chunk dispatch |
| `raw/mdx_vers.pdg.txt`, `raw/mdx_geos.pdg.txt` | `VERS` / `GEOS` readers |
| `raw/geosub/pdg_0x6f3486*.txt` … | geoset record + 8 fixed sections + bounds + UVAS |
| `raw/skin_transform.pdg.txt` | `fcn.6f0d3bc0` — software skinning (the contract) |
| `raw/geoset_draw.pdg.txt` | `fcn.6f0d3fe0` — 13-argument device state |
| `raw/glstate/pdg_0x6f0cdf20.txt`, `raw/drawmode/pdg_0x6f0d30e0.txt` | GL / D3D8 state flush |
| `raw/glstate/pdg_0x6f0c99a0.txt` | alpha-ref table accessor |
| `raw/mats/*.pdg.txt` | `MTLS`, material body, `LAYS`, layer body |
| `raw/readers/*.pdg.txt` | `SEQS`, `GLBS`, `GEOA`, `BONE`, `PIVT`, `PRE2`, `RIBB` |
| `raw/mgroup/*.pdg.txt` | `CMatrixGroup` container + group tree |
| `raw/pivot/*.pdg.txt` | `PIVT` copy + the per-model field map |
| `tools/xref.py`, `tools/pe_rtti.py`, `tools/pdg_multi.sh`, `tools/mdx_geoset_check.py` | the analysis tools above (reusable, no third-party deps) |
| `constants.json` | machine-readable constants (blend modes, alpha thresholds, struct sizes, layout offsets) |
