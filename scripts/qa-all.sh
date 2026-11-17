#!/usr/bin/env bash
# Deterministic QA gate for wc3rust (Termux/Android, root).
#
#   scripts/qa-all.sh                 # build + install + launch + visual + gameplay
#   scripts/qa-all.sh --fast          # visual only (no gameplay)
#   scripts/qa-all.sh --no-build      # test the installed APK
#   scripts/qa-all.sh --gallery       # also render/grade 2048 MDX, 691 icons, 111 tiles
#
# Exit 0 only when every enabled hard gate passes. Evidence:
#   analysis/agent-qa/latest/summary.json
#   analysis/agent-qa/latest/visual.json + screen-start.annotated.png
#   analysis/agent-qa/latest/gameplay.json + gameplay/*.png
#   analysis/agent-qa/gallery/gates.json + sheets/
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! su -c 'id' 2>/dev/null | grep -q 'uid=0(root)'; then
  echo "QA_ERROR: root (su) is required for device automation" >&2
  exit 2
fi
if ! python3 -c 'import PIL, numpy' 2>/dev/null; then
  echo "QA_ERROR: python3 needs Pillow and numpy (pkg install python-pillow python-numpy)" >&2
  exit 2
fi
if ! command -v cargo >/dev/null; then
  echo "QA_ERROR: cargo missing" >&2
  exit 2
fi

exec python3 -m scripts.qa.suite "$@"
