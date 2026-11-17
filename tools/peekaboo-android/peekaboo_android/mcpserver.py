"""MCP stdio server for peekaboo-android (newline-delimited JSON-RPC 2.0)."""

from __future__ import annotations

import base64
import json
import os
import sys
import time

from .backend import AndroidBackend, DeviceError
from .guard import ACTION_TOOLS, DeviceSlotGuard

SERVER_NAME = "peekaboo-android"
SERVER_VERSION = "1.0.0"
SUPPORTED_PROTOCOLS = ("2025-06-18", "2025-03-26", "2024-11-05")
DEFAULT_PROTOCOL = "2024-11-05"
MAX_SLEEP_MS = 60_000


def _schema(properties: dict, required: list[str] | None = None) -> dict:
    return {
        "type": "object",
        "properties": properties,
        "required": required or [],
        "additionalProperties": False,
    }


TOOL_DEFINITIONS: list[dict] = [
    {
        "name": "screenshot",
        "description": "Capture the device screen as PNG (su screencap). Returns dimensions, byte size, sha256, saved path and inline base64 image. Always allowed; no device slot needed.",
        "inputSchema": _schema({"path": {"type": "string", "description": "Optional output path (default: ~/.local/state/peekaboo-android/shots/)."}}),
    },
    {
        "name": "ui_tree",
        "description": "Dump the accessibility tree (su uiautomator dump) as structured JSON: node tree with bounds/text/resource-id/clickable plus a flat interactive list with tap centers. Always allowed.",
        "inputSchema": _schema(
            {
                "max_nodes": {"type": "integer", "minimum": 1, "maximum": 5000, "default": 400},
                "max_interactive": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 120},
                "package": {"type": "string", "description": "Keep only nodes of this package (ancestors retained)."},
            }
        ),
    },
    {
        "name": "tap",
        "description": "Tap screen coordinates (su input tap). ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema(
            {
                "x": {"type": "integer", "minimum": 0, "maximum": 100000},
                "y": {"type": "integer", "minimum": 0, "maximum": 100000},
            },
            ["x", "y"],
        ),
    },
    {
        "name": "swipe",
        "description": "Swipe/drag between two points (su input swipe). ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema(
            {
                "x1": {"type": "integer", "minimum": 0, "maximum": 100000},
                "y1": {"type": "integer", "minimum": 0, "maximum": 100000},
                "x2": {"type": "integer", "minimum": 0, "maximum": 100000},
                "y2": {"type": "integer", "minimum": 0, "maximum": 100000},
                "duration_ms": {"type": "integer", "minimum": 1, "maximum": 60000, "default": 300},
            },
            ["x1", "y1", "x2", "y2"],
        ),
    },
    {
        "name": "long_press",
        "description": "Long-press coordinates (su input swipe x y x y ms). ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema(
            {
                "x": {"type": "integer", "minimum": 0, "maximum": 100000},
                "y": {"type": "integer", "minimum": 0, "maximum": 100000},
                "duration_ms": {"type": "integer", "minimum": 1, "maximum": 60000, "default": 800},
            },
            ["x", "y"],
        ),
    },
    {
        "name": "type",
        "description": "Type text into the focused field. ASCII via su input text (spaces as %s, literal %s split into two calls, newlines as KEYCODE_ENTER); non-ASCII via clipboard paste when `cmd clipboard` exists, else explicit unsupported_text_encoding error. ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema({"text": {"type": "string", "minLength": 1}}, ["text"]),
    },
    {
        "name": "key",
        "description": "Send one key event (su input keyevent <code|KEYCODE_NAME>). ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema({"code": {"type": "string", "pattern": "^(?:[0-9]{1,4}|KEYCODE_[A-Z0-9_]+)$"}}, ["code"]),
    },
    {
        "name": "window",
        "description": "Parse dumpsys window: current focus, focused app, visible windows with bounds/type. Always allowed.",
        "inputSchema": _schema({"max_windows": {"type": "integer", "minimum": 1, "maximum": 200, "default": 30}}),
    },
    {
        "name": "apps",
        "description": "List installed packages (su pm list packages), optional substring filter and third-party-only. Always allowed.",
        "inputSchema": _schema(
            {
                "filter": {"type": "string"},
                "third_party": {"type": "boolean", "default": False},
                "max_packages": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 200},
            }
        ),
    },
    {
        "name": "start",
        "description": "Launch an app (su am start -W). target = package or package/activity; resolves the launcher activity when only a package is given. ACTION: requires H2_DEVICE_SLOT=1.",
        "inputSchema": _schema(
            {
                "target": {"type": "string", "minLength": 1, "description": "Package name or package/activity component."},
                "activity": {"type": "string", "description": "Explicit activity to combine with a package target."},
                "wait": {"type": "boolean", "default": True},
            },
            ["target"],
        ),
    },
    {
        "name": "analyze",
        "description": "Image metadata (format/dimensions/bytes/sha256) + real local OCR via tesseract (if installed) + optional external hook (PEEKABOO_ANDROID_ANALYZE_CMD). No LLM: answer stays null with an explicit note. Always allowed.",
        "inputSchema": _schema(
            {
                "path": {"type": "string", "minLength": 1},
                "question": {"type": "string"},
                "ocr": {"type": "boolean", "default": True},
                "lang": {"type": "string", "default": "eng"},
            },
            ["path"],
        ),
    },
    {
        "name": "sleep",
        "description": "Sleep for N milliseconds (capped at 60000). Always allowed.",
        "inputSchema": _schema({"ms": {"type": "integer", "minimum": 0, "maximum": MAX_SLEEP_MS}}, ["ms"]),
    },
]

