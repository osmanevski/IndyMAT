"""PNG oracle decoding only; imread/imwrite create no graphics objects."""
from pathlib import Path
import shutil
import tempfile
import unittest
from tests import test_figure_dalga26_interp_export as support


@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class AlphaPngRound4(unittest.TestCase):
    octave = support.InterpolatedExport.octave

    def test_rgb_gray_bilevel_uint16_and_indexed(self):
        with tempfile.TemporaryDirectory(prefix='alpha-png-round4-', dir='/Users/osmanevski/.cache/indymat-fleet/dalga26') as folder:
            path = str(Path(folder)/'sample.png').replace("'", "''")
            self.octave(f"""
addpath('tests/fixtures/figure_v3');before=__go_handles__(true);
p='{path}';
a=uint8(cat(3,[1 2;3 4],[5 6;7 8],[9 10;11 12]));imwrite(a,p);assert(isequal(d26_read_png_rgb(p),a));
a=uint8([0 127;255 9]);imwrite(a,p);assert(isequal(d26_read_png_rgb(p),repmat(a,[1 1 3])));
a=logical([0 1;1 0]);imwrite(a,p);assert(isequal(d26_read_png_rgb(p),uint8(255*repmat(a,[1 1 3]))));
a=uint16([0 257;32768 65535]);imwrite(a,p);assert(isequal(d26_read_png_rgb(p),repmat(uint8([0 1;128 255]),[1 1 3])));
map=[1 0 0;0 128/255 1;1 1 1];imwrite(uint8([0 1;2 0]),map,p);
actual=d26_read_png_rgb(p);assert(isequal(reshape(actual(1,1,:),1,3),uint8([255 0 0])));
assert(isequal(reshape(actual(1,2,:),1,3),uint8([0 128 255])));
assert(isequal(reshape(actual(2,1,:),1,3),uint8([255 255 255])));
assert(isequal(before,__go_handles__(true)));
""")

if __name__ == '__main__':
    unittest.main()
