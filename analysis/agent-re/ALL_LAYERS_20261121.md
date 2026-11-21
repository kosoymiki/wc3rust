# AGENT L — guest PE / RE layer (all-layers push, 2026-11-21)

Ledger 608. Owner paths: `crates/wc3-guest/**`, `crates/wc3-re/**`. Read-only everything else.
Verification command for this layer: `scripts/h2-cargo test -p wc3-guest -p wc3-re --offline`.

## Goal

Guest machine + RE tooling correctness: PE32 census (section table, exports by
name and ordinal, imports by name and ordinal), HLE ABI parity, typed errors on
truncated/forged input instead of panics or unbounded allocation, named tests,
and no `unsafe`.

## Environment and oracle

- Termux/Android aarch64, `rabin2 6.2.6` (independent PE reader) — observed
  2026-11-21.
- The operator's PE32 binaries are present in the repository as MPQ-extracted
  copies: `analysis/game_files/Files100_War3.exe` (713 494 B),
  `Files100_Game.dll` (7 725 118 B), `Files100_Storm.dll` (311 360 B),
  `Files100_ijl15.dll` (372 736 B), plus `Mss32.dll`, `BNUpdate.exe`,
  `WorldEdit.exe`, `Warcraft III.exe`. They are untracked (600), so the
  real-corpus tests skip when the corpus is absent, exactly like
  `crates/wc3-re/tests/real_corpus.rs`.
- No live Warcraft III installation or `war3.exe` path exists on this device:
  the census below is against the extracted binaries, not against a running
  game. That boundary is stated, not worked around.

## Baseline (observed before any change)

`target/debug/examples/pe_inspect` (pre-change binary) on the four binaries
already matched `rabin2` on section/export/import counts:

| file | sections | exports | imports |
|---|---|---|---|
| War3.exe | 7 | 695 (all named) | 487 |
| Game.dll | 5 | 1 (`GameMain`) | 931 |
| Storm.dll | 5 | 370 (ordinal-only) | 226 |
| ijl15.dll | 6 | 6 (named) | 59 |

So the happy path was correct. The defects were in forged-input behaviour,
census completeness and the guest-host boundary.

## Root causes fixed

### 1. PE32 import table: amplified memory and silent truncation (audit L547-01)

Measured by lane 547 before the fix: a 37 474-byte image produced 4 193 280
imports, +363 780 kB peak RSS and 10 493 ms; a 98 914-byte image cost
+1 455 104 kB and 46 003 ms before returning `Err`. Root cause: descriptor loop
(4096) and thunk loop (4096) bounded independently, descriptors may share one
thunk array, and every thunk cloned the module `String`.

Fix (`crates/wc3-guest/src/pe.rs`):

- `MAX_IMPORTS = 65_536` on the flattened result (`pe.rs:61`), with
  `MAX_IMPORT_DESCRIPTORS` (`:63`) and `MAX_THUNKS_PER_DESCRIPTOR` (`:65`);
- module names parsed once per name RVA and shared as `Arc<str>` (`Import::module`, `pe.rs:156`);
- import names cached by hint/name RVA and shared, `BTreeMap` not a linear scan;
- a thunk array must reach its `0` terminator inside the per-descriptor window —
  running out is `BadImportDirectory`, never a silent stop;
- name reads are bounded per name (`MAX_NAME_LEN = 4096`, `pe.rs:67`) and in
  total (`MAX_TOTAL_NAME_BYTES = 4 MiB`, `pe.rs:69`) through `NameBudget`;
  without it, thousands of pointers into one long string still multiplied.

After, on the same probe (`analysis/full-audit-2026-11-18/probes/lane-547/pe_imports_amp.rs`,
rebuilt against the new rlib):

```
PROBE imports desc=1024 thunks=4095 input_bytes=37474 result=Err(BadImportDirectory) imports=0 peak_delta_kb=2588 elapsed_ms=35
PROBE imports desc=4096 thunks=4095 input_bytes=98914 result=Err(BadImportDirectory) imports=0 peak_delta_kb=2568 elapsed_ms=30
PROBE realistic 4 descriptors x 32 thunks input_bytes=822 result=Ok imports=128 peak_delta_kb=12
```

363 MB → 2.6 MB and 46 s → 30 ms; the over-cap image is now a typed error.

### 2. Export directory: structural validation, forwarders, aliases, ordinals

`parse_exports` (`pe.rs:352`) now:

- validates the whole function/name/ordinal tables against the file buffer
  before allocating (`functions + 4·NumberOfFunctions ≤ len`, …), so a 200-byte
  file claiming 65 536 exports is a typed error rather than a 256 KB read;
