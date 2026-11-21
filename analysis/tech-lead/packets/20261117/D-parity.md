# PACKET D — seed/engine parity tail (owner: parity)

## Mission
Close the known staleness: the H2 corpus is at 591 principles / 211 gates in the live DB, while the
Windows engines and the portable base still pin seed `c990f1a1` (574 principles). Rebuild the engines
with the current seed, regenerate the portable base, and prove every gate.

## Evidence snapshot (from the session state)
- `python3 tools/seed-sync.py --check` → `SEED_PARITY_OK principles=591 db=591` (live).
- `~/src/opencode-h2-custom/dist-windows/engines/MANIFEST.json` →
  `corpus.source_seed_sha256[0:16]=c990f1a121ffef14`, `built_at=2026-11-16T20:14:35Z`, 11 binaries.
- `sha256sum ~/src/rust-hooks/src/corpus_seed.json` → `c990f1a121ffef14...` was the pre-merge value;
  after `tools/seed-sync.py` the live seed is the merged 591 set (`dist-seed/corpus_seed.merged.json`).
- Gates already verified today: `tools/h2-prompt-gate.sh` PASS, secret scan 0 rows,
  `tools/check-portable.py --base ~/src/h2-portable-base` PASS.

## Invariants
- Rust-first: rebuild `h2oc`/`h2turbod` from `~/src/rust-hooks`, atomic install (temp + rename), never
  `cp` over a running binary (ETXTBSY).
- Never publish H2 to git (no `git init`, no remote, no push) — H2 is not a git project.
- The user's Windows machine is remote: you cannot run `opencode.exe` here. Verify what is verifiable
  locally (engines build, manifest pins, portable base, gates) and state exactly what remains for the
  operator to run on Windows.

## OWNER_PATHS
- ~/src/opencode-h2-custom/tools/windows/**
- ~/src/opencode-h2-custom/dist-windows/**
- ~/src/h2-portable-base/**
- ~/src/opencode-h2-custom/analysis/parity-20261117/**

## READ_ONLY
~/src/rust-hooks/** (source of truth — read, do not edit), ~/.harness2/** (runtime), packages/**.

## Phase plan
1. Confirm the live seed sha (`h2oc corpus-seed` output / `corpus_seed.json`) and the manifest pin.
2. Run the engine build (`bash tools/windows/rust-patch/build-engines.sh`) with the current seed;
   record the new `source_seed_sha256`, `built_at`, binary count, and each binary's sha256.
3. Regenerate/refresh the portable base (`tools/check-portable.py` gates) and run the prompt gate +
   secret audit again.
4. Write `analysis/parity-20261117/REPORT.md`: before/after pins, gate outputs, and the exact
   one-line command the operator runs on Windows to swap the engines (rename-live, no overwrite).

## Acceptance artifacts
- `dist-windows/engines/MANIFEST.json` with the 591 seed sha and a fresh `built_at`.
- `analysis/parity-20261117/REPORT.md` with before/after table + gate output tails.

## Pitfalls
- `build-engines.sh` takes several minutes: run it detached with a log (`setsid nohup ... > log`),
  then read the log; do not poll in a loop.
- `scripts/h2-cargo` is herofolends-only; for rust-hooks use plain `cargo build --release` there.
- Keep the `.bak` copies of replaced binaries.

## Report format
Goal | Actions | Evidence | Result | Risks/Next | H2_PERSISTED ids.
