# Figma → HUD pipeline

Turns the operator's Figma file into (a) a HUD-pack manifest that
`crates/wc3-game/src/hudskin/pack.rs` can load, (b) exported PNGs, and (c) a
pixel diff against the game's real layout dump. The diff is the ground truth
that replaces hand-tuned offsets: an element is placed where the design says
it is, and when it is not, the table shows by how many pixels.

```
tools/figma_pull.py    fetch + extract + export + manifest
tools/figma_diff.py    Figma rects vs `hud_layout` dump, exit 1 over threshold
analysis/figma/        raw JSON, node extraction, PNGs, manifest, diff reports
```

## Endpoints and auth

The personal access token is sent as the `X-Figma-Token` header. The operator's
token lives in `~/.config/h2/figma.env` (mode 0600, `FIGMA_TOKEN=...`); load it
in the same shell as the call:

```bash
set -a; . ~/.config/h2/figma.env; set +a
python3 tools/figma_pull.py --me
```

Verified 2026-11-16: `GET https://api.figma.com/v1/me` → HTTP 200, user
`Mikel Lair`, keys `email, handle, id, img_url`. The token is never printed,
never written to disk and never sent to the S3 URLs that `/v1/images` returns.

| Call | Used for |
| --- | --- |
| `GET /v1/me` | token check (`--me`) |
| `GET /v1/files/{key}?depth=N` | the document tree (depth-limited, `0` = full file) |
| `GET /v1/images/{key}?ids=…&format=png&scale=S&use_absolute_bounds=true` | PNG export (one request for all ids) |

Docs: <https://developers.figma.com/docs/rest-api/> (access date 2026-11-16);
rate limits: <https://developers.figma.com/docs/rest-api/rate-limits/> (access
date 2026-11-16, “Rate limits are per-minute unless otherwise specified”,
updated limits in effect since 2025-11-17).

### Rate limits (observed on the docs page)

* Tier 1 = `GET file`, `GET file nodes`, `GET image`.
  Dev/Full seats: 10/min (Starter), 15/min (Professional), 20/min
  (Organization/Enterprise). View/Collab seats: **up to 20 per month**.
* Tier 2 (25–100/min for Dev/Full) covers folders/projects/teams; this token
  cannot call them (see below).
* Exceeding the bucket returns HTTP 429 with `Retry-After`,
  `X-Figma-Plan-Tier`, `X-Figma-Rate-Limit-Type`, `X-Figma-Upgrade-Link`.
  `figma_pull.py` honours `Retry-After` (max 60 s) and retries up to 4 times;
  export uses **one** `/v1/images` request for all node ids to stay inside the
  Tier-1 budget.

### No file listing (why `--file` is mandatory)

Figma's REST API has no “list my files” endpoint. Probed with the operator's
token on 2026-11-16:

* `GET /v1/files` → `HTTP 404 {"status":404,"err":"Not found"}`;
* `GET /v1/teams/<user id>/projects` → `HTTP 403` “This endpoint requires the
  file_read or files:read or projects:read scope” (the token has
  `file_content:read`, `folders:read`, … but not `projects:read`).

So the file key/URL must come from the operator. Do not guess keys.

## Run

```bash
set -a; . ~/.config/h2/figma.env; set +a

# 1. inspect the file tree and pick node ids
python3 tools/figma_pull.py --file-url 'https://www.figma.com/design/<KEY>/<Name>' --list

# 2. fetch + export + manifest (one command)
python3 tools/figma_pull.py --file <KEY> --nodes '1:2,1:3,1:4' --scale 2 --stage-pack
#    -> analysis/figma/<KEY>.json            raw API response (byte-exact)
#    -> analysis/figma/<KEY>.nodes.json      extraction (frames/texts/all_nodes)
#    -> analysis/figma/png/*.png             exports
#    -> analysis/figma/manifest.generated.json
#    -> analysis/figma/pack/                 manifest.json + png/ + fonts/ (--stage-pack)

# 3. diff the design against the game's real layout
python3 tools/figma_diff.py --run-layout            # or --layout analysis/figma/layout.dump.txt
```

`--dry-run` needs no network and no token: it reads a saved
`<out>/<KEY>.json` plus `<out>/<KEY>.images.json` (`--input`/`--exports`
override the paths). This is what the test suite uses.

### Manifest schema (same as the HUD pack)

Required by `HudPack::load` (`pack.rs:189-280`) and therefore emitted by
`build_manifest()`:

