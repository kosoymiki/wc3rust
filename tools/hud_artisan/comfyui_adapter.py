#!/usr/bin/env python3
"""ComfyUI API adapter for HUD Artisan asset jobs.

The workflow must be exported from ComfyUI in API format and contain explicit nodes
for positive prompt, negative prompt, latent width/height, seed and SaveImage output.
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
from pathlib import Path
from urllib import error, parse, request

MAX_RESPONSE_BYTES = 256 * 1024 * 1024

def open_http(req, base: str, timeout: int):
    """Bypass environment proxies for loopback; keep normal proxy support for remote hosts."""
    hostname = parse.urlsplit(base).hostname
    if hostname in ("127.0.0.1", "localhost", "::1"):
        opener = request.build_opener(request.ProxyHandler({}))
        return opener.open(req, timeout=timeout)
    return request.urlopen(req, timeout=timeout)

class ComfyError(RuntimeError):
    pass

def read_json(path: Path):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ComfyError(f"cannot read JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ComfyError(f"JSON root must be an object: {path}")
    return value

def http_json(base: str, endpoint: str, payload=None, timeout: int = 60):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Accept": "application/json"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = request.Request(base.rstrip("/") + endpoint, data=data, headers=headers)
    try:
        with open_http(req, base, timeout) as response:
            raw = response.read(MAX_RESPONSE_BYTES + 1)
    except error.HTTPError as exc:
        detail = exc.read(4096).decode("utf-8", "replace")
        raise ComfyError(f"ComfyUI HTTP {exc.code}: {detail}") from exc
    except (error.URLError, TimeoutError, OSError) as exc:
        raise ComfyError(f"ComfyUI request failed: {exc}") from exc
    if len(raw) > MAX_RESPONSE_BYTES:
        raise ComfyError("ComfyUI JSON response exceeds size limit")
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ComfyError(f"ComfyUI returned invalid JSON from {endpoint}") from exc

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

def download_image(base: str, image_meta: dict, timeout: int) -> bytes:
    for key in ("filename", "type"):
        if not isinstance(image_meta.get(key), str) or not image_meta[key]:
            raise ComfyError(f"ComfyUI output image is missing {key}")
    filename=image_meta["filename"]
    if Path(filename).name!=filename or filename in (".","..") or "\\" in filename:
        raise ComfyError("ComfyUI returned an unsafe output filename")
    if image_meta["type"]!="output":
        raise ComfyError(f"expected a ComfyUI output image, got type={image_meta['type']!r}")
    subfolder=image_meta.get("subfolder","")
    if not isinstance(subfolder,str) or "\\" in subfolder or Path(subfolder).is_absolute() or ".." in Path(subfolder).parts:
        raise ComfyError("ComfyUI returned an unsafe output subfolder")
    query = {"filename": filename, "type": "output"}
    if subfolder:
        query["subfolder"] = subfolder
    endpoint = "/view?" + parse.urlencode(query)
    req = request.Request(base.rstrip("/") + endpoint, headers={"Accept": "image/png"})
    try:
        with request.urlopen(req, timeout=timeout) as response:
            data = response.read(MAX_RESPONSE_BYTES + 1)
    except error.HTTPError as exc:
        detail = exc.read(4096).decode("utf-8", "replace")
        raise ComfyError(f"ComfyUI image download HTTP {exc.code}: {detail}") from exc
    except (error.URLError, TimeoutError, OSError) as exc:
        raise ComfyError(f"ComfyUI image download failed: {exc}") from exc
    if len(data) > MAX_RESPONSE_BYTES:
        raise ComfyError("ComfyUI image exceeds 256 MiB limit")
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
        cfg = read_json(args.config)
        if cfg.get("schema_version") != 1:
            raise ComfyError("unsupported ComfyUI adapter config schema_version (expected 1)")
        base = os.environ.get("COMFYUI_URL", cfg.get("server", "http://127.0.0.1:8188"))
        if not isinstance(base,str) or not base.strip():
            raise ComfyError("ComfyUI server URL must be a non-empty string")
        parsed = parse.urlsplit(base)
        if (parsed.scheme not in ("http", "https") or not parsed.netloc or parsed.username or parsed.password
            or parsed.path not in ("", "/") or parsed.query or parsed.fragment):
            raise ComfyError("server must be a root http(s) URL without embedded credentials, path, query or fragment")
        timeout = cfg.get("request_timeout_seconds", 60)
        max_wait = cfg.get("max_wait_seconds", 1800)
        interval = cfg.get("poll_interval_seconds", 1.0)
        if (type(timeout) is not int or type(max_wait) is not int or type(interval) not in (int,float)
            or not 1 <= timeout <= 300 or not 1 <= max_wait <= 86400 or not 0.2 <= interval <= 30):
            raise ComfyError("invalid request timeout / max wait / poll interval")
        workflow_value = cfg.get("workflow_file")
        if not isinstance(workflow_value,str) or not workflow_value.strip():
            raise ComfyError("config.workflow_file must be a non-empty path")
        workflow_file = Path(workflow_value)
        if not workflow_file.is_absolute():
            workflow_file = (args.config.resolve().parent / workflow_file).resolve()
        workflow = read_json(workflow_file)
        mappings = cfg.get("node_inputs")
        if not isinstance(mappings, dict):
            raise ComfyError("config.node_inputs must define positive_prompt, negative_prompt, width, height, seed")
        for key, value in (
            ("positive_prompt", job["prompt"]),
            ("negative_prompt", job["negative_prompt"]),
            ("width", int(job["width"])),
            ("height", int(job["height"])),
            ("seed", int(job["job_id"][:8], 16)),
        ):
            mapping = mappings.get(key)
            if not isinstance(mapping, dict):
                raise ComfyError(f"missing workflow node mapping: {key}")
            apply_node_input(workflow, mapping, key, value)
        client_id = str(uuid.uuid4())
        submitted = http_json(base, "/prompt", {"prompt": workflow, "client_id": client_id}, timeout)
        prompt_id = submitted.get("prompt_id") if isinstance(submitted, dict) else None
        if not isinstance(prompt_id, str) or not prompt_id:
            raise ComfyError(f"ComfyUI did not return prompt_id: {submitted!r}")
        output_node_id = str(cfg.get("output_node_id", ""))
        if not output_node_id:
            raise ComfyError("config.output_node_id must identify the SaveImage node")
        deadline = time.monotonic() + max_wait
        image_meta = None
        while time.monotonic() < deadline:
            history = http_json(base, "/history/" + parse.quote(prompt_id, safe=""), timeout)
            record = history.get(prompt_id, {}) if isinstance(history, dict) else {}
            outputs = record.get("outputs", {}) if isinstance(record, dict) else {}
            node_output = outputs.get(output_node_id, {}) if isinstance(outputs, dict) else {}
            images = node_output.get("images", []) if isinstance(node_output, dict) else []
            if images:
                image_meta = images[0]
                break
            status = record.get("status", {})
            if isinstance(status, dict) and status.get("status_str") == "error":
                raise ComfyError(f"ComfyUI generation failed: {status}")
            time.sleep(interval)
        if image_meta is None:
            raise ComfyError(f"ComfyUI job timed out after {max_wait}s (prompt_id={prompt_id})")
        raw = download_image(base, image_meta, timeout)
        try:
            with Image.open(io.BytesIO(raw)) as image:
                image.load()
                if image.format != "PNG":
                    raise ComfyError(f"ComfyUI returned {image.format}, expected PNG")
                rgba = image.convert("RGBA")
                expected = (int(job["width"]), int(job["height"]))
                if rgba.size != expected:
                    raise ComfyError(f"ComfyUI output size {rgba.size} != requested {expected}; no silent resizing")
                if job.get("requires_alpha") and rgba.getchannel("A").getextrema() == (255, 255):
                    raise ComfyError("this job requires a transparent silhouette; workflow output is fully opaque")
                args.output.parent.mkdir(parents=True, exist_ok=True)
                with tempfile.NamedTemporaryFile(dir=args.output.parent, suffix=".png.tmp", delete=False) as tmp:
                    temporary = Path(tmp.name)
                try:
                    rgba.save(temporary, format="PNG", optimize=True, compress_level=9)
                    temporary.replace(args.output)
                finally:
                    temporary.unlink(missing_ok=True)
        except ComfyError:
            raise
        except Exception as exc:
            raise ComfyError(f"cannot decode ComfyUI image: {exc}") from exc
        print(f"COMFYUI_ARTISAN_OK job={job.get('job_id')} output={args.output}")
        return 0
    except (ComfyError, KeyError, ValueError, OSError) as exc:
        print(f"COMFYUI_ARTISAN_ERROR {exc}", file=sys.stderr)
        return 2

if __name__ == "__main__":
    raise SystemExit(main())
