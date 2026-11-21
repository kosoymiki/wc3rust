# LANE 547 — network protocol, `wc3-guest` host boundary, replay serializer (audit + fix)

Ledger: **547** (monitor id 550, `LEDGER-547-NET-GUEST`)
Source of truth: `/storage/emulated/0/Download/WC3Rust_INTERIM_AUDIT_TZ.md` §9 (unfinished zones)
Assignment: `analysis/tech-lead/INTERIM-TZ-ASSIGNMENT.md` (row “§9 network protocol, command validation,
`wc3-guest` boundary” — the table says 546; the dispatched ledger for this lane is **547**)
Non-duplication inputs read first: `analysis/full-audit-2026-11-18/lane-I-sim-input-media.md`,
`analysis/full-audit-2026-11-18/lane-K-platform-build.md`

| item | value |
|---|---|
| repository | `~/src/wc3rust` |
| HEAD | `d7af459daf5c507d46c8a63741385e5d1091af26` (2026-11-18 05:57:18 +0300) |
| worktree at audit time | dirty, **229 paths** (product lanes in flight; this lane touches only its own files) |
| tooling | `scripts/h2-cargo` only (FIFO slot). Probes that could not wait for the slot were compiled with `rustc` against the **crate's own already-built rlib** — the exact command and rlib are recorded in §4 |
| operator constraints | **no new tests** (`#[test]`, `tests/**`), **no device commands**, no licence bookkeeping |
| evidence rule | every finding = `file:line` + a one-shot probe under `probes/lane-547/` + exact output |

Files hashed so line numbers can be re-anchored if the tree moves:

```
8a94d3a6de4ed296  crates/wc3-sim/src/net.rs                    (2092 lines)
bd25ebc0a1457617  crates/wc3-guest/src/pe.rs                  (455)
6e4e352db360ace8  crates/wc3-guest/src/cpu.rs                 (751)
da351fbe193229e8  crates/wc3-guest/src/hle.rs                 (315)
cf2a4c33af724e9f  crates/wc3-guest/src/layout.rs              (257)
f6b88e7ca14ecfa4  crates/wc3-guest/src/lib.rs                 (47)
f04ef66f6bd1f708  crates/wc3-re/src/guest_host.rs             (404)
84b0028f7d846727  crates/wc3-replay/src/lib.rs                (792)
dcf09f3c969c30f1  crates/wc3-sim/src/save.rs                  (6137)
```

Probes: `analysis/full-audit-2026-11-18/probes/lane-547/{pe_imports_amp,guest_host_base,net_replay_identity,replay_serializer}.rs`.

---

## 0. Reachability first — what this zone protects today

Before severity, the honest reachability result, because it changes every priority in this report:

| zone | who calls it in production | evidence |
|---|---|---|
| `wc3_sim::net` (`NetSession`, `LockstepClient/Pair`, `Replay`, `MapScene`) | **nobody** | `grep -rln "NetSession\|LockstepPair\|LockstepClient\|net::Replay" apps/ crates/ tools/` → only `crates/wc3-sim/src/net.rs`, `crates/wc3-sim/tests/multiplayer.rs`, `crates/wc3-sim/examples/save_net_cli.rs`, `crates/wc3-game/tests/input_limits.rs`. `wc3-game/src` and `apps/android-native/src` have zero hits |
| `wc3_guest` / `wc3_re::guest_host` | **nobody** | `grep -rn "GuestHost\|guest_host::" apps/ crates/` → only `crates/wc3-re/src/guest_host.rs` itself. The crate doc says the lifter “is a later donor unit” (`guest_host.rs:11-13`) |
| `wc3_replay::Replay::write` | `apps/headless` only | `apps/headless/src/main.rs:135` `from_sim`, `:144` `read` back in the same run |
| `Command`/`Sim` decode used by net | `Sim::step` is production (the app) | `net.rs` is only the *transport*; the command **decoder** it uses (`save::command_decode`) is shared with the save format, which lane I audited |

So §9's “network protocol / guest boundary” is **not a shipped attack surface today** — it is a library plus a
dev CLI. That is the reason every finding below is rated P3/P4 instead of the P2 impact class its mechanism
deserves (a hostile-peer abort/OOM *would* be P2 the day a multiplayer or module-loading screen ships).
It is **not** a claim that the zone is safe: §3 lists exactly what was and was not proven.

Two consequences worth stating to the tech-lead:

1. **No defect in this lane is reachable from the shipped product path**, so per the assignment rule
   (“fix only defects reachable from the product path”) this lane made **one** fix — the replay
   *serializer*, which is exercised by the release-QA binary `apps/headless` in the same run that
   verifies it (§2, L547-04) — and recorded fix specifications for the rest.
2. The `wc3-guest` boundary cannot be judged end-to-end yet: there is no loader, no lifter and no guest
   memory, so “what a malformed module can reach” is limited to what `PeImage::parse` + `GuestHost::boot`
   allocate in the host process. That is exactly what §2 measures.

---

## 1. Answers to the four scope questions

### (a) `net.rs` protocol framing, command validation, resource bounds, desync paths

**Framing is bounded and total; the remaining defects are semantic, not unbounded-parse.**

Bounded, verified by reading the code and one negative probe:

- `Fr::len` (`net.rs:324-340`) rejects any count that exceeds `limit` **or** the remaining bytes; every
  scene vector (`read_scene:887-943`) and the raw byte slices (`Fr::bytes:366-369`) go through it.
  `read_bitmap:1570-1587` additionally requires `bytes.len() == cells.div_ceil(8)`.
- `Replay::decode` (`:1429-1492`) ties the declared payload length to the file length
  (`declared != actual → LengthMismatch`), so a small file cannot announce a large payload; frame count is
  capped by `MAX_FRAMES = 2_000_000` **and** by `p.remaining()`, and `Vec::with_capacity(count.min(frame_bytes))`
  cannot exceed the file size.
- `recv_message` (`:1969-1983`) rejects `len == 0 || len > 64 MiB` before allocating.
- `recv_batch` (`:2008-2046`) caps the command count at 4096 and validates the frame tick and the player
  seat against the handshake; `command_decode` → `save::read_command` → `save::R::len` (`save.rs:545-554`)
  bounds every id list by the remaining bytes of that command.
- `Command::CancelTrain { index }` is range-checked **before** `production.remove` (`lib.rs:1805-1808`), so
  a crafted index is a no-op, not a panic (this is the one place where a hostile packet could have
  panicked the match — it does not).
- Handshake (`establish:1765-1847`) rejects seat equality, seat > 1, setup-digest mismatch and start-hash
  mismatch before tick 0; `remote_seat` is forced to `1 - local_seat`.

Remaining defects found: `from_path` reads unbounded (L547-03), `seat_force` unbounded (L547-06),
`DesyncDetector::check` accepts unknown ticks (L547-07), no accept/connect timeout and unbounded session
logs (L547-08), `recv_batch` ignores trailing bytes (L547-09), and — the structural one — **the frame's seat
is routing metadata, not authority**: `Sim::step` applies every command with no issuer, so a peer can
command the opponent's units (L547-11).

**Not duplicated:** `Replay::verify` loops `while sim.tick < self.end_tick` (`:1508-1558`) with `end_tick`
straight from the file, and `state_hash` feeds a hand-picked field list — those are **lane 539 (I-005)** and
**lane I (WC3-SIM-001)** respectively; both are referenced, not re-filed here.

### (b) `wc3-guest` PE32 host boundary — import binding, thunk arity, memory window, malformed module

**Import binding and thunk arity are enforced; the memory-window rule is not, and the import table
amplifies a small file into a large allocation.**

- Arity: `HleRegistry::bind` refuses `Arity::Unknown` (`hle.rs:148-155`) and `call` refuses an argument
  count that differs from the entry (`hle.rs:221-228`). Resolution rejects out-of-window and
  non-16-byte-stride addresses (`hle.rs:236-247`) — a guest-supplied value is never rounded into a slot.
- Address plan: `Region::end` saturates, `thunk_va/thunk_slot` are checked, `region_is_valid` uses
  `checked_add` (`layout.rs:56-70,132-168`).
