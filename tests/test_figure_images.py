"""Real serializer tests for bounded interactive images (figure data v3).

Same engine path as tests/test_figure_data3d.py: a real Octave session through
Kernel, no fake graphics. Geometry is the patch Octave computed itself.
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


def column(descriptor, index):
    rows = descriptor['shape'][0]
    return descriptor['values'][index * rows:(index + 1) * rows]


def face(descriptor, index):
    rows, cols = descriptor['shape']
    return [descriptor['values'][index + c * rows] for c in range(cols)]


class FigureImageTests(unittest.TestCase):
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

    def run_code(self, code):
        self.k.submit(code)
        state = self.wait()
        self.assertFalse(state['error'], state)
        return state

    def snapshot(self, code):
        output = self.work / 'extracted.json'
        self.run_code("close all; figure(1,'visible','off'); " + code +
                      "; mf_test_result=__mf_figure_data__(gcf,struct('job',repmat('f',1,32),'figure',1)); " +
                      f"mf_test_fid=fopen({Kernel.quote(output)},'w'); " +
                      "fputs(mf_test_fid,jsonencode(mf_test_result)); fclose(mf_test_fid); clear mf_test_fid mf_test_result;")
        return json.loads(output.read_text())

    def series(self, code):
        data = self.snapshot(code)
        self.assertTrue(data['supported'], data)
        self.assertEqual(data['version'], 3)
        return data['axes'][0]['series'], data

    def test_scaled_and_rgb(self):
        series, data = self.series("imagesc([30 10],[8 4],[1 2 3;4 5 6]);axis image;colormap([1 0 0;0 1 0;0 0 1]);colorbar")
        image = series[0]
        self.assertEqual(image['kind'], 'image')
        self.assertEqual((image['shape'], image['x'], image['y']), ([2, 3], [30, 10], [8, 4]))
        self.assertEqual(image['cdata'], {'shape': [2, 3], 'order': 'column-major', 'values': [1, 4, 2, 5, 3, 6]})
        self.assertEqual((image['encoding'], image['mapping'], image['cdata_class'], image['pixel_count']), ('indexed', 'scaled', 'double', 6))
        self.assertEqual((data['pixel_count'], data['vertex_count'], data['triangle_count']), (6, 0, 0))
        a = data['axes'][0]
        self.assertEqual((a['xlim'], a['ylim'], a['ydir'], a['data_aspect_ratio_mode']), ([5, 35], [2, 10], 'reverse', 'manual'))
        self.assertEqual(len(a['colorbars']), 1)
        for dtype in ['uint8', 'uint16', 'single', 'double']:
            series, data = self.series("image(cat(3," + dtype + "([0 1;2 3])," + dtype + "([4 5;6 7])," + dtype + "([8 9;10 11])))")
            image = series[0]
            self.assertEqual((image['encoding'], image['cdata_class'], image['cdata']['shape']), ('truecolor', dtype, [2, 2, 3]))
            self.assertEqual(image['cdata']['values'], [0, 2, 1, 3, 4, 6, 5, 7, 8, 10, 9, 11])
        series, data = self.series("imagesc(7)")
        self.assertEqual((series[0]['shape'], series[0]['x'], series[0]['y'], data['pixel_count']), ([1, 1], [1, 1], [1, 1], 1))

    def test_direct_indexed_and_mixed_lines(self):
        for dtype in ['uint8', 'uint16', 'single', 'double']:
            series, data = self.series("image(" + dtype + "([0 1;2 3]));hold on;plot([1 2],[1 2],'r');set(gca,'xdir','reverse','ydir','normal')")
            self.assertEqual([s['kind'] for s in series], ['image', 'line'])
            self.assertEqual((series[0]['mapping'], series[0]['cdata_class']), ('direct', dtype))
            self.assertEqual((data['pixel_count'], data['vertex_count']), (4, 2))
            self.assertEqual((data['axes'][0]['xdir'], data['axes'][0]['ydir']), ('reverse', 'normal'))

    def test_rejections_and_aggregate_raster_budget(self):
        cases = [
            ("imagesc(zeros(513,512))", 'budget_exceeded', {'budget': 'image_pixels', 'actual': 262656, 'limit': 262144}),
            ("imagesc(zeros(512));hold on;image(1)", 'budget_exceeded', {'budget': 'image_pixels', 'actual': 262145, 'limit': 262144}),
            ("h=imagesc([1 2;3 4]);set(h,'alphadata',.5)", 'transparency', {}),
            ("h=imagesc([1 2;3 4]);set(h,'alphadatamapping','scaled')", 'transparency', {}),
            ("imagesc([1 NaN;3 4])", 'invalid_data', {'property': 'image.cdata'}),
            ("imagesc([1 2;3 4]);set(gca,'xscale','log')", 'unsupported_object', {'type': 'image'}),
            ("imagesc([1 2;3 4]);view(3)", 'unsupported_object', {'type': 'image'}),
            ("imagesc([2 8],[5 9],7)", 'unsupported_object', {'type': 'image_coordinates'}),
        ]
        for code, reason, args in cases:
            with self.subTest(code=code):
                data = self.snapshot(code)
                self.assertEqual((data['version'], data['supported'], data['reason_code'], data['reason_args']), (3, False, reason, args), data)
                self.assertEqual((data['axes'], data['pixel_count']), ([], 0))

    def test_source_figure_size(self):
        _, data = self.series("set(gcf,'units','pixels','position',[10 10 640 320]);imagesc([1 2;3 4])")
        self.assertEqual(data['source']['figure_size'], [640, 320])
        _, data = self.series("set(gcf,'units','normalized','position',[.1 .1 .4 .3]);imagesc([1 2;3 4])")
        self.assertEqual(len(data['source']['figure_size']), 2)
        self.assertTrue(all(n > 0 for n in data['source']['figure_size']))

    def test_extraction_does_not_mutate(self):
        helper = self.work / 'mf_image_invariant.m'
        helper.write_text("""function mf_image_invariant(f)
handles=sort(findall(f));before=cell(numel(handles),1);
for j=1:numel(handles),before{j}=get(handles(j));endfor
vars=evalin('base','whos');data=__mf_figure_data__(f);assert(data.supported);
assert(isequaln(vars,evalin('base','whos')));assert(isequal(handles,sort(findall(f))));
for j=1:numel(handles),assert(isequaln(before{j},get(handles(j))));endfor
endfunction
""")
        self.run_code("close all;figure('visible','off');imagesc([1 2 3;4 5 6]);axis image;colorbar;hold on;plot(1:3,1:3);kept=magic(3);kept_handle=@sin;" +
                      f"addpath({Kernel.quote(self.work)});mf_image_invariant(gcf);")


if __name__ == '__main__':
    unittest.main()
