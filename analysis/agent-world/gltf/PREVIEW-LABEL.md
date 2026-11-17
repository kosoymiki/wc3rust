# Preview labels (honest art standard)

- `TownHall.gltf` / `GreatHall.gltf`: geometry+texture exports of the real MDX
  files from the operator's install (`tools/mdx-gltf`).
- `preview-TownHall.png` / `preview-GreatHall.png`: software-rasterizer previews
  of that real geometry (`tools/gltf_preview.py`, numpy z-buffer, untextured
  shading, ground line at y=0). Not hand-drawn art, not claimed as final
  Blizzard-level art; they verify geometry, orientation and the model pivot.
- Authoritative visual path remains the in-game wgpu/Vulkan renderer.
