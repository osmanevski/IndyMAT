"""Serverless contour helper tests; no graphics objects or pixel claims."""
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class ContourFilledGeometry(unittest.TestCase):
    def octave(self, body):
        result = subprocess.run(
            [shutil.which("octave-cli"), "--quiet", "--no-init-file",
             "--no-site-file", "--no-history", "--eval",
             "addpath('octave');before=__go_handles__(true);" + body +
             "\nassert(isequal(before,__go_handles__(true)));"],
            cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_native_coordinates_edges_and_concave_area(self):
        self.octave("""
v=[0 0;3 0;3 3;1 1;0 3];f=1:5;
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,f,2);
assert(isempty(c)&&isequal(out,[v,zeros(5,1)]));
assert(isequal(size(t),[3 3])&&isequal(o,ones(3,1)));
assert(isequal(e,[1 2 1;2 3 1;3 4 1;4 5 1;5 1 1]));
area=0;
for i=1:rows(t)
 p=out(t(i,:),:);area+=abs(det([p(2,1:2)-p(1,1:2);p(3,1:2)-p(1,1:2)]))/2;
endfor
assert(abs(area-polyarea(v(:,1),v(:,2)))<1e-12);
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,f,2,7*ones(5,1));
assert(isempty(c)&&all(out(:,3)==7));
[out,t,o,e,c]=__mf_figure_contourf_geometry__([v,7*ones(5,1)],f,2);
assert(isempty(c)&&all(out(:,3)==7));
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,[f 1 NaN],2);
assert(isempty(c)&&rows(t)==3&&rows(e)==5);
""")

    def test_holes_are_rejected_without_partial_geometry(self):
        self.octave("""
v=[0 0;1 0;1 1;0 1];
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,1:4,NaN);
assert(strcmp(c,'contour_filled')&&strcmp(a.detail,'background_hole')&&isempty(t));
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__([v;NaN NaN],1:5,2);
assert(strcmp(c,'contour_filled')&&strcmp(a.detail,'holes_or_nonfinite')&&isempty(t));
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,[1 2 3;1 3 4],2);
assert(strcmp(c,'contour_filled')&&strcmp(a.detail,'compound_faces')&&isempty(t));
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,[1 NaN 2 3],2);
assert(strcmp(c,'invalid_data')&&isempty(t));
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,[1 3 2 4],2);
assert(strcmp(c,'unsupported_patch')&&isempty(t)&&isempty(e));
""")

    def test_defensive_shapes_and_z_consistency(self):
        self.octave("""
v=[0 0;1 0;0 1];
for bad={[],[1 2],Inf,1+i,sparse(1),uint64(2)^uint64(63)}
 [out,t,o,e,c]=__mf_figure_contourf_geometry__(v,1:3,bad{1});
 assert(strcmp(c,'invalid_data')&&isempty(t));
endfor
for bad={[],{1},sparse(v),v+i,ones(2,2,2),ones(3,4)}
 [out,t,o,e,c]=__mf_figure_contourf_geometry__(bad{1},1:3,1);
 assert(strcmp(c,'invalid_data')&&isempty(t));
endfor
for bad={[],{1},sparse(1:3),ones(2,2,2)}
 [out,t,o,e,c]=__mf_figure_contourf_geometry__(v,bad{1},1);
 assert(strcmp(c,'invalid_data')&&isempty(t));
endfor
for bad={[0 0],{0},[0 Inf 0],ones(1,1,3),sparse([0 0 0])}
 [out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,1:3,1,bad{1});
 assert(strcmp(c,'invalid_data')&&strcmp(a.property,'zdata')&&isempty(t));
endfor
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__([v,ones(3,1)],1:3,1,zeros(3,1));
assert(strcmp(c,'invalid_data')&&strcmp(a.property,'zdata'));
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,[1 2 4],1);
assert(strcmp(c,'invalid_index')&&isempty(t));
""")

    def test_polygon_budget_and_pre_expansion_guard(self):
        self.octave("""
theta=(0:63)'*2*pi/64;v=[cos(theta),sin(theta)];
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,1:64,1);
assert(isempty(c)&&rows(t)==62&&rows(e)==64);
[out,t,o,e,c]=__mf_figure_contourf_geometry__(v,[1:64 1],1);
assert(isempty(c)&&rows(t)==62);
theta=(0:64)'*2*pi/65;v=[cos(theta),sin(theta)];
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,1:65,1);
assert(strcmp(c,'budget_exceeded')&&strcmp(a.budget,'polygon_vertices')&&a.limit==64);
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(zeros(40001,2),1:3,1);
assert(strcmp(c,'budget_exceeded')&&strcmp(a.budget,'patch_vertices')&&columns(out)==2);
""")

    def test_serializer_load_is_read_only_without_figures(self):
        self.octave("""
root_before=get(0);d=__mf_figure_data__(0);
assert(~d.supported&&strcmp(d.reason_code,'no_axes'));
assert(isequaln(root_before,get(0)));
""")


class ContourSourceContract(unittest.TestCase):
    def test_native_order_and_3d_text_boundary(self):
        source = (ROOT / "octave/__mf_figure_data__.m").read_text()
        contour_branch = source.split("if any(strcmp(kind,{'contour','contourf'}))", 1)[1]
        contour_branch = contour_branch.split("for wanted=allowed", 1)[0]
        self.assertIn("for c=fliplr(children)", contour_branch)
        self.assertIn("role='contour_label'", contour_branch)
        self.assertIn("leaves=[leaves,found];continue;", contour_branch)
        self.assertIn("s.level=double(level)", source)
        self.assertIn("s.contour_group=group_index-1", source)
        self.assertIn("code='contour_labels';args=struct('property','3d_text_depth')", source)
        self.assertNotIn("if ~strcmp(__mf_prop__(h,'showtext','off'),'off')", source)


if __name__ == "__main__":
    unittest.main()