| Key | Value |
| --- | --- |
| `reference_resolution.landscape/portrait` | HUD frame `[w, h]` from Figma |
| `design_height` | `768` (the pack's UI-unit height; 1080 px design → 768 units) |
| `safe_area_1080p.landscape/portrait` | 4 insets; Figma safe-area frame if present, else the canonical pack values (`safe_area_source`) |
| `command_card` | `columns/rows/cell/gap/order`; detected from the 4×3 child grid, else canonical (`command_card_source`) |
| `control_groups` | `count/cell`; detected from the group pads (10 children), else canonical |
| `atlas.commandbuttons` | `columns/rows/cell` from the canonical pack (the design does not ship the atlas sheet) |
| `assets[]` | `name/file/w/h/insets{l,t,r,b}/states/atlas/mips/format` + `node_id`, `node_name`, `design` |
| `font_files` | canonical pack font list (the staged pack links the real `fonts/`) |

Provenance is explicit so nothing is silently invented:
`provenance.{file_key,file_name,fetched_at,hud_frame,export_scale,stats,
safe_area_source,command_card_source,control_groups_source,atlas_source,
manual_insets,generated_at}`.

### Insets (`insets_source`)

9-slice insets are derived from the node, never guessed:

* `derived:cornerRadius+strokeWeight` — `cornerRadius` (or
  `rectangleCornerRadii`) plus `strokeWeight`, exported scale applied,
  per-corner `max()` mapping into `[l,t,r,b]`;
* `derived:stroke-only`, `derived:square` — zero radius, with/without stroke;
* `derived:clamped` — the derived value exceeded half the shorter texture
  side and was clamped (documented in the label);
* `text` — TEXT exports are whole textures, insets `0`;
* `manual:*` — `cornerRadius` mixed/absent or the insets exceed the texture:
  the PNG is **not** added to `assets[]`; the node is listed in
  `provenance.manual_insets` with the reason. Fill it with
  `--insets-overrides '{"1:2": [12,12,12,12]}'` (source `override`).

`--assume-zero-radius` labels the assumption as `derived:assumed-square`
instead of `manual` — opt-in, never the default.

## Diff semantics (`figma_diff.py`)

* Layout source: `--layout <dump>` (the output of
  `scripts/h2-cargo run -p wc3-game --example hud_layout --offline`) or
  `--run-layout` (prebuilt `target/debug/examples/hud_layout` if present).
* Frame: `--frame <id|name>`; by default the largest top-level frame matching
  the viewport aspect, preferring names with `hud/game/ui/layout/screen/console`.
* Scale: `sx = viewport.w / frame.w`, `sy = viewport.h / frame.h`. If the two
  differ by more than 1.5 % the run fails with the numbers and asks for
  `--scale` + `--anchor` (`topleft|bottomleft|center|auto`) — no silent stretch.
* Matching: layout element name → normalised Figma node name. `DEFAULT_ALIASES`
  in the tool maps `console`, `top strip`, `card plate`, `card slot`,
  `groups pad`, `group plate`, `minimap image`, `portrait`, `inventory panel`,
  `inventory slot`, `crest`, `army band`, `army slot`, `tab button` to common
  Figma spellings. Repeated names (12 card slots, 10 group plates) are
  **auto-picked**: shallowest node first, then largest; the count is printed.
  `--map '{"console": "12:34"}'` pins an exact node id.
* Output: one row per matched element — Figma rect, ours, `dx/dy/dw/dh`, max,
  `OK|FAIL`. Reports are written to `analysis/figma/diff-<orientation>.{md,json}`.
* Exit: `0` all matched deltas ≤ threshold (default 2 px), `1` any delta over,
  `2` setup error (no frame, no matches, aspect mismatch).

## Tests

```bash
python3 -m unittest discover -s analysis/figma/tests -v
```

19 tests cover the layout parser, tree extraction (auto-layout, fills, text
style), insets derivation (uniform/per-corner/mixed/clamp/scale), the manifest
schema + `pack.rs` mirror validation, staged-pack font/link validation, the
zero-delta diff, a +5 px failure diff, nested-node matching and URL/PNG helpers.
No network and no token are required.

## Demo fixture (offline proof)

`analysis/figma/fixtures/` mirrors the real 2414×1080 `hud_layout` dump 1:1 so
the pipeline can be proven without the operator's file:

```bash
./target/debug/examples/hud_layout > analysis/figma/layout.dump.txt
python3 analysis/figma/fixtures/make_demo.py
python3 tools/figma_pull.py --dry-run --file demo-hud \
    --input analysis/figma/fixtures/demo-file.json \
    --nodes '10:2,10:3,10:4,10:5,10:6,10:7,10:8,10:9,10:10,10:12' \
    --out analysis/figma/fixtures \
    --exports analysis/figma/fixtures/demo.images.json
python3 tools/figma_diff.py --nodes analysis/figma/fixtures/demo-hud.nodes.json \
    --layout analysis/figma/layout.dump.txt --out-dir analysis/figma/fixtures
```

The demo manifest is `file_key=demo-hud` and **is not** the operator's design;
it only proves that extraction, insets, grid detection, manifest validation and
the diff run end to end (13/13 elements, 0.0 px). The real run writes
`analysis/figma/manifest.generated.json` from the real key.

## Known limits

* TEXT nodes are extracted (characters/fontSize/fontName/lineHeight/fills) but
  only diffed when `--map` pins them to a layout element; the HUD layout dump
  has no text rows.
* PNGs are exported through `/v1/images`; `use_absolute_bounds=true` crops to
  the node bounds so texture size == `bbox × scale`.
* `--stage-pack` links `png/` and the canonical `fonts/` into
  `analysis/figma/pack/`; it does not copy art or fonts.
* Renderer sampling is untouched: the manifest only describes textures and
  insets; visual parity is verified by the existing HUD tests.
