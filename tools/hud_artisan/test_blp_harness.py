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


if __name__ == "__main__":
    unittest.main()
