# Lane B — real chapter titles + sim seat binding (2026-11-18, ledger 519)

Owner: `crates/wc3-game/src/campaign.rs`, `analysis/campaign/**`.
Read-only respected: `crates/wc3-game/src/game.rs` (tech-lead),
`crates/wc3-sim/**` (lane A, ledger 518), `crates/wc3-game/src/mobile_hud.rs`
(lane D), `crates/wc3-game/src/assets.rs` (lane C).

Verification is headless only. No `am`/`input`/`screencap`/`pm`/`logcat -c` was
run. Device confirmation (screen/HUD) needs a device slot owned by the operator
and driven by the tech-lead.

## 1. Titles

`title_from_id` is now the **last** fallback. The order is:

1. `UI\CampaignStrings.txt` / `UI\CampaignStrings_exp.txt` from the mounted
   archives (`War3.mpq` base table, `War3xlocal.mpq` locale/TFT tables; the
   locale archive wins on merge).
2. The chapter map's own `war3map.w3i` name resolved through `war3map.wts`.
3. `title_from_id` only when neither exists.

Both shipped table formats are parsed (`campaign.rs`):

* 1.0 (`War3.mpq`): `[Orc]` section, `MissionN="name"` + parallel `FileN="Orc01"`.
* 1.07+ (`War3xlocal.mpq`): `MissionN="header","name","Maps\...\Id.w3m"`.
  Quoted commas (`"Глава 7, часть 1"`) stay inside their field.
* A localised file name in `_exp.txt` (`NightElfX08Финал.w3x`) resolves to the
  archive stem (`NightElfX08Finale`) only when the ASCII prefix is unique.

Exact shipped rows (observed, extracted from the operator's install):

```text
War3.mpq        [Tutorial] Mission0="В погоне за видением" File0="Prologue01"
War3.mpq        [Orc]      Mission0="Чужие берега"          File0="Orc01"
War3xlocal.mpq  [Human]    Mission0="Глава 1","Чужая работа","Maps\FrozenThrone\Campaign\HumanX01.w3x"
```

TFT chapters are discovered from `Maps\FrozenThrone\Campaign\*.w3x` in
`War3xlocal.mpq` (`campaign_archives()` now mounts `War3xlocal.mpq`, and
`archive_campaign_entries()` accepts both campaign prefixes). RoC chapters sort
before TFT chapters (`chapter_rank` carries a generation bit), and the unlock
chain (`campaign_order`) treats TFT Human01/NightElf01/... as the first chapter
of their own campaign instead of requiring the whole RoC campaign first.

## 2. Seat binding (lane A API)

`crates/wc3-sim/src/lib.rs` (lane 518) now carries:

```rust
pub const MAX_PLAYERS: usize = 8;
pub fn bind_human_seat(&mut self, seat: u8) -> Result<SeatBinding, SeatError>;
```

`Game::start_campaign_mission` consumes it at the single integration point
`campaign.rs::bind_campaign_seat`:

* seat in `0..MAX_PLAYERS` -> `cam bind_human_seat(seat)`; logs
  `CAMPAIGN_HUMAN_SLOT … sim_bound=true units=… visible=…` and appends
  `title=… sim_bound=… seat_units=… seat_visible=…` to `CAMPAIGN_LOAD`;
* seat outside `MAX_PLAYERS` -> hard `CAMPAIGN_SEAT_UNBOUND …` error and the
  mission start fails (no silent `sim_bound=false`).

`Game::player` stays inside the game layer's two-seat arrays (`score:
[SeatScore; 2]`, direct `sim.resources[self.player]` reads) for seats >= 2; the
sim-side local view is `Sim::local_player()`.

## 3. Headless evidence

Command (headless example, real install):

```text
WC3RUST_DATA=/data/data/com.termux/files/usr/tmp/opencode/wc3_iso \
  scripts/h2-cargo run -p wc3-game --example campaign_report -- --mission Prologue01 --frames 30
```

`CAMPAIGN_LOAD` / `CAMPAIGN_HUMAN_SLOT` lines, observed 2026-11-18:

```text
CAMPAIGN_HUMAN_SLOT idx=0 name=Prologue01 slot=4 sim_bound=true units=0 visible=0
CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Prologue01.w3m bytes=305448 units=60 quests=1 script=true title=В погоне за видением sim_bound=true seat_units=0 seat_visible=0
CAMPAIGN_HUMAN_SLOT idx=24 name=Orc01 slot=0 sim_bound=true units=5 visible=5
CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Orc01.w3m bytes=311106 units=189 quests=2 script=true title=Чужие берега sim_bound=true seat_units=5 seat_visible=5
CAMPAIGN_HUMAN_SLOT idx=46 name=HumanX01 slot=0 sim_bound=true units=7 visible=7
CAMPAIGN_LOAD source=mpq path=Maps\FrozenThrone\Campaign\HumanX01.w3x bytes=330305 units=215 quests=2 script=true title=Чужая работа sim_bound=true seat_units=7 seat_visible=7
```

Prologue01 and Orc01 came from `campaign_report`; the TFT line came from the
headless probe `analysis/campaign/lane_b_probe.rs` (same registry, same game
path, prints the campaign log). Both are real product runs, not tests.

`scripts/h2-cargo check -p wc3-game --lib` -> rc=0 (2026-11-18).

## 4. Remaining seat-ownership gap (tech-lead integration, not lane B files)

`seat_units=0` on Prologue01 is real. Its script-created units are remapped to
the neutral owner before they reach the sim, because the game-layer bridge still
hard-codes the old two-seat mapping:

* `crates/wc3-game/src/sim_bridge.rs:1106-1109` —
  `owner_of_player = vec![2; 16]; [0]=0; [1]=1`, so JASS `Player(4)` -> owner 2.
* `crates/wc3-game/src/sim_bridge.rs:241-245` — `spawn_map_scene` maps every map
  record with `player > 1` to owner 2.
* `crates/wc3-game/src/game.rs` render/resource reads still index with
  `self.player` (`6634`, `7188`, `8054`, `2671-2672`, `4685-4686`, `6674-6675`,
  `7432-7433`, `7737-7738`, `2875`, `6452`, `6463`, `7155`) and `score:
  [SeatScore; 2]` at `612`/`3011`.

With `MAX_PLAYERS = 8`, the bridge must map `0..8` identically (and `12`/`27`
to their neutral owners), and `game.rs` must cull/render with
`Sim::local_player()` / `Sim::gold(seat)` / `Sim::lumber_of(seat)`. Those files
are outside ledger 519's OWNER_PATHS; the exact lines above are the handoff.