- rejects a non-zero count with a zero table RVA (previously `rva_to_offset(0)`
  read the DOS header as a table);
- clamps `NumberOfRvaAndSizes` to the space actually present in the optional
  header, so a short optional header cannot make the parser read section
  headers as data directories;
- records forwarded exports (`Export::forwarder`, `pe.rs:144`) when the RVA
  points inside the export directory;
- converts a `Base + index` ordinal that does not fit `u16` into
  `BadExportDirectory` instead of truncating it;
- preserves alias names: two names on one function index produce two export
  records (previously one name silently overwrote the other).

### 3. x86-32 semantics: two real flag/result divergences and a latent panic (audit L547-10)

`crates/wc3-guest/src/cpu.rs`:

- `normalize_bits` + total `width_mask`/`sign_bit` (`cpu.rs:604-626`): any width
  other than 8/16/32 is treated as 32; no `bits` value can panic (the previous
  `1u32 << (bits - 1)` underflowed at `bits == 0` and `bit % bits` divided by
  zero).
- `rol`/`ror` (`:338`, `:360`): a masked count that is a whole multiple of the
  8/16-bit operand width (`rol al, 8`) leaves the value but still loads `CF`
  from the result. The old code returned early and left `CF` stale.
- `shld`/`shrd` (`:487`, `:514`): at `count == operand width` the result is the
  source (defined x86 behaviour), not 0/destination.

### 4. HLE registry: owned names, explicit calling-convention contract

`crates/wc3-guest/src/hle.rs`:

- `HleEntry::module`/`symbol` are owned `String`s (`hle.rs:128-131`). The donor
  binds static literals; names here come from the parsed image. The previous
  consumer (`guest_host::bind_fixed` → `intern`) leaked every distinct name for
  the process lifetime — a forged image with thousands of names grew that pool
  without bound. Binding is boot-time only, so the two allocations per entry
  never touch the call path; the global `OnceLock`/`Box::leak` pool is gone.
- `CallConv::pops_args`/`stack_bytes` (`hle.rs:53-64`) state the ABI contract the
  dispatcher must apply (stdcall callee pops `4·arity`, cdecl pops 0). It was
  stored metadata before and unreachable from the host API.
- `HleRegistry::call` copies `(arity, handler)` out of the entry instead of
  relying on `Copy`, so owned entries stay allocation-free on the call path.

### 5. Guest host: image-base accept-list, range collision, entry bounds, bind lies (audit L547-02)

`crates/wc3-re/src/guest_host.rs`:

- `boot` (`guest_host.rs:133`) accepts exactly `WAR3_IMAGE_BASE` (the layout
  does not list the executable in its arena table) and then requires
  `[base, base + SizeOfImage)` to stay clear of every `layout::REGIONS` entry.
  The old predicate rejected only `Owner::Unmapped`, so an image based in the
  HLE thunk window (`Overlap`), a thread stack, the TEB or the heap booted
  successfully — code and thunk table aliasing in the HLE case.
- `EntryOutOfRange` (`:51`) rejects `AddressOfEntryPoint >= SizeOfImage`;
  `zero SizeOfImage` and range overflow are `ImageRange` (`:49`).
- After `bind` returns true the host verifies the slot is actually bound;
  a callback that lies can no longer leave a thunk that resolves `Unbound`
  (`:197`).
- `BoundImport` now carries the module as a shared `Arc<str>` and the import
  ordinal (`:83`), and `GuestHost::entry` (`:251`) exposes the full ABI record
  (module, symbol, arity, convention) to the dispatcher.

## Tests added (mechanic prefix `recomp.`)

`crates/wc3-guest/tests/recomp_pe32.rs`:

