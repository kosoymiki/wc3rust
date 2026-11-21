# P0 GAP CLOSE — JASS parser edge cases (ledger 350, RE)

Date: 2026-11-13 · Layer: RE (`crates/wc3-re/**`, `crates/wc3-jass/**`)
Trigger: QA report gates `triggers.jass` 39/45, `triggers.return` 0/3,
`triggers.elseif` 0/1 FAIL.

## Root causes (observed, file:line before fix)

All three failures were in the **standalone** `wc3-jass` parser
(`crates/wc3-jass/src/lib.rs`), not in `wc3-re::jass`:

1. **Bare `return` consumed the next line** — the `return` arm called
   `skip_newlines()` *before* checking for a newline, so a bare `return`
   parsed the following `endfunction` / `set` / `endif` token as its return
   expression. This is also why 6 retail maps failed with
   `parse -> unknown statement endfunction` (Human06, Human09, NightElf06,
   NightElf07, Orc08, Undead05).
2. **`elseif` overwrote the else branch** — inside the `if` arm,
   `else_branch = vec![Stmt::If(cond, body, Vec::new())]` was reassigned for
   every `elseif` and then replaced by the `else` body. Only the last branch
   survived, so the first-true branch was lost (gate produced `n = 3`
   instead of `2`).

For contrast, `wc3-re::jass` was already correct: `parse_stmt` checks
`at_newline_or_eof()` before parsing a return value
(`crates/wc3-re/src/jass/parser.rs:307-318`), and `parse_if` keeps a
`Vec<(cond, body)>` branch list plus `else_body`
(`crates/wc3-re/src/jass/parser.rs:327-361`); the VM executes the first true
branch (`crates/wc3-re/src/jass/vm.rs:825-840`).

## Fixes

### `crates/wc3-jass/src/lib.rs`

- `return` now terminates immediately on `Token::Newline` / `Token::End`,
  accepts `return nothing` on the same line, and otherwise parses the
  expression **without** skipping newlines.
- `if` now collects `(cond, body)` branch pairs and the optional `else` body,
  then builds the else-chain from the last branch backwards:

  ```rust
  let mut else_branch = final_else;
  for (cond, body) in branches.into_iter().rev() {
      else_branch = vec![Stmt::If(cond, body, else_branch)];
  }
  ```

  This preserves first-true semantics for any chain length and keeps the final
  `else`.

### `crates/wc3-jass/Cargo.toml`

- Removed the **unused** `wc3-sim` dependency (only comments referenced it).
  This decouples the parser crate/gate from gameplay-layer build state and
  unblocked the parser tests while `wc3-sim` was briefly red during another
  owner's change. No API change; `wc3-jass` still depends on `wc3-core` and
  `wc3-re`.

## Tests (per case)

`crates/wc3-jass/src/lib.rs` (5 new unit tests, 11 total):

- `bare_return_terminates_at_the_newline` — return at end, before a statement,
  and inside `if`.
- `bare_return_skips_following_statements_at_runtime` — the `set` after a bare
  `return` never executes.
- `elseif_keeps_the_first_true_branch` — `false / true / true` → `n = 2`.
- `elseif_chain_falls_through_to_else_once` — no branch true → else body.
- `elseif_chain_runs_a_middle_branch` — `false / false / true / true` → `n = 3`.

`crates/wc3-jass/tests/triggers_gates.rs` (new integration gate) replicates the
three QA gates exactly (same scripts, same 44-map corpus, same assertions):

- `triggers.return`: 3/3 bare-return scripts parse.
- `triggers.elseif`: chain keeps `n = 2`.
- `triggers.jass`: synthetic timer trigger fires twice + all 44 retail
  `war3map.j` scripts parse.

`crates/wc3-re/src/jass/parser.rs` (2 new tests, same edge cases, locks the
engine parser against the standalone parser drifting again):

- `bare_return_does_not_consume_the_next_statement`
- `elseif_chain_keeps_every_branch_and_the_else`

## Evidence

| Gate | Command | rc | Result |
|---|---|---:|---|
| wc3-jass lib | `cargo test -p wc3-jass --lib --offline` | **0** | 11 passed — `analysis/agent-re/round9-jass-tests.log` |
| Trigger gates (integration) | `cargo test -p wc3-jass --test triggers_gates --offline -- --nocapture` | **0** | `GATE triggers.return PASS 3/3`, `GATE triggers.elseif PASS 1/1`, `GATE triggers.jass PASS 45/45 (44 maps, 19 420 functions)` — `analysis/agent-re/round9-trigger-gates.log` |
| wc3-re parser tests | `cargo test -p wc3-re --lib --offline jass::parser` | **0** | 4 passed (2 new) |
| Full RE/JASS | `cargo test -p wc3-re -p wc3-jass --offline` | **0** | wc3-re lib **63**, wc3-jass lib **11**, triggers gate **1**, integration 5+1+2+3 — `analysis/agent-re/round9-final-tests.log` |
| wc3-jass all targets | `cargo check -p wc3-jass --all-targets --offline` | **0** | `analysis/agent-re/round9-jass-check.log` |
| Release build | `cargo build --release -p wc3-re --bin wc3re --offline` | **0** | `target/release/wc3re` 1 991 744 B — `analysis/agent-re/round9-release-build.log` |
| Release scan | `target/release/wc3re scan analysis/war3_full3 --json analysis/agent-re/scan-r9.json --md analysis/agent-re/SCAN-r9.md` | **0** | `ok=true`, `maps.errors=0`, `scripts.unimplemented={}`, `main_ok=44/44`, `natives=791`, `steps=1 313 749` — `analysis/agent-re/scan-r9.json` |
| Canonical QA gates | `cargo run --offline -- --out ../../../analysis/agent-qa/parity/report-r9.json --assets ... --maps ... --tft ...` (scripts/qa/parity) | 2 (PARTIAL: 3 GAP rows, 0 FAIL) | `GATE triggers.jass PASS 45/45`, `GATE triggers.return PASS 3/3`, `GATE triggers.elseif PASS 1/1`; overall `PARITY PARTIAL gates=23 fail=0 gap=3` — `analysis/agent-re/round9-parity-gates-canonical.log`, `analysis/agent-qa/parity/report-r9.json` |

Note: the QA parity binary depends on the gameplay crates. It was briefly
unbuildable while `wc3-sim` was red during another owner's change
(`missing field buildable`, `creeps_gen.rs`); once `cargo check -p wc3-sim`
returned 0, the canonical binary ran and all three trigger gates passed. The
integration gate above is the same logic and remains runnable independently.

## Result

`triggers.jass`, `triggers.return`, `triggers.elseif` now pass at the parser
root. No engine/map-runtime regression: release scan is still 0 errors,
`unimplemented={}`, 44/44 `main`, 791 natives.

## Risks / Next

- QA should re-run its full parity report on the current tree; the three
  trigger gates are closed, remaining FAIL/GAP rows (`train.menu_linkage`,
  `items.schema`, `cast.coverage`, …) are other layers.
- `wc3-jass` and `wc3-re::jass` now both have regression tests for bare return
  and elseif chains; keep them in lockstep if either parser changes.
- The standalone `wc3-jass` runtime does not initialize `globals` declarations
  (observed while adding the runtime test; it reads missing globals as Null and
  `set` creates them). Not part of the failing gates; flagged for the layer
  owner if exact global initial values are needed.

## H2

- Capture **762**; experience **4428**.
- Ledger **350** closed `completed`, `verification.ok=true`, warnings 0.
