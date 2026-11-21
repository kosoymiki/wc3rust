# FILE_COVERAGE.csv — integrity check (ledger 560, 2026-11-18)

File: `analysis/full-audit-2026-11-18/FILE_COVERAGE.csv`
Size: 142.5 KB · schema: `path,subsystem,status,lines,probes,findings,reviewer,notes` (8 columns)

## Verdict

**UNION — not last-writer-wins.** All five lane contributions are present at the same time;
the 11 lane-547 rows are additive. No lane overwrote another.

## Structural checks

| Check | Result |
|---|---|
| Header lines (`path,subsystem,status,…`) | 1 |
| Data rows | 784 |
| Unique `path` values | 784 (duplicates: **0**) |
| Malformed/short rows | 0 |
| Blank lines | 0 |
| Empty `status` values | **0** |
| Status values | `AUDITED` 696 · `NOT_APPLICABLE` 70 · `BLOCKED` 18 |

## Reviewer counts (sum = 784)

| Reviewer | Rows |
|---|---|
| lane-K | 228 |
| I/529 | 203 |
| lane-J | 149 |
| H (ledger 528) | 102 |
| lane-G | 91 |
| 547 | 11 |
| **Total** | **784** |

## Union proof (every older snapshot is a subset)

| Snapshot | Rows | Paths missing from current |
|---|---|---|
| `FILE_COVERAGE.csv.bak-lane547-20261118-161507` (pre-547 merge) | 773 | **0** |
| `FILE_COVERAGE.csv.bak-laneI-merge-142821` (I+H merge) | 305 | **0** |
| `FILE_COVERAGE.csv.bak-laneJ` | 228 | **0** |
| `lane-J-coverage.csv` | 149 | **0** |
| `lane-K-coverage.csv` | 228 | **0** |

Rows added after the pre-547 snapshot (exactly the lane-547 guest crate, 11 files):

```
crates/wc3-guest/Cargo.toml
crates/wc3-guest/examples/pe_inspect.rs
crates/wc3-guest/src/{cpu,hle,layout,lib,pe}.rs
crates/wc3-guest/tests/{recomp_cpu,recomp_hle_abi,recomp_layout,recomp_pe32}.rs
```

The lane-I overwrite incident is closed: the 305-row I+H snapshot survives in full inside the
784-row file, and the current file is also a strict superset of the pre-547 union (773 rows).

## Reproduce

```sh
python3 - <<'EOF'
import csv
cur=set(r['path'] for r in csv.DictReader(open('analysis/full-audit-2026-11-18/FILE_COVERAGE.csv')))
for f in ['FILE_COVERAGE.csv.bak-lane547-20261118-161507',
          'FILE_COVERAGE.csv.bak-laneI-merge-142821',
          'FILE_COVERAGE.csv.bak-laneJ',
          'lane-J-coverage.csv','lane-K-coverage.csv']:
    s=set(r['path'] for r in csv.DictReader(open('analysis/full-audit-2026-11-18/'+f)))
    print(f, len(s), 'missing', len(s-cur))
EOF
```
