#!/usr/bin/env python3
"""Smoke client: send one tools/call through the real MCP stdio server.

Usage:
    python3 mcp_call.py tools/list
    python3 mcp_call.py window '{"max_windows": 10}'
    python3 mcp_call.py screenshot --raw
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def send(proc, payload: dict) -> None:
    proc.stdin.write(json.dumps(payload, ensure_ascii=False) + "\n")
    proc.stdin.flush()


def read(proc) -> dict:
    line = proc.stdout.readline()
    if not line:
        stderr = proc.stderr.read() if proc.stderr else ""
        raise RuntimeError(f"server closed stdout before responding; stderr={stderr!r}")
    return json.loads(line)


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    raw = "--raw" in argv
    argv = [item for item in argv if item != "--raw"]
    method = "tools/list" if argv[0] in ("tools/list", "list") else "tools/call"
    params: dict = {}
    if method == "tools/call":
        params = {"name": argv[0], "arguments": json.loads(argv[1]) if len(argv) > 1 else {}}

    env = dict(os.environ)
    proc = subprocess.Popen(
        [sys.executable, str(HERE / "server.py")],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        bufsize=1,
    )
    try:
        send(proc, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}})
        read(proc)
        send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized"})
        send(proc, {"jsonrpc": "2.0", "id": 2, "method": method, "params": params})
        response = read(proc)
    finally:
        try:
            proc.stdin.close()
        except Exception:
            pass
        proc.wait(timeout=10)

    if raw:
        print(json.dumps(response, ensure_ascii=False))
        return 0
    result = response.get("result", {})
    if method == "tools/list":
        print(json.dumps({"tools": [tool["name"] for tool in result["tools"]]}, ensure_ascii=False))
        return 0
    content = result.get("content", [])
    for block in content:
        if block.get("type") == "text":
            print(block["text"])
        elif block.get("type") == "image":
            print(json.dumps({"type": "image", "mimeType": block.get("mimeType"),
                              "base64_bytes": len(block.get("data", ""))}, ensure_ascii=False))
    print(json.dumps({"isError": result.get("isError", False)}, ensure_ascii=False))
    if proc.stderr:
        for line in proc.stderr.read().splitlines()[:3]:
            if line.strip():
                print(f"stderr: {line}")
    return 0 if not result.get("isError") else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
