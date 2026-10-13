"""Nongraphical scalar alpha and conservative data-mapping admission."""
import shutil
import unittest
from tests import test_figure_dalga26_interp_export as support


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class AlphaRound3(unittest.TestCase):
    octave = support.InterpolatedExport.octave

    def test_scalar_does_not_use_alpha_data_or_map(self):
        self.octave("""
for value=[0 .25 .75 1]
 [d,c,a]=__mf_figure_alpha__(value,NaN,'unknown',[],[],'unknown',0);
 assert(isempty(c)&&strcmp(d.mode,'constant')&&d.value==value);
endfor
for mode={-1,2,NaN,Inf,[0 1],1+i,sparse(1),'flat','interp','texturemap'}
 [d,c,a]=__mf_figure_alpha__(mode{1},[0;.5;1],'none',[0 1],[0;1],'vertex',3);
 assert(isempty(d)&&strcmp(c,'transparency'));
endfor
""")


if __name__ == "__main__":
    unittest.main()
