#!/usr/bin/env python3
"""Read-only preflight for a local ComfyUI renderer and its HUD Artisan workflow."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

import comfyui_adapter as adapter


def sha_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def inspect_runtime(config_path: Path) -> dict:
    cfg = adapter.read_json(config_path)
    if cfg.get("schema_version") != 1:
        raise adapter.ComfyError("unsupported ComfyUI adapter config schema_version")
    base = adapter.validate_server_url(cfg.get("server", "http://127.0.0.1:8188"))
    timeout = cfg.get("request_timeout_seconds", 60)
    if type(timeout) is not int or not 1 <= timeout <= 300:
        raise adapter.ComfyError("invalid request_timeout_seconds")
    alpha_strategy=cfg.get("alpha_strategy","auto")
    if alpha_strategy not in adapter.ALPHA_STRATEGIES:
        raise adapter.ComfyError(f"unsupported alpha_strategy: {alpha_strategy!r}")
    if alpha_strategy=="u2net":
        import importlib.util
        configured_model=cfg.get("matte_model")
        if configured_model is not None and (not isinstance(configured_model,str) or not configured_model.strip()):
            raise adapter.ComfyError("matte_model must be a non-empty path string")
        model_value=configured_model or os.environ.get("U2NET_MODEL") or str(
            Path.home()/"src"/"comfyui-termux"/"models"/"u2net.onnx"
        )
        model_path=Path(model_value).expanduser()
        if configured_model and not model_path.is_absolute():
            model_path=config_path.resolve().parent/model_path
        if model_path.is_symlink() or not model_path.is_file():
            raise adapter.ComfyError(f"u2net model must be a regular non-symlink file: {model_path}")
        for module in ("numpy","onnxruntime"):
            try:
                available=importlib.util.find_spec(module) is not None
            except (ImportError,ValueError):
                available=False
            if not available:
                raise adapter.ComfyError(f"u2net mode requires {module}; install alpha matte dependencies")

    stats = adapter.http_json(base, "/system_stats", timeout=timeout)
    queue = adapter.http_json(base, "/queue", timeout=timeout)
    object_info = adapter.http_json(base, "/object_info", timeout=timeout)

    workflow_value = cfg.get("workflow_file")
    if not isinstance(workflow_value, str) or not workflow_value.strip():
        raise adapter.ComfyError("config.workflow_file must be a non-empty path")
    workflow_path = Path(workflow_value).expanduser()
    if not workflow_path.is_absolute():
        # Resolve only the config directory. Keep the workflow leaf unresolved
        # so verify_workflow_sha256 can still detect a configured symlink.
        workflow_path = config_path.resolve().parent / workflow_path
    workflow_digest = adapter.verify_workflow_sha256(workflow_path, cfg.get("workflow_sha256"))
    workflow = adapter.read_json(workflow_path)
    workflow_format = cfg.get("workflow_format", "api")
    is_ui = isinstance(workflow.get("nodes"), list)
    if workflow_format == "ui" or (workflow_format == "auto" and is_ui):
        prompt = adapter.ui_workflow_to_api(workflow, object_info)
        detected_format = "ui"
    elif workflow_format == "api":
        prompt = workflow
        detected_format = "api"
    else:
        raise adapter.ComfyError("workflow_format does not match a supported workflow")

    errors = []
    warnings = []
    for node_id, node in prompt.items():
        if not isinstance(node, dict) or not isinstance(node.get("class_type"), str):
            errors.append(f"node {node_id}: missing class_type")
            continue
        schema = object_info.get(node["class_type"])
        if not isinstance(schema, dict):
            errors.append(f"node {node_id}: class {node['class_type']} is unavailable on this ComfyUI server")
            continue
        inputs = node.get("inputs", {})
        if not isinstance(inputs, dict):
            errors.append(f"node {node_id}: inputs is not an object")
            continue
        required = schema.get("input", {}).get("required", {})
        if isinstance(required, dict):
            missing = [name for name in required if name not in inputs]
            if missing:
                errors.append(f"node {node_id} ({node['class_type']}): missing required inputs {', '.join(missing)}")
        for input_name, model_key in (
            ("unet_name", "UNETLoader"),
            ("clip_name", "CLIPLoader"),
            ("vae_name", "VAELoader"),
        ):
            if node["class_type"] != model_key or input_name not in inputs:
                continue
            spec = schema.get("input", {}).get("required", {}).get(input_name)
            choices = spec[0] if isinstance(spec, list) and spec and isinstance(spec[0], list) else None
            if choices == [] or choices == ["pixel_space"]:
                warnings.append(
                    f"node {node_id}: server exposes no concrete model enumeration for {input_name}; "
                    "the workflow selection is retained and ComfyUI remains the runtime authority"
                )
            elif choices and inputs[input_name] not in choices:
                errors.append(f"node {node_id}: selected {input_name}={inputs[input_name]!r} is not in server model list")

    mappings = cfg.get("node_inputs")
    required_mappings = ("positive_prompt", "negative_prompt", "width", "height", "seed")
    if not isinstance(mappings, dict) or set(mappings) != set(required_mappings):
        errors.append("node_inputs must map exactly positive_prompt, negative_prompt, width, height and seed")
        mappings = {}
    mapping_report = {}
    for key in required_mappings:
        mapping = mappings.get(key)
        if not isinstance(mapping, dict):
            errors.append(f"missing node mapping for {key}")
            continue
        node_id = str(mapping.get("node_id", ""))
        input_name = mapping.get("input")
        node = prompt.get(node_id)
        if not isinstance(node, dict) or not isinstance(node.get("inputs"), dict):
            errors.append(f"{key}: mapped node {node_id!r} is missing")
            continue
        if not isinstance(input_name, str) or input_name not in node["inputs"]:
            errors.append(f"{key}: mapped input {input_name!r} is missing on node {node_id}")
            continue
        mapping_report[key] = {"node_id": node_id, "class_type": node["class_type"], "input": input_name}

    output_id = str(cfg.get("output_node_id", ""))
    output_node = prompt.get(output_id)
    if not isinstance(output_node, dict) or output_node.get("class_type") != "SaveImage":
        errors.append(f"output_node_id {output_id!r} must identify a SaveImage node")

    system = stats.get("system", {}) if isinstance(stats, dict) else {}
    devices = stats.get("devices", []) if isinstance(stats, dict) else []
    queue_running = queue.get("queue_running", []) if isinstance(queue, dict) else []
    queue_pending = queue.get("queue_pending", []) if isinstance(queue, dict) else []
    if queue_running or queue_pending:
        warnings.append(
            f"ComfyUI queue is active (running={len(queue_running)}, pending={len(queue_pending)}); "
            "wait for it to drain before starting a production batch"
        )
    report = {
        "status": "PASS" if not errors else "FAIL",
        "transport": base,
        "comfyui": {
            "version": system.get("comfyui_version"),
            "python": system.get("python_version"),
            "pytorch": system.get("pytorch_version"),
            "os": system.get("os"),
            "ram_total_bytes": system.get("ram_total"),
            "ram_free_bytes": system.get("ram_free"),
            "devices": [{"name": d.get("name"), "type": d.get("type"),
                         "vram_total": d.get("vram_total"), "vram_free": d.get("vram_free")}
                        for d in devices if isinstance(d, dict)],
        },
        "workflow": {
            "path": str(workflow_path),
            "format": detected_format,
            "sha256": workflow_digest,
            "api_node_count": len(prompt),
            "save_image_node": output_id,
            "mapped_inputs": mapping_report,
        },
        "queue": {"running": len(queue_running), "pending": len(queue_pending)},
        "alpha": {
            "strategy": cfg.get("alpha_strategy", "preserve"),
            "chroma_key_color": cfg.get("chroma_key_color", [255, 0, 255]),
            "chroma_key_tolerance": cfg.get("chroma_key_tolerance", 48),
        },
        "model_nodes": [
            {"node_id": node_id, "class_type": node["class_type"],
             "selected": {k: v for k, v in node.get("inputs", {}).items()
                          if k in ("unet_name", "clip_name", "vae_name", "type", "weight_dtype")}}
            for node_id, node in prompt.items()
            if node.get("class_type") in ("UNETLoader", "CLIPLoader", "VAELoader")
        ],
        "warnings": warnings,
        "errors": errors,
        "read_only": True,
    }
    return report


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Read-only HUD Artisan ComfyUI preflight")
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = parser.parse_args(argv)
    try:
        report = inspect_runtime(args.config)
        if args.json:
            print(json.dumps(report, ensure_ascii=False, indent=2))
        else:
            comfy = report["comfyui"]
            print(f"HUD_ARTISAN_COMFYUI_DOCTOR {report['status']}")
            print(f"ComfyUI={comfy.get('version')} Python={comfy.get('python')} PyTorch={comfy.get('pytorch')}")
            print(f"workflow={report['workflow']['format']} nodes={report['workflow']['api_node_count']} sha256={report['workflow']['sha256']}")
            print(f"queue running={report['queue']['running']} pending={report['queue']['pending']}")
            for key, mapping in report["workflow"]["mapped_inputs"].items():
                print(f"map {key}: node={mapping['node_id']} {mapping['class_type']}.{mapping['input']}")
            for warning in report["warnings"]:
                print("WARN " + warning)
            for error in report["errors"]:
                print("ERROR " + error)
        return 0 if report["status"] == "PASS" else 2
    except (adapter.ComfyError, OSError, ValueError, TypeError) as exc:
        print(f"HUD_ARTISAN_COMFYUI_DOCTOR_ERROR {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
