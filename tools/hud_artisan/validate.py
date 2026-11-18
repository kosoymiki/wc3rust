#!/usr/bin/env python3
"""Validate generated HUD Artisan output without third-party packages."""
from __future__ import annotations
import argparse, hashlib, json, struct, sys, zlib
from pathlib import Path
SIG=b"\x89PNG\r\n\x1a\n"
def inspect_png(path: Path):
    data=path.read_bytes()
    if not data.startswith(SIG): raise ValueError("bad PNG signature")
    pos=8; width=height=None; idat=0; ended=False
    while pos<len(data):
        if pos+12>len(data): raise ValueError("truncated chunk header")
        n=struct.unpack(">I",data[pos:pos+4])[0]; tag=data[pos+4:pos+8]; end=pos+12+n
        if end>len(data): raise ValueError(f"truncated {tag!r} chunk")
        payload=data[pos+8:pos+8+n]; crc=struct.unpack(">I",data[pos+8+n:end])[0]
        if zlib.crc32(tag+payload)&0xffffffff != crc: raise ValueError(f"CRC mismatch in {tag!r}")
        if tag==b"IHDR":
            if n!=13 or width is not None: raise ValueError("invalid/duplicate IHDR")
            width,height,depth,color,comp,filt,interlace=struct.unpack(">IIBBBBB",payload)
            if depth!=8 or color!=6 or comp or filt or interlace: raise ValueError("expected non-interlaced RGBA8")
        elif tag==b"IDAT": idat+=n
        elif tag==b"IEND":
            if n: raise ValueError("IEND must be empty")
            ended=True
            if end!=len(data): raise ValueError("trailing bytes after IEND")
            break
        pos=end
    if width is None or not ended or not idat: raise ValueError("incomplete PNG")
    return width,height,len(data)
def main():
    p=argparse.ArgumentParser();p.add_argument("--pack",required=True,type=Path);a=p.parse_args()
    root=a.pack.resolve();mp=root/"manifest.json";errors=[];checked=[]
    try: doc=json.loads(mp.read_text(encoding="utf-8"))
    except Exception as e: print(f"HUD_ARTISAN_CHECK FAIL manifest: {e}");return 2
    assets=doc.get("assets")
    if not isinstance(assets,list) or not assets: errors.append("manifest assets[] missing")
    seen=set()
    for item in assets or []:
        name=item.get("name","<unnamed>");rel=item.get("file","")
        path=(root/rel).resolve()
        if not rel or not path.is_relative_to(root): errors.append(f"{name}: unsafe path {rel!r}");continue
        if rel in seen: errors.append(f"{name}: duplicate output path {rel}");continue
        seen.add(rel)
        if not path.is_file(): errors.append(f"{name}: missing {rel}");continue
        try:
            w,h,size=inspect_png(path)
            if (w,h)!=(item.get("w"),item.get("h")): errors.append(f"{name}: {w}x{h} != manifest {item.get('w')}x{item.get('h')}")
            checked.append({"name":name,"file":rel,"width":w,"height":h,"bytes":size,"sha256":hashlib.sha256(path.read_bytes()).hexdigest()})
        except Exception as e: errors.append(f"{name}: {e}")
        try:
            ins=item["insets"];l,t,r,b=(float(ins[k]) for k in ("l","t","r","b"))
            if min(l,t,r,b)<0 or l+r>=item["w"] or t+b>=item["h"]: errors.append(f"{name}: invalid insets {(l,t,r,b)}")
        except Exception: errors.append(f"{name}: malformed insets")
    report={"assets_declared":len(assets or []),"assets_checked":len(checked),"failures":errors,"files":checked}
    (root/"reports").mkdir(exist_ok=True);(root/"reports/validation.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    for e in errors: print("HUD_ARTISAN_CHECK FAIL "+e)
    print(f"HUD_ARTISAN_CHECK {'FAIL' if errors else 'PASS'} assets={len(checked)} failures={len(errors)}")
    return 1 if errors else 0
if __name__=="__main__":sys.exit(main())
