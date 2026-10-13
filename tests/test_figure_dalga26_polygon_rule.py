"""Nongraphical admission tests for measured interpolation scope."""
import shutil
import unittest
from tests import test_figure_dalga26_interp_export as support


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class PolygonRule(unittest.TestCase):
    octave = support.InterpolatedExport.octave

    def test_only_triangle_faces_have_pinned_interpolation(self):
        self.octave("""
assert(__mf_figure_patch_interp_faces__([1 2 3]));
assert(__mf_figure_patch_interp_faces__([1 2 3 1]));
assert(__mf_figure_patch_interp_faces__([1 2 3 NaN;1 3 4 1]));
for f={[1 2 3 4],[4 3 2 1],[2 3 4 1],[3 4 1 2],1:5,1:64}
 assert(~__mf_figure_patch_interp_faces__(f{1}));
endfor
assert(~__mf_figure_patch_interp_faces__([1 2 3 NaN;1 2 3 4]));
""")