TOOL_INDEX = {tool["name"]: tool for tool in TOOL_DEFINITIONS}


# --------------------------------------------------------------- validation
def _require(args: dict, name: str) -> object:
    if name not in args:
        raise DeviceError("invalid_argument", f"missing required argument: {name}")
    return args[name]


def _int_arg(args: dict, name: str, *, required: bool = False, default: int | None = None,
             minimum: int | None = None, maximum: int | None = None) -> int | None:
    if name not in args or args[name] is None:
        if required:
            raise DeviceError("invalid_argument", f"missing required argument: {name}")
        return default
    value = args[name]
    if isinstance(value, bool) or not isinstance(value, int):
        raise DeviceError("invalid_argument", f"{name} must be an integer")
    if minimum is not None and value < minimum:
        raise DeviceError("invalid_argument", f"{name} must be >= {minimum}")
    if maximum is not None and value > maximum:
        raise DeviceError("invalid_argument", f"{name} must be <= {maximum}")
    return value


def _str_arg(args: dict, name: str, *, required: bool = False, default: str | None = None) -> str | None:
    if name not in args or args[name] is None:
        if required:
            raise DeviceError("invalid_argument", f"missing required argument: {name}")
        return default
    value = args[name]
    if not isinstance(value, str):
        raise DeviceError("invalid_argument", f"{name} must be a string")
    return value


def _bool_arg(args: dict, name: str, default: bool) -> bool:
    value = args.get(name)
    if value is None:
        return default
    if not isinstance(value, bool):
        raise DeviceError("invalid_argument", f"{name} must be a boolean")
    return value


def _check_text(text: str) -> str:
    if not text:
        raise DeviceError("invalid_argument", "text must not be empty")
    if len(text) > 20_000:
        raise DeviceError("invalid_argument", "text exceeds 20000 characters")
    return text


# ----------------------------------------------------------------- handlers
def _h_screenshot(args: dict, backend: AndroidBackend) -> dict:
    return backend.screenshot(_str_arg(args, "path"))


def _h_ui_tree(args: dict, backend: AndroidBackend) -> dict:
    return backend.ui_tree(
        max_nodes=_int_arg(args, "max_nodes", default=400, minimum=1, maximum=5000),
        max_interactive=_int_arg(args, "max_interactive", default=120, minimum=1, maximum=1000),
        package=_str_arg(args, "package"),
    )


def _h_tap(args: dict, backend: AndroidBackend) -> dict:
    return backend.tap(_int_arg(args, "x", required=True, minimum=0, maximum=100000),
                       _int_arg(args, "y", required=True, minimum=0, maximum=100000))


