#!/usr/bin/env python3
"""MCP stdio entry point: `python3 .../peekaboo-android/server.py`."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from peekaboo_android.mcpserver import main  # noqa: E402

if __name__ == "__main__":
    raise SystemExit(main())
