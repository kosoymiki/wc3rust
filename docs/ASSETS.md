# ASSETS — форматы, pipeline, recompilation

## 1. Источники данных

| формат | что | читаем | примечание |
|---|---|---|---|
| MPQ | classic WC3 (`War3.mpq`, `War3x.mpq`, карты) | StormLib-compatible reader | listfile/encryption/protected maps |
| CASC | WC3 1.30+/Reforged | CascLib-compatible reader | BLTE, encoding keys, root handlers |
| MDX/MDL | 3D models | parser → glTF | versions 800/900/1000/1100 |
| BLP1 | textures | decoder → KTX2 | BLP0/BLP2 — отдельно; BLP1 основной |
| SLK/INI/TXT | data tables | typed parser | units/abilities/items/upgrades |
| W3M/W3X | maps | map reader | terrain, doodads, regions, triggers, pathing |
| DDS/TGA | textures | decoder | DXT1/3/5, BC5, RGBA |
| WAV/MP3 | audio | symphonia/kira | UI/units/music |

## 1.1 Runtime libraries

`wgpu` + `naga` (Vulkan/WGSL), SDL3 (platform/events/audio), `gltf` (scene import), `image`
(PNG/WebP), `ktx2` + `basis-universal` (GPU textures), `parry` (collision), `kira` + `symphonia`
(audio), `serde` + `postcard`/`rkyv` (cache), `tracing` (instrumentation). Licenses verified
before shipping; SDL3 zlib, wgpu Apache/MIT, parry/kira Apache-2.0.

## 2. Pipeline

```
user data
  → source index (MPQ/CASC/loose)
  → raw extract
  → decode (MDX/BLP/SLK/...)
  → transcode (glTF/KTX2/typed tables)
  → optimize (mips, ASTC/BC, LOD, atlas)
  → pack/cache (content hash, dependency graph)
  → runtime asset DB (async, streaming)
  → GPU upload / sim data
```

**Правило:** runtime никогда не читает MPQ/CASC напрямую; только `wc3-assetdb`.
Offline CLI (`tools/wc3extract`) делает тяжёлую конвертацию; на устройстве — кэш.

## 3. MDX → glTF

- Coordinate systems: WC3 Z-up → glTF Y-up; scale units; pivot/bone rest transforms.
- Nodes: bones/helpers/attachments; geosets → primitives; skin weights (max 4 influences?).
- Materials: layers, blend modes, two-sided, unshaded, team color, replaceable IDs, environ maps.
- Textures: paths → BLP assets; wrap/filter; alpha.
- Animations: sequences → glTF animations; global sequences; interpolation (linear/hermite/bezier);
  non-looping/looping; rarity.
- Extras: collision shapes, extents, events, attachment points, particle emitters, ribbons,
  cameras, face effects.
- Team color/replaceable IDs: shader uniforms, not baked textures.
- Output: glTF 2.0 + KTX2 + metadata JSON; validate with `gltf-validator`/own tests.
- Round-trip: preserve unknown chunks in metadata for debugging; do not silently drop.

## 4. BLP → KTX2

- BLP1 header: magic, version, width/height, mip count, compression, alpha depth, palette/JPEG.
- Decode paletted + JPEG; alpha 8/4/1/0; mip levels.
- Convert to RGBA8; optional premultiply; sRGB for color, linear for masks.
- Encode ASTC 4×4/6×6 (mobile), ETC2 (fallback), BC7/BC1 (desktop); mips; KTX2 container.
- UI atlases may stay PNG/WebP for crisp text; world textures KTX2.
- Test: decode→PNG visual diff, GPU sample, mip generation, alpha edges.

## 5. Data tables

- SLK: rows/columns, typed cells; INI: sections/key=value; TXT: tab-separated.
- Build typed structs for units, abilities, items, upgrades, buffs, doodads, destructables.
- Object data: original + custom (map) modifications; merge rules; IDs.
- Validation: required fields, ranges, references (icon/model/sound), cycles.
- Output: binary/rkyv/postcard cache for fast load; JSON for debugging.

## 6. Maps (W3M/W3X)

- MPQ container; `war3map.w3e` terrain, `w3i` info, `w3u/w3a/...` object data, `war3map.j` JASS,
  `war3map.wpm` pathing, `war3map.shd` shadow, `war3map.doo` doodads, `war3mapUnits.doo` units.
- Validate map hash; reject malformed; sandbox scripts.
- Terrain: heightmap, tileset, cliffs, water; pathing grid; fog/visibility.

## 7. Shader strategy

- WGSL for all pipelines; compile at build/runtime; pipeline cache.
- PBR: albedo, normal, ORM (occlusion/roughness/metallic), emissive, team color, alpha.
- Terrain: splat maps, cliff, water; fog of war as texture.
- Shadows: cascaded shadow maps; blob shadows low profile.
- UI: signed-distance text or bitmap fonts; 9-slice; safe areas.
- Variants: graphics set (classic/HD), quality (low/mid/high), platform (ASTC/BC), team color.
- No runtime HLSL/D3D9; offline conversion if source shaders exist.

## 8. Xbox 360 / native port track (separate)

- Xenos uCode container → HLSL via `XenosRecomp`; DXC → DXIL/SPIR-V.
- Texture tiling/endianness: X360 tiled formats → linear; DXT/BC; endian swap.
- Xenia/libxemit as research references; not a WC3 dependency.
- Use only for future native ports with legally owned games; no game assets in repo.
- Track status: R&D, not on critical path for WC3 PC.

## 9. Performance budgets

- Texture VRAM: classic <256 MB, HD <1 GB; ASTC 4×4/6×6; mips; streaming.
- Draw calls: <500/frame target; instancing; batching; culling.
- Asset load: cold start <4 s classic; map load <5 s; background decode.
- Cache: content hash; LRU; eviction; no per-frame allocations.
- Compression: KTX2 zstd? GPU transcoding; avoid runtime CPU decode.

## 10. Tests

- Golden corpus: `War3.mpq`, sample maps, MDX/BLP from classic/Reforged.
- Parser fuzz: malformed headers, truncated, huge counts; no panic/OOM.
- Round-trip: MDX→glTF→render vs original viewer; BLP→PNG diff.
- GPU: screenshot diff, mip/alpha, color space, team color.
- Performance: load time, VRAM, draw calls, frame time.
