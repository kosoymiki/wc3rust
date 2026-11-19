#!/usr/bin/env python3
"""ComfyUI API adapter for HUD Artisan asset jobs.

The adapter accepts only ComfyUI API-format workflows, applies explicit node
mappings, waits for a completed execution, and imports exactly one validated PNG.
"""
from __future__ import annotations

import argparse
import io
import json
import os
import sys
import tempfile
import time
import uuid
from pathlib import Path, PurePosixPath
from urllib import error, parse, request

MAX_RESPONSE_BYTES = 256 * 1024 * 1024
MAX_PROMPT_ID_LENGTH = 128


class ComfyError(RuntimeError):
    pass


def open_http(req, base: str, timeout: int):
    """Bypass environment proxies for loopback; retain proxies for remote HTTPS."""
    hostname = parse.urlsplit(base).hostname
    if hostname in ("127.0.0.1", "localhost", "::1"):
        opener = request.build_opener(request.ProxyHandler({}))
        return opener.open(req, timeout=timeout)
    return request.urlopen(req, timeout=timeout)


def read_json(path: Path):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ComfyError(f"cannot read JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ComfyError(f"JSON root must be an object: {path}")
    return value


def http_json(base: str, endpoint: str, payload=None, timeout: int = 60):
    data = None if payload is None else json.dumps(payload, allow_nan=False).encode("utf-8")
    headers = {"Accept": "application/json"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = request.Request(base.rstrip("/") + endpoint, data=data, headers=headers)
    try:
        with open_http(req, base, timeout) as response:
            raw = response.read(MAX_RESPONSE_BYTES + 1)
    except error.HTTPError as exc:
        detail = exc.read(4096).decode("utf-8", "replace")
        raise ComfyError(f"ComfyUI HTTP {exc.code} at {endpoint}: {detail}") from exc
    except (error.URLError, TimeoutError, OSError) as exc:
        raise ComfyError(f"ComfyUI request failed at {endpoint}: {exc}") from exc
    if len(raw) > MAX_RESPONSE_BYTES:
        raise ComfyError(f"ComfyUI JSON response exceeds {MAX_RESPONSE_BYTES} bytes")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ComfyError(f"ComfyUI returned invalid JSON from {endpoint}") from exc
    if not isinstance(value, dict):
        raise ComfyError(f"ComfyUI returned a non-object JSON response from {endpoint}")
    return value


def validate_server_url(base: object) -> str:
    if not isinstance(base, str) or not base.strip():
        raise ComfyError("ComfyUI server URL must be a non-empty string")
    parsed = parse.urlsplit(base)
    if (parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username or parsed.password
            or parsed.path not in ("", "/") or parsed.query or parsed.fragment):
        raise ComfyError("server must be a root http(s) URL without embedded credentials, path, query or fragment")
    if parsed.scheme != "https" and parsed.hostname not in ("127.0.0.1", "localhost", "::1"):
        raise ComfyError("non-loopback ComfyUI endpoints must use HTTPS")
    return base.rstrip("/")


def validate_job(job: dict) -> None:
    job_id = job.get("job_id")
    if not isinstance(job_id, str) or len(job_id) != 16 or any(ch not in "0123456789abcdef" for ch in job_id):
        raise ComfyError("job_id must be a 16-character lowercase hexadecimal id")
    for key in ("prompt", "negative_prompt"):
        if not isinstance(job.get(key), str) or not job[key].strip():
            raise ComfyError(f"job.{key} must be a non-empty string")
    width, height = job.get("width"), job.get("height")
    if (type(width) is not int or type(height) is not int
            or not 1 <= width <= 8192 or not 1 <= height <= 8192
            or width * height > 64_000_000):
        raise ComfyError("job width/height are outside the supported integer/pixel range")
    seed = job.get("seed")
    if type(seed) is not int or seed < 0 or seed > 0xFFFFFFFF:
        raise ComfyError("job.seed must be an unsigned 32-bit integer")
    if "requires_alpha" in job and type(job["requires_alpha"]) is not bool:
        raise ComfyError("job.requires_alpha must be a boolean")
    if "family" in job and not isinstance(job["family"], str):
        raise ComfyError("job.family must be a string")


def apply_node_input(workflow: dict, mapping: dict, key: str, value):
    node_id = str(mapping.get("node_id", ""))
    input_name = mapping.get("input")
    if not node_id or not isinstance(input_name, str) or not input_name:
        raise ComfyError(f"workflow node mapping for {key} needs node_id and input")
    node = workflow.get(node_id)
    if not isinstance(node, dict) or not isinstance(node.get("inputs"), dict):
        raise ComfyError(f"workflow node {node_id!r} or its inputs are missing ({key})")
    if input_name not in node["inputs"]:
        raise ComfyError(f"workflow node {node_id} has no input {input_name!r} ({key})")
    node["inputs"][input_name] = value
    return node_id, input_name


def cancel_prompt(base: str, prompt_id: str, timeout: int) -> None:
    """Best-effort removal of a timed-out prompt from ComfyUI's pending queue."""
    try:
        http_json(base, "/queue", {"delete": [prompt_id]}, timeout)
    except ComfyError as exc:
        print(f"COMFYUI_CANCEL_WARNING prompt_id={prompt_id}: {exc}", file=sys.stderr)


def wait_for_output(base: str, prompt_id: str, output_node_id: str,
                    timeout: int, max_wait: int, interval: float) -> dict:
    deadline = time.monotonic() + max_wait
    while time.monotonic() < deadline:
        history = http_json(base, "/history/" + parse.quote(prompt_id, safe=""), timeout)
        record = history.get(prompt_id)
        if record is not None and not isinstance(record, dict):
            raise ComfyError("ComfyUI history record has an invalid shape")
        if isinstance(record, dict):
            status = record.get("status", {})
            if status is not None and not isinstance(status, dict):
                raise ComfyError("ComfyUI history status has an invalid shape")
            status = status or {}
            status_name = status.get("status_str")
            if status_name == "error":
                messages = status.get("messages", [])
                raise ComfyError(f"ComfyUI generation failed: {json.dumps(messages, ensure_ascii=False)[:6000]}")
            outputs = record.get("outputs", {})
            if outputs is not None and not isinstance(outputs, dict):
                raise ComfyError("ComfyUI history outputs have an invalid shape")
            output = (outputs or {}).get(output_node_id)
            if output is not None and not isinstance(output, dict):
                raise ComfyError("configured SaveImage output has an invalid shape")
            images = output.get("images", []) if isinstance(output, dict) else []
            if images and (status.get("completed") is True or status_name == "success"):
                if not isinstance(images, list) or len(images) != 1 or not isinstance(images[0], dict):
                    raise ComfyError(f"expected exactly one SaveImage result, received {len(images) if isinstance(images, list) else 'invalid'}")
                return images[0]
            if status.get("completed") is True or status_name == "success":
                raise ComfyError(f"ComfyUI completed without an image from SaveImage node {output_node_id}")
        time.sleep(interval)
    cancel_prompt(base, prompt_id, timeout)
    raise ComfyError(f"ComfyUI job timed out after {max_wait}s (prompt_id={prompt_id}); cancellation was requested")


def download_image(base: str, image_meta: dict, timeout: int) -> bytes:
    for key in ("filename", "type"):
        if not isinstance(image_meta.get(key), str) or not image_meta[key]:
            raise ComfyError(f"ComfyUI output image is missing {key}")
    filename = image_meta["filename"]
    if Path(filename).name != filename or filename in (".", "..") or "\\" in filename or "\x00" in filename:
        raise ComfyError("ComfyUI returned an unsafe output filename")
    if image_meta["type"] != "output":
        raise ComfyError(f"expected a ComfyUI output image, got type={image_meta['type']!r}")
    subfolder = image_meta.get("subfolder", "")
    if (not isinstance(subfolder, str) or "\\" in subfolder or "\x00" in subfolder
            or PurePosixPath(subfolder).is_absolute()
            or any(part in ("..", ".") for part in PurePosixPath(subfolder).parts)):
        raise ComfyError("ComfyUI returned an unsafe output subfolder")
    query = {"filename": filename, "type": "output"}
    if subfolder:
        query["subfolder"] = subfolder
    endpoint = "/view?" + parse.urlencode(query)
    req = request.Request(base.rstrip("/") + endpoint, headers={"Accept": "image/png"})
    try:
        with open_http(req, base, timeout) as response:
            data = response.read(MAX_RESPONSE_BYTES + 1)
    except error.HTTPError as exc:
        detail = exc.read(4096).decode("utf-8", "replace")
        raise ComfyError(f"ComfyUI image download HTTP {exc.code}: {detail}") from exc
    except (error.URLError, TimeoutError, OSError) as exc:
        raise ComfyError(f"ComfyUI image download failed: {exc}") from exc
    if len(data) > MAX_RESPONSE_BYTES:
        raise ComfyError("ComfyUI image exceeds 256 MiB limit")
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ComfyError("ComfyUI SaveImage output is not a PNG")
    return data


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Run one HUD Artisan job through ComfyUI")
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--job-json", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        try:
            from PIL import Image
        except ImportError as exc:
            raise ComfyError("Pillow is required; install tools/hud_artisan/requirements.txt") from exc

        job = read_json(args.job_json)
        validate_job(job)
        cfg = read_json(args.config)
        if cfg.get("schema_version") != 1:
            raise ComfyError("unsupported ComfyUI adapter config schema_version (expected 1)")

        if args.output.suffix.lower() != ".png":
            raise ComfyError("output path must end in .png")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        if args.output.is_symlink() or (args.output.exists() and not args.output.is_file()):
            raise ComfyError("output path must be a regular file path, not a symlink or directory")
        output_parent = args.output.parent.resolve()
        output_path = output_parent / args.output.name

        base = validate_server_url(os.environ.get("COMFYUI_URL", cfg.get("server", "http://127.0.0.1:8188")))
        timeout = cfg.get("request_timeout_seconds", 60)
        max_wait = cfg.get("max_wait_seconds", 1800)
        interval = cfg.get("poll_interval_seconds", 1.0)
        if (type(timeout) is not int or type(max_wait) is not int or type(interval) not in (int, float)
                or not 1 <= timeout <= 300 or not 1 <= max_wait <= 86400
                or not 0.2 <= interval <= 30):
            raise ComfyError("invalid request timeout / max wait / poll interval")

        workflow_value = cfg.get("workflow_file")
        if not isinstance(workflow_value, str) or not workflow_value.strip():
            raise ComfyError("config.workflow_file must be a non-empty path")
        workflow_file = Path(workflow_value)
        if not workflow_file.is_absolute():
            workflow_file = (args.config.resolve().parent / workflow_file).resolve()
        workflow = read_json(workflow_file)
        if not workflow:
            raise ComfyError("ComfyUI API workflow must contain at least one node")

        mappings = cfg.get("node_inputs")
        required_mappings = ("positive_prompt", "negative_prompt", "width", "height", "seed")
        if not isinstance(mappings, dict) or set(mappings) != set(required_mappings):
            raise ComfyError("config.node_inputs must define exactly positive_prompt, negative_prompt, width, height and seed")
        mapped_inputs = set()
        for key in required_mappings:
            mapping = mappings.get(key)
            if not isinstance(mapping, dict):
                raise ComfyError(f"missing workflow node mapping: {key}")
            target = apply_node_input(workflow, mapping, key, {
                "positive_prompt": job["prompt"],
                "negative_prompt": job["negative_prompt"],
                "width": job["width"],
                "height": job["height"],
                "seed": job["seed"],
            }[key])
            if target in mapped_inputs:
                raise ComfyError(f"workflow mappings must target distinct node inputs; duplicate target {target}")
            mapped_inputs.add(target)

        output_node_id = cfg.get("output_node_id")
        if not isinstance(output_node_id, str) or not output_node_id.strip():
            raise ComfyError("config.output_node_id must identify the SaveImage node")
        output_node = workflow.get(output_node_id)
        if not isinstance(output_node, dict) or output_node.get("class_type") != "SaveImage":
            raise ComfyError(f"configured output node {output_node_id!r} must exist with class_type='SaveImage'")
        if not isinstance(output_node.get("inputs"), dict) or "images" not in output_node["inputs"]:
            raise ComfyError("configured SaveImage node must have an images input")

        client_id = str(uuid.uuid4())
        submitted = http_json(base, "/prompt", {"prompt": workflow, "client_id": client_id}, timeout)
        if submitted.get("node_errors"):
            raise ComfyError(f"ComfyUI rejected workflow nodes: {json.dumps(submitted['node_errors'], ensure_ascii=False)[:6000]}")
        prompt_id = submitted.get("prompt_id")
        if not isinstance(prompt_id, str) or not prompt_id or len(prompt_id) > MAX_PROMPT_ID_LENGTH:
            raise ComfyError(f"ComfyUI did not return a valid prompt_id: {submitted!r}")

        image_meta = wait_for_output(base, prompt_id, output_node_id, timeout, max_wait, float(interval))
        raw = download_image(base, image_meta, timeout)
        try:
            with Image.open(io.BytesIO(raw)) as image:
                image.load()
                if image.format != "PNG":
                    raise ComfyError(f"ComfyUI returned {image.format}, expected PNG")
                rgba = image.convert("RGBA")
                expected = (job["width"], job["height"])
                if rgba.size != expected:
                    raise ComfyError(f"ComfyUI output size {rgba.size} != requested {expected}; no silent resizing")
                alpha = rgba.getchannel("A")
                if alpha.getbbox() is None:
                    raise ComfyError("ComfyUI output is fully transparent")
                if job.get("requires_alpha"):
                    if job.get("family") in ("portrait-frame", "minimap-frame"):
                        w, h = rgba.size
                        aperture = alpha.crop((int(w * 0.30), int(h * 0.30),
                                               max(int(w * 0.30) + 1, int(w * 0.70)),
                                               max(int(h * 0.30) + 1, int(h * 0.70))))
                        histogram = aperture.histogram()
                        transparent = sum(histogram[:9]) / max(1, aperture.width * aperture.height)
                        if transparent < 0.97:
                            raise ComfyError(f"frame aperture transparency is {transparent:.3f}; expected >=0.970")
                    elif alpha.getextrema() == (255, 255):
                        raise ComfyError("this job requires transparency; workflow output is fully opaque")
                with tempfile.NamedTemporaryFile(dir=output_parent, suffix=".png.tmp", delete=False) as temporary:
                    temporary_path = Path(temporary.name)
                try:
                    rgba.save(temporary_path, format="PNG", optimize=True, compress_level=9)
                    temporary_path.replace(output_path)
                finally:
                    temporary_path.unlink(missing_ok=True)
        except ComfyError:
            raise
        except Exception as exc:
            raise ComfyError(f"cannot decode ComfyUI image: {exc}") from exc

        print(f"COMFYUI_ARTISAN_OK job={job.get('job_id')} output={args.output}")
        return 0
    except (ComfyError, KeyError, ValueError, OSError, TypeError, OverflowError) as exc:
        print(f"COMFYUI_ARTISAN_ERROR {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
