#!/usr/bin/env python3
"""HUD Artisan tests: python -m unittest discover -s tools/hud_artisan -p 'test_*.py'"""
import importlib.util,json,tempfile,unittest,struct,zlib
from pathlib import Path
SPEC=importlib.util.spec_from_file_location("artisan",Path(__file__).with_name("generate.py"))
artisan=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(artisan)
def png(w,h,rgba):
    def chunk(t,b):
        x=t+b;return struct.pack(">I",len(b))+x+struct.pack(">I",zlib.crc32(x)&0xffffffff)
    raw=b"".join(b"\0"+bytes(rgba*w) for _ in range(h))
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,6,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")
class Tests(unittest.TestCase):
    def test_png_parse(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";p.write_bytes(png(17,11,(1,2,3,255)))
            self.assertEqual(artisan.png_info(p),(17,11,8,6))
    def test_crc_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";b=bytearray(png(8,8,(1,2,3,255)));b[29]^=1;p.write_bytes(b)
            with self.assertRaises(ValueError):artisan.png_info(p)
    def test_traversal_rejected(self):
        for x in ("../x.png","/tmp/x.png",""): 
            with self.assertRaises(ValueError):artisan.safe_rel(x)
    def test_semantic_prompt(self):
        a={"name":"icon_gold_normal_32","file":"icons/gold.png","w":32,"h":32,"insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        j=artisan.make_job(a,"classic-modern",9);self.assertTrue(j["requires_alpha"]);self.assertIn("9-slice",j["prompt"])
    def test_engine_roles(self):self.assertEqual(len(artisan.ENGINE_ROLES),24)
    def test_manifest_and_plan(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            a={"name":"panel","file":"panels/panel.png","w":64,"h":64,"insets":{"l":8,"t":8,"r":8,"b":8},"states":["normal"]}
            at={"name":"atlas","file":"atlas/cmd.png","w":320,"h":256,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":5,"rows":4,"cell":[64,64],"entries":[{"id":"a","x":0,"y":0,"w":64,"h":64}]}},
             "assets":[a,at],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc));self.assertEqual(len(artisan.load_manifest(m)["assets"]),2)
            out=root/"plan";self.assertEqual(artisan.plan(m,out,"classic-modern",12,False),0)
            self.assertEqual(json.loads((out/"jobs.json").read_text())["job_count"],2)
    def test_ingest_assembles_atlas_from_separate_cells(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);manifest=root/"manifest.json";drop=root/"drop";out=root/"pack";drop.mkdir()
            panel={"name":"panel","file":"panels/panel.png","w":16,"h":16,"insets":{"l":2,"t":2,"r":2,"b":2},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/cmd.png","w":16,"h":16,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":1,"rows":1,
             "cell":[8,8],"entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            manifest.write_text(json.dumps(doc))
            jobs=root/"jobs";artisan.plan(manifest,jobs,"classic-modern",3,False)
            payload=json.loads((jobs/"jobs.json").read_text())
            self.assertEqual(payload["job_count"],2)
            (drop/"panels").mkdir();Image.new("RGBA",(16,16),(40,35,30,255)).save(drop/"panels/panel.png")
            cell=Image.new("RGBA",(8,8),(0,0,0,0))
            for x in range(2,6):
                for y in range(2,6):cell.putpixel((x,y),(220,150,40,255))
            (drop/"atlas_sources").mkdir();cell.save(drop/"atlas_sources/icon_a.png")
            (drop/"fonts").mkdir()
            for name in doc["font_files"]:(drop/"fonts"/name).write_bytes(b"font-dependency-placeholder")
            rc=artisan.ingest(manifest,jobs/"jobs.json",drop,out,"strict",False)
            self.assertEqual(rc,0)
            self.assertEqual(artisan.png_info(out/"atlas/cmd.png")[:2],(16,16))
            self.assertEqual(artisan.validate(manifest,out,False,False),0)
    def test_duplicate_name_path_guard(self):
        with tempfile.TemporaryDirectory() as d:
            m=Path(d)/"m.json";a={"name":"a","file":"same.png","w":1,"h":1,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":1,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
             "atlas":{"commandbuttons":{"entries":[{"id":"x"}]}},"assets":[a,{**a,"name":"b"}],"font_files":["a","b","c","d","e"]}
            m.write_text(json.dumps(doc))
            with self.assertRaises(ValueError):artisan.load_manifest(m)
if __name__=="__main__":unittest.main()
