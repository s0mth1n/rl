import importlib.util
from pathlib import Path
import unittest
from PIL import Image

spec = importlib.util.spec_from_file_location('pixel_diff', Path(__file__).with_name('pixel-diff.py'))
pixel_diff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pixel_diff)


class PixelDiffTests(unittest.TestCase):
    def test_exact_and_one_changed_pixel(self):
        actual = Image.new('RGBA', (2, 2), (10, 20, 30, 255))
        reference = actual.copy()
        self.assertEqual(pixel_diff.compare(actual, reference, 0)[1]['changed_pixels'], 0)
        actual.putpixel((1, 1), (14, 20, 30, 255))
        report = pixel_diff.compare(actual, reference, 0)[1]
        self.assertEqual(report['changed_pixels'], 1)
        self.assertEqual(report['changed_percent'], 25)
        self.assertEqual(report['mean_absolute_error'], 0.25)
        self.assertEqual(report['maximum_channel_error'], 4)
        self.assertEqual(pixel_diff.compare(actual, reference, 4)[1]['changed_pixels'], 0)
        with self.assertRaisesRegex(ValueError, 'size mismatch'):
            pixel_diff.compare(actual, Image.new('RGBA', (3, 2)), 0)

    def test_crop_is_explicit_and_nonempty(self):
        self.assertEqual(pixel_diff.crop_box('2,3,4,5'), (2, 3, 6, 8))
        for value in ['-1,0,2,2', '0,0,0,2', '1,2', 'a,b,c,d']:
            with self.assertRaises(pixel_diff.argparse.ArgumentTypeError):
                pixel_diff.crop_box(value)


if __name__ == '__main__':
    unittest.main()
