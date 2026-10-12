"""No-graphics syntax and pure serializer colour-descriptor checks.

The harness copies the actual local subfunctions verbatim, without replacing
graphics properties or pretending to produce a measured figure fixture.
"""
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CLI = shutil.which('octave-cli')


@unittest.skipUnless(CLI, 'octave-cli unavailable')
class FigureInterpSyntaxTests(unittest.TestCase):
    def run_octave(self, code):
        result = subprocess.run([CLI, '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', code], cwd=ROOT,
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_serializer_and_acceptance_function_parse(self):
        self.run_octave("addpath('octave');addpath('tests/fixtures/figure_v3');"
                        "assert(~isempty(which('__mf_figure_data__')));"
                        "assert(~isempty(which('verify_dalga24_interp')));")

    def test_actual_pure_descriptor_helpers(self):
        source = (ROOT / 'octave/__mf_figure_data__.m').read_text()
        functions = []
        for name in ['__mf_surface_color__', '__mf_descriptor__', '__mf_array__', '__mf_rgb_or_none__']:
            match = re.search(r'^function [^\n]*\b' + name + r'\([^\n]*\n.*?^endfunction',
                              source, re.MULTILINE | re.DOTALL)
            self.assertIsNotNone(match, name)
            functions.append(match.group())
        body = """
function interp_descriptor_harness()
  for type={'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64'}
    c=cast([0 2;1 3],type{1});
    for mapping={'scaled','direct'}
      [d,code]=__mf_surface_color__('interp',c,mapping{1});
      assert(isempty(code));assert(strcmp(d.mode,'interp'));
      assert(strcmp(d.association,'vertex'));assert(strcmp(d.encoding,'indexed'));
      assert(strcmp(d.mapping,mapping{1}));assert(strcmp(d.cdata_class,type{1}));
      assert(isequal(d.data.shape,{2,2}));assert(isequal(d.data.values,{0,1,2,3}));
      [edge,code]=__mf_surface_color__('interp',c,mapping{1},'vertex');
      assert(strcmp(code,'interpolated_color'));
      [flat,code]=__mf_surface_color__('flat',c,mapping{1});
      assert(isempty(code)&&strcmp(flat.mode,'flat')&&strcmp(flat.association,'cell'));
    endfor
  endfor
  for type={'double','single','uint8','uint16'}
    c=cast(cat(3,[0 1;1 0],[1 0;0 1],[0 0;1 1]),type{1});
    [d,code]=__mf_surface_color__('interp',c,'scaled');
    assert(isempty(code)&&strcmp(d.encoding,'truecolor'));
    assert(isequal(d.data.shape,{2,2,3}));assert(strcmp(d.cdata_class,type{1}));
  endfor
  [d,code]=__mf_surface_color__('interp',[1 NaN;2 3],'scaled');
  assert(isempty(code));assert(strcmp(jsonencode(d.data),'{"shape":[2,2],"order":"column-major","values":[1,2,null,3]}'));
  [d,code]=__mf_surface_color__('interp',ones(2),'unknown');assert(strcmp(code,'unsupported_color'));
  [d,code]=__mf_surface_color__('texturemap',ones(2),'scaled');assert(strcmp(code,'unsupported_surface'));
  [d,code]=__mf_surface_color__('none',ones(2),'scaled');assert(isempty(code)&&strcmp(d.mode,'none'));
  [d,code]=__mf_surface_color__([.2 .4 .6],ones(2),'scaled');assert(isempty(code)&&strcmp(d.mode,'constant'));
endfunction
"""
        # Fleet artifacts remain inside the lane, never under /tmp.
        with tempfile.TemporaryDirectory(prefix='.interp-test-', dir=ROOT) as work:
            path = Path(work) / 'interp_descriptor_harness.m'
            path.write_text(body + '\n' + '\n'.join(functions) + '\n')
            quote = str(work).replace("'", "''")
            self.run_octave(f"addpath('{quote}');interp_descriptor_harness();")


if __name__ == '__main__':
    unittest.main()
