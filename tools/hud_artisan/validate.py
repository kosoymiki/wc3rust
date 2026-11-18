#!/usr/bin/env python3
"""Compatibility entry point: run the canonical HUD Artisan validate subcommand."""
from __future__ import annotations
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate import main
if __name__ == "__main__":
    raise SystemExit(main(["validate", *sys.argv[1:]]))
