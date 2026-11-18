#!/usr/bin/env python3
"""HUD Artisan: deterministic clean-room RGBA asset generation from the canonical manifest.

Python standard library only. The manifest remains the schema authority.
"""
from __future__ import annotations
import argparse, hashlib, json, math, random, shutil, struct, zlib
from pathlib import Path

VERSION = "0.1.0"
PALETTE = {
    "gold": (190, 126, 38), "gold_hi": (255, 211, 111), "gold_lo": (76, 43, 17),
    "iron": (75, 77, 73), "iron_hi": (164, 157, 132), "stone": (54, 49, 43),
    "stone_hi": (104, 91, 72), "green": (20, 91, 53), "green_hi": (71, 174, 99),
    "red": (126, 35, 27), "blue": (35, 72, 119), "ink": (12, 12, 13),
}
def clamp(v): return max(0, min(255, int(v)))
def mix(a,b,t): return tuple(clamp(x+(y-x)*t) for x,y in zip(a,b))
def png_chunk(tag,data):
    body=tag+data
    return struct.pack(">I",len(data))+body+struct.pack(">I",zlib.crc32(body)&0xffffffff)

class Raster:
    def __init__(self,w,h):
        self.w,self.h=w,h
        self.p=bytearray(w*h*4)
    def blend(self,x,y,c,a=255):
        if x<0 or y<0 or x>=self.w or y>=self.h: return
        i=(y*self.w+x)*4; a=clamp(a); ia=255-a
        self.p[i]=clamp((c[0]*a+self.p[i]*ia)/255)
        self.p[i+1]=clamp((c[1]*a+self.p[i+1]*ia)/255)
        self.p[i+2]=clamp((c[2]*a+self.p[i+2]*ia)/255)
        self.p[i+3]=clamp(a+self.p[i+3]*ia/255)
    def rect(self,x,y,w,h,c,a=255):
        for yy in range(max(0,int(y)),min(self.h,int(y+h))):
            for xx in range(max(0,int(x)),min(self.w,int(x+w))): self.blend(xx,yy,c,a)
    def grad(self,x,y,w,h,top,bottom,alpha=255):
        if w<=0 or h<=0:return
        for yy in range(max(0,int(y)),min(self.h,int(y+h))):
            t=(yy-y)/max(1,h-1); col=mix(top,bottom,t)
            self.rect(x,yy,w,1,col,alpha)
    def line(self,x0,y0,x1,y1,c,width=1,a=255):
        dx=x1-x0;dy=y1-y0;n=max(abs(int(dx)),abs(int(dy)),1)
        r=max(0,int(width)//2)
        for k in range(n+1):
            x=round(x0+dx*k/n);y=round(y0+dy*k/n)
            for yy in range(y-r,y+r+1):
                for xx in range(x-r,x+r+1): self.blend(xx,yy,c,a)
    def poly(self,pts,c,a=255):
        ys=[p[1] for p in pts]; lo=max(0,int(min(ys)));hi=min(self.h-1,int(max(ys)))
        for y in range(lo,hi+1):
            xs=[]
            for i,(x1,y1) in enumerate(pts):
                x2,y2=pts[(i+1)%len(pts)]
                if y1==y2: continue
                if min(y1,y2)<=y<max(y1,y2): xs.append(x1+(y-y1)*(x2-x1)/(y2-y1))
            xs.sort()
            for j in range(0,len(xs)-1,2):
                for x in range(max(0,int(xs[j])),min(self.w,int(xs[j+1])+1)): self.blend(x,y,c,a)
    def ellipse(self,cx,cy,rx,ry,col,a=255):
        if rx<=0 or ry<=0:return
        for y in range(max(0,int(cy-ry)),min(self.h,int(cy+ry+1))):
            for x in range(max(0,int(cx-rx)),min(self.w,int(cx+rx+1))):
                if ((x-cx)/rx)**2+((y-cy)/ry)**2<=1:self.blend(x,y,col,a)
    def frame(self,x,y,w,h,seed,thick=None):
        t=thick or max(2,min(w,h)//18)
        # Cast shadow, dark forged backing, bevel and bright worn lip.
        self.rect(x+2,y+3,w,h,(0,0,0),180)
        self.grad(x,y,w,h,PALETTE["gold_hi"],PALETTE["gold_lo"])
        self.rect(x+t,y+t,w-2*t,h-2*t,PALETTE["iron"])
        self.grad(x+t+1,y+t+1,w-2*t-2,h-2*t-2,(58,53,46),(19,18,17))
        self.rect(x+t,y+t,w-2*t,1,PALETTE["gold_hi"],180)
        self.rect(x+t,y+h-t-1,w-2*t,1,PALETTE["gold_lo"])
        self.rect(x+t,y+t,1,h-2*t,PALETTE["gold_hi"],120)
        self.rect(x+w-t-1,y+t,1,h-2*t,PALETTE["gold_lo"])
        # Corner rivets and inset glints.
        for px,py in ((x+t,y+t),(x+w-t-1,y+t),(x+t,y+h-t-1),(x+w-t-1,y+h-t-1)):
            self.ellipse(px,py,max(1,t//3),max(1,t//3),PALETTE["gold_hi"])
            self.ellipse(px,py,max(1,t//6),max(1,t//6),PALETTE["ink"])
        # Small deterministic hammered marks, restrained to the frame.
        rng=random.Random(seed)
        for _ in range(max(8,(w+h)//5)):
            side=rng.randrange(4); q=rng.random()
            if side<2:
                px=int(x+q*w);py=y+(t//2 if side==0 else h-t//2)
            else:
                px=x+(t//2 if side==2 else w-t//2);py=int(y+q*h)
            self.blend(px,py,PALETTE["gold_hi"],rng.randrange(18,75))
    def gem(self,cx,cy,r):
        self.poly([(cx,cy-r),(cx+r,cy-r//3),(cx+r//2,cy+r),(cx-r//2,cy+r),(cx-r,cy-r//3)],PALETTE["green"])
        self.line(cx,cy-r,cx-r//2,cy+r,PALETTE["green_hi"],max(1,r//5))
        self.line(cx,cy-r,cx+r,cy-r//3,PALETTE["gold_hi"],1)
    def texture(self,seed,amount=250):
        rng=random.Random(seed)
        for _ in range(min(amount,self.w*self.h//2)):
            x=rng.randrange(self.w);y=rng.randrange(self.h)
            i=(y*self.w+x)*4
            if self.p[i+3]:
                d=rng.choice((-13,-8,-4,4,7,11))
                self.p[i]=clamp(self.p[i]+d);self.p[i+1]=clamp(self.p[i+1]+d);self.p[i+2]=clamp(self.p[i+2]+d)
    def save(self,path):
        raw=b"".join(b"\0"+bytes(self.p[y*self.w*4:(y+1)*self.w*4]) for y in range(self.h))
        data=b"\x89PNG\r\n\x1a\n"+png_chunk(b"IHDR",struct.pack(">IIBBBBB",self.w,self.h,8,6,0,0,0))+png_chunk(b"IDAT",zlib.compress(raw,9))+png_chunk(b"IEND",b"")
        path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)

def glyph(r,name,cx,cy,size,seed):
    s=max(3,size//2); gold=PALETTE["gold_hi"]; white=(205,205,190)
    n=name.lower()
    if any(k in n for k in ("gold","coin","resource")):
        r.ellipse(cx,cy,s,s,PALETTE["gold"]);r.ellipse(cx,cy,s-3,s-3,gold);r.ellipse(cx,cy,s-6,s-6,PALETTE["gold_lo"]);r.ellipse(cx,cy,s-8,s-8,PALETTE["gold"])
        r.line(cx-s//2,cy,cx+s//2,cy,gold,max(2,s//5))
    elif "wood" in n or "lumber" in n:
        for j in range(3):
            yy=cy-s//2+j*s//3;r.line(cx-s,yy,cx+s,yy+2,PALETTE["gold"],max(3,s//4));r.ellipse(cx-s,yy,3,3,gold)
    elif any(k in n for k in ("food","meat","supply")):
        r.ellipse(cx,cy,s//2,s,PALETTE["red"]);r.line(cx,cy-s,cx+s//2,cy-s//2,PALETTE["green_hi"],2)
    elif any(k in n for k in ("attack","sword","damage","weapon")):
        r.poly([(cx-s,cy+s//2),(cx+s//2,cy-s),(cx+s,cy-s//2),(cx-s//2,cy+s)],white)
        r.line(cx-s,cy+s,cx+s,cy-s,gold,max(2,s//5))
    elif any(k in n for k in ("armor","shield","defense")):
        r.poly([(cx-s,cy-s),(cx+s,cy-s),(cx+s//2,cy+s//2),(cx,cy+s),(cx-s//2,cy+s//2)],PALETTE["blue"])
        r.line(cx,cy-s,cx,cy+s,white,max(2,s//6))
    elif any(k in n for k in ("health","life","heal")):
        r.ellipse(cx-s//2,cy-s//3,s//2,s//2,PALETTE["red"]);r.ellipse(cx+s//2,cy-s//3,s//2,s//2,PALETTE["red"]);r.poly([(cx-s,cy-s//4),(cx+s,cy-s//4),(cx,cy+s)],PALETTE["red"])
    elif any(k in n for k in ("mana","magic","spell","rune")):
        r.poly([(cx,cy-s),(cx+s//2,cy),(cx,cy+s),(cx-s//2,cy)],PALETTE["blue"]);r.ellipse(cx,cy,s//4,s//4,PALETTE["green_hi"])
    elif any(k in n for k in ("move","arrow","rally")):
        r.poly([(cx-s,cy-s//3),(cx+s//3,cy-s//3),(cx+s//3,cy-s),(cx+s,cy),(cx+s//3,cy+s),(cx+s//3,cy+s//3),(cx-s,cy+s//3)],gold)
    elif any(k in n for k in ("close","cancel","stop")):
        r.line(cx-s,cy-s,cx+s,cy+s,PALETTE["red"],max(3,s//3));r.line(cx+s,cy-s,cx-s,cy+s,PALETTE["red"],max(3,s//3))
    else:
        # Unique seed-derived rune silhouette; never a text glyph.
        rng=random.Random(seed); sides=5+rng.randrange(4); pts=[]
        for i in range(sides):
            a=-math.pi/2+2*math.pi*i/sides;rad=s*(0.65 if i%2 else 1)
            pts.append((cx+math.cos(a)*rad,cy+math.sin(a)*rad))
        r.poly(pts,PALETTE["gold"]);r.ellipse(cx,cy,max(2,s//4),max(2,s//4),PALETTE["green_hi"])

def render(asset,theme,seed):
    name=asset["name"];w=asset["w"];h=asset["h"];low=name.lower()
    r=Raster(w,h);rng=random.Random(seed)
    # Transparent canvas; panel families get full alpha, icons stay isolated.
    icon=low.startswith(("icon_","ability_","item_")) or "glyph" in low
    if icon:
        pad=max(1,min(w,h)//14);r.ellipse(w//2+1,h//2+2,min(w,h)//2-pad,min(w,h)//2-pad,(0,0,0),150)
        r.ellipse(w//2,h//2,min(w,h)//2-pad,min(w,h)//2-pad,PALETTE["gold_lo"])
        r.ellipse(w//2,h//2,min(w,h)//2-pad-3,min(w,h)//2-pad-3,PALETTE["iron"])
        glyph(r,name,w//2,h//2,max(6,min(w,h)//2-7),seed)
    else:
        t=max(2,min(w,h)//22)
        r.frame(0,0,w,h,seed,t)
        # Interior dark material: smoky parchment for menus, charcoal for HUD.
        inset=t+2
        if any(k in low for k in ("menu","modal","dialog","tooltip","loading","save","campaign")):
            r.grad(inset,inset,w-2*inset,h-2*inset,(71,57,40),(24,21,18))
        else:
            r.grad(inset,inset,w-2*inset,h-2*inset,(38,37,34),(12,13,14))
        # Distinct semantic decoration.
        if any(k in low for k in ("portrait","hero","unit")):
            r.gem(w//2,t+max(3,t//2),max(3,t))
        elif any(k in low for k in ("resource","topbar","console")):
            r.gem(w//2,t+max(3,t//2),max(3,t))
            r.poly([(w//2-8,h-1),(w//2,h-max(4,t*2)),(w//2+8,h-1)],PALETTE["green"])
        elif any(k in low for k in ("slot","button","command","tab")):
            r.rect(inset+2,inset+2,max(1,w-2*inset-4),max(1,h-2*inset-4),PALETTE["ink"],80)
            r.line(inset+3,inset+3,w-inset-4,inset+3,PALETTE["gold_hi"],80)
        if "minimap" in low:
            # Abstract terrain relief texture; no copied map data.
            for _ in range(max(20,w*h//90)):
                x=rng.randrange(inset,max(inset+1,w-inset));y=rng.randrange(inset,max(inset+1,h-inset))
                r.ellipse(x,y,rng.randrange(1,5),rng.randrange(1,5),rng.choice([PALETTE["green"],PALETTE["stone_hi"],PALETTE["blue"]]),130)
        r.texture(seed,max(20,(w+h)//2))
    return r

def main():
    ap=argparse.ArgumentParser(description="Deterministic clean-room HUD asset generator")
    ap.add_argument("--manifest",required=True,type=Path)
    ap.add_argument("--out",required=True,type=Path)
    ap.add_argument("--theme",choices=("classic-modern","orc-iron","human-gold","undead-obsidian"),default="classic-modern")
    ap.add_argument("--seed",type=int,default=20261118)
    ap.add_argument("--limit",type=int,default=0,help="debug only: generate first N assets")
    args=ap.parse_args()
    source=args.manifest.resolve();doc=json.loads(source.read_text(encoding="utf-8"))
    assets=doc.get("assets")
    if not isinstance(assets,list) or not assets: raise SystemExit("manifest has no assets[]")
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
    shutil.copy2(source,out/"manifest.json")
    hashes=[];generated=[]
    for index,a in enumerate(assets):
        if args.limit and index>=args.limit:break
        for key in ("name","file","w","h"):
            if key not in a:raise SystemExit(f"asset[{index}] missing {key}")
        if not isinstance(a["w"],int) or not isinstance(a["h"],int) or a["w"]<1 or a["h"]<1 or a["w"]*a["h"]>64_000_000:
            raise SystemExit(f"unsafe dimensions for {a['name']}: {a['w']}x{a['h']}")
        seed=int(hashlib.sha256(f"{args.seed}:{a['name']}".encode()).hexdigest()[:8],16)
        dest=out/a["file"]
        render(a,args.theme,seed).save(dest)
        digest=hashlib.sha256(dest.read_bytes()).hexdigest()
        hashes.append({"name":a["name"],"file":a["file"],"sha256":digest,"width":a["w"],"height":a["h"]})
        generated.append(a["name"])
    report={"tool":"HUD Artisan","version":VERSION,"theme":args.theme,"seed":args.seed,
            "source_manifest_sha256":hashlib.sha256(source.read_bytes()).hexdigest(),
            "generated_count":len(generated),"generated":hashes}
    reports=out/"reports";reports.mkdir(exist_ok=True)
    (reports/"generation.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(f"HUD_ARTISAN_OK assets={len(generated)} theme={args.theme} seed={args.seed} out={out}")
if __name__=="__main__":main()