def _h_swipe(args: dict, backend: AndroidBackend) -> dict:
    return backend.swipe(
        _int_arg(args, "x1", required=True, minimum=0, maximum=100000),
        _int_arg(args, "y1", required=True, minimum=0, maximum=100000),
        _int_arg(args, "x2", required=True, minimum=0, maximum=100000),
        _int_arg(args, "y2", required=True, minimum=0, maximum=100000),
        _int_arg(args, "duration_ms", default=300, minimum=1, maximum=60000),
    )


def _h_long_press(args: dict, backend: AndroidBackend) -> dict:
    return backend.long_press(
        _int_arg(args, "x", required=True, minimum=0, maximum=100000),
        _int_arg(args, "y", required=True, minimum=0, maximum=100000),
        _int_arg(args, "duration_ms", default=800, minimum=1, maximum=60000),
    )


def _h_type(args: dict, backend: AndroidBackend) -> dict:
    return backend.type_text(_check_text(_str_arg(args, "text", required=True)))


def _h_key(args: dict, backend: AndroidBackend) -> dict:
    code = _str_arg(args, "code", required=True)
    assert code is not None
    if not (code.isdigit() and 1 <= len(code) <= 4) and not (code.startswith("KEYCODE_") and code[8:].isalnum()):
        raise DeviceError("invalid_argument", "code must be an integer 0-9999 or KEYCODE_*")
    return backend.key(code)


def _h_window(args: dict, backend: AndroidBackend) -> dict:
    return backend.window(max_windows=_int_arg(args, "max_windows", default=30, minimum=1, maximum=200))


def _h_apps(args: dict, backend: AndroidBackend) -> dict:
    return backend.apps(
        filter_text=_str_arg(args, "filter"),
        third_party=_bool_arg(args, "third_party", False),
        max_packages=_int_arg(args, "max_packages", default=200, minimum=1, maximum=1000),
    )


def _h_start(args: dict, backend: AndroidBackend) -> dict:
    target = _str_arg(args, "target", required=True)
    assert target is not None
    return backend.start(target, activity=_str_arg(args, "activity"), wait=_bool_arg(args, "wait", True))


def _h_analyze(args: dict, backend: AndroidBackend) -> dict:
    return backend.analyze(
        _str_arg(args, "path", required=True),
        question=_str_arg(args, "question"),
        ocr=_bool_arg(args, "ocr", True),
        lang=_str_arg(args, "lang", default="eng") or "eng",
    )


def _h_sleep(args: dict, backend: AndroidBackend) -> dict:
    ms = _int_arg(args, "ms", required=True, minimum=0, maximum=MAX_SLEEP_MS)
    time.sleep((ms or 0) / 1000)
    return {"ok": True, "slept_ms": ms}


HANDLERS = {
    "screenshot": _h_screenshot,
    "ui_tree": _h_ui_tree,
    "tap": _h_tap,
    "swipe": _h_swipe,
    "long_press": _h_long_press,
    "type": _h_type,
    "key": _h_key,
    "window": _h_window,
    "apps": _h_apps,
    "start": _h_start,
    "analyze": _h_analyze,
    "sleep": _h_sleep,
}

assert set(HANDLERS) == set(TOOL_INDEX), "tool definitions and handlers drifted"


