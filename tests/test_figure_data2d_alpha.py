"""Real serializer tests for scalar transparency of 2D patches and scatter.

Same engine path as tests/test_figure_data2d.py: a real Octave session through
Kernel. Data-driven alpha, 3D objects and the textbox role keep their PNG.
"""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend.kernel import Kernel

CLI = shutil.which('octave-cli')


def const(value):
    return {'mode': 'constant', 'association': 'constant', 'value': value}


class FigureData2DAlphaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get('INDYMAT_FIGURE_TEST_OCTAVE') or CLI
        if not executable:
            raise unittest.SkipTest('octave-cli unavailable')
        cls.temp = tempfile.TemporaryDirectory()
        cls.work = Path(cls.temp.name)
        cls.k = Kernel(ROOT, cls.work / 'runtime', cls.work, executable=executable)
        try:
            cls.wait()
            cls.k.submit("figure('visible','off'); close all;")
            state = cls.wait()
            if state['error'] and 'no graphics toolkits' in state['error']:
                raise unittest.SkipTest('this Octave has no graphics toolkit')
            if state['error']:
                raise AssertionError(state)
        except BaseException:
            cls.k.close()
            cls.temp.cleanup()
            raise

    @classmethod
    def tearDownClass(cls):
        cls.k.close()
        cls.temp.cleanup()

    @classmethod
    def wait(cls, seconds=60):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            state = cls.k.snapshot()
            if state['status'] not in ('starting', 'running', 'stopping'):
                return state
            time.sleep(.05)
        raise AssertionError('figure job timeout')

    def snapshot(self, code):
        output = self.work / 'extracted.json'
        self.k.submit("close all; figure(1,'visible','off'); " + code +
                      "; mf_test_result=__mf_figure_data__(gcf,struct('job',repmat('f',1,32),'figure',1)); " +
                      f"mf_test_fid=fopen({Kernel.quote(output)},'w'); " +
                      "fputs(mf_test_fid,jsonencode(mf_test_result)); fclose(mf_test_fid); clear mf_test_fid mf_test_result;")
        state = self.wait()
        self.assertFalse(state['error'], state)
        return json.loads(output.read_text())

    def series(self, code):
        data = self.snapshot(code)
        self.assertTrue(data['supported'], data)
        return data['axes'][0]['series']

    def test_01_fill_and_patch_scalar_alpha(self):
        s = self.series("fill([0 1 1],[0 0 1],'r','FaceAlpha',.5,'EdgeAlpha',.3)")[0]
        self.assertEqual((s['kind'], s['face_alpha'], s['edge_alpha']), ('patch2d', const(.5), const(.3)))
        s = self.series("h=patch([2 3 3],[0 0 1],'g');set(h,'FaceAlpha',.25)")[0]
        self.assertEqual(s['face_alpha'], const(.25))
        self.assertNotIn('edge_alpha', s)

    def test_02_alpha_one_is_absent_and_zero_is_kept(self):
        s = self.series("fill([0 1 1],[0 0 1],'r','FaceAlpha',1)")[0]
        self.assertNotIn('face_alpha', s)
        self.assertNotIn('edge_alpha', s)
        s = self.series("fill([0 1 1],[0 0 1],'r','FaceAlpha',0)")[0]
        self.assertEqual(s['face_alpha'], const(0))

    def test_03_area_and_bar_children(self):
        s = self.series("area(1:4,[1 3 2 4],'FaceAlpha',.3)")
        self.assertEqual((s[0]['role'], s[0]['face_alpha']), ('area', const(.3)))
        s = self.series("h=bar(1:4,[1 3 2 4]);set(findobj(h,'type','patch'),'facealpha',.4)")
        self.assertEqual((s[0]['role'], s[0]['face_alpha']), ('bar', const(.4)))
        self.assertEqual(s[1]['kind'], 'line')

    def test_04_scatter_marker_alpha(self):
        s = self.series("scatter(1:4,[1 3 2 4],36,'b','filled','MarkerFaceAlpha',.4,'MarkerEdgeAlpha',.6)")[0]
        self.assertEqual((s['kind'], s['marker_face_alpha'], s['marker_edge_alpha']), ('scatter', const(.4), const(.6)))
        self.assertNotIn('face_alpha', s)
        s = self.series("scatter(1:4,[1 3 2 4],36,'b','filled','MarkerFaceAlpha',.4)")[0]
        self.assertNotIn('marker_edge_alpha', s)
        s = self.series("scatter(1:4,[1 3 2 4])")[0]
        self.assertFalse([k for k in s if k.endswith('_alpha')])

    def test_05_data_driven_and_3d_alpha_keep_png(self):
        cases = [
            "patch([0 1 1],[0 0 1],'r','FaceAlpha','flat','FaceVertexAlphaData',[.1;.5;.9])",
            "patch([0 1 1],[0 0 1],'r','FaceAlpha','interp','FaceVertexAlphaData',[.1;.5;.9])",
            "patch([0 1 1],[0 0 1],'r','FaceAlpha',.5,'FaceVertexAlphaData',[.1;.5;.9])",
            "scatter3(1:4,[1 3 2 4],1:4,36,'b','filled','MarkerFaceAlpha',.4)",
            "image([1 2;3 4],'AlphaData',.5)",
        ]
        for code in cases:
            with self.subTest(code=code):
                data = self.snapshot(code)
                self.assertFalse(data['supported'], code)
                self.assertEqual(data['reason_code'], 'transparency')
        # One data-driven object still sends the whole figure to its PNG.
        data = self.snapshot("fill([0 1 1],[0 0 1],'r','FaceAlpha',.5);hold on;patch([2 3 3],[0 0 1],'g','FaceAlpha','flat','FaceVertexAlphaData',[.1;.5;.9])")
        self.assertFalse(data['supported'])

    def test_06_other_kinds_carry_no_alpha_keys(self):
        for code in ["plot(1:3,'o')", "stem(1:3)", "stairs(1:3)"]:
            with self.subTest(code=code):
                for s in self.series(code):
                    self.assertFalse([k for k in s if k.endswith('_alpha')])


if __name__ == '__main__':
    unittest.main()