- `recomp_pe32_parses_forwarded_export`
- `recomp_pe32_export_aliases_are_preserved`
- `recomp_pe32_import_cap_bounds_shared_thunk_arrays` (16×4095 parses, 17×4095 is a typed error, one `Arc` for the module)
- `recomp_pe32_import_name_budget_is_bounded` (1300 suffix pointers → typed error)
- `recomp_pe32_overlong_name_is_typed`
- `recomp_pe32_import_thunk_table_must_terminate`
- `recomp_pe32_shipped_census_matches_rabin2` (real corpus, rabin2 oracle, skip-if-absent)
- `recomp_pe32_shipped_host_ordinals_resolve` (Game.dll's 644 War3 ordinals, 92 Storm ordinals and 3 ijl ordinals all resolve to exports)

`crates/wc3-guest/src/pe.rs` (unit): short optional header, data-directory
clamp, zero-RVA export table, export table beyond file.

`crates/wc3-guest/tests/recomp_cpu.rs` and `cpu.rs` (unit): rotate-by-width `CF`,
double-shift-by-width, total width argument.

`crates/wc3-guest/tests/recomp_hle_abi.rs` and `hle.rs` (unit): calling-convention
stack contract, runtime (owned) names.

`crates/wc3-re/src/guest_host.rs` (unit): reserved-base rejection (HLE window,
stack, thread-stack, TEB, heap), arena collision, overflow, zero size, entry out
of range, bind-that-did-not-bind, ordinal imports, exposed convention.

`crates/wc3-re/tests/recomp_host_shipped.rs` (new file): boots the shipped
`War3.exe` (487 imports, 43 ordinal, entry `0x467fdc`, 695 exports) and refuses
the shipped `Game.dll` base (`0x6f000000`).

## Verification

Diagnostic compile/run (guest has no dependencies; `rustc` direct, while the
shared cargo FIFO was held by a release build):

```
wc3-guest lib:                rc=0
lib unit tests:               29 passed; 0 failed
recomp_pe32:                  12 passed; 0 failed
recomp_cpu:                   16 passed; 0 failed
recomp_hle_abi:                9 passed; 0 failed
recomp_layout:                 5 passed; 0 failed
```

Official gate: `scripts/h2-cargo test -p wc3-guest -p wc3-re --offline` → **rc=101**.
Every target this layer owns is green; the failures in that run were outside the
guest PE/RE layer (one is now fixed, the other has an external owner):

1. `wc3-re --lib mapboot::tests::maps_jass_natives_cover_called_set` — the
   shipped maps call 13 JASS natives that are not in the registry:
   `defineStartLocation`, `forcePlayerStartLocation`, `getGameTypeSelected`,
   `getUnitRace`, `groupEnumUnitsSelected`, `isQuestDiscovered`, `pauseCompAI`,
   `setPlayerRacePreference`, `setPlayerRaceSelectable`, `setStartLocPrio`,
   `setStartLocPrioCount`, `syncSelections`, `unitRemoveBuffs`. The panic is at
   `crates/wc3-re/src/mapboot.rs:385`. `crates/wc3-re/src/jass/natives.rs`
   carries +159 uncommitted lines from the campaign/JASS lane, so implementing
   them here would collide with in-flight work. Owner: campaign-jass lane
   (tech lead to schedule); nothing in this layer can close it without touching
   that file.
2. `wc3-re --lib mapcontent::tests::mapcontent_melee_ponds_2_…` — the donor map
   `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x`
   is absent on this device. Fixed in this pass: the corpus test now skips when
   its fixture is absent (the same contract as `tests/real_corpus.rs`), and the
   lib target is 93 passed / 1 failed (only mapboot) after the fix.

Per-target results (`--no-fail-fast`, same selection):

| target | result |
|---|---|
| `wc3-guest` lib | 29 passed, 0 failed |
| `recomp_cpu` | 16 passed |
| `recomp_hle_abi` | 9 passed |
| `recomp_layout` | 5 passed |
| `recomp_pe32` | 12 passed |
| `wc3-re` lib | 93 passed, 1 failed (mapboot, above) |
| `campaign_boot` | 2 passed |
| `campaign_seats` | 1 passed |
| `jass_suspension` | 6 passed |
| `merge_parity` | 5 passed |
| `re_surface` | 1 passed |
| `real_corpus` | 2 passed |
| `runtime_wiring` | 3 passed |
| `recomp_host_shipped` | 2 passed (rerun after the test-expectation fix) |

The first no-fail-fast run caught two wrong expectations in the new
`recomp_host_shipped` test — War3 imports 68 ordinals (43 Storm + 25 wsock32,
`rabin2` prints wsock32's names from its own database) and its export names run
`A1 … LD`, not all starting with `A`. Both were corrected and the target
re-ran green; the corrections are recorded here so the mistake is visible, not
smoothed over.

Targeted gates (same script, exact targets):

```
scripts/h2-cargo test -p wc3-guest --offline                    → rc=0 (71 tests)
scripts/h2-cargo test -p wc3-re --test recomp_host_shipped ...  → rc=0 (2 tests)
```

Census after the change (same binaries, `pe_inspect`, rc=0 for all four):

| file | sections | exports | imports | matches rabin2 |
|---|---|---|---|---|
| War3.exe | 7 | 695 named / 0 forwarders | 487 (12 modules) | yes |
| Game.dll | 5 | 1 named (`GameMain` @ 0x4180) | 931 (13 modules) | yes |
| Storm.dll | 5 | 370 ordinal-only (101…647) | 226 (7 modules) | yes |
| ijl15.dll | 6 | 6 named | 59 (1 module) | yes |

Full per-file census output (sections, every export prefix, import modules):
`analysis/agent-re/pe_census_20261121.md` (and the raw `.txt` beside it),
generated by `wc3-guest/examples/pe_inspect`, rc=0 for all four.

`unsafe`: zero occurrences in `crates/wc3-guest` and `crates/wc3-re`
(`rg -n unsafe crates/wc3-guest crates/wc3-re` → no matches).

## Follow-up 2026-11-21 — campaign native coverage (ledger 615)

The campaign lane measured 13 unimplemented natives / 1360 calls and a red
`mapboot::tests::maps_jass_natives_cover_called_set`. The exact patch
(`analysis/agent-jass/PATCH_wc3-re_natives_campaign.md`) was applied to
`crates/wc3-re/src/jass/natives.rs`:

- three arms that could never match — the dispatcher lowercases the name — were
  corrected: `setsky model` → `setskymodel` (already present),
  `setunit CreepGuard` → `setunitcreepguard`,
  `define startlocation`/`defineStartLocation` → `definestartlocation`;
- `isquestdiscovered` reads `Quest::discovered`, the getter half of the
  existing `questsetdiscovered`;
- `getunitrace` maps the rawcode's first letter to the `common.j` RACE_*
  values (`h`=1, `o`=2, `u`=3, `e`=4, neutral/other=7, invalid handle=0), which
  restores Undead07's shield region filters;
- the remaining ten lobby/selection/AI/buff natives are explicit arms:
  `setplayerracepreference`, `setplayerraceselectable`,
  `forceplayerstartlocation`, `setstartlocprio`, `setstartlocpriocount`,
  `pausecompai`, `unitremovebuffs`, `syncselections` are the semantically
  correct no-ops for a headless VM; `groupenumunitsselected` clears the group
  (no interactive selection); `getgametypeselected` returns 0 so
  `Blizzard.j:6414 InitGenericPlayerSlots` leaves campaign placement alone.

While applying the patch, the same "arm can never match" defect was found in 15
more literals of the same dispatch (a literal with a space, CamelCase or a
non-ASCII character is matched against a lowercased name). They were repaired
against the 797 native names extracted from the shipped `Scripts/*.j`:
`getunitdefaultacquire range` → `getunitdefaultacquirerange`,
`groupenumunitsoft Typecounted` → `groupenumunitsoftypecounted`,
`setfog staterect` → `setfogstaterect`, plus dead duplicate variants removed
(`regionclear cell`, `trigger syncstart`, `gettrigger eventid`,
`geteventgame state`, `gettriggerexecCount`, `geta iplayer`,
`gettow nunitcount`, `getnext expansion`, `getmega target`, `setexpansiò n`,
`getplayerunittypeCount`, `gettriggeringe ...`, and the two wildcard
`settrackable*`/`trackable*` arms, which Rust matches literally). A scan of the
file now reports zero dead literals.

One stale assertion remained in `maps_jass_natives_cover_called_set`: it
required `pancameratotimed`, which the 44-map boot window never calls — the
campaign lane's own pre-change coverage JSON
(`analysis/agent-jass/native_coverage_20261121.json`) does not contain it
either, so the requirement was masked by the unimplemented panic. It now
requires `camerasetupsetdestposition` (1000 calls), which the boot does drive;
the other three camera natives stay.

Results:

```
scripts/h2-cargo test -p wc3-re --lib mapboot --offline   → rc=0 (2 passed, 121.70 s)
wc3re map-boot ...                                        → rc=0
  map-boot maps=44 ok=44 main_ok=44 executed_triggers=682 unique_natives=325 unimplemented=0
analysis/agent-re/native_coverage_after_natives_20261121.json → "unimplemented": {}
scripts/h2-cargo test -p wc3-re --offline                 → rc=0 (lib 94 passed, 0 failed; 12 targets all ok)
```

Before/after: **13 unimplemented / 1360 calls → 0 / 0**.

## Risks / next

- The real-corpus tests skip when `analysis/game_files` is absent (untracked).
  A fresh clone exercises only the synthetic fixtures; that is the same
  contract as `real_corpus.rs`.
- `import_cap` uses 65 536 flattened imports; a legitimate binary with more
  would be rejected with `BadImportDirectory`. The shipped WC3 binaries use 487
  and 931.
- Export forwarders and aliases are parsed but `GuestHost` still projects only
  `(name, rva)` pairs; a dispatcher that maps exports to code must skip
  `Export::is_forwarder()` — the data is now available.
- The HLE `esp` bookkeeping is intentionally left to the dispatcher; the ABI
  contract is published as `CallConv::stack_bytes` and pinned by tests.
- Other agents' uncommitted changes in `crates/wc3-re` (`engine.rs`, `jass/*`,
  `mapfiles.rs`) were not touched.
