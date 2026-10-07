"""Real serializer tests. Lane defaults to octave-cli; no fake graphics/fixtures.

The orchestrator may set INDYMAT_FIGURE_TEST_OCTAVE to its Qt executable.
Kernel supplies the same detached session protocol as tests/test_kernel.py.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend.kernel import Kernel

CLI = shutil.which('octave-cli')


@unittest.skipUnless(CLI, 'octave-cli unavailable')
class FigureDataSyntaxTests(unittest.TestCase):
    def test_helpers_and_generator_parse_in_real_octave(self):
        code = (f"addpath({Kernel.quote(ROOT / 'octave')}); "
                f"addpath({Kernel.quote(ROOT / 'tests/fixtures/figure_v3')}); "
                "assert(~isempty(which('__mf_figure_data__'))); "
                "assert(~isempty(which('__mf_execute__'))); assert(~isempty(which('uret')));")
        result = subprocess.run([CLI, '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', code], capture_output=True,
                                text=True, timeout=20, cwd=ROOT)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_real_jsonencode_preserves_cell_arrays_and_utf8_byte_count(self):
        code = ("s=struct('shape',{num2cell([1 2])},'values',{num2cell([1 NaN])}); "
                "assert(strcmp(jsonencode(s),'{\"shape\":[1,2],\"values\":[1,null]}')); "
                "assert(strcmp(jsonencode(struct('values',{{1}})),'{\"values\":[1]}')); "
                "assert(strcmp(jsonencode(struct('values',{{}})),'{\"values\":[]}')); "
                "encoded=jsonencode('ğ'); assert(numel(encoded)==numel(unicode2native(encoded,'UTF-8')));")
        result = subprocess.run([CLI, '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', code], capture_output=True,
                                text=True, timeout=20, cwd=ROOT)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_read_only_inventory_and_real_empty_root_extraction(self):
        # Root-only call tests the actual traversal/fallback without constructing
        # fake graphics. It is not a figure fixture or camera measurement.
        code = (f"addpath({Kernel.quote(ROOT / 'octave')}); "
                "before=get(0);h=__go_handles__(true);assert(isequaln(before,get(0))); "
                "d=__mf_figure_data__(0);assert(~d.supported&&strcmp(d.reason_code,'no_axes')); "
                "assert(isequaln(before,get(0)));")
        result = subprocess.run([CLI, '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', code], capture_output=True,
                                text=True, timeout=20, cwd=ROOT)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class FigureData3DTests(unittest.TestCase):
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
                raise unittest.SkipTest('Octave 11.3 CLI has no graphics toolkit; real graphics tests require the orchestrator Qt gate')
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

    def test_01_versions_coordinates_and_nan_gap(self):
        old = self.snapshot("plot([1 2 4],[3 5 4],'-o')")
        self.assertTrue(old['supported'])
        self.assertEqual(old['version'], 2)
        data = self.snapshot("plot3([1 2 NaN 4 5],[2 3 NaN 6 7],[3 4 NaN 8 9],'-o')")
        self.assertTrue(data['supported'], data)
        self.assertEqual(data['version'], 3)
        s = data['axes'][0]['series'][0]
        self.assertEqual(s['x'], [1, 2, None, 4, 5])
        self.assertEqual(s['z'], [3, 4, None, 8, 9])
        self.assertEqual(s['source_indices'], list(range(5)))
        self.assertEqual(s['original_points'], 5)
        self.assertEqual(data['axes'][0]['dimension'], 3)

    def test_02_scatter3_sizes_and_single_color(self):
        data = self.snapshot("scatter3([1 2 3],[4 5 6],[7 8 9],[9 25 49],[.2 .4 .6],'filled')")
        self.assertTrue(data['supported'], data)
        s = data['axes'][0]['series'][0]
        self.assertEqual(s['sizes'], [9, 25, 49])
        self.assertEqual(s['marker_face_color'], [.2, .4, .6])
        self.assertEqual(s['color_data']['cdata_class'], 'double')
        self.assertEqual(s['color_data']['data']['values'], [.2, .4, .6])

    def test_03_surface_vector_layout_cell_colors_and_grid_edges(self):
        data = self.snapshot("surf([10 20 30],[40 50],[1 3 5;2 4 6])")
        self.assertTrue(data['supported'], data)
        s = data['axes'][0]['series'][0]
        self.assertEqual(s['shape'], [2, 3])
        self.assertEqual(s['x'], {'shape': [1, 3], 'order': 'column-major', 'values': [10, 20, 30]})
        self.assertEqual(s['y']['values'], [40, 50])
        self.assertEqual(s['z']['values'], [1, 2, 3, 4, 5, 6])
        self.assertEqual(s['cell_origins'], [0, 2])
        self.assertEqual(s['face_color']['data']['values'], [1, 2, 3, 4, 5, 6])
        self.assertEqual(s['cell_color_owner'], 'row-column-origin')
        self.assertEqual(s['triangle_count'], 4)
        self.assertEqual(s['edge_indices']['values'], [0, 2, 4, 0, 1, 2, 3, 1, 3, 5, 2, 3, 4, 5])

    def test_04_matrix_coordinates_and_missing_incident_cells(self):
        data = self.snapshot("surf([10 20 30;11 21 31],[40 41 42;50 51 52],[1 3 NaN;2 4 6])")
        self.assertTrue(data['supported'], data)
        s = data['axes'][0]['series'][0]
        self.assertEqual(s['coordinate_layout'], {'x': 'matrix', 'y': 'matrix'})
        self.assertEqual(s['x']['values'], [10, 11, 20, 21, 30, 31])
        self.assertEqual(s['y']['values'], [40, 50, 41, 51, 42, 52])
        self.assertEqual(s['z']['values'][4], None)
        self.assertEqual(s['cell_origins'], [0])
        self.assertEqual(s['triangle_count'], 2)
        self.assertEqual(s['edge_indices']['shape'], [5, 2])

    def test_05_mesh_white_faces_flat_edges(self):
        data = self.snapshot("mesh([10 20 30],[40 50],[1 3 5;2 4 6])")
        self.assertTrue(data['supported'], data)
        s = data['axes'][0]['series'][0]
        self.assertEqual(s['face_color'], {'mode': 'constant', 'association': 'constant', 'rgb': [1, 1, 1]})
        self.assertEqual(s['edge_color']['mode'], 'flat')
        self.assertEqual(s['mesh_style'], 'both')

    def test_06_integer_cdata_explicit_clim_and_direct_mapping(self):
        data = self.snapshot("surf([1 3 5;2 4 6],uint8([0 2 4;1 3 5])); set(gca,'clim',[-1 8]); colormap(gca,[0 0 0;1 0 0;0 1 0;0 0 1])")
        self.assertTrue(data['supported'], data)
        a = data['axes'][0]
        self.assertEqual(a['clim'], [-1, 8])
        self.assertEqual(a['colormap']['shape'], [4, 3])
        s = a['series'][0]
        self.assertEqual(s['face_color']['cdata_class'], 'uint8')
        self.assertEqual(s['face_color']['mapping'], 'scaled')
        self.assertEqual(s['cdata']['values'], [0, 1, 2, 3, 4, 5])
        data = self.snapshot("surf([1 3 5;2 4 6],uint8([0 2 4;1 3 5]),'cdatamapping','direct')")
        self.assertEqual(data['axes'][0]['series'][0]['face_color']['mapping'], 'direct')

    def test_07_colorbar_peer_and_orientation(self):
        for location, orientation in [('eastoutside', 'vertical'), ('southoutside', 'horizontal')]:
            with self.subTest(location=location):
                data = self.snapshot(f"surf([1 3 5;2 4 6]); colorbar('{location}')")
                self.assertTrue(data['supported'], data)
                bar = data['axes'][0]['colorbars'][0]
                self.assertEqual(bar['peer_axes'], 0)
                self.assertEqual(bar['orientation'], orientation)
                self.assertEqual(len(bar['position']), 4)
                self.assertGreater(len(bar['ticks']), 0)
                self.assertEqual(bar['colormap'], data['axes'][0]['colormap'])

    def test_08_camera_metadata_reversed_axes_and_aspect(self):
        data = self.snapshot("plot3(1:3,4:6,7:9);view(25,35);set(gca,'xdir','reverse','ydir','reverse','zdir','reverse','dataaspectratio',[1 2 3]);assert(isequaln(get(gca,'cameraposition'),cell2mat(__mf_figure_data__(gcf).axes{1}.camera.position)))")
        self.assertTrue(data['supported'], data)
        a = data['axes'][0]
        self.assertEqual(a['view'], [25, 35])
        self.assertEqual([a[k + 'dir'] for k in 'xyz'], ['reverse'] * 3)
        self.assertEqual(a['data_aspect_ratio'], [1, 2, 3])
        self.assertEqual(a['data_aspect_ratio_mode'], 'manual')
        self.assertEqual(a['camera']['projection'], 'orthographic')
        self.assertTrue(all(a['camera'][k] == 'auto' for k in ['position_mode', 'target_mode', 'upvector_mode', 'viewangle_mode']))

    def test_09_decimation_keeps_z_extrema_source_indices_and_gaps(self):
        data = self.snapshot("x=1:9000;y=zeros(1,9000);z=y;z(3100)=99;z(6200)=-77;x(4500:4502)=NaN;plot3(x,y,z)")
        self.assertTrue(data['supported'], data)
        s = data['axes'][0]['series'][0]
        self.assertLessEqual(s['rendered_points'], 2000)
        self.assertEqual(s['original_points'], 9000)
        self.assertTrue(s['decimated'])
        self.assertIn(3099, s['source_indices'])
        self.assertIn(6199, s['source_indices'])
        self.assertIn(4499, s['source_indices'])
        self.assertIsNone(s['x'][s['source_indices'].index(4499)])
        self.assertEqual(s['source_indices'][0], 0)
        self.assertEqual(s['source_indices'][-1], 8999)
        self.assertEqual(max(v for v in s['z'] if v is not None), 99)
        self.assertEqual(min(v for v in s['z'] if v is not None), -77)

    def test_10_extraction_changes_no_properties_or_workspace(self):
        helper = self.work / 'mf_test_no_mutation.m'
        helper.write_text("""function mf_test_no_mutation(f)
  handles=sort(findall(f)); before=cell(numel(handles),1);
  for j=1:numel(handles),before{j}=get(handles(j));endfor
  root_before=get(0,'currentfigure');vars_before=evalin('base','whos');
  data=__mf_figure_data__(f);assert(data.supported);
  assert(isequaln(vars_before,evalin('base','whos')));
  assert(isequaln(root_before,get(0,'currentfigure')));
  for j=1:numel(handles),assert(isequaln(before{j},get(handles(j))));endfor
