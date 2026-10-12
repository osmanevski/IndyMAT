"""Load the integrated serializer in real CLI without creating graphics."""
from pathlib import Path
import shutil
import subprocess
import unittest
ROOT = Path(__file__).resolve().parents[1]

@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class PatchSerializerSyntax(unittest.TestCase):
    def test_integrated_serializer_and_acceptance_recipe_load(self):
        code = "addpath('octave');addpath('tests/fixtures/figure_v3');" + (
            "d=__mf_figure_data__(0);assert(~d.supported&&strcmp(d.reason_code,'no_axes'));"
            "assert(~isempty(which('verify_dalga25_stage4')));"
            "assert(~isempty(which('verify_dalga25_visibility')));"
            "[t,o,e,c]=__mf_patch3d_geometry__([0 0 1;1 0 1;0 1 1],[1 2 3]);assert(isempty(c));")
        p = subprocess.run([shutil.which('octave-cli'),'--quiet','--no-init-file',
             '--no-site-file','--no-history','--eval',code],cwd=ROOT,
             capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stdout+p.stderr)
