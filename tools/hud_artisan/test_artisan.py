#!/usr/bin/env python3
"""Fast deterministic PNG/primitive tests."""
import importlib.util, tempfile, unittest, struct, zlib
from pathlib import Path
SPEC=importlib.util.spec_from_file_location("artisan",Path(__file__).with_name("generate.py"))
artisan=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(artisan)
class ArtisanTests(unittest.TestCase):
    def test_png_header_and_dimensions(self):
        r=artisan.Raster(17,11);r.frame(0,0,17,11,4,2)
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";r.save(p);b=p.read_bytes()
            self.assertEqual(b[:8],b"\x89PNG\r\n\x1a\n")
            self.assertEqual(struct.unpack(">II",b[16:24]),(17,11))
            self.assertEqual(zlib.crc32(b[12:29])&0xffffffff,struct.unpack(">I",b[29:33])[0])
    def test_deterministic_bytes(self):
        a=artisan.Raster(32,32);b=artisan.Raster(32,32)
        a.frame(0,0,32,32,123);b.frame(0,0,32,32,123)
        self.assertEqual(a.p,b.p)
    def test_icon_alpha_is_not_empty(self):
        a={"name":"icon_gold_normal_32","w":32,"h":32}
        r=artisan.render(a,"classic-modern",7)
        self.assertGreater(sum(1 for i in range(3,len(r.p),4) if r.p[i]),100)
if __name__=="__main__":unittest.main()
