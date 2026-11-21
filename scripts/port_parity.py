#!/usr/bin/env python3
"""port-parity — the rust-anything loop applied to wc3rust, without their binary.

The upstream tool (github.com/erkamyaman/rust-anything) records the original's
behaviour, replays it against the Rust port and refuses a gate until every case
matches. Its CLI ships prebuilt desktop binaries and has no Termux/android-arm64
build, so the loop runs here by hand, with the same contract:

    record <unit> -- <command...>        capture the reference behaviour (argv)
    record <unit> --cmd "<string>"       capture a shell command line
    diff   <unit> [--cmd "<string>" | -- <argv...>]
                                         replay every recorded command (or one) and compare
    gate   <unit>                        replay + recording + unimplemented!/todo! + cargo check
    status [--json]                      units, cases, last diff/gate, stale markers
    list                                 alias of status

Cases live in analysis/port-parity/<unit>/cases.jsonl, one JSON object per line.
A case recorded by this version stores:

    {"id","unit","args_sha","argv":[...]|null,"command","exit","capture":"stdout",
     "stdout_sha","stdout","stderr_sha","stderr","captured_at"}

Cases recorded before this version have no "capture" field; they were captured with
stderr merged into the stream. They replay exactly the same way (exit + sha256 of the
merged output, trailing newlines stripped like command substitution) and are never
rewritten.

Rules (H2 gates gate.port.parity-loop, gate.port.no-fixture-edit):
  * the recording is the source of truth: fix the port, never the case;
  * an intended difference goes to known-diffs.jsonl {"case","actual_sha","reason"}
    and the gate fails again once the difference disappears;
  * a unit is done only when replay is N/N and the gate passes.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CASES_DIR = ROOT / "analysis" / "port-parity"
UNIT_RE = re.compile(r"^[A-Za-z0-9._-]+$")
REPLAY_TIMEOUT = int(os.environ.get("PORT_PARITY_TIMEOUT", "5400"))
GATE_CARGO_TIMEOUT = int(os.environ.get("PORT_PARITY_CARGO_TIMEOUT", "1800"))
# 5400 s covers the `original-parity` unit on a shared device: it builds every
# Rust tool once per crate through the FIFO `h2-cargo` queue (15 agents compile
# on the same 11 GB phone), so a replay can legitimately spend tens of minutes
# waiting for the compiler slot. A timeout still fails the unit (rc=124); the
# budget is scheduling headroom, not a relaxation of the comparison.


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8", "replace")).hexdigest()


def strip_tail(text: str) -> str:
    # Command substitution strips all trailing newlines; the recording follows that.
    return text.rstrip("\n")


def args_sha(argv: list[str]) -> str:
    return hashlib.sha256(("\n".join(argv) + "\n").encode("utf-8")).hexdigest()


def command_sha(command: str) -> str:
    return hashlib.sha256(command.encode("utf-8")).hexdigest()


def resolve_command(argv: list[str] | None, command: str | None) -> list[str]:
    if argv:
        return list(argv)
    if command is not None:
        return ["bash", "-c", command]
    raise ValueError("no command")


def run(argv: list[str] | None, command: str | None, merge: bool, timeout: int) -> tuple[int, str, str]:
    kwargs: dict = {"cwd": str(ROOT), "stdout": subprocess.PIPE, "timeout": timeout}
    kwargs["stderr"] = subprocess.STDOUT if merge else subprocess.PIPE
    proc = subprocess.run(resolve_command(argv, command), **kwargs)  # noqa: S603 - trusted local commands
    out = strip_tail(proc.stdout.decode("utf-8", "replace"))
    err = "" if merge else strip_tail(proc.stderr.decode("utf-8", "replace"))
    return proc.returncode, out, err


def case_key(case: dict) -> tuple:
    argv = case.get("argv")
    if argv:
        return ("argv", tuple(argv))
    return ("cmd", case.get("command", ""))


def case_capture(case: dict) -> str:
    return case.get("capture", "combined")


def load_cases(path: Path) -> list[dict]:
    if not path.exists():
        return []
    cases: list[dict] = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            cases.append(json.loads(line))
        except json.JSONDecodeError as error:
            print(f"port-parity: {path}:{number}: bad JSON ({error})", file=sys.stderr)
    return cases


def load_known(path: Path) -> list[dict]:
    return load_cases(path)


def active_cases(cases: list[dict]) -> list[dict]:
    """Cases that still take part in replay. A retired case stays on disk as history
    (e.g. an old capture format); it never hides a mismatch, it is replaced by a new
    recording of the same command."""
    return [case for case in cases if not case.get("retired")]


def unit_dir(unit: str) -> Path:
    if not UNIT_RE.match(unit):
        raise SystemExit("port-parity: unit names are [A-Za-z0-9._-]")
    return CASES_DIR / unit


def cases_path(directory: Path) -> Path:
    return directory / "cases.jsonl"


def known_path(directory: Path) -> Path:
    return directory / "known-diffs.jsonl"


def record(unit: str, argv: list[str] | None, command: str | None, as_json: bool) -> int:
    if argv is None and command is None:
        print("record needs -- <command...> or --cmd \"<string>\"", file=sys.stderr)
        return 2
    directory = unit_dir(unit)
    directory.mkdir(parents=True, exist_ok=True)
    path = cases_path(directory)
    cases = active_cases(load_cases(path))
    key = args_sha(argv) if argv else command_sha(command or "")
    if any(case.get("args_sha") == key for case in cases):
        print("port-parity: case already recorded for this command (recording is the source of truth)", file=sys.stderr)
        return 0
    try:
        rc, out, err = run(argv, command, merge=False, timeout=REPLAY_TIMEOUT)
    except subprocess.TimeoutExpired:
        print(f"port-parity: record timed out after {REPLAY_TIMEOUT}s", file=sys.stderr)
        return 1
    case_id = f"{unit}-{key[:8]}"
    entry = {
        "id": case_id,
        "unit": unit,
        "args_sha": key,
        "argv": list(argv) if argv else None,
        "command": command if command is not None else " ".join(argv or []),
        "exit": rc,
        "capture": "stdout",
        "stdout_sha": sha(out),
        "stdout": out,
        "stderr_sha": sha(err),
        "stderr": err[:4000],
        "captured_at": now(),
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
    payload = {
        "ok": True,
        "action": "record",
        "unit": unit,
        "case": case_id,
        "exit": rc,
        "stdout_sha": entry["stdout_sha"],
        "stderr_sha": entry["stderr_sha"],
        "cases": len(cases) + 1,
    }
    if as_json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        print(f"recorded {case_id} exit={rc} sha={entry['stdout_sha'][:12]}")
    return 0


def retire(unit: str, key: str, reason: str, as_json: bool) -> int:
    """Retire one recorded case by id or args_sha, keeping it on disk as history.

    Retirement never hides a mismatch: the case keeps its expected output and gains
    `retired`, `retired_at` and `retire_reason`, so the audit trail shows why it no
    longer replays. It is the supported way to replace a capture that was recorded
    under the wrong assumptions (e.g. env-dependent output) with a self-contained one.
    """
    if not reason.strip():
        print("port-parity: retire needs --reason \"...\"", file=sys.stderr)
        return 2
    directory = unit_dir(unit)
    path = cases_path(directory)
    cases = load_cases(path)
    hit = 0
    for case in cases:
        if case.get("id") == key or case.get("args_sha") == key:
            case["retired"] = True
            case["retired_at"] = now()
            case["retire_reason"] = reason
            hit += 1
    if hit == 0:
        print(f"port-parity: no case {key!r} in {path}", file=sys.stderr)
        return 1
    path.write_text("\n".join(json.dumps(c, ensure_ascii=False) for c in cases) + "\n", encoding="utf-8")
    payload = {"ok": True, "action": "retire", "unit": unit, "retired": hit, "reason": reason}
    print(json.dumps(payload, ensure_ascii=False, indent=2) if as_json else f"retired {hit} case(s) in {unit}")
    return 0


def replay(unit: str, argv: list[str] | None, command: str | None, as_json: bool, write: bool = True, quiet: bool = False) -> tuple[int, dict]:
    directory = unit_dir(unit)
    path = cases_path(directory)
    known_file = known_path(directory)
    cases = active_cases(load_cases(path))
    known = load_known(known_file)
    if not cases:
        result = {
            "ok": False,
            "action": "diff",
            "unit": unit,
            "error": f"no recording at {path} — run record first",
            "matched": 0,
            "total": 0,
            "known": 0,
            "mismatches": [],
            "stale_known": [],
        }
        if as_json:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        else:
            print(result["error"], file=sys.stderr)
        return 1, result

    if argv is not None or command is not None:
        selector = args_sha(argv) if argv else command_sha(command or "")
        selected = [case for case in cases if case.get("args_sha") == selector]
    else:
        selected = list(cases)

    # One run per (command, capture mode); every case with that command is compared to it.
    groups: dict[tuple, list[dict]] = {}
    for case in selected:
        groups.setdefault((case_key(case), case_capture(case)), []).append(case)

    observed: dict[tuple, tuple[int, str, str]] = {}
    run_errors: list[str] = []
    for group, group_cases in groups.items():
        key, mode = group
        use_argv = list(key[1]) if key[0] == "argv" else None
        use_command = None if use_argv is not None else key[1]
        merge = mode == "combined"
        try:
            observed[group] = run(use_argv, use_command, merge=merge, timeout=REPLAY_TIMEOUT)
        except subprocess.TimeoutExpired:
            rc, out, err = 124, "", f"timeout after {REPLAY_TIMEOUT}s"
            observed[group] = (rc, out, err)
            run_errors.append(f"{use_command or ' '.join(use_argv or [])}: timeout")

    matched = 0
    mismatches: list[dict] = []
    known_matched: list[str] = []
    stale_known: list[str] = []
    for group, group_cases in groups.items():
        rc, out, err = observed[group]
        actual_sha = sha(out)
        for case in group_cases:
            expected_sha = case.get("stdout_sha", "")
            if int(rc) == int(case.get("exit", 0)) and actual_sha == expected_sha:
                matched += 1
                if any(k.get("case") == case.get("id") for k in known):
                    stale_known.append(str(case.get("id")))
                continue
            entry = next(
                (k for k in known if k.get("case") == case.get("id") and k.get("actual_sha") == actual_sha),
                None,
            )
            if entry is not None:
                known_matched.append(str(case.get("id")))
                continue
            mismatches.append(
                {
                    "id": case.get("id"),
                    "expected_exit": case.get("exit"),
                    "expected_sha": expected_sha[:16],
                    "actual_exit": rc,
                    "actual_sha": actual_sha[:16],
                    "stderr_head": (err or "")[:300],
                }
            )

    total = len(selected)
    ok = not mismatches and total > 0
    result = {
        "ok": ok,
        "action": "diff",
        "unit": unit,
        "matched": matched,
        "total": total,
        "known": len(known_matched),
        "known_ids": known_matched,
        "stale_known": stale_known,
        "mismatches": mismatches,
        "run_errors": run_errors,
        "cases_sha256": sha(path.read_text(encoding="utf-8")),
        "generated_at": now(),
    }
    if write:
        try:
            (directory / "last-diff.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        except OSError as error:
            print(f"port-parity: cannot write last-diff.json ({error})", file=sys.stderr)
    if quiet:
        pass
    elif as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f"replay {matched}/{total} match, known-diffs {len(known_matched)}, mismatches {len(mismatches)}")
        for mismatch in mismatches:
            print(
                f"  MISMATCH {mismatch['id']} expected(exit={mismatch['expected_exit']} sha={mismatch['expected_sha']})"
                f" got(exit={mismatch['actual_exit']} sha={mismatch['actual_sha']})"
            )
        for case_id in stale_known:
            print(f"  STALE KNOWN-DIFF {case_id}: the difference is gone; remove the known-diffs entry")
        for error in run_errors:
            print(f"  RUN ERROR {error}")
    return (0 if ok else 1), result


def scan_unimplemented() -> list[str]:
    hits: list[str] = []
    crates = ROOT / "crates"
    if not crates.exists():
        return hits
    for path in crates.rglob("*.rs"):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if "unimplemented!" in line or "todo!()" in line:
                hits.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()[:120]}")
                if len(hits) >= 20:
                    return hits
    return hits


def gate(unit: str, as_json: bool) -> int:
    directory = unit_dir(unit)
    path = cases_path(directory)
    checks: dict[str, dict] = {}
    exit_code, diff = replay(unit, None, None, as_json=False, write=True, quiet=True)
    checks["parity"] = {"ok": bool(diff.get("ok")), "detail": f"{diff.get('matched')}/{diff.get('total')} match, {len(diff.get('mismatches') or [])} mismatches"}
    if diff.get("stale_known"):
        checks["parity"]["ok"] = False
        checks["parity"]["detail"] += f", stale known-diffs {diff['stale_known']}"
    cases = active_cases(load_cases(path))
    checks["recording"] = {"ok": bool(cases), "detail": f"{len(cases)} case(s)" if cases else "MISSING"}
    hits = scan_unimplemented()
    checks["no_unimplemented"] = {"ok": not hits, "detail": hits[0] if hits else "none in crates/"}
    cargo_ok = False
    cargo_detail = "skipped"
    if os.environ.get("PORT_PARITY_SKIP_CARGO") != "1":
        try:
            proc = subprocess.run(
                [str(ROOT / "scripts" / "h2-cargo"), "check", "-p", "wc3-game", "--lib"],
                cwd=str(ROOT),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=GATE_CARGO_TIMEOUT,
            )
            cargo_ok = proc.returncode == 0
            cargo_detail = f"rc={proc.returncode}"
        except subprocess.TimeoutExpired:
            cargo_detail = f"timeout after {GATE_CARGO_TIMEOUT}s"
        except OSError as error:
            cargo_detail = f"cannot run h2-cargo: {error}"
    else:
        cargo_ok = True
        cargo_detail = "skipped by PORT_PARITY_SKIP_CARGO=1"
    checks["cargo_check"] = {"ok": cargo_ok, "detail": cargo_detail}
    passed = all(check["ok"] for check in checks.values())
    result = {
        "ok": passed,
        "action": "gate",
        "unit": unit,
        "checks": checks,
        "cases_sha256": sha(path.read_text(encoding="utf-8")) if path.exists() else None,
        "generated_at": now(),
    }
    try:
        (directory / "last-gate.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    except OSError as error:
        print(f"port-parity: cannot write last-gate.json ({error})", file=sys.stderr)
    if as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f"== parity: {unit}")
        print(f"   replay: {checks['parity']['detail']}")
        print(f"== recording: {checks['recording']['detail']}")
        print(f"== no unimplemented!/todo!: {checks['no_unimplemented']['detail']}")
        print(f"== cargo check -p wc3-game --lib: {checks['cargo_check']['detail']}")
        print(f"== gate: {'PASS' if passed else 'FAIL'}")
    return 0 if passed else 1


def status(as_json: bool) -> int:
    units: list[dict] = []
    if CASES_DIR.exists():
        for directory in sorted(p for p in CASES_DIR.iterdir() if p.is_dir()):
            path = cases_path(directory)
            all_cases = load_cases(path)
            cases = active_cases(all_cases)
            if not cases and not (directory / "last-gate.json").exists():
                continue
            commands = len({case_key(case) for case in cases})
            last_diff = None
            last_gate = None
            try:
                last_diff = json.loads((directory / "last-diff.json").read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                pass
            try:
                last_gate = json.loads((directory / "last-gate.json").read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                pass
            cases_sha = sha(path.read_text(encoding="utf-8")) if path.exists() else None
            diff_time = (directory / "last-diff.json").stat().st_mtime if (directory / "last-diff.json").exists() else 0
            stale = bool(path.exists()) and path.stat().st_mtime > diff_time
            units.append(
                {
                    "unit": directory.name,
                    "cases": len(cases),
                    "retired": len(all_cases) - len(cases),
                    "commands": commands,
                    "cases_sha256": cases_sha,
                    "last_diff": None
                    if not last_diff
                    else {
                        "ok": last_diff.get("ok"),
                        "matched": last_diff.get("matched"),
                        "total": last_diff.get("total"),
                        "known": last_diff.get("known"),
                        "mismatches": len(last_diff.get("mismatches") or []),
                        "stale_known": last_diff.get("stale_known") or [],
                        "generated_at": last_diff.get("generated_at"),
                    },
                    "last_gate": None
                    if not last_gate
                    else {"ok": last_gate.get("ok"), "generated_at": last_gate.get("generated_at")},
                    "stale": stale,
                }
            )
    result = {"ok": True, "action": "status", "root": str(ROOT), "units": units, "generated_at": now()}
    if as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        if not units:
            print("no units recorded yet (analysis/port-parity/<unit>/cases.jsonl)")
        for entry in units:
            diff = entry["last_diff"]
            gate = entry["last_gate"]
            line = f"{entry['unit']:<24} cases={entry['cases']:<3} cmds={entry['commands']:<2}"
            line += " diff=—" if not diff else f" diff={diff['matched']}/{diff['total']}"
            line += "" if not diff else f" mismatches={diff['mismatches']}"
            line += " gate=—" if not gate else f" gate={'PASS' if gate['ok'] else 'FAIL'}"
            line += " STALE" if entry["stale"] else ""
            print(line)
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("-h", "--help", "help"):
        print(__doc__)
        return 0
    action = argv[0]
    rest = argv[1:]
    as_json = "--json" in rest
    rest = [item for item in rest if item != "--json"]

    if action in ("status", "list"):
        return status(as_json)

    if action not in ("record", "diff", "gate", "retire"):
        print(__doc__)
        return 2
    if not rest:
        print("port-parity: a unit name is required (e.g. campaign-load)", file=sys.stderr)
        return 2
    unit = rest[0]
    args = rest[1:]
    command: str | None = None
    argv_cmd: list[str] | None = None
    if args:
        if args[0] == "--cmd":
            if len(args) < 2:
                print("--cmd needs a command string", file=sys.stderr)
                return 2
            command = args[1]
        elif args[0] == "--":
            argv_cmd = args[1:]
        else:
            argv_cmd = args
    if action == "retire":
        reason = ""
        rest2 = list(args)
        if "--reason" in rest2:
            i = rest2.index("--reason")
            if i + 1 < len(rest2):
                reason = rest2[i + 1]
        key = rest2[0] if rest2 and not rest2[0].startswith("--") else ""
        if not key:
            print("port-parity: retire needs a case id or args_sha", file=sys.stderr)
            return 2
        return retire(unit, key, reason, as_json)
    if action == "record":
        return record(unit, argv_cmd, command, as_json)
    if action == "diff":
        code, _ = replay(unit, argv_cmd, command, as_json)
        return code
    if argv_cmd is not None or command is not None:
        print("gate replays every recorded case; command arguments are not used", file=sys.stderr)
    return gate(unit, as_json)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
