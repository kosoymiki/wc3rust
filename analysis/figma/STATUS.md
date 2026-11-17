# Figma pipeline status — 2026-11-16 (LEDGER 459)

## Ready

* `tools/figma_pull.py` — `/v1/files` (depth-limited), full FRAME/COMPONENT/GROUP
  + TEXT extraction, `/v1/images` PNG export, HUD-pack-schema manifest
  (`analysis/figma/manifest.generated.json`), `--dry-run` (no network/token),
  `--stage-pack` (loader-ready `pack/` with `png/` + canonical `fonts/` links),
  and a `pack.rs`-mirroring validator.
* `tools/figma_diff.py` — parses the `hud_layout` dump, matches elements by
  name/alias (or `--map`), scales design → device px, prints
  `element | Figma rect | ours | dx dy dw dh | max | verdict`, writes
  `diff-<orientation>.{md,json}`, exits 1 when any delta > 2 px.
* 19 unittest cases (`analysis/figma/tests/`) — all pass offline.
* Offline demo: `analysis/figma/fixtures/` mirrors the real 2414×1080 layout
  dump; 13/13 elements diff at 0.0 px (`fixtures/diff-landscape.md`).

## Token (verified)

`GET /v1/me` → HTTP 200, user `Mikel Lair`, keys `email, handle, id, img_url`.
Token scopes include `file_content:read`, `file_metadata:read`,
`file_versions:read`, `folders:read`, `library_assets:read`; **no**
`projects:read` / `team:read`.

## Missing: the file key

Evidence (2026-11-16):

* `GET /v1/files` → HTTP 404 `{"status":404,"err":"Not found"}` (no listing API);
* `GET /v1/teams/<user id>/projects` → HTTP 403 “This endpoint requires the
  file_read or files:read or projects:read scope”.

No Figma URL/file key exists on disk (checked `~/.config`, `~/.harness2`,
opencode session DB, repo docs). The key must come from the operator.

## One command once the key arrives

```bash
set -a; . ~/.config/h2/figma.env; set +a
cd ~/src/herofolends-rs
python3 tools/figma_pull.py --file-url '<FIGMA_URL>' --list          # pick node ids
python3 tools/figma_pull.py --file-url '<FIGMA_URL>' --nodes '<ids>' --scale 2 --stage-pack
python3 tools/figma_diff.py --run-layout
```

That writes the real `analysis/figma/<KEY>.json`, `<KEY>.nodes.json`,
`analysis/figma/png/*.png`, `analysis/figma/manifest.generated.json`,
`analysis/figma/pack/`, and the first real `diff-landscape.{md,json}`.