endfunction
""")
        self.run_code("close all;figure('visible','off');surf([1 3 5;2 4 6]);colorbar;kept=magic(3);kept_handle=@sin;" +
                      f"addpath({Kernel.quote(self.work)});mf_test_no_mutation(gcf);" +
                      "assert(isequal(kept,magic(3)));assert(kept_handle(.5)==sin(.5));")

    def test_11_all_fixtures_and_reasons_from_real_serializer(self):
        folder = self.work / 'fixtures'
        folder.mkdir(exist_ok=True)
        self.run_code(f"addpath({Kernel.quote(ROOT / 'tests/fixtures/figure_v3')}); uret({Kernel.quote(folder)});")
        files = list(folder.glob('*.json'))
        self.assertEqual(len(files), 38)
        for file in files:
            data = json.loads(file.read_text())
            if file.stem in ['line2d', 'plot3_gap', 'scatter3_sizes', 'surf_vector', 'surf_matrix_nan', 'mesh_default', 'surf_integer_clim', 'colorbar', 'reversed_view',
                             'bar_grouped', 'barh_stacked', 'hist_flat', 'area_stacked', 'patch_missing_vertex',
                             'text_boxes', 'group_lines', 'figure_title']:
                self.assertTrue(data['supported'], file.name)
            else:
                self.assertFalse(data['supported'], file.name)
                self.assertEqual(data['reason_code'], file.stem)
        if shutil.which('node'):
            command = "const fs=require('fs'),p=require('path'),{validate}=require(process.argv[1]); for(const f of fs.readdirSync(process.argv[2])) {const r=validate(JSON.parse(fs.readFileSync(p.join(process.argv[2],f),'utf8')));if(!r.ok)throw Error(f+JSON.stringify(r));}"
            result = subprocess.run(['node', '-e', command, str(ROOT / 'frontend/figure_data_utils.cjs'), str(folder)], text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_12_preconversion_budgets_and_whole_figure_fallback(self):
        for code, budget in [("surf(zeros(201,200))", 'surface_vertices'),
                             ("scatter3(1:2001,1:2001,1:2001,36,[1 0 0])", 'samples'),
                             ("plot3(1:3,4:6,7:9);colormap(gca,zeros(4097,3))", 'colormap')]:
            with self.subTest(budget=budget):
                data = self.snapshot(code)
                self.assertFalse(data['supported'])
                self.assertEqual(data['axes'], [])
                self.assertEqual(data['reason_code'], 'budget_exceeded')
                self.assertEqual(data['reason_args']['budget'], budget)
        data = self.snapshot("subplot(1,2,1);plot(1:3);subplot(1,2,2);surf([1 3;2 4],'facecolor','interp')")
        self.assertEqual(data['reason_code'], 'interpolated_color')
        self.assertEqual(data['axes'], [])


if __name__ == '__main__':
    unittest.main()
