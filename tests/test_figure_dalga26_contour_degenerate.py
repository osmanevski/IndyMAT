"""Regression boundary exposed by numerical replay of the exact UI ramp.

This records the current rejection, not desired native/browser acceptance.
The orchestrator must decide the empty-fill protocol before changing it.
"""
import shutil
import unittest
from tests import test_figure_dalga26_interp_export as support


@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class DegenerateContourBoundary(unittest.TestCase):
    octave = support.InterpolatedExport.octave

    def test_extremal_level_with_two_distinct_points_remains_rejected(self):
        self.octave("""
raw=[3 1;3 1;3 1;3 2;3 2;3 2];
[v,t,o,e,c,a,nf]=__mf_figure_contourf_geometry__(raw,1:6,2);
assert(isequal(v(:,1:2),raw)&&isequal(nf,[1 4]));
assert(strcmp(c,'invalid_data')&&strcmp(a.property,'faces'));
assert(isempty(t)&&isempty(o)&&isempty(e));
""")
