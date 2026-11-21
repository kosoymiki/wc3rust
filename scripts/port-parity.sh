#!/data/data/com.termux/files/usr/bin/bash
# port-parity — the rust-anything loop for wc3rust, without their binary.
#
# The upstream tool (github.com/erkamyaman/rust-anything) records the original's
# behaviour, replays it against the Rust port and refuses a gate until every case
# matches. Its CLI ships prebuilt desktop binaries and has no Termux/android-arm64
# build, so the loop runs here by hand, with the same contract:
#
#   record <unit> -- <command...>    capture the reference behaviour (argv)
#   record <unit> --cmd "<string>"   capture a shell command line
#   diff   <unit>                    replay every recorded command and compare
#   diff   <unit> -- <command...>    replay one command line
#   gate   <unit>                    replay + recording + unimplemented!/todo! + cargo check
#   status [--json]                  units, cases, last diff/gate, stale markers (alias: list)
#
# Cases live in analysis/port-parity/<unit>/cases.jsonl; the recording is the source of
# truth. Intended differences go to known-diffs.jsonl with a reason. H2 gates:
# gate.port.parity-loop, gate.port.no-fixture-edit.
#
# The engine is scripts/port_parity.py (same contract, JSON support, all-command replay).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/port_parity.py" "$@"