- PE parser: every read goes through a checked `Reader` (`pe.rs:311-355`); export counts are capped at
  65 536 (`:247`); section table ≤ 96 (`:166`); a section whose raw data leaves the buffer is rejected
  (`:186-190`). Existing tests cover truncation and malformed headers
  (`crates/wc3-guest/tests/recomp_pe32.rs:147-176`).
- **Defect 1 — import amplification** (L547-01): `parse_imports` bounds each loop at 4096 but not their
  product, and every descriptor may point at the *same* thunk array. Measured: **37 474-byte image →
  4 193 280 `Import` values, +363 604 kB RSS (355 MB), 8.3 s, `Ok`** (§4).
- **Defect 2 — the image base rule accepts the HLE window, stack, TEB and heap** (L547-02): `boot` only
  rejects `Owner::Unmapped` (`guest_host.rs:106-114`). Probe: bases `0x7F000000` (HLE thunk window,
  `owner=Overlap`), `0x31000000` (stack), `0x3F000000` (TEB), `0x36000000` (heap) all `boot=Ok`; the
  image's entry then lands at `0x7F001000` while import thunk 0 is `0x7F000000` — its own code window and
  the thunk table are the same 1 MiB.
- What a malformed module can reach: **only the host allocator**. There is no guest memory and no
  executor in-tree (`Cpu` is a flag/register model, `guest_host.rs:11-13`), so a malformed module cannot
  dereference a host pointer, install a thunk on its own, or reach a syscall. The reachable primitive is
  memory/CPU exhaustion (L547-01) plus the mis-based image aliasing (L547-02). `grep -c unsafe` over the
  guest crate, `guest_host.rs`, `wc3-replay` and `net.rs` → **0 in every file**, so there is no
  hand-rolled pointer arithmetic anywhere on this boundary.

### (c) `wc3-replay` end-to-end: is write → read → verify an identity?

**Verified for a normal match, and broken at the boundaries.** Two containers, two probes:

- **`wc3_sim::net::Replay`** — identity holds end to end: `encode → decode → encode` is byte-identical, every
  field (setup, start/end tick, end hash, frames) is equal, `verify()` re-simulates 8 ticks to the recorded
  hash, and a flipped checksum byte is rejected. §4.3 for the verbatim output.
- **`wc3_replay::Replay`** — identity holds for a normal match, and the *writer* has no idea what the
  *reader* enforces: a replay that `write` accepts can be rejected by `read` in the same process — measured
  for `ticks > MAX_TICKS`, for a record at `tick == ticks`, and for unordered ticks (§4.4). **Fixed in this
  lane** (§2, L547-04).

### (d) Coverage gaps

See §3. Headline: `crates/wc3-guest/**` had **zero rows** in `FILE_COVERAGE.csv` before this lane (the only
guest-related row was `crates/wc3-re/src/guest_host.rs`, reviewer H). There is **no fuzz target anywhere**
for PE32, replay or the wire protocol, and the operator test freeze blocks adding one — recorded as `GAP`,
not as “clean”.

---

## 2. Findings (TZ §7 template)

Index: **7×P3, 4×P4 = 11 findings, 1 FIXED_ON_HEAD (L547-04), 0 NEEDS_REPRO, 0 FALSE_POSITIVE.**

| ID | P | one line | status |
|---|---|---|---|
| L547-01 | P3 | PE32 import table: 99 KB image → 1.39 GiB peak / 46 s | CONFIRMED |
| L547-02 | P3 | `GuestHost::boot` accepts an image based in the HLE thunk window, stack, TEB or heap | CONFIRMED |
| L547-03 | P3 | `wc3_sim::net::Replay::from_path` reads an unbounded file before parsing | CONFIRMED |
| L547-04 | P3 | replay `write` emits files its own `read` rejects (ticks/count/order) | **FIXED_ON_HEAD** |
| L547-05 | P4 | `Record::player` is neither validated nor used by `verify_with` | CONFIRMED |
| L547-06 | P4 | `MatchSetup::validate` does not bound `MapScene::seat_force`; decode does | CONFIRMED |
| L547-07 | P4 | `DesyncDetector::check` treats an unknown tick as agreement | CONFIRMED |
| L547-08 | P4 | transport has no accept/connect timeout; session logs grow unbounded | CONFIRMED |
| L547-09 | P4 | `recv_batch` accepts trailing bytes the frame parser rejects | CONFIRMED |
| L547-10 | P4 | public `wc3_guest::cpu` unit helpers panic on `bits == 0` | CONFIRMED (latent) |
| L547-11 | P3 | the wire protocol authenticates a seat per frame, but `Sim::step` applies commands with no seat binding — a peer can command the opponent's units | CONFIRMED |

### L547-01 — a 37 KB PE32 image becomes 4.19 M imports and 355 MB of RSS

