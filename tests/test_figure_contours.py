"""Nongraphics path tests; actual contour objects require orchestrator acceptance."""
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class ContourPaths(unittest.TestCase):
    def test_resolved_paths_in_real_cli(self):
        code = """
[x,y,z,c,code]=__mf_figure_contours__([1 2 3],[4 5 6],[7 7 7],1:3);
assert(isempty(code)&&c&&isequal(x,[1 2 3 1])&&isequal(z,[7 7 7 7]));
[x,y,z,c,code]=__mf_figure_contours__([1 2 NaN],[4 5 NaN],[],1:3);
assert(isempty(code)&&~c&&isnan(x(end))&&isempty(z));
[x,y,z,c,code]=__mf_figure_contours__([1 2],[3 4],[],[2 1]);assert(strcmp(code,'invalid_data'));
[x,y,z,c,code]=__mf_figure_contours__([1 2],[3 4],[1],1:2);assert(strcmp(code,'invalid_data'));
[x,y,z,c,code,args]=__mf_figure_contours__(1:2000,1:2000,[],1:2000);
assert(strcmp(code,'budget_exceeded')&&args.actual==2001);
assert(~isempty(which('__mf_figure_data__')));
map=[1 0 0;0 1 0;0 0 1];
[rgb,ok]=__mf_figure_index_color__(0,'scaled',map,[-1e308 1e308]);assert(ok&&isequal(rgb,[0 1 0]));
[rgb,ok]=__mf_figure_index_color__(100,'scaled',map,[-1 1]);assert(ok&&isequal(rgb,[0 0 1]));
[rgb,ok]=__mf_figure_index_color__(uint8(1),'direct',map,[-1 1]);assert(ok&&isequal(rgb,[0 1 0]));
[rgb,ok]=__mf_figure_index_color__(2.9,'direct',map,[-1 1]);assert(ok&&isequal(rgb,[0 1 0]));
"""
        result = subprocess.run([shutil.which('octave-cli'), '--quiet', '--no-init-file',
            '--no-site-file', '--no-history', '--eval', "addpath('octave');" + code],
            cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
