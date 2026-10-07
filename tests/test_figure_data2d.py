"""Real serializer tests for 2D filled shapes and text (figure data v3).

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


class FigureData2DTests(unittest.TestCase):
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

    def test_01_bar_geometry_colour_baseline_and_tip_data(self):
        s, data = self.series("bar([10 20 30],[1 3 2],0.5)")
        self.assertEqual([x['kind'] for x in s], ['patch2d', 'line'])
        bar = s[0]
        self.assertEqual(bar['role'], 'bar')
        self.assertEqual(bar['vertices']['shape'], [12, 2])
        self.assertEqual(column(bar['vertices'], 0), [7.5, 7.5, 12.5, 12.5, 17.5, 17.5, 22.5, 22.5, 27.5, 27.5, 32.5, 32.5])
        self.assertEqual(column(bar['vertices'], 1), [0, 1, 1, 0, 0, 3, 3, 0, 0, 2, 2, 0])
        self.assertEqual(bar['faces']['shape'], [3, 4])
        self.assertEqual([face(bar['faces'], k) for k in range(3)], [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]])
        self.assertEqual(bar['index_base'], 0)
        self.assertEqual(bar['face_color'], [0, .447, .741])
        self.assertEqual(bar['edge_color'], [0, 0, 0])
        self.assertEqual(bar['bar'], {'horizontal': False, 'layout': 'grouped', 'width': .5, 'base_value': 0,
                                      'positions': [10, 20, 30], 'values': [1, 3, 2]})
        self.assertEqual((bar['original_points'], bar['rendered_points'], bar['decimated']), (12, 12, False))
        self.assertEqual((s[1]['span'], s[1]['y'], s[1]['role']), ('horizontal', [0, 0], 'bar'))
        self.assertEqual(data['vertex_count'], 14)
        self.assertEqual(data['triangle_count'], 0)

    def test_02_grouped_stacked_horizontal_and_base_value(self):
        s, _ = self.series("bar([1 2;3 4]);legend('a','b')")
        bars = [x for x in s if x['kind'] == 'patch2d']
        self.assertEqual([x['display_name'] for x in bars], ['a', 'b'])
        for got, want in zip(column(bars[0]['vertices'], 0), [.68, .68, .96, .96, 1.68, 1.68, 1.96, 1.96]):
            self.assertAlmostEqual(got, want, places=12)
        self.assertEqual(column(bars[1]['vertices'], 1), [0, 2, 2, 0, 0, 4, 4, 0])
        self.assertEqual(bars[1]['face_color'], [.85, .325, .098])
        s, _ = self.series("barh([1 2;3 4],'stacked')")
        bars = [x for x in s if x['kind'] == 'patch2d']
        self.assertEqual(column(bars[1]['vertices'], 0), [1, 3, 3, 1, 3, 7, 7, 3])
        self.assertEqual(bars[1]['bar']['horizontal'], True)
        self.assertEqual(bars[1]['bar']['layout'], 'stacked')
        self.assertEqual(bars[1]['bar']['values'], [2, 4])
        self.assertEqual([x['span'] for x in s if x['kind'] == 'line'], ['vertical'])
        s, _ = self.series("bar([1 -2 3],0.5,'basevalue',1)")
        self.assertEqual(column(s[0]['vertices'], 1), [1, 1, 1, 1, 1, -2, -2, 1, 1, 3, 3, 1])
        self.assertEqual(s[0]['bar']['base_value'], 1)
        self.assertEqual(s[1]['y'], [1, 1])
        # A missing bar keeps its face; its vertices are null, never zero.
        s, _ = self.series("bar([1 NaN 3])")
        self.assertEqual(column(s[0]['vertices'], 1)[4:8], [0, None, None, 0])
        self.assertEqual(s[0]['bar']['values'], [1, None, 3])

    def test_03_hist_flat_scalar_colour_area_and_histogram(self):
        s, data = self.series("hist([1 2 2 3 3 3],3)")
        self.assertEqual([x['kind'] for x in s], ['patch2d'])
        self.assertEqual(s[0]['role'], 'patch')
        self.assertNotIn('bar', s[0])
        self.assertEqual(column(s[0]['vertices'], 1), [0, 1, 1, 0, 0, 2, 2, 0, 0, 3, 3, 0])
        # Scalar scaled CData resolved through the axes colormap and CLim.
        axes = data['axes'][0]
        rows = axes['colormap']['shape'][0]
        index = min(rows - 1, max(0, int(rows * (1 - axes['clim'][0]) / (axes['clim'][1] - axes['clim'][0]))))
        expected = [axes['colormap']['values'][index + c * rows] for c in range(3)]
        self.assertEqual(s[0]['face_color'], expected)
        s, _ = self.series("area(1:3,[1 2;2 1;3 3])")
        self.assertEqual([(x['kind'], x['role']) for x in s], [('patch2d', 'area'), ('patch2d', 'area')])
        self.assertEqual(s[1]['vertices']['shape'], [7, 2])
        self.assertEqual(column(s[1]['vertices'], 0), [1, 1, 2, 3, 3, 2, 1])
        self.assertEqual(column(s[1]['vertices'], 1), [1, 3, 3, 6, 3, 2, 1])
        self.assertEqual(face(s[1]['faces'], 0), [0, 1, 2, 3, 4, 5, 6])

    def test_04_fill_patch_edges_and_missing_vertex(self):
        s, _ = self.series("fill([0 1 1],[0 0 1],'r','linestyle','--','linewidth',2);hold on;"
                           "patch([2 3 3 2;4 5 4.5 NaN]',[0 0 1 1;0 0 1 NaN]',[0 .6 0],'edgecolor','none')")
        self.assertEqual((s[0]['face_color'], s[0]['edge_color'], s[0]['line_style'], s[0]['line_width']), ([1, 0, 0], [0, 0, 0], '--', 2))
        self.assertEqual(column(s[0]['vertices'], 0), [0, 1, 1])
        self.assertEqual(face(s[0]['faces'], 0), [0, 1, 2])
        self.assertEqual((s[1]['face_color'], s[1]['edge_color']), ([0, .6, 0], 'none'))
        self.assertEqual(column(s[1]['vertices'], 0), [2, 3, 3, 2, 4, 5, 4.5, None])
        self.assertEqual([face(s[1]['faces'], k) for k in range(2)], [[0, 1, 2, 3], [4, 5, 6, 7]])
        s, _ = self.series("patch('vertices',[0 0;1 0;1 1;0 1;2 2],'faces',[1 2 3 4;1 2 5 NaN],'facecolor','flat','facevertexcdata',[.2 .4 .6])")
        self.assertEqual(s[0]['face_color'], [.2, .4, .6])
        self.assertEqual(face(s[0]['faces'], 1), [0, 1, 4, None])

    def test_05_text_properties_lines_and_units(self):
        s, _ = self.series("plot(1:3);"
                           "text(1.5,2.5,{'two lines','\\sigma_x^2'},'backgroundcolor','w','edgecolor','k','verticalalignment','top','margin',4,'linewidth',2);"
                           "text(2,1.5,['ab ';'cde'],'rotation',-90,'fontweight','bold','fontangle','italic','color',[0 .5 0],'fontsize',14);"
                           "text(.5,.9,sprintf('a\\nb'),'units','normalized','horizontalalignment','center','interpreter','none')")
        self.assertEqual([x['kind'] for x in s], ['line', 'text', 'text', 'text'])
        box = s[1]
        self.assertEqual(box['lines'], ['two lines', '\\sigma_x^2'])
        self.assertEqual((box['units'], box['position'], box['interpreter']), ('data', [1.5, 2.5], 'tex'))
        self.assertEqual((box['horizontal_alignment'], box['vertical_alignment'], box['rotation']), ('left', 'top', 0))
        self.assertEqual((box['color'], box['background_color'], box['edge_color']), ([0, 0, 0], [1, 1, 1], [0, 0, 0]))
        self.assertEqual((box['margin'], box['line_width'], box['line_style'], box['font_size']), (4, 2, '-', 10))
        self.assertEqual((box['font_weight'], box['font_angle'], box['clipping']), ('normal', 'normal', False))
        self.assertEqual((box['original_points'], box['rendered_points'], box['decimated']), (0, 0, False))
        turned = s[2]
        self.assertEqual(turned['lines'], ['ab', 'cde'])
        self.assertEqual((turned['rotation'], turned['font_weight'], turned['font_angle'], turned['font_size']), (270, 'bold', 'italic', 14))
        self.assertEqual((turned['color'], turned['background_color'], turned['edge_color']), ([0, .5, 0], 'none', 'none'))
        self.assertEqual((s[3]['units'], s[3]['position'], s[3]['lines'], s[3]['interpreter'], s[3]['horizontal_alignment']),
                         ('normalized', [.5, .9], ['a', 'b'], 'none', 'center'))

    def test_06_hidden_title_axes_groups_and_annotation_textbox(self):
        data = self.snapshot("plot(1:3);axes('position',[0 .94 1 .06],'visible','off');"
                             "text(.5,.5,'Figure title','horizontalalignment','center','fontweight','bold','fontsize',12)")
        self.assertTrue(data['supported'], data)
        self.assertEqual([a['visible'] for a in data['axes']], [True, False])
        self.assertEqual(data['axes'][1]['position'], [0, .94, 1, .06])
        self.assertEqual([(x['kind'], x['lines']) for x in data['axes'][1]['series']], [('text', ['Figure title'])])
        s, _ = self.series("errorbar(1:3,[1 2 3],[.1 .2 .1]);hold on;xline(2,'--','L');yline(2.5,'r')")
        self.assertEqual([(x['kind'], x['role'], x.get('span')) for x in s],
                         [('line', 'errorbar', None), ('line', 'errorbar', None), ('line', 'xline', 'vertical'),
                          ('text', 'xline', None), ('line', 'yline', 'horizontal')])
        self.assertEqual(s[0]['y'], [1, 2, 3])
        self.assertEqual(s[1]['y'][:9], [.9, 1.1, None, 1.1, 1.1, None, .9, .9, None])
        self.assertEqual((s[2]['x'], s[2]['line_style'], s[3]['lines']), ([2, 2], '--', ['L']))
        self.assertEqual((s[4]['y'], s[4]['line_color']), ([2.5, 2.5], [1, 0, 0]))
        data = self.snapshot("plot(1:3);annotation('textbox',[.2 .6 .3 .15],'string',{'box','two'},'backgroundcolor','w');sgtitle('All')")
        self.assertTrue(data['supported'], data)
        overlay = data['axes'][-1]
        self.assertEqual((overlay['visible'], overlay['position'], overlay['xlim'], overlay['ylim']), (False, [0, 0, 1, 1], [0, 1], [0, 1]))
        self.assertEqual(sorted((x['kind'], x['role']) for x in overlay['series']),
                         [('patch2d', 'textbox'), ('patch2d', 'textbox'), ('text', 'textbox'), ('text', 'textbox')])
        self.assertIn(['All'], [x['lines'] for x in overlay['series'] if x['kind'] == 'text'])
        self.assertIn([1, 1, 1], [x['face_color'] for x in overlay['series'] if x['kind'] == 'patch2d'])

    def test_07_version_rule_and_additive_axes_fields(self):
        old = self.snapshot("plot([1 2 4],[3 5 4]);set(gca,'xtick',[1 2 4],'xticklabel',{'a','b','c'})")
        self.assertEqual((old['supported'], old['version']), (True, 2))
        a = old['axes'][0]
        self.assertEqual((a['visible'], a['xtickmode'], a['xticklabelmode'], a['ytickmode']), (True, 'manual', 'manual', 'auto'))
        self.assertEqual(a['interpreters'], {'title': 'tex', 'xlabel': 'tex', 'ylabel': 'tex', 'ticks': 'tex'})
        for code in ["plot(1:3);text(1,2,'t')", "bar(1:3)", "fill([0 1 1],[0 0 1],'r')", "plot(1:3);xline(2)", "plot(1:3);axis off"]:
            with self.subTest(code=code):
                data = self.snapshot(code)
                self.assertEqual((data['supported'], data['version']), (True, 3), data)
        hidden = self.snapshot("plot(1:3);title('T');axis off")
        self.assertEqual((hidden['axes'][0]['visible'], hidden['axes'][0]['title']), (False, 'T'))

    def test_08_excluded_forms_keep_png_with_precise_reasons(self):
        cases = [
            ("fill([0 1 1],[0 0 1],'r','facealpha',.5)", 'transparency', {}),
            ("area(1:3,[1 2 3],'facealpha',.4)", 'transparency', {}),
            ("fill([0 1 1],[0 0 1],[1 2 3])", 'interpolated_color', {}),
            ("patch([0 1 1;2 3 3]',[0 0 1;0 0 1]',[1;2])", 'patch_colors', {}),
            ("patch([0 1 1],[0 0 1],1,'cdatamapping','direct')", 'unsupported_color', {}),
            ("fill([0 1 1],[0 0 1],'r','marker','o')", 'unsupported_patch', {'property': 'marker'}),
            ("bar(1:3);set(gca,'yscale','log')", 'unsupported_patch', {'property': 'scale'}),
            ("pie([1 2 3])", 'unsupported_patch', {'property': 'dataaspectratio'}),
            ("patch('vertices',[0 0 0;1 0 1;0 1 0],'faces',[1 2 3],'facecolor','r')", 'unsupported_object', {'type': 'patch'}),
            ("patch('vertices',[0 0 0;1 0 0;0 1 0],'faces',[1 2 3],'facecolor',[1 0 0]);view(3)", 'unsupported_object', {'type': 'patch'}),
            ("plot3(1:3,1:3,1:3);text(1,1,1,'t')", 'unsupported_object', {'type': 'text'}),
            ("plot(1:3);text(40,40,'p','units','pixels')", 'unsupported_text', {'property': 'units'}),
            ("plot(1:3);text(1,2,'p','fontunits','normalized')", 'unsupported_text', {'property': 'fontunits'}),
            ("plot(1:3);text(1,2,'$x$','interpreter','latex')", 'unsupported_text', {'property': 'interpreter'}),
            ("plot(1:3);xline(2,'alpha',.5)", 'transparency', {}),
            ("plot(1:3);annotation('arrow',[.2 .4],[.2 .4])", 'unsupported_group', None),
            ("plot(1:3);text(1,2,repmat({'x'},1,257))", 'budget_exceeded', {'budget': 'text_lines', 'actual': 257, 'limit': 256}),
            ("plot(1:3);text(1,2,repmat('x',1,65537))", 'budget_exceeded', {'budget': 'text_chars', 'actual': 65537, 'limit': 65536}),
            ("t=linspace(0,1,40001);fill(t,t.^2,'r')", 'budget_exceeded', {'budget': 'patch_vertices', 'actual': 40001, 'limit': 40000}),
        ]
        for code, reason, args in cases:
            with self.subTest(code=code):
                data = self.snapshot(code)
                self.assertFalse(data['supported'], code)
                self.assertEqual(data['axes'], [])
                self.assertEqual(data['version'], 3)
                self.assertEqual(data['reason_code'], reason)
                if args is not None:
                    self.assertEqual(data['reason_args'], args)
        # One excluded object still means the whole figure keeps its PNG.
        data = self.snapshot("subplot(1,2,1);bar(1:3);subplot(1,2,2);fill([0 1 1],[0 0 1],[1 2 3])")
        self.assertEqual((data['supported'], data['reason_code'], data['axes']), (False, 'interpolated_color', []))

    def test_09_extraction_changes_no_properties_or_workspace(self):
        helper = self.work / 'mf_test_no_mutation2d.m'
        helper.write_text("""function mf_test_no_mutation2d(f)
  handles=sort(findall(f)); before=cell(numel(handles),1);
  for j=1:numel(handles),before{j}=get(handles(j));endfor
  root_before=get(0,'currentfigure');vars_before=evalin('base','whos');
  data=__mf_figure_data__(f);assert(data.supported);
  assert(isequaln(vars_before,evalin('base','whos')));
  assert(isequaln(root_before,get(0,'currentfigure')));
  assert(isequal(handles,sort(findall(f))));
  for j=1:numel(handles),assert(isequaln(before{j},get(handles(j))));endfor
endfunction
""")
        self.run_code("close all;figure('visible','off');subplot(2,2,1);bar([1 2;3 4]);legend('a','b');"
                      "subplot(2,2,2);hist([1 2 2 3]);text(2,1,{'a','b'},'backgroundcolor','w');"
                      "subplot(2,2,3);area(1:3,[1 2;2 1;3 3]);xline(2);subplot(2,2,4);errorbar(1:3,1:3,[.1 .1 .1]);"
                      "annotation('textbox',[.4 .45 .2 .08],'string','mid');sgtitle('All');kept=magic(3);kept_handle=@sin;" +
                      f"addpath({Kernel.quote(self.work)});mf_test_no_mutation2d(gcf);" +
                      "assert(isequal(kept,magic(3)));assert(kept_handle(.5)==sin(.5));")


if __name__ == '__main__':
    unittest.main()
