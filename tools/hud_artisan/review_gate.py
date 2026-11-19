#!/usr/bin/env python3
"""HUD Artisan independent visual-review gate.

The gate validates evidence and binds every review to the exact PNG bytes. It does
not claim that a machine can prove artistic quality or reviewer independence.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path, PurePosixPath

CRITERIA = (
    "semantic_readability", "silhouette_at_native_size", "material_credibility",
    "lighting_coherence", "value_hierarchy", "craftsmanship", "style_cohesion",
    "state_consistency", "engine_fitness", "originality_and_ip_safety",
)
ROLES = {"art_director", "red_team"}


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def read(path: Path):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"JSON root must be an object: {path}")
    return value


def safe_pack_file(pack: Path, relative: object) -> Path:
    """Resolve a manifest asset path without traversal or symlink escapes."""
    if not isinstance(relative, str) or not relative or "\\\\" in relative or "\\x00" in relative or ":" in relative:
        raise ValueError(f"unsafe asset path: {relative!r}")
    posix = PurePosixPath(relative)
    if posix.is_absolute() or any(part in ("", ".", "..") for part in posix.parts):
        raise ValueError(f"unsafe asset path: {relative!r}")
    if posix.suffix.lower() != ".png":
        raise ValueError(f"reviewed asset must be a PNG path: {relative!r}")
    root = pack.resolve()
    cursor = root
    for part in posix.parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise ValueError(f"symlink is not allowed in pack asset path: {relative!r}")
    target = cursor.resolve()
    try:
        target.relative_to(root)
    except ValueError as exc:
        raise ValueError(f"asset path escapes pack root: {relative!r}") from exc
    return target


def safe_evidence_file(root: Path, relative: object) -> Path:
    """Resolve a raw reviewer-evidence path without traversal or symlink escapes."""
    if not isinstance(relative, str) or not relative or "\\" in relative or "\\x00" in relative:
        raise ValueError(f"unsafe reviewer evidence path: {relative!r}")
    posix = PurePosixPath(relative)
    if posix.is_absolute() or any(part in ("", ".", "..") for part in posix.parts):
        raise ValueError(f"unsafe reviewer evidence path: {relative!r}")
    if root.is_symlink():
        raise ValueError(f"review evidence root must not be a symlink: {root}")
    base = root.resolve()
    cursor = base
    for part in posix.parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise ValueError(f"symlink is not allowed in reviewer evidence path: {relative!r}")
    target = cursor.resolve()
    try:
        target.relative_to(base)
    except ValueError as exc:
        raise ValueError(f"reviewer evidence path escapes root: {relative!r}") from exc
    if not target.is_file():
        raise ValueError(f"reviewer evidence file is missing or not regular: {relative!r}")
    return target


def validate_review_schema(evidence: dict) -> None:
    schema_path = Path(__file__).with_name("review.schema.json")
    schema = read(schema_path)
    try:
        from jsonschema import Draft202012Validator
    except ImportError as exc:
        raise RuntimeError("review validation requires jsonschema; install tools/hud_artisan/requirements.txt") from exc
    Draft202012Validator.check_schema(schema)
    errors = sorted(Draft202012Validator(schema).iter_errors(evidence),
                    key=lambda error: (list(map(str, error.absolute_path)), error.message))
    if errors:
        first = errors[0]
        where = "/".join(str(part) for part in first.absolute_path) or "<root>"
        raise ValueError(f"review evidence schema violation at {where}: {first.message}")


def evaluate(manifest: Path, jobs: Path, pack: Path, reviews: Path) -> dict:
    manifest_hash = digest(manifest)
    jobs_hash = digest(jobs)
    # Recompile the deterministic plan before trusting review coverage. A correctly
    # hashed but incomplete jobs.json must never be able to release a partial pack.
    try:
        from generate import verify_plan
        j, m = verify_plan(manifest, jobs)
    except Exception as exc:
        raise ValueError(f"manifest/jobs plan verification failed: {exc}") from exc
    r = read(reviews)

    validate_review_schema(r)

    if j.get("source_manifest_sha256") != manifest_hash:
        raise ValueError("jobs are not bound to this manifest")
    if r.get("manifest_sha256") != manifest_hash or r.get("jobs_sha256") != jobs_hash:
        raise ValueError("review evidence is not bound to the exact manifest and jobs")

    manifest_assets = m.get("assets")
    plan_jobs = j.get("jobs")
    if not isinstance(manifest_assets, list) or not isinstance(plan_jobs, list):
        raise ValueError("manifest.assets and jobs.jobs must be arrays")

    assets = {}
    for asset in manifest_assets:
        if not isinstance(asset, dict) or not isinstance(asset.get("name"), str):
            raise ValueError("manifest contains a malformed asset entry")
        name = asset["name"]
        if name in assets:
            raise ValueError(f"manifest contains duplicate asset name: {name}")
        assets[name] = asset

    jobs_by_id = {}
    seen_assets = set()
    for job in plan_jobs:
        if not isinstance(job, dict):
            raise ValueError("jobs contains a malformed job entry")
        jid = job.get("job_id")
        if not isinstance(jid, str) or len(jid) != 16 or any(c not in "0123456789abcdef" for c in jid):
            raise ValueError(f"invalid job_id: {jid!r}")
        if jid in jobs_by_id:
            raise ValueError(f"duplicate job ID: {jid}")
        asset_name = job.get("asset")
        asset = assets.get(asset_name)
        if asset is None:
            raise ValueError(f"{jid}: asset {asset_name!r} is absent from manifest")
        if asset_name in seen_assets:
            raise ValueError(f"multiple jobs target the same manifest asset: {asset_name}")
        if job.get("output") != asset.get("file"):
            raise ValueError(f"{jid}: job output does not match manifest file for {asset_name}")
        if job.get("width") != asset.get("w") or job.get("height") != asset.get("h"):
            raise ValueError(f"{jid}: job dimensions do not match manifest for {asset_name}")
        jobs_by_id[jid] = job
        seen_assets.add(asset_name)

    reviewers = r.get("reviewers")
    # The release protocol is explicitly a two-role gate, not a vote-counting system.
    if not isinstance(reviewers, list) or len(reviewers) != 2:
        raise ValueError("exactly two independent reviewers are required: one art_director and one red_team")
    ids = set()
    models = set()
    roles = set()
    sessions = set()
    evidence_files = set()
    for reviewer in reviewers:
        rid = reviewer["reviewer_id"].strip()
        provider = reviewer["provider"].strip()
        model = reviewer["model"].strip()
        role = reviewer["role"]
        session_id = reviewer["session_id"].strip()
        if not rid or not provider or not model or not session_id:
            raise ValueError("reviewer ID, provider, model and session ID must not be blank")
        if (rid != reviewer["reviewer_id"] or provider != reviewer["provider"]
                or model != reviewer["model"] or session_id != reviewer["session_id"]):
            raise ValueError("reviewer ID, provider, model and session ID must not contain surrounding whitespace")
        if rid in ids or model in models or role in roles or session_id in sessions:
            raise ValueError("reviewers must have distinct IDs, model identifiers, roles and session IDs")
        ids.add(rid)
        models.add(model)
        roles.add(role)
        sessions.add(session_id)
        if len(reviewer["review_prompt"].strip()) < 80:
            raise ValueError(f"{rid}: review prompt must contain at least 80 non-whitespace characters")
        prompt_hash = hashlib.sha256(reviewer["review_prompt"].encode("utf-8")).hexdigest()
        if prompt_hash != reviewer["review_prompt_sha256"]:
            raise ValueError(f"{rid}: review prompt hash mismatch")
        raw_path = safe_evidence_file(reviews.parent, reviewer["raw_response_file"])
        if reviewer["raw_response_file"] in evidence_files:
            raise ValueError("reviewers must have separate raw response evidence files")
        evidence_files.add(reviewer["raw_response_file"])
        raw_size = raw_path.stat().st_size
        if raw_size < 1 or raw_size > 10 * 1024 * 1024:
            raise ValueError(f"{rid}: raw reviewer response size must be between 1 byte and 10 MiB")
        try:
            raw_path.read_text(encoding="utf-8")
        except UnicodeError as exc:
            raise ValueError(f"{rid}: raw reviewer response must be UTF-8 text") from exc
        if digest(raw_path) != reviewer["raw_response_sha256"]:
            raise ValueError(f"{rid}: raw reviewer response hash mismatch")
    if roles != ROLES:
        raise ValueError("reviewers must include exactly one art_director and one red_team")

    evidence = r.get("reviews")
    if not isinstance(evidence, dict):
        raise ValueError("reviews must be an object keyed by job_id")

    failures = []
    rows = []
    if set(evidence) != set(jobs_by_id):
        failures.append(
            f"review coverage mismatch: missing={sorted(set(jobs_by_id)-set(evidence))}, "
            f"extra={sorted(set(evidence)-set(jobs_by_id))}"
        )

    pack_root = pack.resolve()
    for jid, job in jobs_by_id.items():
        asset = assets[job["asset"]]
        try:
            image = safe_pack_file(pack_root, asset.get("file"))
        except ValueError as exc:
            failures.append(f"{jid}: {exc}")
            continue
        if not image.is_file():
            failures.append(f"{jid}: missing final image {asset.get('file')}")
            continue

        actual_hash = digest(image)
        checks = evidence.get(jid)
        if not isinstance(checks, list) or len(checks) != len(reviewers):
            failures.append(f"{jid}: expected exactly {len(reviewers)} reviewer reports")
            continue

        seen = set()
        scores_all = []
        for check in checks:
            if not isinstance(check, dict):
                failures.append(f"{jid}: malformed review entry")
                continue
            rid = check.get("reviewer_id")
            if rid not in ids or rid in seen:
                failures.append(f"{jid}: unknown or duplicate reviewer {rid!r}")
                continue
            seen.add(rid)
            if check.get("image_sha256") != actual_hash:
                failures.append(f"{jid}/{rid}: reviewed image hash mismatch")
            scores = check.get("scores")
            if not isinstance(scores, dict) or set(scores) != set(CRITERIA):
                failures.append(f"{jid}/{rid}: score rubric incomplete or contains unknown criteria")
                continue
            if any(type(value) is not int or not 1 <= value <= 10 for value in scores.values()):
                failures.append(f"{jid}/{rid}: scores must be integer 1..10")
                continue
            if check.get("verdict") != "approve" or check.get("blockers") or check.get("improvements"):
                failures.append(f"{jid}/{rid}: reviewer did not approve cleanly")
            confidence = check.get("confidence")
            if type(confidence) not in (int, float) or not 0.95 <= confidence <= 1:
                failures.append(f"{jid}/{rid}: confidence must be in [0.95,1.0]")
            rationale = check.get("rationale")
            if not isinstance(rationale, str) or len(rationale.strip()) < 80:
                failures.append(f"{jid}/{rid}: rationale must contain at least 80 characters")
            low = [criterion for criterion in CRITERIA if scores[criterion] != 10]
            if low:
                failures.append(f"{jid}/{rid}: not 10/10 in {', '.join(low)}")
            scores_all.append(scores)

        if seen != ids:
            failures.append(f"{jid}: missing reviewer reports {sorted(ids-seen)}")
        passed = len(scores_all) == len(reviewers) and not any(item.startswith(jid) for item in failures)
        rows.append({
            "job_id": jid,
            "asset": job["asset"],
            "image_sha256": actual_hash,
            "reviewers": len(seen),
            "score": "10/10" if passed else "REJECT",
        })

    report = {
        "tool": "HUD Artisan review gate",
        "schema_version": 2,
        "manifest_sha256": manifest_hash,
        "jobs_sha256": jobs_hash,
        "asset_count": len(jobs_by_id),
        "reviewed_count": len(rows),
        "reviewers": [
            {"reviewer_id": item["reviewer_id"], "provider": item["provider"], "model": item["model"],
             "role": item["role"], "session_id": item["session_id"],
             "review_prompt_sha256": item["review_prompt_sha256"],
             "raw_response_file": item["raw_response_file"],
             "raw_response_sha256": item["raw_response_sha256"]}
            for item in reviewers
        ],
        "criteria": list(CRITERIA),
        "result": "PASS" if not failures and len(rows) == len(jobs_by_id) else "REJECT",
        "failures": failures,
        "assets": rows,
    }
    return report


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify independent 10/10 visual review evidence bound to exact final PNG hashes"
    )
    for argument in ("manifest", "jobs", "pack", "reviews", "out"):
        parser.add_argument("--" + argument, required=True, type=Path)
    args = parser.parse_args()
    try:
        result = evaluate(args.manifest, args.jobs, args.pack, args.reviews)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        if args.out.is_symlink():
            raise ValueError(f"review report output must not be a symlink: {args.out}")
        fd, temp_name = tempfile.mkstemp(prefix=f".{args.out.name}.", suffix=".tmp", dir=args.out.parent)
        temporary = Path(temp_name)
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
                stream.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, args.out)
        finally:
            if temporary.exists():
                temporary.unlink()
        print(
            f"HUD_ARTISAN_REVIEW {result['result']} assets={result['reviewed_count']}/"
            f"{result['asset_count']} failures={len(result['failures'])} report={args.out}"
        )
        return 0 if result["result"] == "PASS" else 1
    except (ValueError, OSError, KeyError, TypeError, RuntimeError) as exc:
        print(f"HUD_ARTISAN_REVIEW_ERROR {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
