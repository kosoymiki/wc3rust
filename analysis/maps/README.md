# Maps: acquisition, census and registry (ledger 449)

The game offers melee maps from a curated directory, and every offered map is
proven to load before it appears in the list. This directory holds the
provenance ledger (`SOURCES.json`), the generated catalog (`REGISTRY.json`),
the rendered census (`CENSUS.md`) and the table renderer.

## Layout

| path | role |
| --- | --- |
| `donors/maps/melee/` | curated map containers (38 maps, 8,011,118 bytes), stable names |
| `analysis/maps/SOURCES.json` | where each file came from: URL, access date, method, size, SHA-256 |
| `analysis/maps/REGISTRY.json` | generated catalog: per-map census + verification flag + native index |
| `analysis/maps/CENSUS.md` | the census table rendered from the registry |
| `analysis/maps/make_registry_table.py` | renderer (registry → markdown, never invents a row) |
| `crates/wc3-game/src/map_registry.rs` | census + registry reader/builder used by the game and the tests |
| `crates/wc3-game/tests/map_registry.rs` | `maps.registry.build` / `maps.registry.all-load` |

`donors/` is git-ignored (`/donors/` in `.gitignore`) and is not part of any
release artifact: the maps are the operator's own Warcraft III assets that the
game reads at runtime from the device install or from this curated directory.

## Sources (2026-11-16)

Discovery used the stealth-browser MCP (headless Chromium via
`spawn_browser` → `navigate` → `execute_script` → `close_instance`) against the
public EpicWar archive, which serves map containers anonymously — no login, no
CAPTCHA, no paywall. Downloads used `curl -L` with a desktop user-agent and the
page's time-bound `/maps/<id>/download/` URL. Nothing was bypassed: every host
answered with HTTP 200 and an `HM3W`/`MPQ\x1a` container.

23 maps came from EpicWar mirrors (map page ids in `SOURCES.json`), the other
15 are the operator's local War3Net test-corpus copies (official 1.31 TFT/ROC
ladder maps and the beta RoC 1.10/1.20 set), copied to stable names. Every row
in `SOURCES.json` carries `size_bytes` and `sha256`, and the ingestion test
re-hashes the bytes it loads.

## Pipeline

1. **Curate** — containers land in `donors/maps/melee/` under canonical names
   (`(4)TwistedMeadows.w3x`, `(2)PlunderIsle.w3m`, …). Non-melee customs,
   protected maps and multi-megabyte novelty maps are not curated.
2. **Census** — `map_registry::census` opens the container through the canonical
   `wc3_re::mapfiles::MapBundle` parser and records: w3i name/author/recommended
   players, start locations (`sloc`), placed units (non-marker), gold mines
   (`ngol`/`ugol`), destructables (`war3map.doo`), tileset and terrain size
   (`war3map.w3e`), script size, `CreateUnit(` call sites and the distinct JASS
   natives the script declares (`war3map.j`), plus the container SHA-256.
3. **Load proof** — `map_registry::verify_playable` runs the real path:
   `Assets::load(None, Some(map))` → `Game::new` → `enter_battle` → 300
   simulation ticks (`Sim::step`, the tick the game loop drives), requiring a
   live player unit at every tick. The computer-player AI roster is cleared so
   the per-map budget measures map loading and the core simulation, not AI
   strength (`ai.*` mechanics own that).
4. **Registry** — `maps.registry.build` (`H2_MAPS_BUILD=1`) writes
   `REGISTRY.json`; unloadable containers are recorded in `rejected` with the
   exact error instead of being silently dropped.
5. **Acceptance** — `maps.registry.all-load` re-reads the registry from disk,
   re-hashes every container, replays the load proof and fails if any entry
   drifts, dies or exceeds the 10 s per-map budget.

```bash
# regenerate the catalog (writes analysis/maps/REGISTRY.json)
H2_MAPS_BUILD=1 scripts/h2-cargo test -p wc3-game --test map_registry maps_registry_build --offline -- --nocapture
# acceptance: every registered map loads within 10 s
scripts/h2-cargo test -p wc3-game --test map_registry maps_registry_all_load --offline -- --nocapture
# render the census table
python3 analysis/maps/make_registry_table.py
```

## Device install

The curated set is copied to the phone so the game can list it without the
repo:

```bash
su -c 'cp donors/maps/melee/*.w3x donors/maps/melee/*.w3m /data/data/org.herofolends.p0/files/Maps/'
su -c 'chown u0_a354:u0_a354 /data/data/org.herofolends.p0/files/Maps/*'
su -c 'chmod 0644 /data/data/org.herofolends.p0/files/Maps/*'
```

38 files (7.7 MB) verified byte-identical on the device by SHA-256, readable by
the app uid `u0_a354` under `u:object_r:app_data_file:s0`.