class McpServer:
    def __init__(self, backend: AndroidBackend | None = None, guard: DeviceSlotGuard | None = None) -> None:
        self.backend = backend or AndroidBackend()
        self.guard = guard or DeviceSlotGuard(self.backend.state_dir)

    # ------------------------------------------------------------- dispatch
    def dispatch(self, name: str, arguments: dict | None) -> dict:
        start = time.monotonic()
        args = arguments or {}
        result: dict | None = None
        try:
            if name not in HANDLERS:
                raise DeviceError("unknown_tool", f"unknown tool: {name}")
            self.guard.require_slot(name)
            result = HANDLERS[name](args, self.backend)
        except DeviceError as exc:
            result = exc.as_result()
        except Exception as exc:  # noqa: BLE001 - surfaced as a tool error, never a crash
            result = DeviceError(
                "internal",
                f"unexpected error in {name}: {exc}",
                detail={"type": type(exc).__name__},
                retry_safe=False,
            ).as_result()
        finally:
            self.guard.log(name, args, result or {"ok": False}, (time.monotonic() - start) * 1000)
        return result or {"ok": False, "error": {"code": "internal", "message": "no result"}}

    # ---------------------------------------------------------------- shape
    @staticmethod
    def tool_result(result: dict) -> dict:
        payload = dict(result)
        png = payload.pop("_png", None)
        content: list[dict] = [{"type": "text", "text": json.dumps(payload, ensure_ascii=False, separators=(",", ":"))}]
        if png:
            content.append(
                {
                    "type": "image",
                    "data": base64.b64encode(png).decode("ascii"),
                    "mimeType": "image/png",
                }
            )
        return {"content": content, "isError": not payload.get("ok", False)}

    # --------------------------------------------------------------- rpc
    def handle(self, message: object) -> dict | list[dict] | None:
        if isinstance(message, list):
            responses = [handled for item in message if (handled := self._handle_one(item)) is not None]
            return responses or None
        return self._handle_one(message)

    def _handle_one(self, message: object) -> dict | None:
        if not isinstance(message, dict):
            return self._error(None, -32600, "invalid request")
        has_id = "id" in message
        request_id = message.get("id")
        method = message.get("method")
        if not isinstance(method, str):
            return self._error(request_id, -32600, "missing method") if has_id else None
        params = message.get("params")
        params = params if isinstance(params, dict) else {}

        try:
            if method == "initialize":
                requested = params.get("protocolVersion")
                protocol = requested if requested in SUPPORTED_PROTOCOLS else DEFAULT_PROTOCOL
                result = {
                    "protocolVersion": protocol,
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                    "instructions": (
                        "Android device control (screenshot, ui_tree, tap, swipe, long_press, type, key, "
                        "window, apps, start, analyze). Input tools require H2_DEVICE_SLOT=1; read tools are "
                        "always allowed. Every call is audited in ~/.local/state/peekaboo-android/actions.log."
                    ),
                }
            elif method.startswith("notifications/"):
                return None
            elif method == "ping":
                result = {}
            elif method == "tools/list":
                result = {"tools": TOOL_DEFINITIONS}
            elif method == "tools/call":
                name = params.get("name")
                if not isinstance(name, str):
                    return self._error(request_id, -32602, "tools/call requires a string name")
                arguments = params.get("arguments")
                if arguments is not None and not isinstance(arguments, dict):
                    return self._error(request_id, -32602, "arguments must be an object")
                return {"jsonrpc": "2.0", "id": request_id, "result": self.tool_result(self.dispatch(name, arguments))}
            elif method == "resources/list":
                result = {"resources": []}
            elif method == "prompts/list":
                result = {"prompts": []}
            elif method == "logging/setLevel":
                result = {}
            else:
                return self._error(request_id, -32601, f"method not found: {method}") if has_id else None
        except Exception as exc:  # noqa: BLE001
            return self._error(request_id, -32603, f"internal error: {exc}") if has_id else None

        if not has_id:
            return None
        return {"jsonrpc": "2.0", "id": request_id, "result": result}

    @staticmethod
    def _error(request_id: object, code: int, message: str) -> dict:
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}

    # ---------------------------------------------------------------- serve
    def serve(self, stdin, stdout) -> None:
        for raw in stdin:
            line = raw.strip()
            if not line:
                continue
            try:
                message = json.loads(line)
            except json.JSONDecodeError as exc:
                response: object = self._error(None, -32700, f"parse error: {exc.msg}")
            else:
                response = self.handle(message)
            if response is None:
                continue
            for item in response if isinstance(response, list) else [response]:
                stdout.write(json.dumps(item, ensure_ascii=False, separators=(",", ":")) + "\n")
            stdout.flush()


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if "--version" in argv:
        print(f"{SERVER_NAME} {SERVER_VERSION}")
        return 0
    if "--list-tools" in argv:
        print(json.dumps({"tools": TOOL_DEFINITIONS}, ensure_ascii=False, indent=2))
        return 0
    sys.stdin.reconfigure(encoding="utf-8", errors="replace")
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    server = McpServer()
    print(
        f"{SERVER_NAME} {SERVER_VERSION}: ready (slot={'granted' if server.guard.slot_granted() else 'absent'}, "
        f"state={server.backend.state_dir})",
        file=sys.stderr,
        flush=True,
    )
    try:
        server.serve(sys.stdin, sys.stdout)
    except KeyboardInterrupt:
        return 0
    return 0
