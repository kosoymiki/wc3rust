"""Android device backend for peekaboo-android.

Every privileged interaction goes through `su -c '<argv...>'`. Commands are
built as argv lists and shell-quoted token by token, so caller-provided text
can never break out of the shell command.
"""

from __future__ import annotations

import hashlib
import os
import re
import shlex
import shutil
import struct
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from xml.etree import ElementTree

DEFAULT_TIMEOUT = 30.0
UI_DUMP_PATH = "/data/local/tmp/peekaboo-android-ui.xml"
SCREENSHOT_LIMIT = 25 * 1024 * 1024
TEXT_LIMIT = 64 * 1024
PERMISSION_HINTS = (
    "permission denied",
    "not permitted",
    "operation not permitted",
    "not allowed",
    "su: ",
    "cannot open",
)
IMAGE_MAGIC = (
    (b"\x89PNG\r\n\x1a\n", "png"),
    (b"\xff\xd8\xff", "jpeg"),
    (b"GIF87a", "gif"),
    (b"GIF89a", "gif"),
    (b"RIFF", "webp"),
)


class DeviceError(Exception):
    """Structured tool failure. `code` is stable and machine-readable."""

    def __init__(
        self,
        code: str,
        message: str,
        *,
        detail: dict | None = None,
        dispatched: bool = False,
        retry_safe: bool = True,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.detail = detail or {}
        self.dispatched = dispatched
        self.retry_safe = retry_safe

    def as_result(self) -> dict:
        return {
            "ok": False,
            "error": {"code": self.code, "message": self.message, "detail": self.detail},
            "dispatched": self.dispatched,
            "retry_safe": self.retry_safe,
        }


@dataclass
class RawRun:
    rc: int
    stdout: bytes
    stderr: str

    @property
    def out(self) -> str:
        return self.stdout.decode("utf-8", "replace")


def su_command(argv: list[str]) -> list[str]:
    """Wrap argv in `su -c` with per-token shell quoting."""
    script = " ".join(shlex.quote(str(token)) for token in argv)
    return ["su", "-c", script]


def decode_component(raw: str) -> dict:
    """Parse `pkg/activity` (activity may be relative, e.g. `.app.Foo`)."""
    raw = raw.strip()
    if "/" not in raw:
        return {"component": raw, "package": raw, "activity": None}
    package, activity = raw.split("/", 1)
    if activity.startswith("."):
        activity = package + activity
    return {"component": f"{package}/{activity}", "package": package, "activity": activity}


def parse_bounds(text: str) -> list[int] | None:
    match = re.fullmatch(r"\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]", text.strip())
    if not match:
        return None
    return [int(value) for value in match.groups()]


def png_size(data: bytes) -> tuple[int, int] | None:
    if len(data) < 24 or not data.startswith(b"\x89PNG\r\n\x1a\n") or data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", data[16:24])
    return int(width), int(height)


def _jpeg_size(data: bytes) -> tuple[int, int] | None:
    index = 2
    length = len(data)
    while index + 9 < length:
        if data[index] != 0xFF:
            index += 1
            continue
        marker = data[index + 1]
        if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
            index += 2
            continue
        if marker == 0xD9 or marker == 0xDA:
            return None
        segment_length = int.from_bytes(data[index + 2 : index + 4], "big")
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            height = int.from_bytes(data[index + 5 : index + 7], "big")
            width = int.from_bytes(data[index + 7 : index + 9], "big")
            return width, height
        index += 2 + segment_length
    return None


def _gif_size(data: bytes) -> tuple[int, int] | None:
    if len(data) < 10:
        return None
    width, height = struct.unpack("<HH", data[6:10])
    return int(width), int(height)


def _webp_size(data: bytes) -> tuple[int, int] | None:
    if len(data) < 30 or data[12:16] != b"VP8X":
        return None
    width = int.from_bytes(data[24:27], "little") + 1
    height = int.from_bytes(data[27:30], "little") + 1
    return width, height


def image_size(data: bytes) -> tuple[int, int] | None:
    detected = detect_format(data)
    if detected == "png":
        return png_size(data)
    if detected == "jpeg":
        return _jpeg_size(data)
    if detected == "gif":
        return _gif_size(data)
    if detected == "webp":
        return _webp_size(data)
    return None


def detect_format(data: bytes) -> str | None:
    for magic, name in IMAGE_MAGIC:
        if data.startswith(magic):
            if name == "webp" and len(data) >= 12 and data[8:12] != b"WEBP":
                continue
            return name
    return None


class AndroidBackend:
    """su-backed Android operations. `runner` is injectable for tests."""

    def __init__(self, runner=None, state_dir: Path | None = None) -> None:
        self._runner = runner or self._default_runner
        self.state_dir = Path(
            os.environ.get(
                "PEEKABOO_ANDROID_STATE_DIR",
                str(Path.home() / ".local" / "state" / "peekaboo-android"),
            )
        )
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.shots_dir = self.state_dir / "shots"
        self.shots_dir.mkdir(parents=True, exist_ok=True)
        self._clipboard_ok: bool | None = None

    # ------------------------------------------------------------------ core
    @staticmethod
    def _default_runner(argv: list[str], timeout: float) -> RawRun:
        try:
            proc = subprocess.run(
                su_command(argv),
                capture_output=True,
                timeout=timeout,
                check=False,
            )
        except FileNotFoundError as exc:
            raise DeviceError(
                "unavailable",
                "su binary not found; root is required for device access",
                detail={"reason": str(exc)},
            ) from exc
        except subprocess.TimeoutExpired as exc:
            raise DeviceError(
                "timeout",
                f"command timed out after {timeout:.0f}s",
                detail={"argv": argv},
                retry_safe=True,
            ) from exc
        return RawRun(proc.returncode, proc.stdout, proc.stderr.decode("utf-8", "replace"))

    def run(self, argv: list[str], timeout: float = DEFAULT_TIMEOUT) -> RawRun:
        return self._runner(argv, timeout)

    def run_ok(self, argv: list[str], timeout: float = DEFAULT_TIMEOUT) -> RawRun:
        result = self.run(argv, timeout)
        if result.rc != 0:
            raise self._failure(argv, result)
        return result

    def run_text(self, argv: list[str], timeout: float = DEFAULT_TIMEOUT) -> RawRun:
        return self.run_ok(argv, timeout)

    @staticmethod
    def _failure(argv: list[str], result: RawRun) -> DeviceError:
        stderr = result.stderr.strip()
        lowered = stderr.lower()
        permission = any(hint in lowered for hint in PERMISSION_HINTS)
        code = "permission" if permission else "command_failed"
        message = "su/command permission failure" if permission else "device command failed"
        return DeviceError(
            code,
            f"{message}: {' '.join(argv[:3])}",
            detail={
                "argv": argv,
                "rc": result.rc,
                "stderr": stderr[-2000:],
                "stdout": result.out.strip()[-2000:],
            },
            dispatched=False,
            retry_safe=True,
        )

    # ------------------------------------------------------------- screenshot
    def screenshot(self, path: str | None = None) -> dict:
        stamp = time.strftime("%Y%m%d-%H%M%S")
        target = Path(path) if path else self.shots_dir / f"shot-{stamp}-{os.getpid()}.png"
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
        except OSError as exc:
            raise DeviceError(
                "invalid_path",
                f"cannot create screenshot directory: {target.parent}",
                detail={"reason": str(exc)},
            ) from exc
        self.run_ok(["screencap", "-p", str(target)], timeout=20.0)
        # screencap runs as root; make the file readable by the Termux user.
        self.run(["chmod", "0644", str(target)], timeout=10.0)
        if not target.exists():
            raise DeviceError(
                "screenshot_failed",
                "screencap reported success but produced no file",
                detail={"path": str(target)},
            )
        data = target.read_bytes()
        if not data:
            raise DeviceError("screenshot_failed", "screencap produced an empty file", detail={"path": str(target)})
        if len(data) > SCREENSHOT_LIMIT:
            raise DeviceError(
                "screenshot_too_large",
                f"screenshot exceeds {SCREENSHOT_LIMIT} bytes",
                detail={"path": str(target), "bytes": len(data)},
            )
        size = png_size(data)
        if size is None:
            raise DeviceError(
                "screenshot_invalid",
                "captured file is not a valid PNG",
                detail={"path": str(target), "head": data[:16].hex()},
            )
        return {
            "ok": True,
            "path": str(target),
            "format": "png",
            "width": size[0],
            "height": size[1],
            "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
            "_png": data,
        }

    # ---------------------------------------------------------------- ui tree
    def ui_tree(self, max_nodes: int = 400, max_interactive: int = 120, package: str | None = None) -> dict:
        self.run(["rm", "-f", UI_DUMP_PATH], timeout=10.0)
        dump = None
        for attempt in range(2):
            candidate = self.run(["uiautomator", "dump", UI_DUMP_PATH], timeout=60.0)
            banner = "dumped to" in (candidate.out + candidate.stderr).lower()
            if candidate.rc == 0 and banner:
                dump = candidate
                break
            if candidate.rc != 0 and any(hint in candidate.stderr.lower() for hint in PERMISSION_HINTS):
                raise self._failure(["uiautomator", "dump", UI_DUMP_PATH], candidate)
            dump = candidate
            if attempt == 0:
                time.sleep(0.6)  # transient "could not get idle state" on animated UIs
        assert dump is not None
        if dump.rc != 0:
            raise self._failure(["uiautomator", "dump", UI_DUMP_PATH], dump)
        if "dumped to" not in (dump.out + dump.stderr).lower():
            raise DeviceError(
                "ui_unavailable",
                "uiautomator dump failed",
                detail={
                    "rc": dump.rc,
                    "stdout": dump.out.strip()[-1000:],
                    "stderr": dump.stderr.strip()[-1000:],
                },
            )
        raw = self.run(["cat", UI_DUMP_PATH], timeout=20.0)
        if raw.rc != 0 or not raw.stdout:
            raise DeviceError(
                "ui_unavailable",
                "could not read uiautomator dump",
                detail={"rc": raw.rc, "stderr": raw.stderr[-1000:]},
            )
        try:
            root = ElementTree.fromstring(raw.stdout)
        except ElementTree.ParseError as exc:
            raise DeviceError(
                "ui_parse_failed",
                "uiautomator dump is not valid XML",
                detail={"reason": str(exc), "bytes": len(raw.stdout)},
            ) from exc

        budget = {"nodes": 0, "truncated": False}

        def convert(element: ElementTree.Element) -> dict | None:
            if budget["nodes"] >= max_nodes:
                budget["truncated"] = True
                return None
            budget["nodes"] += 1
            attrs = element.attrib
            bounds = parse_bounds(attrs.get("bounds", ""))
            node: dict = {
                "cls": attrs.get("class", ""),
                "pkg": attrs.get("package", ""),
                "id": attrs.get("resource-id", ""),
                "clickable": attrs.get("clickable") == "true",
                "long_clickable": attrs.get("long-clickable") == "true",
                "scrollable": attrs.get("scrollable") == "true",
                "enabled": attrs.get("enabled") == "true",
                "focused": attrs.get("focused") == "true",
                "selected": attrs.get("selected") == "true",
            }
            if attrs.get("text"):
                node["text"] = attrs["text"]
            if attrs.get("content-desc"):
                node["desc"] = attrs["content-desc"]
            if bounds:
                node["bounds"] = bounds
                node["center"] = [(bounds[0] + bounds[2]) // 2, (bounds[1] + bounds[3]) // 2]
            children = []
            for child in element:
                converted = convert(child)
                if converted is not None:
                    children.append(converted)
            if children:
                node["children"] = children
            return node

        tree = convert(root)
        if tree is None:
            raise DeviceError("ui_parse_failed", "uiautomator dump has no hierarchy node")
        if not tree.get("pkg") and not tree.get("cls") and len(tree.get("children", ())) == 1:
            tree = tree["children"][0]

        emitted = {"nodes": 0}
        interactive: list[dict] = []

        def collect(node: dict, depth: int) -> None:
            emitted["nodes"] += 1
            actionable = (
                node.get("clickable")
                or node.get("long_clickable")
                or node.get("scrollable")
                or bool(node.get("text"))
                or bool(node.get("desc"))
            )
            if actionable and len(interactive) < max_interactive:
                interactive.append(
                    {
                        "cls": node["cls"],
                        "id": node["id"],
                        "pkg": node["pkg"],
                        "text": node.get("text", ""),
                        "desc": node.get("desc", ""),
                        "clickable": node["clickable"],
                        "long_clickable": node["long_clickable"],
                        "scrollable": node["scrollable"],
                        "bounds": node.get("bounds"),
                        "center": node.get("center"),
                        "depth": depth,
                    }
                )
            for child in node.get("children", ()):
                collect(child, depth + 1)

        collect(tree, 0)

        if package:
            tree = _prune_to_package(tree, package)

        return {
            "ok": True,
            "node_count": emitted["nodes"],
            "interactive_count": len(interactive),
            "truncated": budget["truncated"],
            "package_filter": package,
            "interactive": interactive,
            "tree": tree,
        }

    # ------------------------------------------------------------------ input
    def tap(self, x: int, y: int) -> dict:
        self.run_text(["input", "tap", str(x), str(y)], timeout=15.0)
        return {"ok": True, "action": "tap", "x": x, "y": y, "dispatched": True}

    def swipe(self, x1: int, y1: int, x2: int, y2: int, duration_ms: int = 300) -> dict:
        self.run_text(["input", "swipe", str(x1), str(y1), str(x2), str(y2), str(duration_ms)], timeout=20.0)
        return {
            "ok": True,
            "action": "swipe",
            "from": [x1, y1],
            "to": [x2, y2],
            "duration_ms": duration_ms,
            "dispatched": True,
        }

    def long_press(self, x: int, y: int, duration_ms: int = 800) -> dict:
        self.run_text(["input", "swipe", str(x), str(y), str(x), str(y), str(duration_ms)], timeout=20.0)
        return {"ok": True, "action": "long_press", "x": x, "y": y, "duration_ms": duration_ms, "dispatched": True}

    def key(self, code: str) -> dict:
        self.run_text(["input", "keyevent", code], timeout=15.0)
        return {"ok": True, "action": "key", "code": code, "dispatched": True}

    @staticmethod
    def text_argv(text: str) -> list[str]:
        """Build `input text` argv for one line. Spaces become %s (InputShellCommand)."""
        return ["input", "text", text.replace(" ", "%s")]

    @classmethod
    def input_text_segments(cls, line: str) -> list[list[str]]:
        """Split a literal `%s` into `%` + `s` calls: input text has no escape for it."""
        if "%s" not in line:
            return [cls.text_argv(line)]
        segments: list[list[str]] = []
        parts = line.split("%s")
        for index, part in enumerate(parts):
            if part:
                segments.append(cls.text_argv(part))
            if index != len(parts) - 1:
                segments.append(cls.text_argv("%"))
                segments.append(cls.text_argv("s"))
        return segments

    def type_text(self, text: str) -> dict:
        if not text:
            raise DeviceError("invalid_argument", "text must not be empty")
        lines = text.split("\n")
        needs_clipboard = any(_needs_clipboard(line) for line in lines)
        if needs_clipboard:
            self._require_clipboard()
        modes: list[str] = []
        for index, line in enumerate(lines):
            if line and _needs_clipboard(line):
                self.run_ok(["cmd", "clipboard", "settext", line], timeout=15.0)
                self.run_ok(["input", "keyevent", "279"], timeout=15.0)
                modes.append("clipboard_paste")
            elif line:
                for segment in self.input_text_segments(line):
                    self.run_ok(segment, timeout=20.0)
                modes.append("input_text")
            if index != len(lines) - 1:
                self.run_ok(["input", "keyevent", "66"], timeout=15.0)
        return {
            "ok": True,
            "action": "type",
            "chars": len(text),
            "lines": len(lines),
            "modes": sorted(set(modes)),
            "dispatched": True,
        }

    def _require_clipboard(self) -> None:
        if self._clipboard_ok is None:
            probe = self.run(["cmd", "clipboard"], timeout=10.0)
            combined = (probe.out + probe.stderr).lower()
            self._clipboard_ok = probe.rc == 0 and "no shell command" not in combined and "settext" in combined
            if not self._clipboard_ok:
                raise DeviceError(
                    "unsupported_text_encoding",
                    "non-ASCII text needs clipboard paste, but `cmd clipboard` is not implemented on this Android build",
                    detail={
                        "rc": probe.rc,
                        "stderr": probe.stderr.strip()[-500:],
                        "stdout": probe.out.strip()[-500:],
                        "remedy": "install an ADB-ready IME (e.g. ADBKeyboard) or send ASCII text",
                    },
                    dispatched=False,
                )

    # ----------------------------------------------------------------- window
    def window(self, max_windows: int = 30) -> dict:
        # Geometry lives in `windows` (full blocks with `Frames:`); focus lives in
        # `displays` (`mCurrentFocus` / `mFocusedApp`) — plain `dumpsys window`
        # abbreviates blocks and drops the frame line.
        focus = self.run(["dumpsys", "window", "displays"], timeout=20.0)
        dump = self.run_text(["dumpsys", "window", "windows"], timeout=40.0)
        result = parse_dumpsys_window(dump.out, max_windows=max_windows, focus_output=focus.out)
        if focus.rc != 0:
            result["focus_error"] = {"rc": focus.rc, "stderr": focus.stderr.strip()[-1000:]}
        return result

    # ------------------------------------------------------------------- apps
    def apps(self, filter_text: str | None = None, third_party: bool = False, max_packages: int = 200) -> dict:
        argv = ["pm", "list", "packages"]
        if third_party:
            argv.append("-3")
        result = self.run_text(argv, timeout=30.0)
        packages = sorted(
            line[len("package:") :].strip()
            for line in result.out.splitlines()
            if line.startswith("package:")
        )
        total = len(packages)
        if filter_text:
            needle = filter_text.lower()
            packages = [name for name in packages if needle in name.lower()]
        truncated = len(packages) > max_packages
        return {
            "ok": True,
            "total": total,
            "matched": len(packages),
            "third_party_only": third_party,
            "filter": filter_text,
            "truncated": truncated,
            "packages": packages[:max_packages],
        }

    # ------------------------------------------------------------------ start
    def start(self, target: str, activity: str | None = None, wait: bool = True) -> dict:
        component = target.strip()
        resolved_from: str | None = None
        if activity:
            component = f"{component.rstrip('/')}/{activity}"
        elif "/" not in component:
            resolved = self.run(["cmd", "package", "resolve-activity", "--brief", "-c",
                                 "android.intent.category.LAUNCHER", component], timeout=30.0)
            lines = [line.strip() for line in resolved.out.splitlines() if line.strip()]
            match = next((line for line in reversed(lines) if re.fullmatch(r"[\w.]+/[\w.$]+", line)), None)
            if resolved.rc != 0 or not match:
                raise DeviceError(
                    "resolve_failed",
                    f"could not resolve a launcher activity for {component}",
                    detail={
                        "rc": resolved.rc,
                        "stdout": resolved.out.strip()[-1000:],
                        "stderr": resolved.stderr.strip()[-1000:],
                    },
                )
            component = match
            resolved_from = f"resolve-activity:{target}"
        argv = ["am", "start"]
        if wait:
            argv.append("-W")
        argv += ["-n", component]
        result = self.run(argv, timeout=60.0)
        parsed = parse_am_start(result.out)
        if result.rc != 0 or parsed.get("status") not in ("ok", "okay", None):
            raise DeviceError(
                "start_failed",
                f"am start failed for {component}",
                detail={
                    "rc": result.rc,
                    "component": component,
                    "stdout": result.out.strip()[-2000:],
                    "stderr": result.stderr.strip()[-2000:],
                },
            )
        if parsed.get("error"):
            raise DeviceError("start_failed", parsed["error"], detail={"component": component,
                                                                      "stdout": result.out.strip()[-2000:]})
        return {
            "ok": True,
            "component": component,
            "resolved_from": resolved_from,
            "status": parsed.get("status"),
            "activity": parsed.get("activity"),
            "this_time_ms": parsed.get("this_time"),
            "total_time_ms": parsed.get("total_time"),
            "wait_time_ms": parsed.get("wait_time"),
            "dispatched": True,
        }

    # ---------------------------------------------------------------- analyze
    def analyze(self, path: str, question: str | None = None, ocr: bool = True, lang: str = "eng") -> dict:
        target = Path(path)
        if not target.exists():
            raise DeviceError("not_found", f"no such file: {path}")
        if not target.is_file():
            raise DeviceError("invalid_argument", f"not a regular file: {path}")
        size_bytes = target.stat().st_size
        if size_bytes > 10 * 1024 * 1024:
            raise DeviceError(
                "input_too_large",
                "image exceeds the donor contract 10 MiB cap",
                detail={"bytes": size_bytes, "limit": 10 * 1024 * 1024},
            )
        data = target.read_bytes()
        fmt = detect_format(data)
        dimensions = image_size(data)
        result: dict = {
            "ok": True,
            "path": str(target),
            "format": fmt,
            "width": dimensions[0] if dimensions else None,
            "height": dimensions[1] if dimensions else None,
            "bytes": size_bytes,
            "sha256": hashlib.sha256(data).hexdigest(),
            "question": question,
            "answer": None,
            "ocr": {"requested": ocr, "engine": None, "text": None},
        }
        hook = os.environ.get("PEEKABOO_ANDROID_ANALYZE_CMD")
        if hook:
            result["hook"] = self._run_analyze_hook(hook, target, question)
        if ocr:
            engine = _find_tesseract()
            if engine is None:
                result["ocr"]["error"] = "tesseract not installed (pkg install tesseract)"
            else:
                proc = subprocess.run(
                    [engine, str(target), "stdout", "-l", lang],
                    capture_output=True,
                    timeout=90,
                    check=False,
                )
                if proc.returncode == 0:
                    text = proc.stdout.decode("utf-8", "replace").strip()
                    result["ocr"].update(
                        {
                            "engine": "tesseract",
                            "lang": lang,
                            "text": text[:20000],
                            "truncated": len(text) > 20000,
                        }
                    )
                else:
                    result["ocr"]["error"] = (
                        proc.stderr.decode("utf-8", "replace").strip()[-500:] or "tesseract failed"
                    )
        if question and result["answer"] is None:
            result["answer_note"] = "no vision model configured; OCR text and metadata returned"
        return result

    @staticmethod
    def _run_analyze_hook(hook: str, target: Path, question: str | None) -> dict:
        argv = shlex.split(hook) + [str(target)]
        try:
            proc = subprocess.run(argv, capture_output=True, timeout=120, check=False)
        except (OSError, subprocess.TimeoutExpired) as exc:
            return {"ok": False, "error": str(exc)}
        payload = {
            "ok": proc.returncode == 0,
            "rc": proc.returncode,
            "stdout": proc.stdout.decode("utf-8", "replace")[:20000],
            "stderr": proc.stderr.decode("utf-8", "replace")[-2000:],
            "question": question,
        }
        return payload


def _needs_clipboard(line: str) -> bool:
    return any(ord(char) < 32 or ord(char) > 126 for char in line)


def _find_tesseract() -> str | None:
    override = os.environ.get("PEEKABOO_ANDROID_TESSERACT")
    if override:
        return override if Path(override).exists() else None
    found = shutil.which("tesseract")
    if found:
        return found
    fallback = Path("/data/data/com.termux/files/usr/bin/tesseract")
    return str(fallback) if fallback.exists() else None


def _prune_to_package(node: dict, package: str) -> dict | None:
    children = [kept for child in node.get("children", ()) if (kept := _prune_to_package(child, package))]
    if node.get("pkg") != package and not children:
        return None
    pruned = {key: value for key, value in node.items() if key != "children"}
    if children:
        pruned["children"] = children
    return pruned


def parse_dumpsys_window(output: str, max_windows: int = 30, focus_output: str | None = None) -> dict:
    focus_source = output if focus_output is None else focus_output
    focus_line = re.search(r"mCurrentFocus=(Window\{[^}]*\})", focus_source)
    focused_app_line = re.search(r"mFocusedApp=(ActivityRecord\{[^}]*\})", focus_source)
    focus = _parse_window_token(focus_line.group(1)) if focus_line else None
    focused_app = _parse_activity_token(focused_app_line.group(1)) if focused_app_line else None

    windows: list[dict] = []
    blocks = re.split(r"\n\s+Window #\d+ ", output)
    total = max(len(blocks) - 1, 0)
    truncated = False
    for block in blocks[1:]:
        header = block.splitlines()[0].rstrip(":") if block.splitlines() else ""
        token = _parse_window_token(header)
        if "isVisible=true" in block:
            visible = True
        elif "isVisible=" in block:
            visible = False
        else:
            visible = "mHasSurface=true" in block
        if not token or not visible:
            continue
        bounds = re.search(r"mBounds=\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]", block)
        if not bounds:
            bounds = re.search(r"\bframe=\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]", block)
        window_type = re.search(r"\bty=([A-Z_]+)", block)
        title = re.search(r'"[^"]*"', block[:400])
        row = {
            "hash": token["hash"],
            "user": token["user"],
            "package": token["package"],
            "activity": token["activity"],
            "component": token["component"],
            "type": window_type.group(1) if window_type else None,
            "bounds": [int(value) for value in bounds.groups()] if bounds else None,
            "title": title.group(0).strip('"') if title else None,
        }
        windows.append(row)
        if len(windows) >= max_windows:
            truncated = len(windows) < total
            break
    return {
        "ok": True,
        "focus": focus,
        "focused_app": focused_app,
        "window_count": total,
        "visible_windows": windows,
        "truncated": truncated,
    }


def _parse_window_token(token: str) -> dict | None:
    match = re.fullmatch(r"Window\{([0-9a-fA-F]+) u(\d+) ([^}]+)\}", token.strip())
    if not match:
        return None
    hash_value, user, component_raw = match.groups()
    component = decode_component(component_raw)
    return {"hash": hash_value, "user": int(user), **component, "raw": token.strip()}


def _parse_activity_token(token: str) -> dict | None:
    match = re.fullmatch(r"ActivityRecord\{([0-9a-fA-F]+) u(\d+) ([^ }]+)(?: t(\d+))?\}", token.strip())
    if not match:
        return None
    hash_value, user, component_raw, task = match.groups()
    component = decode_component(component_raw)
    return {"hash": hash_value, "user": int(user), "task": int(task) if task else None, **component}


def parse_am_start(output: str) -> dict:
    fields: dict = {}
    patterns = {
        "status": r"Status:\s*(\w+)",
        "activity": r"Activity:\s*(\S+)",
        "this_time": r"ThisTime:\s*(\d+)",
        "total_time": r"TotalTime:\s*(\d+)",
        "wait_time": r"WaitTime:\s*(\d+)",
        "error": r"Error:\s*(.+)",
    }
    for key, pattern in patterns.items():
        match = re.search(pattern, output)
        if match:
            value = match.group(1).strip()
            fields[key] = int(value) if key.endswith("_time") else value
    return fields
