#!/usr/bin/env python3
"""HUD Artisan tests: python -m unittest discover -s tools/hud_artisan -p 'test_*.py'"""
import importlib.util,json,tempfile,unittest,struct,zlib,hashlib,sys,os
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
    def test_job_identity_tracks_prompt_revision(self):
        asset={"name":"icon_fire_spell_32","file":"icons/fire.png","w":32,"h":32,
               "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        first=artisan.make_job(asset,"classic-modern",11)
        repeated=artisan.make_job(asset,"classic-modern",11)
        changed=artisan.make_job(asset,"undead-obsidian",11)
        self.assertEqual(first["job_id"],repeated["job_id"])
        self.assertEqual(first["prompt_sha256"],repeated["prompt_sha256"])
        self.assertNotEqual(first["job_id"],changed["job_id"])
        self.assertNotEqual(first["prompt_sha256"],changed["prompt_sha256"])
    def test_semantic_subjects_and_state_language(self):
        icon={"name":"icon_frost_spell_32","file":"icons/frost.png","w":32,"h":32,
              "insets":{"l":0,"t":0,"r":0,"b":0},"states":["hover","disabled"]}
        job=artisan.make_job(icon,"classic-modern",17)
        self.assertIn("frost crystal",job["prompt"])
        self.assertIn("hover state",job["prompt"])
        self.assertIn("disabled state",job["prompt"])
        self.assertNotIn("generic placeholder rune",job["prompt"])
    def test_9slice_regions_are_in_prompt(self):
        a={"name":"menu_panel_normal","file":"panel.png","w":512,"h":384,
           "insets":{"l":24,"t":32,"r":24,"b":32},"states":["normal"]}
        p=artisan.make_job(a,"classic-modern",1)["prompt"]
        for token in ("L24","T32","R24","B32","central field calm"):
            self.assertIn(token,p)
    def test_inactive_state_does_not_receive_active_treatment(self):
        a={"name":"control_group_button_inactive_44","file":"groups/inactive.png","w":44,"h":44,
           "insets":{"l":2,"t":2,"r":2,"b":2},"states":["inactive"]}
        prompt=artisan.make_job(a,"classic-modern",3)["prompt"]
        self.assertIn("inactive state",prompt)
        self.assertNotIn("active state: stronger",prompt)
    def test_engine_contract_exact_manifest(self):
        assets=[]
        for i,name in enumerate(artisan.ENGINE_ROLES):
            assets.append({"name":name,"file":f"assets/{i}.png","w":64,"h":64})
        assets.append({"name":"commandbuttons_atlas","file":"atlas/commandbuttons.png","w":2048,"h":2048})
        for i in range(67):
            assets.append({"name":f"extra_{i}","file":f"extra/{i}.png","w":64,"h":64})
        doc={"design_height":768,"reference_resolution":{"landscape":[2414,1080],"portrait":[1080,2414]},
          "safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},
          "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
          "control_groups":{"count":10,"cell":[44,44]},
          "assets":assets,"atlas":{"commandbuttons":{"file":"atlas/commandbuttons.png","entries":[{} for _ in range(20)]}}}
        self.assertEqual(len(assets),92)
        artisan.validate_engine_contract(doc)
        doc["atlas"]["commandbuttons"]["entries"].pop()
        with self.assertRaisesRegex(ValueError,"20 entries"):
            artisan.validate_engine_contract(doc)
    def test_font_path_traversal_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            m=Path(d)/"m.json"
            a={"name":"a","file":"a.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            atlas={"name":"atlas","file":"atlas.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
              "atlas":{"commandbuttons":{"file":"atlas.png","entries":[{"id":"a","x":0,"y":0,"w":4,"h":4}]}},
              "assets":[a,atlas],"font_files":["../evil.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"safe basenames"):
                artisan.load_manifest(m)
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
            plan_doc,manifest_doc=artisan.verify_plan(m,out/"jobs.json")
            self.assertEqual(plan_doc["source_manifest_sha256"],hashlib.sha256(m.read_bytes()).hexdigest())
            tampered=json.loads((out/"jobs.json").read_text())
            tampered["atlas"]["cells"][0]["x"]=1
            (out/"jobs.json").write_text(json.dumps(tampered))
            with self.assertRaisesRegex(ValueError,"atlas geometry"):
                artisan.verify_plan(m,out/"jobs.json")
            artisan.plan(m,out,"classic-modern",12,False)
            doc["atlas"]["commandbuttons"]["entries"].append({"id":"overlap","x":0,"y":0,"w":64,"h":64})
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"overlap"):
                artisan.plan(m,root/"bad-plan","classic-modern",12,False)
    def test_provider_dispatch_ingest_atlas_and_provenance(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);manifest=root/"manifest.json";drop=root/"drop";out=root/"pack"
            panel={"name":"panel","file":"panels/panel.png","w":16,"h":16,"insets":{"l":2,"t":2,"r":2,"b":2},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/cmd.png","w":16,"h":16,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":1,"rows":1,
             "cell":[8,8],"entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            manifest.write_text(json.dumps(doc))
            jobs_dir=root/"jobs";artisan.plan(manifest,jobs_dir,"classic-modern",3,False)
            jobs=jobs_dir/"jobs.json";payload=json.loads(jobs.read_text())
            self.assertEqual(payload["job_count"],2)
            adapter_script=root/"provider_adapter.py"
            adapter_script.write_text(
                "import json,sys,os\nfrom PIL import Image,ImageDraw\n"
                "print(os.environ.get('HUD_ARTISAN_TEST_SECRET',''))\n"
                "j=json.load(open(sys.argv[1],encoding='utf-8'))\n"
                "w,h=j['width'],j['height']\n"
                "im=Image.new('RGBA',(w,h),(0,0,0,0) if j['requires_alpha'] else (40,35,30,255))\n"
                "d=ImageDraw.Draw(im)\n"
                "if j['requires_alpha']: d.ellipse((1,1,max(1,w-2),max(1,h-2)),fill=(220,150,40,255))\n"
                "else: d.line((0,0,w-1,h-1),fill=(90,80,65,255),width=1)\n"
                "im.save(sys.argv[2],format='PNG')\n",encoding="utf-8")
            adapter=root/"adapter.json"
            adapter.write_text(json.dumps({"provider":"test-provider","model":"test-model",
              "license":"clean-room test fixture","working_directory":str(root),
              "command":[sys.executable,str(adapter_script),"{job_json}","{output_path}"],
              "environment":["HUD_ARTISAN_TEST_SECRET"],"timeout_seconds":20,"retries":1,"max_parallel":2}))
            os.environ["HUD_ARTISAN_TEST_SECRET"]="TOP-SECRET-DO-NOT-LOG"
            try:
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2),0)
            finally:
                os.environ.pop("HUD_ARTISAN_TEST_SECRET",None)
            self.assertTrue((drop/"provenance.json").is_file())
            logs=list((drop/"reports"/"worker").glob("*.log"))
            self.assertTrue(logs)
            combined="".join(path.read_text() for path in logs)
            self.assertIn("[REDACTED]",combined)
            self.assertNotIn("TOP-SECRET-DO-NOT-LOG",combined)
            (drop/"fonts").mkdir()
            for name in doc["font_files"]:(drop/"fonts"/name).write_bytes(b"font-dependency-placeholder")
            self.assertEqual(artisan.ingest(manifest,jobs,drop,out,"strict",False),0)
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
