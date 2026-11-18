#!/usr/bin/env python3
"""HUD Artisan tests: python -m unittest discover -s tools/hud_artisan -p 'test_*.py'"""
import importlib.util,json,tempfile,unittest,struct,zlib,hashlib,sys,os,io,threading
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import urlsplit,parse_qs
from jsonschema import Draft202012Validator, RefResolver
from pathlib import Path
SPEC=importlib.util.spec_from_file_location("artisan",Path(__file__).with_name("generate.py"))
artisan=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(artisan)
def png(w,h,rgba):
    def chunk(t,b):
        x=t+b;return struct.pack(">I",len(b))+x+struct.pack(">I",zlib.crc32(x)&0xffffffff)
    raw=b"".join(b"\0"+bytes(rgba*w) for _ in range(h))
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,6,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")
class Tests(unittest.TestCase):
    def test_published_adapter_schemas_validate_examples(self):
        base=Path(__file__).parent
        pairs=[
            ("adapter.schema.json","adapter.example.json"),
            ("comfyui.schema.json","comfyui.example.json"),
            ("job.schema.json",None),
            ("provenance.schema.json",None),
            ("plan.schema.json",None),
        ]
        for schema_name,example_name in pairs:
            schema=json.loads((base/schema_name).read_text())
            Draft202012Validator.check_schema(schema)
            if example_name:
                example=json.loads((base/example_name).read_text())
                Draft202012Validator(schema).validate(example)
    def test_png_parse(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";p.write_bytes(png(17,11,(1,2,3,255)))
            self.assertEqual(artisan.png_info(p),(17,11,8,6))
    def test_crc_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";b=bytearray(png(8,8,(1,2,3,255)));b[29]^=1;p.write_bytes(b)
            with self.assertRaises(ValueError):artisan.png_info(p)
    def test_png_rejects_misordered_ihdr(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"bad.png";good=png(8,8,(1,2,3,255));pos=8;chunks=[]
            while pos<len(good):
                n=struct.unpack(">I",good[pos:pos+4])[0]
                end=pos+12+n;chunks.append(good[pos:end]);pos=end
            p.write_bytes(good[:8]+chunks[1]+chunks[0]+chunks[2])
            with self.assertRaisesRegex(ValueError,"IHDR must be the first"):
                artisan.png_info(p)
    def test_atlas_cell_can_reuse_declared_manifest_asset(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            icon={"name":"icon_a","file":"icons/a.png","w":8,"h":8,
                  "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/commands.png","w":16,"h":16,
                   "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},
             "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},
             "atlas":{"commandbuttons":{"file":"atlas/commands.png","entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[icon,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc));out=root/"plan"
            self.assertEqual(artisan.plan(m,out,"classic-modern",7,False),0)
            payload=json.loads((out/"jobs.json").read_text())
            self.assertEqual(payload["job_count"],1)
            self.assertEqual(payload["atlas"]["cells"][0]["source_file"],"icons/a.png")
            self.assertEqual(payload["atlas"]["cells"][0]["source_asset"],"icon_a")
            artisan.verify_plan(m,out/"jobs.json")
    def test_atlas_rejects_mismatched_source_dimensions_and_self_reference(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            icon={"name":"icon_a","file":"icons/a.png","w":16,"h":16,
                  "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/commands.png","w":32,"h":32,
                   "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            def document(source,cell_w=8,cell_h=8):
                return {"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
                 "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},
                 "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
                 "control_groups":{"count":10,"cell":[44,44]},
                 "atlas":{"commandbuttons":{"file":"atlas/commands.png","entries":[
                    {"id":"cell_a","source_asset":source,"x":0,"y":0,"w":cell_w,"h":cell_h}]}},
                 "assets":[icon,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(document("icon_a")))
            with self.assertRaisesRegex(ValueError,"source asset icon_a is 16x16"):
                artisan.plan(m,root/"plan-bad-size","classic-modern",2,False)
            m.write_text(json.dumps(document("atlas")))
            with self.assertRaisesRegex(ValueError,"atlas cannot use itself"):
                artisan.plan(m,root/"plan-self","classic-modern",2,False)
    def test_symlink_alias_rejected_even_when_target_is_inside_root(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);target=root/"real.png";alias=root/"alias.png"
            target.write_bytes(b"x")
            try:
                alias.symlink_to(target)
            except (OSError,NotImplementedError):
                self.skipTest("symlink creation is unavailable on this platform")
            with self.assertRaisesRegex(ValueError,"symlink path is not allowed"):
                artisan.safe_join(root,"alias.png")
            with self.assertRaisesRegex(ValueError,"symlink path is not allowed"):
                artisan.safe_join_any(root,"alias.png")
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
    def test_manifest_reference_requires_existing_file_and_license(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            a={"name":"panel","file":"panel.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0},
               "references":[{"source":"reference.png","license":"CC0-1.0","purpose":"material guidance","file":"reference.png"}]}
            at={"name":"atlas","file":"atlas.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
              "atlas":{"commandbuttons":{"file":"atlas.png","entries":[{"id":"a","x":0,"y":0,"w":4,"h":4}]}},
              "assets":[a,at],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"reference 0 file does not exist"):
                artisan.load_manifest(m)
            (root/"reference.png").write_bytes(b"not decoded as an art image here")
            loaded=artisan.load_manifest(m)
            job=artisan.make_job(loaded["assets"][0],"classic-modern",1)
            self.assertEqual(job["references"],a["references"])
            self.assertIn("CC0-1.0",job["prompt"])
    def test_manifest_semantics_override_filename_heuristics(self):
        asset={"name":"ability_q_custom_32","file":"icons/custom.png","w":32,"h":32,
               "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"],
               "semantic":"a three-pronged frost spear wrapping around a crimson heart",
               "faction":"northern frost cult","visual_tags":["ice","piercing","healing"]}
        prompt=artisan.make_job(asset,"classic-modern",9)["prompt"]
        self.assertIn("three-pronged frost spear",prompt)
        self.assertIn("northern frost cult",prompt)
        self.assertIn("piercing",prompt)
        self.assertNotIn("generic placeholder rune",prompt)
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
            w,h=artisan.ENGINE_ROLE_DIMENSIONS[name]
            assets.append({"name":name,"file":f"assets/{i}.png","w":w,"h":h})
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
        wrong=next(a for a in assets if a["name"]=="button_glow_64")
        wrong["w"]=63
        with self.assertRaisesRegex(ValueError,"button_glow_64 must be 64x64"):
            artisan.validate_engine_contract(doc)
        wrong["w"]=64
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
            plan_data=json.loads((out/"jobs.json").read_text())
            self.assertEqual(plan_data["job_count"],2)
            plan_schema=json.loads((Path(__file__).with_name("plan.schema.json")).read_text())
            job_schema=json.loads((Path(__file__).with_name("job.schema.json")).read_text())
            resolver=RefResolver.from_schema(plan_schema,store={job_schema["$id"]:job_schema})
            Draft202012Validator(plan_schema,resolver=resolver).validate(plan_data)
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
            adapter.write_text(json.dumps({"schema_version":1,"provider":"test-provider","model":"test-model",
              "license":"clean-room test fixture","working_directory":str(root),
              "command":[sys.executable,str(adapter_script),"{job_json}","{output_path}"],
              "environment":["HUD_ARTISAN_TEST_SECRET"],"timeout_seconds":20,"retries":1,"max_parallel":2}))
            os.environ["HUD_ARTISAN_TEST_SECRET"]="TOP-SECRET-DO-NOT-LOG"
            try:
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2),0)
            finally:
                os.environ.pop("HUD_ARTISAN_TEST_SECRET",None)
            self.assertTrue((drop/"provenance.json").is_file())
            provenance_schema=json.loads((Path(__file__).with_name("provenance.schema.json")).read_text())
            Draft202012Validator(provenance_schema).validate(json.loads((drop/"provenance.json").read_text()))
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
    def test_comfyui_adapter_end_to_end_against_fake_api(self):
        from PIL import Image
        from contextlib import redirect_stdout,redirect_stderr
        import importlib.util
        spec=importlib.util.spec_from_file_location("comfy_adapter",Path(__file__).with_name("comfyui_adapter.py"))
        comfy=importlib.util.module_from_spec(spec);spec.loader.exec_module(comfy)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);captured={}
            image=Image.new("RGBA",(8,8),(0,0,0,0))
            for y in range(2,6):
                for x in range(2,6):image.putpixel((x,y),(190,130,40,255))
            stream=io.BytesIO();image.save(stream,format="PNG");image_bytes=stream.getvalue()
            class Handler(BaseHTTPRequestHandler):
                def log_message(self,*args):pass
                def send_json(self,obj):
                    data=json.dumps(obj).encode()
                    self.send_response(200);self.send_header("Content-Type","application/json")
                    self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
                def do_POST(self):
                    n=int(self.headers.get("Content-Length","0"))
                    captured["request"]=json.loads(self.rfile.read(n))
                    self.send_json({"prompt_id":"fake-prompt-1"})
                def do_GET(self):
                    captured.setdefault("get_paths",[]).append(self.path)
                    path=urlsplit(self.path)
                    if path.path=="/history/fake-prompt-1":
                        return self.send_json({"fake-prompt-1":{"outputs":{"9":{"images":[
                            {"filename":"frame.png","subfolder":"","type":"output"}]}}}})
                    if path.path=="/view" and parse_qs(path.query).get("filename")==["frame.png"]:
                        self.send_response(200);self.send_header("Content-Type","image/png")
                        self.send_header("Content-Length",str(len(image_bytes)));self.end_headers();self.wfile.write(image_bytes)
                        return
                    self.send_error(404)
            server=ThreadingHTTPServer(("127.0.0.1",0),Handler)
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            try:
                workflow={"6":{"inputs":{"text":"old-positive"}},"7":{"inputs":{"text":"old-negative"}},
                  "5":{"inputs":{"width":512,"height":512}},"3":{"inputs":{"seed":1}},"9":{"inputs":{}}}
                (root/"workflow.json").write_text(json.dumps(workflow))
                config={"schema_version":1,"server":f"http://127.0.0.1:{server.server_port}","workflow_file":"workflow.json",
                  "node_inputs":{"positive_prompt":{"node_id":"6","input":"text"},
                    "negative_prompt":{"node_id":"7","input":"text"},"width":{"node_id":"5","input":"width"},
                    "height":{"node_id":"5","input":"height"},"seed":{"node_id":"3","input":"seed"}},
                  "output_node_id":"9","request_timeout_seconds":5,"max_wait_seconds":5,"poll_interval_seconds":0.2}
                config_path=root/"comfy.json";config_path.write_text(json.dumps(config))
                job={"job_id":"a1b2c3d4e5f60718","prompt":"a sculpted gold emblem","negative_prompt":"text, watermark",
                  "width":8,"height":8,"requires_alpha":True}
                job_path=root/"job.json";job_path.write_text(json.dumps(job))
                output=root/"out.png"
                stderr_buffer=io.StringIO()
                with redirect_stdout(io.StringIO()),redirect_stderr(stderr_buffer):
                    rc=comfy.main(["--config",str(config_path),"--job-json",str(job_path),"--output",str(output)])
                self.assertEqual(rc,0,stderr_buffer.getvalue()+" paths="+repr(captured.get("get_paths")))
                sent=captured["request"]["prompt"]
                self.assertEqual(sent["6"]["inputs"]["text"],job["prompt"])
                self.assertEqual(sent["7"]["inputs"]["text"],job["negative_prompt"])
                self.assertEqual(sent["5"]["inputs"]["width"],8)
                self.assertEqual(sent["5"]["inputs"]["height"],8)
                self.assertEqual(sent["3"]["inputs"]["seed"],int(job["job_id"][:8],16))
                self.assertEqual(artisan.png_info(output),(8,8,8,6))
            finally:
                server.shutdown();server.server_close();thread.join(timeout=5)
    def test_duplicate_name_path_guard(self):
        with tempfile.TemporaryDirectory() as d:
            m=Path(d)/"m.json";a={"name":"a","file":"same.png","w":1,"h":1,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":1,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
             "atlas":{"commandbuttons":{"entries":[{"id":"x"}]}},"assets":[a,{**a,"name":"b"}],"font_files":["a","b","c","d","e"]}
            m.write_text(json.dumps(doc))
            with self.assertRaises(ValueError):artisan.load_manifest(m)
if __name__=="__main__":unittest.main()
