"""Pure geometry checks for native contour rings; never creates graphics."""
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/contourf_round3_native.json"


@unittest.skipUnless(shutil.which("octave-cli"), "octave-cli unavailable")
class ContourRound3(unittest.TestCase):
    def octave(self, body):
        result = subprocess.run(
            [shutil.which("octave-cli"), "--quiet", "--no-init-file",
             "--no-site-file", "--no-history", "--eval",
             "addpath('octave');before=__go_handles__(true);" + body +
             "\nassert(isequal(before,__go_handles__(true)));"],
            cwd=ROOT, capture_output=True, text=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_fixture_rings_and_measured_corner_cases(self):
        fixture = str(FIXTURE).replace("'", "''")
        stdout = self.octave(f"""
f=jsondecode(fileread('{fixture}'));
assert(numel(f.cases)==4);
all_count=0;noarea_count=0;largest=0;
for ci=1:numel(f.cases)
  item=f.cases(ci).record;
  started=tic();
  for pi=1:numel(item.patches_in_draw_order)
    p=item.patches_in_draw_order(pi);
    level=p.facevertexcdata;if isempty(level),level=NaN;endif
    [v,face,no_area,code,args]=__mf_figure_contour_ring__(...
      p.vertices,p.faces(:).',level,p.zdata);
    assert(isempty(code),[f.cases(ci).name ' patch ' num2str(pi) ': ' code ' ' jsonencode(args)]);
    assert(columns(v)==3&&numel(face)>=1&&all(face>=1)&&all(face<=rows(v)));
    all_count+=1;noarea_count+=no_area;largest=max(largest,numel(face));
  endfor
  elapsed=toc(started);
  if ci==1||ci==2
    fprintf('D26 PURE CONTOUR REPLAY: %s; patches=%d; helper_seconds=%.6f; native_serialization_timing=UNVERIFIED\\n',...
      f.cases(ci).name,numel(item.patches_in_draw_order),elapsed);
  endif
endfor
assert(all_count==sum(arrayfun(@(c)numel(c.record.patches_in_draw_order),f.cases)));
assert(largest>64,'native rings must not inherit the former 64-corner cap');
assert(noarea_count>0,'measured ramp must retain no-area native rings');
% NaN contour level represents a native background level and remains valid.
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;1 0;1 1],[1 2 3],NaN,[]);
assert(isempty(code)&&~no_area&&numel(face)==3);
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;1 0;1 1],[1 2 3],intmax('uint64'),[]);
assert(strcmp(code,'invalid_data')&&strcmp(args.property,'contour.level'));
% A collapsed native ring is retained explicitly as no-area.
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;0 0;0 0],[1 2 3],0,[]);
assert(isempty(code)&&no_area&&numel(face)==1);
% Malformed real children remain rejected rather than silently repaired.
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;NaN 1;1 0],[1 2 3],0,[]);
assert(strcmp(code,'contour_filled')&&strcmp(args.property,'vertices'));
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;1 0;1 1],[1 2 4],0,[]);
assert(strcmp(code,'invalid_index'));
[v,face,no_area,code,args]=__mf_figure_contour_ring__(zeros(100001,2),1:3,0,[]);
assert(strcmp(code,'budget_exceeded')&&strcmp(args.budget,'vertices')&&args.actual==100001&&args.limit==100000);
[v,face,no_area,code,args]=__mf_figure_contour_ring__([0 0;1 0;1 1],ones(1,100002),0,[]);
assert(strcmp(code,'budget_exceeded')&&strcmp(args.budget,'contour_source_indices')&&args.actual==100002&&args.limit==100001);
fprintf('D26 PURE CONTOUR PASS: cases=4; patches=%d; no_area=%d; largest_corners=%d; graphics=none\\n',all_count,noarea_count,largest);
""")
        self.assertIn("native_serialization_timing=UNVERIFIED", stdout)
        self.assertIn("D26 PURE CONTOUR PASS", stdout)
        print(stdout, end="")


if __name__ == "__main__":
    unittest.main()
