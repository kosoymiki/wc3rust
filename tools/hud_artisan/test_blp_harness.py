import struct
import tempfile
import unittest
from pathlib import Path

from PIL import Image

import blp_harness as blp


class BLPHarnessTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_inspect_blp2_header(self):
        data = bytearray(152)
        data[:4] = b"BLP2"
        struct.pack_into("<IBBBBII", data, 4, 1, 3, 8, 0, 0, 64, 64)
        struct.pack_into("<I", data, 20, 148)
        struct.pack_into("<I", data, 84, 4)
        path = self.root/"sample.blp"
        path.write_bytes(data)
        info = blp.inspect_blp(path)
        self.assertEqual(info["format"], "BLP2")
        self.assertEqual((info["width"], info["height"]), (64, 64))

    def test_inspect_blp1_header(self):
        data = bytearray(1184)
        data[:4] = b"BLP1"
        struct.pack_into("<6I", data, 4, 1, 8, 32, 16, 5, 0)
        struct.pack_into("<I", data, 28, 1180)
        struct.pack_into("<I", data, 92, 4)
        path = self.root/"sample.blp"
        path.write_bytes(data)
        info = blp.inspect_blp(path)
        self.assertEqual(info["format"], "BLP1")
        self.assertEqual((info["width"], info["height"]), (32, 16))

    def test_inspect_blp1_jpeg_variable_header(self):
        data=bytearray(168)
        data[:4]=b"BLP1"
        struct.pack_into("<6I",data,4,0,8,8,8,0,0)
        struct.pack_into("<I",data,156,4)
        struct.pack_into("<I",data,28,164)
        struct.pack_into("<I",data,92,4)
        path=self.root/"jpeg.blp";path.write_bytes(data)
        info=blp.inspect_blp(path)
        self.assertEqual(info["content_type"],0)
        self.assertEqual((info["width"],info["height"]),(8,8))

    def test_rejects_bad_offset_and_unknown_magic(self):
        path=self.root/"bad.blp";path.write_bytes(b"NOPE"+bytes(1200))
        with self.assertRaises(blp.BLPError):
            blp.inspect_blp(path)

    def test_compare_png_exact_and_dimension_guard(self):
        a=self.root/"a.png";b=self.root/"b.png"
        Image.new("RGBA",(8,8),(100,50,20,255)).save(a)
        Image.new("RGBA",(8,8),(100,50,20,255)).save(b)
        self.assertTrue(blp.compare_png(a,b)["exact_match"])
        Image.new("RGBA",(9,8),(100,50,20,255)).save(b)
        with self.assertRaisesRegex(blp.BLPError,"dimensions differ"):
            blp.compare_png(a,b)

    def test_raw_rgba_decoder_output_wraps_to_png(self):
        raw=self.root/"image.rgba";png=self.root/"image.png"
        raw.write_bytes(bytes((1,2,3,255))*16)
        blp.raw_rgba_to_png(raw,png,4,4)
        with Image.open(png) as im:
            self.assertEqual(im.size,(4,4))
            self.assertEqual(im.getpixel((0,0)),(1,2,3,255))
        raw.write_bytes(b"short")
        with self.assertRaisesRegex(blp.BLPError,"exactly"):
            blp.raw_rgba_to_png(raw,png,4,4)

    def test_existing_png_to_blp1_encoder_roundtrip_harness(self):
        import subprocess
        import sys
        encoder = Path(__file__).resolve().parents[2] / "tools" / "png_to_blp1.py"
        self.assertTrue(encoder.is_file())
        source = self.root/"source.png"
        im = Image.new("RGBA",(16,16),(0,0,0,0))
        for y in range(4,12):
            for x in range(4,12):
                im.putpixel((x,y),(210,140,35,255))
        im.save(source)
        decoder_code = (
            "import pathlib,struct,sys; b=pathlib.Path(sys.argv[1]).read_bytes(); "
            "w,h=struct.unpack_from('<II',b,12); off=struct.unpack_from('<I',b,28)[0]; "
            "n=w*h; pal=b[156:1180]; idx=b[off:off+n]; alpha=b[off+n:off+2*n]; "
            "out=bytearray(); "
            "[(out.extend((pal[k*4+2],pal[k*4+1],pal[k*4],alpha[i]))) for i,k in enumerate(idx)]; "
            "pathlib.Path(sys.argv[2]).write_bytes(out)"
        )
        result = blp.roundtrip(
            source, self.root/"work",
            [sys.executable, str(encoder), "{input}", "{output}"],
            [sys.executable, "-c", decoder_code, "{input}", "{output}"],
            30, True
        )
        self.assertEqual(result["blp"]["format"],"BLP1")
        self.assertTrue(result["roundtrip"]["exact_match"])
        self.assertEqual(result["roundtrip"]["alpha_silhouette_iou"],1.0)


if __name__ == "__main__":
    unittest.main()
