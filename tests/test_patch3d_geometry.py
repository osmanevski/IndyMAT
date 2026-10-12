"""Nongraphics real octave-cli geometry and exact JS/Octave parity."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
CLI = shutil.which('octave-cli')
NODE = shutil.which('node')


@unittest.skipUnless(CLI and NODE, 'real octave-cli and node required')
class Patch3DGeometryTests(unittest.TestCase):
    def octave(self, code):
        prefix = "addpath('" + str(ROOT / 'octave').replace("'", "''") + "');"
        result = subprocess.run([CLI, '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', prefix + code],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_exact_js_octave_parity(self):
        script = ("const {cases}=require('./tests/figure_patch3d_unit.cjs');"
                  "const {triangulatePatch}=require('./frontend/figure_patch3d_utils.cjs');"
                  "console.log(JSON.stringify(cases.map(c=>({...c,result:triangulatePatch(c.vertices,c.faces)}))));")
        cases = json.loads(subprocess.check_output([NODE, '-e', script], cwd=ROOT, text=True))
        def matrix(rows, columns):
            if not rows:
                return f'zeros(0,{columns})'
            return '[' + ';'.join(' '.join(str(v) for v in row) for row in rows) + ']'
        commands = []
        for case in cases:
            width = max(map(len, case['faces']), default=0)
            faces = [[i + 1 for i in row] + ['NaN']*(width-len(row)) for row in case['faces']]
            commands.append(f"v={matrix(case['vertices'],3)};f={matrix(faces,width)};"
                            "[t,o,e,code,args]=__mf_patch3d_geometry__(v,f);"
                            "tc=cell(1,size(t,1));for k=1:size(t,1),tc{k}=num2cell(t(k,:)-1);end;"
                            "ec=cell(1,size(e,1));for k=1:size(e,1),ec{k}=num2cell(e(k,:)-1);end;"
                            "if isfield(args,'face'),args.face=args.face-1;end;"
                            "s=struct('triangles',{tc},'faceOfTriangle',{num2cell(o(:)'-1)},"
                            "'edges',{ec},'code',code,'args',args);disp(jsonencode(s));")
        records = [json.loads(line) for line in self.octave(''.join(commands)).splitlines() if line.startswith('{')]
        self.assertEqual(len(records),len(cases))
        for case, record in zip(cases, records):
            with self.subTest(case=case['name']):
                self.assertEqual(record,case['result'])

    def test_octave_validation_bounds_and_purity(self):
        self.octave("v=[0 0 0;1 0 0;1 1 0;0 1 0];f=[1 2 3 NaN;1 3 4 NaN];"
                    "beforev=v;beforef=f;before=get(0);"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(v,f);"
                    "assert(isempty(c));assert(isequal(t,[1 2 3;1 3 4]));assert(isequal(o,[1;2]));"
                    "assert(isequal(v,beforev)&&isequaln(f,beforef)&&isequaln(before,get(0)));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(v,[1 NaN 3]);assert(strcmp(c,'invalid_data'));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(v,[1 2 3 NaN(1,63)]);assert(strcmp(c,'budget_exceeded')&&strcmp(a.budget,'polygon_vertices'));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__([0 0 0;1 0 0;0 Inf 0],[1 2 3]);assert(strcmp(c,'invalid_data'));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(zeros(40001,3),[]);assert(strcmp(a.budget,'patch_vertices'));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(v,repmat([1 2 3 4],40001,1));assert(strcmp(a.budget,'patch_triangles'));"
                    "[t,o,e,c,a]=__mf_patch3d_geometry__(v,[1 2 3 4;1 3 2 4]);"
                    "assert(strcmp(c,'unsupported_patch')&&isempty(t)&&isempty(o)&&isempty(e));")


if __name__ == '__main__':
    unittest.main()
