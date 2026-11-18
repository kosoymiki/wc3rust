#!/usr/bin/env python3
"""HUD Artisan independent visual-review gate. It verifies review evidence, not beauty by itself."""
from __future__ import annotations
import argparse, hashlib, json, sys
from pathlib import Path

CRITERIA = (
    "semantic_readability", "silhouette_at_native_size", "material_credibility",
    "lighting_coherence", "value_hierarchy", "craftsmanship", "style_cohesion",
    "state_consistency", "engine_fitness", "originality_and_ip_safety",
)
ROLES = {"art_director", "red_team"}
def digest(path: Path) -> str:
    h=hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda:f.read(1024*1024),b""): h.update(chunk)
    return h.hexdigest()
def read(path: Path):
    try: return json.loads(path.read_text(encoding="utf-8"))
    except (OSError,json.JSONDecodeError) as e: raise ValueError(f"cannot read {path}: {e}") from e
def evaluate(manifest:Path,jobs:Path,pack:Path,reviews:Path)->dict:
    manifest_hash=digest(manifest);jobs_hash=digest(jobs)
    m=read(manifest);j=read(jobs);r=read(reviews)
    if j.get("source_manifest_sha256")!=manifest_hash: raise ValueError("jobs are not bound to this manifest")
    if r.get("schema_version")!=1: raise ValueError("unsupported review schema_version")
    if r.get("manifest_sha256")!=manifest_hash or r.get("jobs_sha256")!=jobs_hash:
        raise ValueError("review evidence is not bound to the exact manifest and jobs")
    reviewers=r.get("reviewers")
    if not isinstance(reviewers,list) or len(reviewers)<2: raise ValueError("at least two independent reviewers are required")
    ids=set();models=set()
    for x in reviewers:
        if not isinstance(x,dict): raise ValueError("reviewer entries must be objects")
        rid=x.get("reviewer_id");model=x.get("model");role=x.get("role")
        if not all(isinstance(v,str) and v.strip() for v in (rid,model)) or role not in ROLES:
            raise ValueError("reviewer requires reviewer_id, model and role=art_director|red_team")
        if x.get("independent") is not True: raise ValueError(f"reviewer {rid} is not declared independent")
        if rid in ids or model in models: raise ValueError("reviewers must have distinct IDs and model identifiers")
        ids.add(rid);models.add(model)
    if not any(x["role"]=="art_director" for x in reviewers) or not any(x["role"]=="red_team" for x in reviewers):
        raise ValueError("reviewers must include both art_director and red_team roles")
    assets={a["name"]:a for a in m.get("assets",[])}
    jobs_by_id={x["job_id"]:x for x in j.get("jobs",[])}
    if len(jobs_by_id)!=len(j.get("jobs",[])): raise ValueError("duplicate job IDs")
    evidence=r.get("reviews")
    if not isinstance(evidence,dict): raise ValueError("reviews must be an object keyed by job_id")
    failures=[];rows=[]
    if set(evidence)!=set(jobs_by_id):
        failures.append(f"review coverage mismatch: missing={sorted(set(jobs_by_id)-set(evidence))}, extra={sorted(set(evidence)-set(jobs_by_id))}")
    for jid,job in jobs_by_id.items():
        a=assets.get(job.get("asset"))
        if not a: failures.append(f"{jid}: asset absent from manifest");continue
        image=pack / a["file"]
        if not image.is_file(): failures.append(f"{jid}: missing final image {a['file']}");continue
        actual_hash=digest(image);checks=evidence.get(jid)
        if not isinstance(checks,list) or len(checks)!=len(reviewers):
            failures.append(f"{jid}: expected exactly {len(reviewers)} reviewer reports");continue
        seen=set();scores_all=[]
        for check in checks:
            if not isinstance(check,dict): failures.append(f"{jid}: malformed review entry");continue
            rid=check.get("reviewer_id")
            if rid not in ids or rid in seen: failures.append(f"{jid}: unknown or duplicate reviewer {rid!r}");continue
            seen.add(rid)
            if check.get("image_sha256")!=actual_hash: failures.append(f"{jid}/{rid}: reviewed image hash mismatch")
            scores=check.get("scores")
            if not isinstance(scores,dict) or set(scores)!=set(CRITERIA):
                failures.append(f"{jid}/{rid}: score rubric incomplete or contains unknown criteria");continue
            if any(type(v)!=int or not 1<=v<=10 for v in scores.values()):
                failures.append(f"{jid}/{rid}: scores must be integer 1..10");continue
            blockers=check.get("blockers")
            improvements=check.get("improvements")
            if not isinstance(blockers,list) or not isinstance(improvements,list):
                failures.append(f"{jid}/{rid}: blockers and improvements must be arrays");continue
            if check.get("verdict")!="approve" or blockers or improvements:
                failures.append(f"{jid}/{rid}: reviewer did not approve cleanly")
            if type(check.get("confidence")) not in (int,float) or check["confidence"]<0.95 or check["confidence"]>1:
                failures.append(f"{jid}/{rid}: confidence must be in [0.95,1.0]")
            if not isinstance(check.get("rationale"),str) or len(check["rationale"].strip())<80:
                failures.append(f"{jid}/{rid}: rationale must contain at least 80 characters")
            scores_all.append(scores)
        if seen!=ids: failures.append(f"{jid}: missing reviewer reports {sorted(ids-seen)}")
        if len(scores_all)==len(reviewers):
            for rid,check in ((x["reviewer_id"],next((z for z in checks if z.get("reviewer_id")==x["reviewer_id"]),{})) for x in reviewers):
                sc=check.get("scores",{})
                low=[k for k in CRITERIA if type(sc.get(k)) is not int or sc[k]!=10]
                if low: failures.append(f"{jid}/{rid}: not 10/10 in {', '.join(low)}")
            rows.append({"job_id":jid,"asset":job["asset"],"image_sha256":actual_hash,"reviewers":len(seen),"score":"10/10" if not any(f.startswith(jid) for f in failures) else "REJECT"})
    report={"tool":"HUD Artisan review gate","schema_version":1,"manifest_sha256":manifest_hash,
        "jobs_sha256":jobs_hash,"asset_count":len(jobs_by_id),"reviewed_count":len(rows),
        "reviewers":[{"reviewer_id":x["reviewer_id"],"model":x["model"],"role":x["role"]} for x in reviewers],
        "criteria":list(CRITERIA),"result":"PASS" if not failures and len(rows)==len(jobs_by_id) else "REJECT",
        "failures":failures,"assets":rows}
    return report
def main():
    p=argparse.ArgumentParser(description="Verify independent 10/10 visual review evidence bound to exact final PNG hashes")
    for arg in ("manifest","jobs","pack","reviews","out"): p.add_argument("--"+arg,required=True,type=Path)
    a=p.parse_args()
    try:
        result=evaluate(a.manifest,a.jobs,a.pack,a.reviews)
        a.out.parent.mkdir(parents=True,exist_ok=True)
        tmp=a.out.with_name(a.out.name+".tmp")
        tmp.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        tmp.replace(a.out)
        print(f"HUD_ARTISAN_REVIEW {result['result']} assets={result['reviewed_count']}/{result['asset_count']} failures={len(result['failures'])} report={a.out}")
        return 0 if result["result"]=="PASS" else 1
    except (ValueError,OSError,KeyError,TypeError) as e:
        print(f"HUD_ARTISAN_REVIEW_ERROR {e}",file=sys.stderr);return 2
if __name__=="__main__": raise SystemExit(main())
