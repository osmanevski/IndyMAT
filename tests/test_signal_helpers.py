import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class SignalHelperTests(unittest.TestCase):
    def run_octave(self, body):
        compat = ROOT / 'octave' / 'compat'
        packages = ROOT / '.packages'
        setup = "addpath('%s'); pkg('prefix','%s','%s'); pkg('local_list','%s'); pkg load signal; " % (
            compat, packages, packages / '.arch', packages / 'octave_packages')
        return subprocess.run([OCTAVE, '--quiet', '--eval', setup + body], cwd=ROOT,
                              text=True, capture_output=True, timeout=20)

    def test_ac2poly_package_function_is_not_shadowed(self):
        result = self.run_octave("assert(!isempty(strfind(which('ac2poly'),fullfile('.packages','signal-1.4.8'))));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_spectrogram_rejects_unimplemented_plot_and_axis_forms_clearly(self):
        result = self.run_octave("failed=false; try, spectrogram(1:16,4,2,8,1,'yaxis'); catch err, failed=!isempty(strfind(err.message,'spectrogram:')); end_try_catch; assert(failed);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_spectrogram_requires_complete_segment(self):
        result = self.run_octave("failed=false; try, s=spectrogram(1:3,4,0,4,1); catch err, failed=!isempty(strfind(err.message,'complete segment')); end_try_catch; assert(failed);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_envelope_rms_mode_and_unsupported_modes(self):
        result = self.run_octave("x=[0 1 0 -1 0 1 0 -1 0 1 0 -1]; [u,l]=envelope(x,3,'rms'); assert(norm(u-[0.70710678 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.70710678])<1e-6 && norm(l+u)<1e-12); failed=false; try, envelope(x,4,'peak'); catch err, failed=!isempty(strfind(err.message,'envelope:')); end_try_catch; assert(failed);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unmeasured_design_helpers_remain_absent(self):
        result = self.run_octave("assert(exist('gaussfir','file')==0 && exist('intfilt','file')==0); assert(norm(poly2ac([1 -0.5],0.75)-[1;0.5])<1e-12);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rc_polynomial_roundtrip_and_autocorrelation_recursion(self):
        result = self.run_octave("k=[0.2 -0.3 0.1]; a=rc2poly(k); assert(norm(poly2rc(a)-k(:))<1e-12); [r,U,kr]=rlevinson([1 -0.5],0.75); assert(norm(r-[1;0.5])<1e-12 && isequal(U,[1 -0.5;0 1]) && abs(kr+0.5)<1e-12);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_fir_phase_classification(self):
        result = self.run_octave("assert(firtype([1 2 1])==1 && firtype([1 2 2 1])==2 && firtype([1 0 -1])==3 && firtype([1 2 -2 -1])==4); assert(islinphase([1 2 1]) && ~islinphase([1 2 3]));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_goertzel_and_transfer_roots(self):
        result = self.run_octave("x=(1:8)'; k=[1 4 8]; y=goertzel(x,k); xf=fft(x); assert(norm(y(:)-xf(k(:)))<1e-10); [z,p,g]=tf2zpk([2 -2],[2 -1]); assert(abs(z-1)<1e-12 && abs(p-0.5)<1e-12 && g==1);")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
