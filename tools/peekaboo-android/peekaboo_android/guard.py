"""Device-slot guard and JSONL audit log for peekaboo-android."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

from .backend import DeviceError

ACTION_TOOLS = frozenset({"tap", "swipe", "long_press", "type", "key", "start"})
READ_TOOLS = frozenset({"screenshot", "ui_tree", "window", "apps", "analyze", "sleep"})
TEXT_FIELDS = ("text", "question")
TEXT_LIMIT = 200


def _truncate_args(args: dict) -> dict:
    clean: dict = {}
    for key, value in args.items():
        if isinstance(value, str) and (key in TEXT_FIELDS or len(value) > TEXT_LIMIT):
            clean[key] = value[:TEXT_LIMIT] + ("…" if len(value) > TEXT_LIMIT else "")
        else:
            clean[key] = value
    return clean


class DeviceSlotGuard:
    """Refuses input-injection tools unless the tech-lead granted H2_DEVICE_SLOT=1."""

    def __init__(self, state_dir: Path) -> None:
        self.state_dir = Path(state_dir)
        self.state_dir.mkdir(parents=True, exist_ok=True)
        try:
            self.state_dir.chmod(0o700)
        except OSError:
            pass
        self.log_path = self.state_dir / "actions.log"

    @staticmethod
    def slot_granted() -> bool:
        return os.environ.get("H2_DEVICE_SLOT") == "1"

    def require_slot(self, tool: str) -> None:
        if tool in ACTION_TOOLS and not self.slot_granted():
            raise DeviceError(
                "permission",
                f"{tool} refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)",
                detail={
                    "tool": tool,
                    "h2_device_slot": os.environ.get("H2_DEVICE_SLOT"),
                    "grant": "start the server (or the calling shell) with H2_DEVICE_SLOT=1",
                },
                dispatched=False,
            )

    def log(self, tool: str, args: dict | None, result: dict, duration_ms: float) -> None:
        entry = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "tool": tool,
            "args": _truncate_args(args or {}),
            "result": _summarize(result),
            "duration_ms": round(duration_ms, 1),
            "slot": self.slot_granted(),
            "pid": os.getpid(),
        }
        line = json.dumps(entry, ensure_ascii=False) + "\n"
        fd = os.open(self.log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        try:
            os.write(fd, line.encode("utf-8"))
        finally:
            os.close(fd)


def _summarize(result: dict) -> dict:
    summary: dict = {"ok": bool(result.get("ok"))}
    error = result.get("error")
    if error:
        summary["code"] = error.get("code")
        summary["message"] = error.get("message", "")[:300]
        return summary
    for key in ("width", "height", "node_count", "interactive_count", "window_count", "matched", "component", "chars"):
        if key in result:
            summary[key] = result[key]
    if "focus" in result and isinstance(result["focus"], dict):
        summary["focus"] = result["focus"].get("component")
    return summary