```
ID: L547-01
Title: PeImage::parse_imports bounds each loop at 4096 but not their product; descriptors may share one
       thunk array, and every thunk clones the module String
Priority: P3            (impact class P2 — OOM/abort on malformed input; exposure P3 — no production
                         consumer of wc3_guest/GuestHost today, see §0)
Status: CONFIRMED
Confidence: HIGH (measured)
Affected commit: d7af459 (working tree)
File(s): crates/wc3-guest/src/pe.rs
Exact line range(s): :283-344 (parse_imports; descriptor loop :287, thunk loop :303,
                     `module.clone()` :315/:325), :96-110 (Import = module String + Option<String> + ordinal)
Affected targets/features: any future loader that parses an operator-supplied or third-party PE32
                     (`GuestHost::boot` → `PeImage::parse`); `wc3-guest/examples/pe_inspect`
Preconditions: the image's import directory describes many descriptors that share one thunk table
Trigger / minimal reproduction:
    rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/pe_imports_amp.rs \
        --extern wc3_guest=target/debug/deps/libwc3_guest-500acfe050084859.rlib \
        -L dependency=target/debug/deps -o $TMPDIR/n547-peamp
    $TMPDIR/n547-peamp 1024 4095
Observed behavior:
    PROBE imports desc=1024 thunks=4095 input_bytes=37474 result=Ok imports=4193280
          peak_delta_kb=363780 elapsed_ms=10493
    PROBE imports desc=4096 thunks=4095 input_bytes=98914 result=Err(BadImportDirectory)
          peak_delta_kb=1455104 elapsed_ms=46003
    (control: 638-byte image → 1 import, +4 kB, 0 ms; canonical h2-cargo run in §4.1 reproduces the
     1024 case byte-for-byte: rss_delta_kb=363604, elapsed_ms=4883)
Expected behavior: a bounded total (e.g. `MAX_IMPORTS = 65_536`, the same order as the export cap at :247),
      or a count bounded by the remaining bytes of the image like `Fr::len`/`save::R::len` do.
Root cause: the two loops are independently capped (4096 × 4096) and nothing relates the descriptor count,
      the thunk count or the produced `imports.len()` to the size of the file. `module.clone()` allocates a
      fresh String per thunk, so one shared name becomes N owned strings.
Impact: a malformed or hostile PE (the exact input this parser exists for) turns **98 914 bytes into a
      measured 1 455 104 kB peak RSS (1.39 GiB) and 46 s of CPU** — 14 700× the file size, 169 symbols per
      input byte. The 1024-descriptor variant succeeds and keeps 355 MB resident. Release profile is
      `panic = "abort"` (root `Cargo.toml:50-54`), so a failed allocation aborts the host instead of
      unwinding. Note the second variant *returns* `Err` — the cost is paid before the error, so “the parser
      rejects it” is not a resource bound.
Why not covered by excluded backlog: AUD-* has no guest/PE item; `crates/wc3-guest/**` had no
      FILE_COVERAGE row at all before this lane.
Fix specification: cap the produced import count before pushing
      (`if imports.len() >= MAX_IMPORTS { return Err(PeError::BadImportDirectory) }` with
      `MAX_IMPORTS = 65_536`), and stop cloning the module name per import — store an `Rc<str>`/index into a
      deduped module table (the flattened `Vec<Import>` is the reason the clone exists).
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the command above must print
      `imports<=65536` and `rss_delta_kb` in the low megabytes.
Verification commands and exact result: quoted verbatim in §4 (`RC=0`).
Related findings: L547-02 (same parser/host seam)
```

### L547-02 — `GuestHost::boot` accepts a base inside the HLE thunk window, the stack, the TEB or the heap

```
ID: L547-02
Title: boot() rejects only Owner::Unmapped, so any base covered by a static region passes — including the
       one deliberate overlap the layout module warns about; the size check always measures the `va` arena
Priority: P3            (tooling-only today: no lifter, no loader, no guest memory)
Status: CONFIRMED
Confidence: HIGH (measured)
Affected commit: d7af459
File(s): crates/wc3-re/src/guest_host.rs, crates/wc3-guest/src/layout.rs
Exact line range(s): guest_host.rs:99-118 (base rule :106-114, `va` arena lookup :115-118),
                     :120-127 (ImageTooLarge), layout.rs:11-21 (the documented overlap),
                     layout.rs:79-113 (REGIONS), :120-131 (owner_of)
Affected targets/features: `GuestHost::boot` (the only entry point that maps a module into the guest plan)
Preconditions: a PE32 whose `ImageBase` is inside any static region
Trigger / minimal reproduction:
    rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/guest_host_base.rs \
        --extern wc3_re=target/debug/deps/libwc3_re-e49b49163b3a75c1.rlib \
        --extern wc3_guest=target/debug/deps/libwc3_guest-95f3e81a039a6184.rlib \
        -L dependency=target/debug/deps -o $TMPDIR/n547-guestbase
    $TMPDIR/n547-guestbase
Observed behavior:
    PROBE base war3      va=0x00400000 owner=Unmapped          boot=Ok entry=0x00401000 thunk0=0x7F000000
    PROBE base hle-window va=0x7F000000 owner=Overlap           boot=Ok entry=0x7F001000 thunk0=0x7F000000
    PROBE base stack     va=0x31000000 owner=Arena("stack")     boot=Ok entry=0x31001000
    PROBE base teb       va=0x3F000000 owner=Arena("teb")       boot=Ok entry=0x3F001000
    PROBE base heap      va=0x36000000 owner=Arena("heap")      boot=Ok entry=0x36001000
    PROBE base unmapped  va=0x10000000 owner=Unmapped          boot=Err(image base 0x10000000 != layout base 0x400000)
Expected behavior: exactly one acceptable base class — `WAR3_IMAGE_BASE` (or, if a relocatable image is a
      real goal, any base whose region is an *image* arena with `Owner::Arena`, never `Overlap`, never
      stack/teb/heap) — and the size rule evaluated against the arena that actually contains the base.
Root cause: the predicate is `if let Owner::Unmapped = arena { reject }`, i.e. `Unmapped` is the only
      rejection. `owner_of` deliberately returns `Overlap` for the HLE window (`layout.rs:120-131`) and the
      layout module documents that a port which assumes disjoint arenas “resolves thunk addresses to arena
      4” (`layout.rs:11-21`) — the check inverts that warning into an accept.
Impact: an image based at `0x7F000000` has `entry = 0x7F001000` inside the thunk window whose slot 0 is
      `0x7F000000`; code and thunk table alias, so a dispatcher that jumps to `entry` and a guest that calls
      slot 0 hit the same 1 MiB. A TEB/stack/heap base aliases thread state or the allocator arena. There is
      no executor in-tree, so nothing exploits this today — it is the boundary rule that a future lifter
      would inherit.
Why not covered by excluded backlog: no guest boundary item in AUD-001…032; the only guest test that
      touches a base is `recomp_host_refuses_a_foreign_image_base` (an `Unmapped` base), which this
      predicate satisfies.
Fix specification: replace the `Unmapped` test with an explicit accept-list
      (`image.image_base == WAR3_IMAGE_BASE`, or `matches!(owner_of(base), Owner::Arena(name) if name is an
      image arena)`) and reject `Owner::Overlap` and the `stack`/`thread-stack`/`teb`/`heap`/`hle-heap`
      arenas; then compare `size_of_image` against the size of the region that contains the base rather
      than the constant `va` arena.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the probe above must print
      `boot=Err(...)` for `hle-window`, `stack`, `teb`, `heap`.
Verification commands and exact result: quoted verbatim in §4 (`RC=0`).
Related findings: L547-01
```

### L547-03 — `wc3_sim::net::Replay::from_path` reads an unbounded file into memory

```
ID: L547-03
Title: from_path() calls std::fs::read() (whole file, no cap) before decode(), while the sibling replay
       crate caps the same read at 256 MiB
Priority: P3            (file input, no network surface, no production consumer — but a one-line abort)
Status: CONFIRMED
Confidence: HIGH (code read; the amplification itself is the same mechanism lane I measured for the
      226 GiB reservation in WC3-RPL-002 — not re-measured here)
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs; contrast crates/wc3-replay/src/lib.rs
Exact line range(s): net.rs:1500-1506 (`from_path` → `std::fs::read`), :1448-1456 (`decode` runs after the
                     whole file is in memory); wc3-replay/src/lib.rs:120-131
                     (`take(MAX_REPLAY_BYTES + 1)` + explicit cap check)
Affected targets/features: any tool that opens an operator-supplied `.w3rp`
                     (`crates/wc3-sim/examples/save_net_cli.rs:193`)
Preconditions: a large or sparse replay file (e.g. `truncate -s 8G fake.w3rp`)
Trigger / minimal reproduction:
    truncate -s 8G $TMPDIR/huge.w3rp
    $TMPDIR/n547-replay-from-path $TMPDIR/huge.w3rp      # 6-line probe; not run here (see note)
Observed behavior (by construction): `std::fs::read` allocates the file size before `decode` can reject
    anything; `decode`'s `declared != actual` check (`:1452-1464`) is a *post*-allocation check.
Expected behavior: read through a bounded reader exactly as `wc3_replay::Replay::read` does
    (`.take(MAX+1)` then an explicit `> MAX` error).
Root cause: the net replay path never got the `wc3-replay` hardening; the two replay containers were
    hardened independently and only one of them bounds the read.
Impact: a crafted 8 GiB `.w3rp` aborts the process (release profile `panic = "abort"`; allocation failure
    is an abort, not an unwind). Local file, operator-supplied → P3.
Why not covered by excluded backlog: WC3-RPL-002 covers *announced lengths inside* the file; this is the
    read that happens before any length is parsed, in a different crate.
Fix specification: `let mut data = Vec::new(); File::open(path)?.take(MAX_REPLAY_BYTES + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_REPLAY_BYTES { return Err(NetError::Io(format!("replay exceeds {MAX_REPLAY_BYTES} bytes"))) }`
    with the cap defined next to `MAX_FRAMES` (`:52`).
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the sparse file above must fail with a
    typed error and a stable RSS.
Verification commands and exact result: static read at the cited lines. **Not probed**: writing an 8 GiB
    sparse file competes with the product lanes for the same flash quota (87 % used, ~30 G free) and the
    mechanism is a single documented `std::fs::read` call. Recorded as NEEDS_PROBE in §3 rather than
    claimed as measured.
Related findings: L547-04, WC3-RPL-002 (lane I)
```

### L547-04 — the replay writer emits files its own reader rejects — **FIXED_ON_HEAD**

```
ID: L547-04
Title: Replay::write validates none of the invariants Replay::parse enforces (ticks, record tick, order,
       count, command size), so a recording can be unreadable by the process that made it
Priority: P3
Status: FIXED_ON_HEAD (fix applied in this lane; before/after probe output in §4)
Confidence: HIGH (measured)
Affected commit: d7af459 → fixed in the working tree
File(s): crates/wc3-replay/src/lib.rs
Exact line range(s): before — write :106-131, parse :171-231 (ticks :197-199, count :187-189,
                     tick<ticks :198-200, order :201-203, len :207-209);
                     after  — write :106-151 (five guards + buffered encode), `reject` :699-701,
                     reader unchanged at :171-231. The file also carries lane 539's I-005 work
                     (`MAX_VERIFY_TICKS`/`ReplayError::Duration`/`verify_with_limit`) — see §6.3
Affected targets/features: `apps/headless` replay record+verify (`main.rs:135` write, `:144` read back in
                     the same process), any future match recorder
Preconditions: a match longer than the reader's ceilings, or a record at/after the declared end tick
Trigger / minimal reproduction:
    scripts/h2-cargo run -q -p wc3-replay --example replay_serializer
Observed behavior (before the fix):
    PROBE identity bytes=380 ticks=12 records=12 write=true read=true rewrite=Ok(true) verify_hash_ok=true fields_and_bytes_identical=true
    PROBE ticks_over_cap write=Ok read=Err(corrupt replay: tick count)
    PROBE record_at_end  write=Ok read=Err(corrupt replay: record tick outside the replay)
    PROBE unordered      write=Ok read=Err(corrupt replay: record ticks are not ordered)
Expected behavior: `write` refuses to produce a file its own reader cannot read.
Root cause: the reader was hardened (lane I, WC3-RPL-002) against hostile files; the writer was left
    as-is, so the two halves of one format disagree about what a valid replay is.
Impact: `MAX_TICKS` is 24 h of game time (2 592 000 ticks) and `MAX_RECORDS` is 1 048 576, so a long
    campaign recording is written successfully and then cannot be reopened or verified — the recording is
    silently unverifiable, which is exactly the property the replay format exists to provide.
    `apps/headless` would print `REPLAY_ERROR … corrupt replay: tick count` and `exit(4)` on its own output.
Why not covered by excluded backlog: WC3-RPL-002 is the reader; no item covered the writer.
Fix specification (applied): the writer now buffers the encoded file, checks the same five invariants the
    reader enforces, and returns `std::io::Error::new(InvalidData, …)` with the reader's wording — the public
    signature is unchanged, so no caller changes, and a rejected replay no longer leaves a half-written file.
Verification commands and exact result:
    before: `scripts/h2-cargo run -q -p wc3-replay --example replay_serializer` → RC=0, three `write=Ok /
            read=Err(...)` rows (§4.4)
    after:  same command → RC=0, three `write_ok=false bytes=0` rows, identity row unchanged (§4.4);
            `$PREFIX/tmp/n547-write-errors` → four `InvalidData` errors with the reader's wording and
            `bytes_written=0` (§4.5); `scripts/h2-cargo run -q -p wc3-replay --example replay_serializer`
            rebuilt the crate, so the fix compiles in the workspace graph
Related findings: L547-05, WC3-RPL-002, lane 539 I-005 (same file, see §6.3)
```

### L547-05 — `Record::player` is neither validated nor used

```
ID: L547-05
Title: parse() accepts any player byte and verify_with() ignores it, so a tampered seat attribution still
       verifies
Priority: P4
Status: CONFIRMED
Confidence: HIGH (measured)
Affected commit: d7af459
File(s): crates/wc3-replay/src/lib.rs
Exact line range(s): :74-82 (Record), :161 and :176-181 (player read, no range check), :193-222
                     (verify_with feeds only `command`)
Affected targets/features: replay attribution / any future multi-seat replay semantics
Preconditions: a replay whose player byte is edited
Trigger / minimal reproduction:
    scripts/h2-cargo run -q -p wc3-replay --example replay_serializer
Observed behavior: `PROBE player_offset byte 0->255 read_ok=true verify_ok=true` — the mutated file parses
    and reproduces the recorded final hash.
Expected behavior: `player <= 1` (or `< slots`) rejected at parse; a replay that claims player 255 is
    corrupt, and the same file with a different seat byte is not the same recording.
Root cause: the field is carried for reporting only; `Sim::step` receives command lists with no seat, and
    `verify_with` never looks at it.
Impact: replay attribution cannot be trusted (a peer can re-label its own commands) and an out-of-range
    byte passes validation. No hash/divergence consequence because the seat is not simulated.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: validate `player <= 1` in parse (the same typed `Corrupt("record player")`), and either
    feed the seat into the verification path or document in `Record` that the field is informational.
Regression test(s): BLOCKED: operator test freeze. Manual: the probe must print `read_ok=false` after a
    `player=255` byte flip.
Verification commands and exact result: §4.
Related findings: L547-04
```

### L547-06 — `MatchSetup::validate` does not bound `MapScene::seat_force`, and `build` loops on it

```
ID: L547-06
Title: seat_force is capped at 32 only on the decode path (read_scene); validate() ignores it and build()
       runs `for index in workers..force.workers`, so an in-process scene can spin the spawn loop
Priority: P4            (requires a caller to construct a MapScene with a huge force)
Status: CONFIRMED
Confidence: HIGH (code read; not probed — a probe would hang the build slot)
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs
Exact line range(s): :689-717 (validate: walkable length, starts count only), :930-940 (read_scene cap 32),
                     :947-986 (ensure_seat_base loops), :883-886 (write side: `force.workers as u32`)
Affected targets/features: `MatchSetup::build` (and therefore every replay verify), `MapScene` used as an
                     in-process API
Preconditions: `MapScene { seat_force: [SeatForce { workers: 1_000_000, .. }, ..] }` built in code
Trigger / minimal reproduction: construct the scene as above and call `MatchSetup::on_map(seed, scene).build()`
Observed behavior (by construction): `ensure_seat_base` iterates `workers..1_000_000`, calling
    `walkable_pos_near` + `spawn_def` for each miss; `write_scene` then serialises `workers as u32`
    (a truncating cast: a `usize` force of 2^32 writes 0) while `read_scene` rejects anything > 32 — the same
    decoder-validates/constructor-does-not family as L547-04.
Expected behavior: `validate()` enforces `workers <= 32 && fighters <= 32`, so construction and decoding
    agree.
Root cause: the cap was added to the decoder (which sees hostile bytes) and not to the type's own
    invariant check.
Impact: a caller that builds a scene programmatically (the documented `MatchSetup::on_map` API) can hang
    `build()` and produce an unreadable replay. No untrusted-input path reaches it today.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: add the two ≤ 32 checks to `MatchSetup::validate` (mirroring `read_scene`'s wording) and
    return `NetError::Setup`; then `write_scene` can keep writing the value unchecked.
Regression test(s): BLOCKED: operator test freeze. Manual: `validate()` must return `Err` for
    `SeatForce { workers: 33, .. }`.
Verification commands and exact result: static read at the cited lines.
Related findings: L547-04
```

### L547-07 — `DesyncDetector::check` treats an unknown tick as agreement

```
ID: L547-07
Title: check() returns Ok(()) when no hash was recorded for the tick, so a report for a tick the local peer
       never expected is silently accepted
Priority: P4
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs
Exact line range(s): :1252-1278 (`check`, `None => Ok(())` at :1273)
Affected targets/features: any consumer of `DesyncDetector` (currently tests only, `multiplayer.rs`)
Preconditions: a remote hash report for a tick that was never `expect`ed
Observed behavior (by construction): `check(999, anything)` → `Ok(())`.
Expected behavior: an unexpected tick is itself a protocol/divergence signal — return
    `NetError::Protocol("hash for tick N was never expected")` (or a dedicated variant), or at least
    count it in a `unexpected` field.
Root cause: `expected` is a sparse map and the `None` arm was written as “nothing to compare”.
Impact: a detector built on this API cannot distinguish “agreed” from “never checked”, which is the same
    class of gap lane I found in `state_hash` (detection quality). Tests-only consumer today.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: replace `None => Ok(())` with a typed error (and a unit test that pins it once the freeze
    lifts).
Regression test(s): BLOCKED: operator test freeze. Manual: `detector.check(1, 0)` with an empty detector must
    be `Err`.
Verification commands and exact result: static read at :1270-1278.
Related findings: WC3-SIM-001 (lane I)
```

### L547-08 — no accept/connect timeout; unbounded per-match session logs

```
ID: L547-08
Title: host()/connect() inherit the OS default timeout (accept blocks forever) and hashes/frames grow without
       bound for the whole match
Priority: P4
Status: CONFIRMED
Confidence: HIGH (code read)
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs
Exact line range(s): :1697-1712 (`host` bind+accept, `connect`), :1731-1747 (`host_on` accept),
                     :1683-1692 (fields), :1876 and :1899 (push per tick, no cap)
Affected targets/features: `save_net_cli` host mode; any future match
Preconditions: no peer connects (host), or a long match (log growth)
Observed behavior (by construction): `listener.accept()` has no timeout and the read/write timeouts are set
    only after the stream exists (`establish:1771-1780`); `hashes` grows 16 B/tick and `frames` ~32 B per
    non-empty frame for the whole match (24 h × 30 Hz ≈ 2.6 M ticks → ≈ 42 MB + 83 MB).
Expected behavior: `set_nonblocking`/`SO_*TIMEO` on the listener or a bounded accept loop; a cap on the
    session log (the replay format itself caps at 256 MiB / 2^20 records — `wc3-replay/src/lib.rs:15-19`).
Root cause: the transport is a v1 skeleton; only the post-handshake stream has timeouts.
Impact: a host with no peer waits forever (no error, no exit code); a long match accumulates ~125 MB with
    no diagnostic. No attacker needed, no crash.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: `listener.set_nonblocking(true)` + a deadline loop around `accept`, and cap `hashes`/
    `frames` (drop or error past `MAX_FRAMES`).
Regression test(s): BLOCKED: operator test freeze. Manual: `host()` on a bound port with no client must
    return `NetError::Transport("accept timeout")` after the configured budget.
Verification commands and exact result: static read at the cited lines.
Related findings: L547-03
```

### L547-09 — `recv_batch` accepts trailing bytes inside a BATCH message

```
ID: L547-09
Title: The wire BATCH parser does not require the payload to be fully consumed, while CommandFrame::from_bytes
       (the replay path) does — two parsers, two strictness rules for one frame format
Priority: P4
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs
Exact line range(s): :420-438 (`from_bytes` rejects `remaining != 0` at :434-436), :2008-2046
                     (`recv_batch` returns after the count loop with no trailing check)
Affected targets/features: the W3LS v1 wire protocol
Preconditions: a peer sends extra bytes after the last command in a BATCH message
Observed behavior (by construction): the message is accepted and the extra bytes are ignored.
Expected behavior: reject (a peer that appends bytes is either buggy or probing for a parser difference).
Root cause: the wire parser was written before `CommandFrame::from_bytes` gained the trailing check.
Impact: silent acceptance of a message that a strict peer would reject; a malleability difference between
    two paths that must agree (a replay decoded from a recorded frame and the same frame on the wire).
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: after the loop, `if r.remaining() != 0 { return Err(NetError::TrailingBytes(r.remaining())) }`.
Regression test(s): BLOCKED: operator test freeze. Manual: hand-craft a BATCH with one extra byte and expect
    `TrailingBytes`.
Verification commands and exact result: static read at :2008-2046.
Related findings: L547-04
```

### L547-10 — public `wc3_guest::cpu` unit helpers panic on `bits == 0`

```
ID: L547-10
Title: rol/ror/rcl/rcr divide by `bits` and `sign_bit` shifts by `bits - 1`, so bits == 0 is a division-by-zero
       panic (also on negative-width shifts for the carry variants)
Priority: P4            (latent: no lifter in-tree, no caller outside cpu.rs)
Status: CONFIRMED (latent)
Confidence: HIGH (arithmetic is unconditional)
Affected commit: d7af459
File(s): crates/wc3-guest/src/cpu.rs
Exact line range(s): :328-336 (`let n = (count & 31) % bits;` → `% 0`), :344-352, :360-377
                     (`let total = bits + 1; … 1u64 << total`), :573-578 (`1u32 << (bits - 1)`)
Affected targets/features: every `pub fn *_flags/rol/ror/rcl/rcr/shl/shr/sar/shld/shrd` caller, i.e. the
                     future lifter
Preconditions: `bits ∉ {8,16,32}` — a lifter bug or a new caller passing a decoded width
Observed behavior (by construction): `rol(0, 1, 0)` → `attempt to calculate the remainder with a divisor of
    zero`; `rcl(…, 63)` → shift overflow. In the release profile `panic = "abort"`, so the host aborts.
Expected behavior: `debug_assert!(matches!(bits, 8|16|32))` at the entry of every helper, or a `Result`.
Root cause: the width is an implicit precondition of the ported donor API and is never stated or checked.
Impact: none today (no caller outside cpu.rs); the crate's own tests (`tests/recomp_cpu.rs`, 11 tests) only
    pass 8/16/32. This is the kind of latent contract that turns a lifter bug into a process abort.
Why not covered by excluded backlog: no guest item in AUD-001…032.
Fix specification: add `debug_assert!(bits == 8 || bits == 16 || bits == 32)` (and clamp `count` inside the
    helpers) to the public unit helpers, or narrow them to a `Width` enum.
Regression test(s): BLOCKED: operator test freeze. Manual: `cpu.rol(0, 1, 0)` must not panic once the
    assertion lands (it would be caught in debug).
Verification commands and exact result: static read; `grep -rn "\.rol(\|\.ror(\|\.rcl(\|\.rcr(" crates/`
    outside `cpu.rs` → no callers.
Related findings: none
```

### L547-11 — the wire protocol authenticates a seat per frame, but the simulation has no seat binding

```
ID: L547-11
Title: recv_batch enforces that a BATCH comes from the remote seat, then NetSession::tick concatenates both
       seats' commands and Sim::step applies them with no issuer — `Command` carries no seat and `apply` never
       checks unit ownership, so a peer can command the opponent's units
Priority: P3            (P1-class integrity the day multiplayer ships; no production consumer today, §0)
Status: CONFIRMED
Confidence: HIGH (both halves read; the wire half is bounded by the seat check, the sim half has no check)
Affected commit: d7af459
File(s): crates/wc3-sim/src/net.rs, crates/wc3-sim/src/lib.rs
Exact line range(s): net.rs:2027-2033 (`player != self.remote_seat` → Protocol error),
                     :1869 + :1874-1879 (both frames concatenated after sorting by seat, then `self.sim.step(&all)`),
                     lib.rs:1647 (`pub fn step(&mut self, commands: &[Command])` — no seat parameter),
                     lib.rs:1696-1900 (17 `Command` arms; the only `owner` use is the CancelTrain refund to
                     the building's own owner, :1822-1830)
Affected targets/features: `NetSession` / lockstep matches; any future “send my commands” model
Preconditions: a seat-1 peer that puts seat-0 unit ids in its own BATCH
Trigger / minimal reproduction (two-process, needs the slot; not run here):
    NetSession::host_with(addr, setup, seat 0)  +  connect_with(addr, setup, seat 1)
    seat 1 sends Command::Move { units: [seat0_unit], .. } — the frame's `player` is 1, which passes
    `recv_batch`'s check, and `Sim::step` moves seat 0's unit on both peers.
Observed behavior (by construction): the frame-level seat check passes (it only proves *who sent* the batch),
    then the seat is discarded; `Sim::apply` acts on any unit id in the command.
Expected behavior: commands must be attributable to a seat and applied only to units that seat owns —
    e.g. `Sim::step_seated(&[(u8, &[Command])])` with an ownership check per unit id (the `Command` variants
    already carry explicit unit ids, so the check is local), or a per-command `player: u8` validated in
    `apply`.
Root cause: the lockstep design treats the frame's `player` as routing metadata for determinism
    (sort by player, `:1869`), not as authority; `Sim` was built for a single-operator game where every
    command is the local player's.
Impact: a hostile (or merely desynced) peer can move, attack with, or cancel production of the
    opponent's units; both peers stay hash-equal because both apply the same illegal command — the desync
    detector cannot see it. This is the one *game-integrity* gap in the zone, and it is invisible to any
    parser-level hardening (the packet is well-formed).
Why not covered by excluded backlog: AUD-* has no multiplayer authority item; WC3-SIM-001/002 are
    determinism/duplication, not authority.
Fix specification: thread the seat from `CommandFrame.player` into `Sim` and check ownership per unit id in
    `apply` (reject with an `Event::Error("not your unit")` and a telemetry counter), or split
    `step` into `step_seated(&[(u8, Vec<Command>)])`. The invariants lane I fixed (`CancelTrain` by index,
    duplicate `Build`) must be re-checked under the new signature.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the two-process scenario above must
    leave seat 0's unit in place and log a rejection.
Verification commands and exact result: static read at the cited lines; the existing loopback tests
    (`crates/wc3-sim/tests/multiplayer.rs`, 4 transport tests) only send the local seat's own commands, so
    they cannot catch it.
Related findings: WC3-SIM-001 (the detector that would not notice), L547-07
```

### 2.1 Checked and NOT a defect (negative results — do not re-file)

| hypothesis | result |
|---|---|
| `Fr::len`/`take` allow an announced length to size an allocation | no — count is checked against `limit` **and** `remaining` (`net.rs:324-340`, `:276-286`) |
| `Replay::decode` reserves memory from the announced payload length | no — `declared` must equal the actual file payload (`:1448-1456`), then count is capped by `MAX_FRAMES` and `remaining` |
| wire message length can be 2^32-1 | no — `recv_message` rejects 0 and > 64 MiB before allocating (`:1974-1976`) |
| BATCH command count unbounded | no — `r.len(4096)` (`:2020`) |
| scene counts unbounded | no — width/height ≤ 1024 (`:890-895`), every vector via `r.len(MAX_SCENE_ENTRIES)` + `remaining`, bitmap length exact (`:1570-1587`) |
| `CancelTrain { index }` panics on an out-of-range index | no — range-checked before `remove` (`lib.rs:1805-1808`) |
| `UpgradeBuilding`/`Research`/`LearnAbility` accept arbitrary ids | no — `intern_static_name` returns `None` for unknown ids (`wc3-replay/src/lib.rs:433-445`) |
| `read_bitmap` allocates before validating the payload | no — the length check precedes `with_capacity` (`:1572-1583`) |
| handshake accepts a mismatched seat/map/hash | no — `SeatConflict`, digest and start-hash checks (`:1807-1845`) |
| a peer can claim the other seat in a BATCH | no for *routing* — `player != self.remote_seat` is a protocol error (`:2027`); but the seat is not honoured as *authority* inside the sim (L547-11) |
| `HleRegistry` can bind an unknown-arity entry or call with the wrong argc | no — `ArityUnknown` at bind (`hle.rs:148-155`), `ArityMismatch` at call (`:221-228`) |
| a guest-supplied address is rounded into a thunk slot | no — stride check (`hle.rs:236-247`, `layout.rs:146-157`) |
| PE export table allocates from an announced count | no — capped at 65 536 (`pe.rs:247`) |
| PE section table / raw ranges escape the buffer | no — ≤ 96 sections, raw end checked against `bytes.len()` (`pe.rs:166,186-190`) |
| `wc3_replay::Lockstep::desync` indexes an empty checksum vector | no — `checksums.len() < 2 → continue` (`lib.rs:611-625`) |
| `intern()` pool poisons on a hostile name | poisoning panics only if a previous bind panicked (`guest_host.rs:230`); the pool holds `&'static` copies of image strings and grows by at most `HLE_SLOTS` = 65 536 names per boot attempt, never released — bounded per boot, unbounded across boots; a cap on imports (L547-01) bounds it too |

---

## 3. Coverage, depth and remaining gaps

### 3.1 What this lane covered

| path | lines | depth | status |
|---|---|---|---|
| `crates/wc3-sim/src/net.rs` | 2092 | read: framing (`Fw/Fr`), scene/setup codec, lockstep client/pair, desync detector, frame clock, replay codec/verify, wire transport; every `unwrap/expect` audited (grep) | AUDITED |
| `crates/wc3-guest/src/pe.rs` | 455 | read fully, line by line | AUDITED |
| `crates/wc3-guest/src/hle.rs` | 315 | read fully (registry, arity, classify) | AUDITED |
| `crates/wc3-guest/src/layout.rs` | 257 | read fully (regions, owner rule, thunk arithmetic) | AUDITED |
| `crates/wc3-guest/src/lib.rs` | 47 | read fully (provenance + module map) | AUDITED |
| `crates/wc3-guest/src/cpu.rs` | 751 | **partial**: struct/offsets, all `pub fn` signatures, the flag helpers, shift/rotate family, `width_mask`/`sign_bit`; the arithmetic bodies of add/sub/logic/inc/dec/bsf/bsr/bittest/cpuid/rdtsc are **spot-read** — no line-by-line claim | AUDITED (partial) |
| `crates/wc3-guest/tests/*.rs` (4 files) | 708 | read: 25 `#[test]`s against the contract they name; **not run** (freeze) | AUDITED (test) |
| `crates/wc3-guest/examples/pe_inspect.rs` | 92 | read (QA tool, not shipped) | AUDITED |
| `crates/wc3-re/src/guest_host.rs` | 404 | read fully; also the only consumer of the guest crate | AUDITED |
| `crates/wc3-replay/src/lib.rs` | 792 | read fully in the **serializer direction** (write/parse/decode/verify/Reader/Lockstep); `encode_command`/`decode_command` bodies were audited by lane I (WC3-RPL-001) and are referenced, not re-derived | AUDITED (second pass) |

`FILE_COVERAGE.csv`: 11 new rows (`crates/wc3-guest/**` had **zero** rows before this lane) plus the three
existing rows for `net.rs`, `wc3-replay/src/lib.rs` and `guest_host.rs` updated with this lane's findings.
See §6.

### 3.2 Still unaudited / not proven — with the reason

| # | zone | state | why |
|---|---|---|---|
| G-1 | `crates/wc3-guest/src/cpu.rs` arithmetic bodies (add/sub/logic/inc/dec/bsf/bsr/bittest/cpuid/rdtsc), 371 of 751 lines | **GAP** | the donor test vectors exist (`tests/recomp_cpu.rs:38-289`) but were not executed (freeze); the remaining risk is flag-value correctness, not memory safety — nothing there indexes memory |
| G-2 | PE32 import amplification at the 4096-descriptor maximum | **RESOLVED by measurement** | peak RSS 1 459 280 kB / 46 s from 98 914 B (§4.1); recorded so the fix wave has the real number instead of an extrapolation |
| G-3 | `Replay::from_path` with a real multi-GiB file (L547-03) | **NEEDS_PROBE** | writing an 8 GiB sparse file competes for the same flash quota (87 % used) that the product lanes build into; the mechanism is one `std::fs::read` call |
| G-4 | adversarial peer over a real socket (slow-loris, 64 MiB message, truncated frame mid-tick) | **GAP** | needs a hostile-peer harness; `tests/multiplayer.rs` has loopback matches (4 tests) but no adversarial peer, and the freeze forbids a new test. The parser-level bounds were proven by reading + the negative probes above |
| G-5 | desync end-to-end with a *real* divergence (two sims, one mutated) | **covered by lane I** (WC3-SIM-001 probe) | not re-derived; the net-side gate is `state_hash` at `:1887-1908` |
| G-6 | `wc3-guest` boundary end-to-end with a real `War3.exe` | **BLOCKED (external)** | the 1.27 binary is operator-supplied and not in the tree; `PeImage::parse` was exercised on synthetic images only |
| G-7 | lifted-code execution / guest memory / syscall surface | **BLOCKED (does not exist)** | no lifter, no loader, no guest memory in-tree (`guest_host.rs:11-13`); any claim about “what a malformed module can execute” would be fabricated |
| G-8 | `wc3-guest` no fuzz target | **GAP** | `ls -d fuzz crates/*/fuzz` → none; `grep -rn "fuzz" --include=Cargo.toml .` → none; `grep -rln "fuzz_target\|libfuzzer"` → none; no `proptest`/`quickcheck` either. The freeze forbids adding a harness. TZ §4.6 asks for bounded-resource fuzzing — L547-01 is exactly the class it would find |
| G-9 | replay container fuzz (net.rs `decode` + wc3-replay `parse`) | **GAP** | same freeze; the length/count relations were proven by reading, one asymmetry class by probe (L547-04) |
| G-10 | `apps/headless` replay path run on a real map | **BLOCKED (device/install)** | needs the operator's MPQ install; `apps/headless` was read (`main.rs:80-170`), not run |
| G-11 | `net.rs` `MapScene::load` on a hostile `.w3m` (WPM 4096×4096 → 16.7 M `bool`) | **GAP** | mechanism read (`net.rs:502-556` + `wc3-formats/src/wpm.rs:36-44` caps 4096); no map fixture, and the result is only reachable through the dead net path |

**Explicit non-claim:** none of G-1…G-11 is “clean”. Each is unproven, and the freeze (no new tests) is the
reason for the test-shaped ones, not the zone's structure.

### 3.3 Zones still unfinished beyond this lane (§9 remainder)

Recorded for the tech-lead so §9 is not read as “done”:

| §9 item | owner / state |
|---|---|
| `crates/wc3-render/src/daynight.rs` + remaining render subsystems | not in this lane; lane 540 covers the render items I-012…I-020 |
| simulation core / deterministic state transitions | lane I (529) covered `wc3-sim/**`; `state_hash` gap = WC3-SIM-001 |
| network protocol, command validation, resource bounds | **this lane** (§2) |
| replay parser/serializer + end-to-end verify | parser = lane 539/I-005; serializer = **this lane** (§2) |
| `wc3-guest` runtime/host boundary | **this lane** (§2) |
| release/build/package scripts, CI, supply-chain | lane K (531), NO-GO on the release gate chain (K-01) |
| Android lifecycle/install/upgrade | lane 541 (I-006/I-007/I-008) |
| test infrastructure, fuzzing, coverage gaps | this lane §3 + lane K §Coverage; no fuzz harness exists |

---

## 4. Exact probe output (verbatim)

### 4.1 `pe_imports_amp` — L547-01

Canonical run through the shared slot (this is the evidence of record):

```
$ scripts/h2-cargo run -q -p wc3-guest --example pe_imports_amp
PROBE control 1 descriptor x 1 thunk
PROBE imports desc=1 thunks=1 input_bytes=638 result=Ok imports=1 rss_before_kb=3708 rss_after_kb=4084 rss_delta_kb=376 elapsed_ms=0
PROBE realistic 4 descriptors x 32 thunks
PROBE imports desc=4 thunks=32 input_bytes=822 result=Ok imports=128 rss_before_kb=4080 rss_after_kb=4092 rss_delta_kb=12 elapsed_ms=0
PROBE hostile 1024 descriptors x 4095 thunks
PROBE imports desc=1024 thunks=4095 input_bytes=37474 result=Ok imports=4193280 rss_before_kb=4084 rss_after_kb=367688 rss_delta_kb=363604 elapsed_ms=4883
PROBE hostile-max 4096 descriptors x 4095 thunks
PROBE imports desc=4096 thunks=4095 input_bytes=98914 result=Err(BadImportDirectory) imports=0 rss_before_kb=4176 rss_after_kb=4280 rss_delta_kb=104 elapsed_ms=22556
RC=0
```

The `hostile-max` row shows `rss_delta_kb=104` because the probe dropped the vector before sampling `VmRSS`.
The probe was therefore extended to sample **`VmHWM` (peak RSS, reset through `/proc/self/clear_refs`)**, and
re-run through `rustc` against the crate's own rlib (the slot was held by the product-lane APK build):

```
$ rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/pe_imports_amp.rs \
    --extern wc3_guest=target/debug/deps/libwc3_guest-500acfe050084859.rlib \
    -L dependency=target/debug/deps -o $PREFIX/tmp/n547-peamp2                          # 0 errors
$ $PREFIX/tmp/n547-peamp2 1 1
PROBE imports desc=1 thunks=1 input_bytes=638 result=Ok imports=1 rss_before_kb=4036 rss_after_kb=4040 peak_before_kb=…  peak_delta_kb=…      elapsed_ms=0
$ $PREFIX/tmp/n547-peamp2 1024 4095
PROBE imports desc=1024 thunks=4095 input_bytes=37474 result=Ok imports=4193280 rss_before_kb=4060 rss_after_kb=367664 peak_before_kb=4104 peak_kb=367884 peak_delta_kb=363780 elapsed_ms=10493
$ $PREFIX/tmp/n547-peamp2 4096 4095
PROBE imports desc=4096 thunks=4095 input_bytes=98914 result=Err(BadImportDirectory) imports=0 rss_before_kb=4068 rss_after_kb=4272 peak_before_kb=4176 peak_kb=1459280 peak_delta_kb=1455104 elapsed_ms=46003
```

Reading: 4 193 280 imports = 1024 × 4095 (each thunk table carries a zero terminator), i.e. **112 imports per
input byte**, **9 700× the file size** resident. The maximum case reaches **peak 1 459 280 kB = 1.39 GiB and
46 s from a 98 914-byte file**, and it *still returns `Err`* — the allocation and the 16.7 M `String` clones
happen before the error. For scale: `MemAvailable` on this device was 4.1 GB at the time of the run.

### 4.2 `guest_host_base` — L547-02

```
$ rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/guest_host_base.rs \
    --extern wc3_re=target/debug/deps/libwc3_re-e49b49163b3a75c1.rlib \
    --extern wc3_guest=target/debug/deps/libwc3_guest-95f3e81a039a6184.rlib \
    -L dependency=target/debug/deps -o $PREFIX/tmp/n547-guestbase     # 0 errors
$ $PREFIX/tmp/n547-guestbase
PROBE layout WAR3_IMAGE_BASE=0x00400000 HLE_BASE=0x7F000000
PROBE base war3 va=0x00400000 owner=Unmapped boot=Ok entry=0x00401000 imports=1 thunk0=Some(2130706432)
PROBE base hle-window va=0x7F000000 owner=Overlap boot=Ok entry=0x7F001000 imports=1 thunk0=Some(2130706432)
PROBE base stack va=0x31000000 owner=Arena("stack") boot=Ok entry=0x31001000 imports=1 thunk0=Some(2130706432)
PROBE base teb va=0x3F000000 owner=Arena("teb") boot=Ok entry=0x3F001000 imports=1 thunk0=Some(2130706432)
PROBE base heap va=0x36000000 owner=Arena("heap") boot=Ok entry=0x36001000 imports=1 thunk0=Some(2130706432)
PROBE base unmapped va=0x10000000 owner=Unmapped boot=Err(image base 0x10000000 != layout base 0x400000)
```

`2130706432 = 0x7F000000 = HLE_BASE` — thunk 0 is the first slot of the window, which for the `hle-window`
image is also its own code region (`entry = 0x7F001000`).

### 4.3 `net_replay_identity` — identity of the net replay container

The `wc3_sim::net::Replay` half of scope item 3 (encode → decode → encode + `verify()` re-simulation),
built against the crate's own rlib because the shared slot was held:

```
$ rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/net_replay_identity.rs \
    --extern wc3_sim=target/debug/deps/libwc3_sim-fa307faa11b6170f.rlib \
    --extern glam=target/debug/deps/libglam-11ef78f8157a441a.rlib \
    -L dependency=target/debug/deps -o $PREFIX/tmp/n547-netreplay        # 0 errors
$ $PREFIX/tmp/n547-netreplay
PROBE net_replay bytes=373 frames=8 end_tick=8 fields_same=true bytes_identical=true verify="Ok(ticks=8 frames=8 end_hash_match=true)" end_hash=0x29C0108D82C0CE3F
PROBE net_replay checksum_flip_last_byte decode="Err"
```

So the net container is an identity: 373-byte payload, 8 frames, setup/start/end/frames all equal after the
round trip, byte-identical re-encode, and `verify()` re-simulates the 8 ticks to the recorded hash. A single
flipped byte in the trailing checksum is rejected. **No defect in this container's serializer.**

### 4.4 `replay_serializer` — identity + L547-04/L547-05 (the `wc3-replay` container)

**BEFORE the fix** (`scripts/h2-cargo run -q -p wc3-replay --example replay_serializer`, RC=0):

```
PROBE caps MAX_TICKS=2592000 MAX_RECORDS=1048576
PROBE identity bytes=380 ticks=12 records=12 write=true read=true rewrite=Ok(true) verify_hash_ok=true fields_and_bytes_identical=true
PROBE ticks_over_cap write=Ok read=Err(corrupt replay: tick count)
PROBE record_at_end write=Ok read=Err(corrupt replay: record tick outside the replay)
PROBE unordered write=Ok read=Err(corrupt replay: record ticks are not ordered)
PROBE player_offset byte 0->255 read_ok=true verify_ok=true
RC=0
```

Reading: the normal round trip is an identity (380 bytes, 12 records, byte-identical re-write, hash
reproduced), while **three replays that `write` accepted are rejected by `read` in the same process** —
the writer/reader asymmetry of L547-04. `player_offset` shows L547-05 (a seat byte of 255 parses and
verifies).

**AFTER the fix** — the same probe (`scripts/h2-cargo run -q -p wc3-replay --example replay_serializer`,
RC=0; the label was corrected between runs because the pre-fix probe hardcoded `write=Ok` in its output):

```
PROBE caps MAX_TICKS=2592000 MAX_RECORDS=1048576
PROBE identity bytes=380 ticks=12 records=12 write=true read=true rewrite=Ok(true) verify_hash_ok=true fields_and_bytes_identical=true
PROBE ticks_over_cap write_ok=false bytes=0 read=Err(corrupt replay: truncated)
PROBE record_at_end write_ok=false bytes=0 read=Err(corrupt replay: truncated)
PROBE unordered write_ok=false bytes=0 read=Err(corrupt replay: truncated)
PROBE player_offset byte 0->255 read_ok=true verify_ok=true
RC=0
```

Reading: the normal round trip is unchanged (still byte-identical, hash reproduced), all three replays the
reader would reject are now refused **before a byte is written** (`bytes=0`, so no truncated file is left
behind), and `player_offset` still parses (L547-05 remains, as designed — see §2).

### 4.5 exact writer errors (L547-04)

```
$ rustc --edition 2024 -O analysis/full-audit-2026-11-18/probes/lane-547/replay_write_errors.rs \
    --extern wc3_replay=target/debug/deps/libwc3_replay-6a31cae19fca4f20.rlib \
    --extern wc3_sim=target/debug/deps/libwc3_sim-fa307faa11b6170f.rlib \
    -L dependency=target/debug/deps -o $PREFIX/tmp/n547-write-errors      # 0 errors
$ $PREFIX/tmp/n547-write-errors
PROBE control write=Ok bytes=60
PROBE ticks_over_cap write=Err(kind=InvalidData msg=corrupt replay: tick count) bytes_written=0
PROBE record_at_end write=Err(kind=InvalidData msg=corrupt replay: record tick outside the replay) bytes_written=0
PROBE unordered write=Err(kind=InvalidData msg=corrupt replay: record ticks are not ordered) bytes_written=0
PROBE count_over_cap write=Err(kind=InvalidData msg=corrupt replay: record count) bytes_written=0
```

All four ceilings are enforced with the reader's own wording, as `InvalidData`, and `bytes_written=0` in
every rejected case — the writer cannot leave a half-written replay on disk.

---

## 5. H2 / ledger evidence

- corpus pack: `ev-229e955c0390efbd` (coding profile, weak-agent strength; gates read: persist-cadence,
  no-open-tails, corpus-cadence, turn-integrity, full-surface-audit).
- monitor ledger: **550** (`LEDGER-547-NET-GUEST`) — registered → checkpointed → **closed**
  (`result=done`, `h2_ids_json` = captures 1083/1084, experience 4766).
- captures: **1083** (measured findings + reachability), **1084** (probe outputs before/after, the L547-04
  fix, the coverage merge, the cross-lane `build-apk.sh` correction, and one self-inflicted measurement
  error: a CSV read-back inside an unflushed write reported 649 rows while the file was intact at 784).
- experiences: **4766** (`code-audit`): audit a container format in both directions; a hardened reader plus
  an unrevised writer produces recordings the same process cannot read; “the parser returns `Err`” is not a
  resource bound — measure `VmHWM`.
- no test files added (`git status` shows no `tests/` change), no device commands run, no
  `H2_CARGO_PRIORITY`, no `H2_CARGO_BYPASS`.
- files touched by this lane:
  - `crates/wc3-replay/src/lib.rs` — L547-04 fix only (`write` :106-151, `reject` :699-701); the file also
    carries lane 539's I-005 work, and its hash at hand-off is `ca5f57382bf18761` so a merge can prove
    which content it kept;
  - `analysis/full-audit-2026-11-18/lane-547-net-guest.md` (this report; its own hash is omitted because the
    hash changes with every edit — the content is the evidence);
  - `analysis/full-audit-2026-11-18/FILE_COVERAGE.csv` (784 rows, `33305ef3ed074915`, backup
    `FILE_COVERAGE.csv.bak-lane547-20261118-161507`);
  - `analysis/full-audit-2026-11-18/probes/lane-547/{pe_imports_amp,guest_host_base,net_replay_identity,replay_serializer,replay_write_errors}.rs`
    (+ `build-copies/` holding the two transient `crates/*/examples/` copies, moved back after the runs).
- `git status` restricted to this lane's paths: `M crates/wc3-replay/src/lib.rs` plus lane I's untracked
  `crates/wc3-replay/examples/` — no stray probe file is left inside a crate tree. (`analysis/**` is
  gitignored except `*.md`, so the probes and the CSV live on disk only, as for every other lane.)

---

## 6. Notes for the tech-lead

1. **`FILE_COVERAGE.csv` had no `crates/wc3-guest/**` rows at all** — 11 files / 2 633 lines of the §9 zone were
   outside the audit matrix. This lane adds them.
2. **§9's net/guest zones have no production consumer.** If the GO/NO-GO report says “network protocol
   audited”, it should also say “not shipped yet”; and if multiplayer is on the roadmap, L547-01/02/03 are
   the pre-ship blockers (impact class P2 the moment a peer or a module loader exists).
3. **Two lanes must not edit `crates/wc3-replay/src/lib.rs` at once — and two already did.** Lane 539's
   I-005 work (`MAX_VERIFY_TICKS`, `ReplayError::Duration`, `verify_with_limit`) landed in this file at
   16:11 while this lane was reading it; the L547-04 fix (write guards, `reject`) was applied at 16:32 on
   top of that version. The two hunks do not overlap (`write` at :106-151, `verify_with_limit` at :250),
   but a whole-file rewrite from either side would drop the other's work — the last writer wins silently.
   The file hash at hand-off is recorded in §5 so a merge can prove which content it kept.
4. **The replay format now has an asymmetric pair of gates** (reader caps, writer guards). The next editor of
   `MAX_*` must bump both; the writer's errors quote the reader's wording so a mismatch is visible.
5. **Lane K's note 3 is stale on this HEAD**: the `examples/*.rs` staleness trap it describes was replaced.
   `scripts/build-apk.sh:117` now asks `scripts/build_inputs.py native --newer-than target/release/libmain.so`
   and the answer on this tree is `["crates/wc3-audio/src/lib.rs", "crates/wc3-re/src/engine.rs"]` — probe
   sources under `examples/` are not build inputs any more (`scripts/build-apk.sh:87` records the change).
   The `analysis/full-audit-2026-11-18/probes/lane-547/build-copies/` convention is still the right place to
   park probe copies, but it no longer protects a red gate by itself.
6. **`crates/wc3-replay/examples/` is untracked and holds lane I's two probe copies** (`audit_i_replay.rs`,
   `audit_i_replay_bounds.rs`); lane I's report says they were meant to move to `probes/lane-I/`. Left in
   place (another lane's evidence, and harmless per note 5) — flagged so the merge does not sweep them into
   a commit.
