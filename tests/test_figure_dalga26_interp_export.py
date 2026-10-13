"""Pure Octave descriptor/colour-read checks; no engine graphics objects."""
from pathlib import Path
import json
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class InterpolatedExport(unittest.TestCase):
    def octave(self, body):
        result = subprocess.run(
            [shutil.which("octave-cli"), "--quiet", "--no-init-file",
             "--no-site-file", "--no-history", "--eval",
             "addpath('octave');before=__go_handles__(true);" + body +
             "\nassert(isequal(before,__go_handles__(true)));"],
            cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_indexed_descriptor_roundtrip_keeps_vertex_class_and_mapping(self):
        stdout = self.octave("""
[d,c]=__mf_figure_patch_interp_color__(int16([-4;7;9]),'direct',3);
assert(isempty(c));disp(jsonencode(d));
""")
        descriptor = json.loads(stdout)
        self.assertEqual(descriptor, {
            "mode": "interp", "association": "vertex", "encoding": "indexed",
            "mapping": "direct", "cdata_class": "int16",
            "data": {"shape": [3, 1], "order": "column-major", "values": [-4, 7, 9]}})

    def test_rgb_classes_preserve_raw_planes(self):
        self.octave("""
for item={eye(3),single(eye(3)),uint8(255*eye(3)),uint16(65535*eye(3))}
 raw=item{1};[d,c]=__mf_figure_patch_interp_color__(raw,'scaled',3);
 assert(isempty(c)&&strcmp(d.mode,'interp')&&strcmp(d.association,'vertex'));
 assert(strcmp(d.encoding,'truecolor')&&strcmp(d.cdata_class,class(raw)));
 assert(isequal(cell2mat(d.data.shape),[3 1 3]));
 assert(isequal(cell2mat(d.data.values),double(raw(:).')));
endfor
""")

    def test_shapes_nonfinite_and_out_of_range_are_rejected(self):
        self.octave("""
for raw={1,[1 2 3],ones(3,2),ones(3,1,3),sparse(ones(3,1)),ones(3,1)+i,{1;2;3}}
 [d,c]=__mf_figure_patch_interp_color__(raw{1},'scaled',3);assert(strcmp(c,'patch_colors'));
endfor
for raw={[1;NaN;3],[1;Inf;3],int16(eye(3)),2*eye(3),-eye(3),uint64([0;1;2])*uint64(flintmax())}
 [d,c]=__mf_figure_patch_interp_color__(raw{1},'scaled',3);assert(strcmp(c,'unsupported_color'));
endfor
[d,c]=__mf_figure_patch_interp_color__((1:3)','other',3);assert(strcmp(c,'unsupported_color'));
[d,c]=__mf_figure_patch_interp_color__((1:3)','scaled',40001);assert(strcmp(c,'invalid_data'));
""")

    def test_surface_interpolated_edges_read_both_endpoints(self):
        self.octave("""
c=[1 NaN;2 4];e=[0 2];
assert(__mf_figure_surface_colors_finite__(c,[],e,2,'none','flat'));
assert(~__mf_figure_surface_colors_finite__(c,[],e,2,'none','interp'));
assert(__mf_figure_surface_colors_finite__(c,[],[0 1],2,'none','interp'));
assert(__mf_figure_surface_colors_finite__(c,0,zeros(0,2),2,'flat','none'));
assert(~__mf_figure_surface_colors_finite__(c,0,zeros(0,2),2,'interp','none'));
assert(__mf_figure_surface_colors_finite__(c,[],zeros(0,2),2,'interp','interp'));
c=repmat(ones(2),[1 1 3]);c(1,2,3)=NaN;
assert(~__mf_figure_surface_colors_finite__(c,[],e,2,'none','interp'));
assert(__mf_figure_surface_colors_finite__(c,[],[0 1],2,'none','interp'));
assert(~__mf_figure_surface_colors_finite__(c,[],[0 4],2,'none','interp'));
""")

    def test_serializer_load_still_has_no_graphics_side_effects(self):
        self.octave("""
d=__mf_figure_data__(0);assert(~d.supported&&strcmp(d.reason_code,'no_axes'));
""")


if __name__ == "__main__":
    unittest.main()
