#!/data/data/com.termux/files/usr/bin/bash
# port-parity-all — gate EVERY port-parity unit (the rust-anything loop) and fail
# if any unit is not N/N + gate PASS. One unit is never "done" alone: the whole
# layer matrix must stay green.
#
# Usage: scripts/port-parity-all.sh [--json] [unit...]
#   With no units, every directory under analysis/port-parity is gated.
# Env:   PORT_PARITY_ALLOW_EMPTY=1  exit 0 when there are no units (default: fail)
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

JSON=0
UNITS=()
for arg in "$@"; do
  case "$arg" in
    --json) JSON=1 ;;
    -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) UNITS+=("$arg") ;;
  esac
done

if [[ ${#UNITS[@]} -eq 0 ]]; then
  while IFS= read -r d; do
    [[ -d "$d" ]] || continue
    UNITS+=("$(basename "$d")")
  done < <(ls -d analysis/port-parity/*/ 2>/dev/null)
fi

if [[ ${#UNITS[@]} -eq 0 ]]; then
  if [[ "${PORT_PARITY_ALLOW_EMPTY:-0}" == "1" ]]; then
    echo "PORT_PARITY_EMPTY units=0 (allowed)"
    exit 0
  fi
  echo "PORT_PARITY_FAIL units=0 (no analysis/port-parity units; the layer matrix is not covered)" >&2
  exit 1
fi

PASS=0
FAIL=0
FAILED=()
for unit in "${UNITS[@]}"; do
  log="/data/data/com.termux/files/usr/tmp/port-parity-all-${unit}.json"
  if bash scripts/port-parity.sh gate "$unit" --json > "$log" 2>/dev/null; then
    PASS=$((PASS + 1))
    if [[ $JSON -eq 0 ]]; then
      python3 - "$log" "$unit" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    print(f"PORT_PARITY_PASS unit={sys.argv[2]}"); raise SystemExit
p = d.get("checks", {}).get("parity", {})
print(f"PORT_PARITY_PASS unit={sys.argv[2]} cases={p.get('detail','?')}")
PY
    fi
  else
    FAIL=$((FAIL + 1))
    FAILED+=("$unit")
    if [[ $JSON -eq 0 ]]; then
      echo "PORT_PARITY_FAIL unit=$unit (log: ${log})" >&2
    fi
  fi
done

if [[ $JSON -eq 1 ]]; then
  printf '{"ok":%s,"units":%d,"passed":%d,"failed":%d,"failed_units":["%s"]}\n' \
    "$([[ $FAIL -eq 0 ]] && echo true || echo false)" \
    "${#UNITS[@]}" "$PASS" "$FAIL" "$(IFS='","'; echo "${FAILED[*]:-}")"
fi

if [[ $FAIL -ne 0 ]]; then
  exit 1
fi
echo "PORT_PARITY_ALL_PASS units=${#UNITS[@]}"
