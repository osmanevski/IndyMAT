import unittest
from pathlib import Path
from scripts import compat

ROOT = Path(__file__).resolve().parents[1]


class SignalDataHelperTests(unittest.TestCase):
    def run_octave(self, body):
        import tempfile
        with tempfile.TemporaryDirectory(prefix='z1-test-') as folder:
            path = Path(folder) / 'test_z1.m'
            path.write_text(compat.setup() + body, encoding='utf-8')
            output, status = compat.octave_result(path, folder, 60)
        self.assertEqual(status, 0, output)

    def test_measured_gaussian_coefficients_and_dc_gain(self):
        self.run_octave("""
        expected=[0.0006043869 0.12596704 0.74685715 0.12596704 0.0006043869];
        assert(norm(gaussfir(.5,1,2)-expected,inf)<5e-9);
        b=gaussfir(.5); assert(isequal(size(b),[1 13]));
        assert(abs(b(7)-.74685703)<5e-9 && abs(b(6)-.12596702)<5e-9);
        assert(isequal(gaussfir(1e308,1,2),[0 0 1 0 0]));
        for bt=[.1 .3 .7 2]
          b=gaussfir(bt,2,3); assert(abs(sum(b)-1)<1e-14 && isequal(b,fliplr(b)));
          assert(all(b>=0) && numel(b)==13);
        endfor
        """)

    def test_measured_bandlimited_interpolator(self):
        self.run_octave("""
        expected=[.023757056 0 -.12438749 0 .60157238 1 .60157238 0 -.12438749 0 .023757056];
        b=intfilt(2,3,.5); assert(norm(b-expected,inf)<5e-9);
        assert(isequal(b,fliplr(b)));
        for l=[2 3 4]
          b=intfilt(l,2,1); t=(-(2*l-1):(2*l-1))/l;
          assert(norm(b-sinc(t),inf)<1e-12);
        endfor
        for l=[2 3 4 5], for alpha=[.4 .7 .9]
          b=intfilt(l,3,alpha); center=3*l;
          assert(abs(b(center)-1)<1e-9);
          assert(norm(b(center+[-2 -1 1 2]*l),inf)<1e-9);
        endfor; endfor
        """)

    def test_lagrange_reproduces_polynomials_on_every_phase(self):
        self.run_octave("""
        assert(isequal(intfilt(2,1,'Lagrange'),[.5 1 .5]));
        assert(norm(intfilt(2,3,'Lagrange')-[-1/16 0 9/16 1 9/16 0 -1/16],inf)<1e-15);
        for l=[2 3 4], for degree=[1 3 5]
          b=intfilt(l,degree,'Lagrange'); original=(-12:12).^degree;
          up=zeros(1,l*numel(original)); up(1:l:end)=original;
          result=conv(up,b); delay=(numel(b)-1)/2;
          positions=(-3*l:3*l); ids=delay+12*l+1+positions;
          assert(norm(result(ids)-(positions/l).^degree,inf)<1e-10);
        endfor; endfor
        """)

    def test_measured_analytic_envelopes_and_shape(self):
        self.run_octave("""
        x=[0 1 0 -1 0 1 0 -1 0 1 0 -1]; [u,l]=envelope(x,5,'analytic');
        expected=[.23489533 1 .46979066 1 .46979066 1 .46979066 1 .46979066 1 .46979066 1];
        assert(norm(u-expected,inf)<5e-9 && norm(u+l,inf)==0);
        [uc,lc]=envelope(x',5,'analytic'); assert(isequal(uc,u') && isequal(lc,l'));
        [u,l]=envelope([1 2 1 2 1],3);
        assert(isequal(round(u,6),[1.800001 2 1.8 2 1.800001]));
        assert(isequal(round(l,6),[.999999 .8 1 .8 .999999]));
        [u,l]=envelope(7*ones(1,6),9,'analytic'); assert(all(u==7) && all(l==7));
        """)

    def test_measured_peak_envelopes_and_fallback(self):
        self.run_octave("""
        [u,l]=envelope([0 1 0 -1 0 1 0 -1 0 1 0 -1],4,'peak');
        expected=[0 -.39393939 -.72727273 -1 -1.2121212 -1.3636364 -1.4545455 -1.4848485 -1.4545455 -1.3636364 -1.2121212 -1];
        assert(norm(u-1,inf)<1e-14 && norm(l-expected,inf)<5e-8);
        [u,l]=envelope((1:5)',2,'peak'); assert(norm(u-(1:5)',inf)<1e-14 && isequal(u,l));
        [u,l]=envelope(3*ones(1,5),2,'peak'); assert(all(u==3) && isequal(u,l));
        """)

    def test_unsupported_and_invalid_forms_fail_clearly(self):
        self.run_octave("""
        calls={'gaussfir(0)';'gaussfir(.5,1.5,2)';'gaussfir(.5,1,0)';'gaussfir(single(.5))';
               'intfilt(2,2,''Lagrange'')';'intfilt(2,3,0)';'intfilt(2,3,1.1)';'intfilt(2,3,1e-4)';
               'intfilt(1,3,.5)';'intfilt(single(2),3,.5)';'envelope(1:5,4,''analytic'')';
               'envelope(single(1:5),3,''analytic'')';
               'envelope([1 2 2 1],2,''peak'')';'envelope([1 Inf 2],3,''analytic'')'};
        for k=1:numel(calls)
          failed=false; try, eval(calls{k}); catch e
            failed=~isempty(regexp(e.message,'^(gaussfir|intfilt|envelope):','once'));
          end_try_catch; assert(failed);
        endfor
        """)


if __name__ == '__main__':
    unittest.main()
