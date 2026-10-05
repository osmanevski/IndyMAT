"""Automatic histogram measurements from MATLAB R2025b, wave 18 Z4.

Expected arrays are copied from olcum/olc_sinyal2.txt, not read at test time.
The scratch measurement directory is not required to run these tests.
"""
import shutil
import subprocess
import unittest
from pathlib import Path
from scripts import compat

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class DataSignalAutoHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = compat.setup() + body
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--eval', command],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_measured_hist_integers(self):
        # hist integers: N2, Xedges, Yedges, N1, edges1.
        self.run_octave('x=[0 1 1 2 3 3 4 5]; y=[1 0 2 2 4 3 5 4]; [n,xe,ye]=histcounts2(x,y); [n1,e1]=histcounts(x); assert(isequal(n,[0 1 0 0 0 0;1 0 1 0 0 0;0 0 1 0 0 0;0 0 0 1 1 0;0 0 0 0 0 1;0 0 0 0 1 0])); assert(isequal(xe,[-0.5 0.5 1.5 2.5 3.5 4.5 5.5])); assert(isequal(ye,[-0.5 0.5 1.5 2.5 3.5 4.5 5.5])); assert(isequal(n1,[1 2 1 2 1 1])); assert(isequal(e1,[-0.5 0.5 1.5 2.5 3.5 4.5 5.5]));')

    def test_measured_hist_wide(self):
        # hist wide: N2, Xedges, Yedges, N1, edges1.
        self.run_octave('x=[-1000 -100 -1 0 1 10 100 1000]; y=[0 1 5 20 80 300 900 2000]; [n,xe,ye]=histcounts2(x,y); [n1,e1]=histcounts(x); assert(isequal(n,[3 0;4 1])); assert(isequal(xe,[-1000 0 1000])); assert(isequal(ye,[0 1000 2000])); assert(isequal(n1,[3 5])); assert(isequal(e1,[-1000 0 1000]));')

    def test_measured_hist_constant(self):
        # hist constant: N2, Xedges, Yedges, N1, edges1.
        self.run_octave('x=0:7; y=3*ones(1,8); [n,xe,ye]=histcounts2(x,y); [n1,e1]=histcounts(x); assert(isequal(n,[1;1;1;1;1;1;1;1])); assert(isequal(xe,[-0.5 0.5 1.5 2.5 3.5 4.5 5.5 6.5 7.5])); assert(isequal(ye,[2.5 3.5])); assert(isequal(n1,[1 1 1 1 1 1 1 1])); assert(isequal(e1,[-0.5 0.5 1.5 2.5 3.5 4.5 5.5 6.5 7.5]));')

    def test_measured_hist_negative(self):
        # hist negative: N2, Xedges, Yedges, N1, edges1.
        self.run_octave('x=[-9 -8 -7 -5 -4 -3 -2 -1]; y=[-12 -10 -8 -8 -6 -4 -2 -1]; [n,xe,ye]=histcounts2(x,y); [n1,e1]=histcounts(x); assert(isequal(n,[1 0 0 0 0 0 0 0 0 0 0 0;0 0 1 0 0 0 0 0 0 0 0 0;0 0 0 0 1 0 0 0 0 0 0 0;0 0 0 0 0 0 0 0 0 0 0 0;0 0 0 0 1 0 0 0 0 0 0 0;0 0 0 0 0 0 1 0 0 0 0 0;0 0 0 0 0 0 0 0 1 0 0 0;0 0 0 0 0 0 0 0 0 0 1 0;0 0 0 0 0 0 0 0 0 0 0 1])); assert(isequal(xe,[-9.5 -8.5 -7.5 -6.5 -5.5 -4.5 -3.5 -2.5 -1.5 -0.5])); assert(isequal(ye,[-12.5 -11.5 -10.5 -9.5 -8.5 -7.5 -6.5 -5.5 -4.5 -3.5 -2.5 -1.5 -0.5])); assert(isequal(n1,[1 1 1 0 1 1 1 1 1])); assert(isequal(e1,[-9.5 -8.5 -7.5 -6.5 -5.5 -4.5 -3.5 -2.5 -1.5 -0.5]));')

    def test_measured_hist_formula100(self):
        # hist formula100: N2, Xedges, Yedges, N1, edges1.
        self.run_octave('x=mod(7*(1:100),23)/3; y=mod(11*(1:100),31)/5-3; [n,xe,ye]=histcounts2(x,y); [n1,e1]=histcounts(x); assert(isequal(n,[5 7 10 4;4 9 7 6;3 10 8 5;5 6 8 3])); assert(isequal(xe,[0 2 4 6 8])); assert(isequal(ye,[-4 -2 0 2 4])); assert(isequal(n1,[26 26 26 22])); assert(isequal(e1,[0 2 4 6 8]));')

    def test_explicit_edges_keep_counts_indices_and_normalization(self):
        self.run_octave("x=[0 1 2 NaN -1]; y=[0 1 2 1 0]; [n,xe,ye,bx,by]=histcounts2(x,y,[0 1 2],[0 1 2]); assert(isequal(n,[1 0;0 2])); assert(isequal(xe,[0 1 2]) && isequal(ye,[0 1 2])); assert(isequal(bx,[1 2 2 0 0]) && isequal(by,[1 2 2 0 0])); assert(isequal(histcounts2(x,y,xe,ye,'Normalization','probability'),n/5));")

    def test_auto_bins_indices_shapes_and_single(self):
        self.run_octave("x=single([0 1;1 2]); y=single([2 1;1 0]); [n,xe,ye,bx,by]=histcounts2(x,y); assert(isa(n,'double') && isa(xe,'single') && isa(ye,'single')); assert(isequal(n,[0 0 1;0 2 0;1 0 0])); assert(isequal(bx,[1 2;2 3]) && isequal(by,[3 2;2 1])); assert(isequal(histcounts2(x,y,'Normalization','probability'),n/4));")

    def test_auto_empty_and_missing(self):
        self.run_octave("[n,xe,ye,bx,by]=histcounts2([],[]); assert(isequal(n,0) && isequal(xe,[0 1]) && isequal(ye,[0 1])); assert(isempty(bx) && isempty(by)); [n,xe,ye,bx,by]=histcounts2([1 NaN 2],[2 0 Inf]); assert(sum(n(:))==1); assert(isequal(bx,[1 0 0]) && isequal(by,[3 0 0]));")

    def test_measured_design_fir_norm(self):
        # design lowpassfir norm: every property, coefficient, tf/filter/filtfilt/freqz label.
        self.run_octave(
            "d=designfilt('lowpassfir','PassbandFrequency',.2,'StopbandFrequency',.65,'PassbandRipple',1,'StopbandAttenuation',40); "
            'x=[1 0 -1 2 0 1 -2 0 3 1 0 -1 2 0 1 -2 0 1 0 2 -1 0 1 0]; [b,a]=tf(d); [h,w]=freqz(d,8); '
            "value=class(d); expected='digitalFilter'; "
            "assert(isa(value,'char') && isequal(size(value),[1 13])); "
            'assert(strcmp(value,expected)); '
            "failed=false; try, value=d.FilterOrder; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "value=d.DesignMethod; expected='equiripple'; "
            "assert(isa(value,'char') && isequal(size(value),[1 10])); "
            'assert(strcmp(value,expected)); '
            "value=d.FrequencyResponse; expected='lowpass'; "
            "assert(isa(value,'char') && isequal(size(value),[1 7])); "
            'assert(strcmp(value,expected)); '
            "value=d.ImpulseResponse; expected='fir'; "
            "assert(isa(value,'char') && isequal(size(value),[1 3])); "
            'assert(strcmp(value,expected)); '
            'value=d.PassbandFrequency; expected=0.2; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.StopbandFrequency; expected=0.65; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.PassbandRipple; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.StopbandAttenuation; expected=40; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.SampleRate; expected=2; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            "failed=false; try, value=d.HalfPowerFrequency; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.PassbandFrequency1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.PassbandFrequency2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandFrequency1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandFrequency2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandAttenuation1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandAttenuation2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            'value=filtord(d); expected=8; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Coefficients); expected=[1 9]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients(1,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients(end,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Numerator); expected=[1 9]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator(1,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator(end,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Denominator); expected=[1 1]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator(1,:); expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator(end,:); expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=b; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=a; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=filter(d,x); expected=[-0.023632279 -0.031577897 0.1000259 0.27265726 0.26415006 0.12915496 0.26506865 0.62702654 0.54132179 -0.1069892 -0.36097557 0.41749889 1.2993905 1.1687053 0.40560039 0.17033496 0.55612971 0.58858634 -0.020201129 -0.52944948 -0.29873837 0.40369947 0.80452938 0.6111353]; '
            "assert(isa(value,'double') && isequal(size(value),[1 24])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            "failed=false; try, value=filtfilt(d,x); catch e, failed=strcmp(e.identifier,'signal:filtfilt:InvalidDimensionsDataShortForFiltOrder'); end; assert(failed); "
            'value=h; expected=[1.0227542;0-1.0203578i;-0.90340181;0+0.57469981i;0.20364767;0-0.016625347i;0.0014737455;0+0.0031149487i]; '
            "assert(isa(value,'double') && isequal(size(value),[8 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=w; expected=[0;0.39269908;0.78539816;1.1780972;1.5707963;1.9634954;2.3561945;2.7488936]; '
            "assert(isa(value,'double') && isequal(size(value),[8 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
        )

    def test_measured_design_fir_fs1000(self):
        # design lowpassfir fs1000: every property, coefficient, tf/filter/filtfilt/freqz label.
        self.run_octave(
            "d=designfilt('lowpassfir','PassbandFrequency',100,'StopbandFrequency',325,'PassbandRipple',1,'StopbandAttenuation',40,'SampleRate',1000); "
            'x=[1 0 -1 2 0 1 -2 0 3 1 0 -1 2 0 1 -2 0 1 0 2 -1 0 1 0]; [b,a]=tf(d); [h,w]=freqz(d,8); '
            "value=class(d); expected='digitalFilter'; "
            "assert(isa(value,'char') && isequal(size(value),[1 13])); "
            'assert(strcmp(value,expected)); '
            "failed=false; try, value=d.FilterOrder; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "value=d.DesignMethod; expected='equiripple'; "
            "assert(isa(value,'char') && isequal(size(value),[1 10])); "
            'assert(strcmp(value,expected)); '
            "value=d.FrequencyResponse; expected='lowpass'; "
            "assert(isa(value,'char') && isequal(size(value),[1 7])); "
            'assert(strcmp(value,expected)); '
            "value=d.ImpulseResponse; expected='fir'; "
            "assert(isa(value,'char') && isequal(size(value),[1 3])); "
            'assert(strcmp(value,expected)); '
            'value=d.PassbandFrequency; expected=100; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.StopbandFrequency; expected=325; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.PassbandRipple; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.StopbandAttenuation; expected=40; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.SampleRate; expected=1000; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            "failed=false; try, value=d.HalfPowerFrequency; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.PassbandFrequency1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.PassbandFrequency2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandFrequency1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandFrequency2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandAttenuation1; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            "failed=false; try, value=d.StopbandAttenuation2; catch e, failed=strcmp(e.identifier,'MATLAB:noSuchMethodOrField'); end; assert(failed); "
            'value=filtord(d); expected=8; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Coefficients); expected=[1 9]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients(1,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Coefficients(end,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Numerator); expected=[1 9]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator(1,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Numerator(end,:); expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=size(d.Denominator); expected=[1 1]; '
            "assert(isa(value,'double') && isequal(size(value),[1 2])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator(1,:); expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=d.Denominator(end,:); expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=b; expected=[-0.023632279 -0.031577897 0.076393621 0.28834392 0.40369947 0.28834392 0.076393621 -0.031577897 -0.023632279]; '
            "assert(isa(value,'double') && isequal(size(value),[1 9])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=a; expected=1; '
            "assert(isa(value,'double') && isequal(size(value),[1 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=filter(d,x); expected=[-0.023632279 -0.031577897 0.1000259 0.27265726 0.26415006 0.12915496 0.26506865 0.62702654 0.54132179 -0.1069892 -0.36097557 0.41749889 1.2993905 1.1687053 0.40560039 0.17033496 0.55612971 0.58858634 -0.020201129 -0.52944948 -0.29873837 0.40369947 0.80452938 0.6111353]; '
            "assert(isa(value,'double') && isequal(size(value),[1 24])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            "failed=false; try, value=filtfilt(d,x); catch e, failed=strcmp(e.identifier,'signal:filtfilt:InvalidDimensionsDataShortForFiltOrder'); end; assert(failed); "
            'value=h; expected=[1.0227542;0-1.0203578i;-0.90340181;0+0.57469981i;0.20364767;0-0.016625347i;0.0014737455;0+0.0031149487i]; '
            "assert(isa(value,'double') && isequal(size(value),[8 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
            'value=w; expected=[0;0.39269908;0.78539816;1.1780972;1.5707963;1.9634954;2.3561945;2.7488936]; '
            "assert(isa(value,'double') && isequal(size(value),[8 1])); "
            'assert(all(abs(value(:)-expected(:))<=5e-8*abs(expected(:))+5e-14)); '
        )

    def test_fir_object_numeric_methods_and_rejected_forms(self):
        self.run_octave("d=designfilt('lowpassfir','PassbandFrequency',.2,'StopbandFrequency',.65,'PassbandRipple',1,'StopbandAttenuation',40); [b,a]=tf(d); x=reshape(sin(1:100),50,2); [y,z]=filter(d,x); [yn,zn]=filter(b,a,x); assert(isequal(y,yn) && isequal(z,zn)); assert(isequal(filtfilt(d,x),filtfilt(b,a,x))); [h,w]=freqz(d,32); [hn,wn]=freqz(b,a,32); assert(isequal(h,hn) && isequal(w,wn)); failed=false; try,d.Numerator=1;catch,failed=true;end;assert(failed); for action={'filter(repmat(d,1,2),x)','filter(d,x,[])','freqz(d)','freqz(d,[.1 .2])'}, failed=false;try,eval(action{1});catch e,failed=~isempty(strfind(e.message,'digitalFilter'));end;assert(failed);end; for args={{'lowpassiir','FilterOrder',4,'HalfPowerFrequency',.3},{'lowpassfir','FilterOrder',10,'CutoffFrequency',.4},{'lowpassfir','PassbandFrequency',.3,'StopbandFrequency',.2,'PassbandRipple',1,'StopbandAttenuation',40}}, failed=false; try,designfilt(args{1}{:});catch e,failed=~isempty(strfind(e.message,'designfilt:'));end;assert(failed);end;")

    def test_fir_neighbor_specifications_meet_both_bands(self):
        # Dense-grid verification independent of the designer's extrema solver.
        self.run_octave("for specs={[.2 .6 1 40],[.1 .7 2 20]}, s=specs{1}; d=designfilt('lowpassfir','PassbandFrequency',s(1),'StopbandFrequency',s(2),'PassbandRipple',s(3),'StopbandAttenuation',s(4)); [b,a]=tf(d); w=linspace(0,pi,20001)'; h=abs(freqz(b,a,w)); ripple=(10^(s(3)/20)-1)/(10^(s(3)/20)+1); assert(max(abs(h(w<=pi*s(1))-1))<=ripple); assert(max(h(w>=pi*s(2)))<=10^(-s(4)/20)); assert(isequal(b,fliplr(b))); end;")
