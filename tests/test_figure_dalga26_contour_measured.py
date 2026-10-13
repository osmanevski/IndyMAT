"""Offline replay of native contour properties supplied by the orchestrator.

No graphics objects are created. Fixture provenance records original log hash
and display precision; these tests do not claim renderer/pixel acceptance.
"""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/contourf_measured_dalga26.json"


def matrix(rows):
    return "[" + ";".join(" ".join("NaN" if v is None else repr(v) for v in row) for row in rows) + "]"


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class MeasuredContourAdmission(unittest.TestCase):
    def run_octave(self, body):
        result = subprocess.run(
            [shutil.which("octave-cli"), "--quiet", "--no-init-file", "--no-site-file",
             "--no-history", "--eval", "addpath('octave');before=__go_handles__(true);" +
             body + "\nassert(isequal(before,__go_handles__(true)));"],
            cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_measured_finite_bands_keep_all_source_vertices_and_native_z(self):
        cases = json.loads(FIXTURE.read_text())["cases"]
        for case in (cases[0], cases[2]):
            for i, patch in enumerate(case["patches_in_draw_order"]):
                with self.subTest(case=case["name"], patch=i):
                    v, f, level, z = (matrix(patch[field]["rows"]) for field in
                                      ("vertices", "faces", "facevertexcdata", "zdata"))
                    canonical = "[1 2 3 5 7 8]" if i == 0 else "[1 2 4 6]"
                    self.run_octave(f"""
raw={v};faces={f};z={z};
[v,t,o,e,c,a,nf]=__mf_figure_contourf_geometry__(raw,faces,{level},z);
assert(isempty(c)&&isequal(nf,{canonical}));
assert(isequal(v(:,1:columns(raw)),raw)&&rows(v)==rows(raw));
assert(rows(t)==numel(nf)-2&&rows(e)==numel(nf)&&all(o==1));
assert(isequal(e,[nf(:),nf([2:end,1]).',ones(numel(nf),1)]));
if isempty(z),assert(all(v(:,3)==0));else,assert(isequal(v(:,3),z));endif
area=0;
for j=1:rows(t)
 p=v(t(j,:),1:2);area+=abs(det([p(2,:)-p(1,:);p(3,:)-p(1,:)]))/2;
endfor
assert(abs(area-polyarea(raw(:,1),raw(:,2)))<1e-12);
""")

    def test_measured_hole_background_is_still_explicit_fallback(self):
        patches = json.loads(FIXTURE.read_text())["cases"][1]["patches_in_draw_order"]
        for i, patch in enumerate(patches):
            v, f, level = (matrix(patch[field]["rows"]) for field in
                           ("vertices", "faces", "facevertexcdata"))
            expectation = ("assert(strcmp(c,'contour_filled')&&strcmp(a.detail,'background_hole')&&isempty(t));"
                           if i == 2 else "assert(isempty(c)&&~isempty(t));")
            self.run_octave(f"[v,t,o,e,c,a,nf]=__mf_figure_contourf_geometry__({v},{f},{level});" + expectation)

    def test_exact_terminal_duplicates_only_and_ordinary_patch_rule_unchanged(self):
        self.run_octave("""
v=[0 0;1 0;1 1;0 1;0 0];
[out,t,o,e,c,a,nf]=__mf_figure_contourf_geometry__(v,1:5,1);
assert(isempty(c)&&isequal(nf,1:4)&&isequal(out(:,1:2),v));
[t,o,e,c]=__mf_patch3d_geometry__([v,zeros(5,1)],1:5);
assert(strcmp(c,'unsupported_patch'));
[out,t,o,e,c]=__mf_figure_contourf_geometry__([0 0;1 0;1 1;1 0;0 1],1:5,1);
assert(strcmp(c,'unsupported_patch')&&isempty(t));
[out,t,o,e,c]=__mf_figure_contourf_geometry__([0 0;1 0;1 1;0 1;1e-12 0],1:5,1);
assert(strcmp(c,'unsupported_patch')&&isempty(t));
theta=(0:64)'*2*pi/65;v=[cos(theta),sin(theta)];
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,1:65,1);
assert(strcmp(c,'budget_exceeded')&&a.limit==64);
theta=(0:63)'*2*pi/64;v=[cos(theta),sin(theta)];
rawfaces=reshape(repmat(1:64,2,1),1,[]);
[out,t,o,e,c,a,nf]=__mf_figure_contourf_geometry__(v,[rawfaces 1],1);
assert(isempty(c)&&isequal(nf,1:64)&&rows(t)==62&&rows(out)==64);
[out,t,o,e,c,a]=__mf_figure_contourf_geometry__(v,ones(1,40002),1);
assert(strcmp(c,'budget_exceeded')&&strcmp(a.budget,'contour_source_indices')&&a.limit==40001);
""")

    def test_numeric_replay_of_native_two_by_two_faces_at_full_precision(self):
        # Numerical contourc replay, not a graphics-object measurement. Native
        # __contour__ excludes its final contour-matrix point from faces even
        # when that point differs from the first in floating-point arithmetic.
        self.run_octave("""
z=[1 2;3 4];levels=linspace(1,4,12);levels=levels(1:end-1);
[x,y]=meshgrid(0:3);padded=-Inf(4);padded(2:3,2:3)=z;
c=contourc(x,y,padded,levels);i=1;count=0;
while i<columns(c)
 n=c(2,i);level=c(1,i);raw=c(:,i+(1:n)).';i+=n+1;
 if all(raw(1,:)==raw(end,:)),raw(end,:)=[];endif
 [out,t,o,e,code,args,nf]=__mf_figure_contourf_geometry__(raw,1:(n-1),level);
 assert(isempty(code)&&~isempty(t)&&isequal(out(:,1:2),raw));
 assert(numel(nf)>=3&&numel(nf)<=5);count+=1;
endwhile
assert(count==11);
""")


class ContourPriorityContract(unittest.TestCase):
    def test_labels_preflight_before_geometry_and_raw_faces_survive(self):
        source = (ROOT / "octave/__mf_figure_data__.m").read_text()
        label_guard = source.index("if contour_3d&&has_text")
        patch_preflight = source.index("elseif strcmp(type,'patch')")
        self.assertLess(label_guard, patch_preflight)
        self.assertIn("data=__mf_fail__(data,'contour_labels',struct('property','3d_text_depth'))", source)
        self.assertIn("s.source_faces=__mf_descriptor__(source_faces-1)", source)


if __name__ == "__main__":
    unittest.main()
