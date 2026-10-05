"""Engineering helper invariants and package-patch checks beyond the probes."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

from scripts import compat, patch_packages

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class EngineeringHelperTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not all((ROOT / ".packages" / p).is_dir() for p in ("control-4.2.3", "signal-1.4.8", "statistics-2.0.0")):
            raise unittest.SkipTest("engineering packages are not installed in this project")

    def run_octave(self, body, timeout=60):
        result = subprocess.run(
            [OCTAVE, '--quiet', '--no-init-file', '--no-site-file', '--no-history',
             '--eval', compat.setup() + body], cwd=ROOT,
            text=True, capture_output=True, timeout=timeout)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_versioned_patches_reconstruct_exact_pinned_bytes(self):
        for package in ('control-4.2.3', 'signal-1.4.8', 'statistics-2.0.0'):
            folder = ROOT / 'octave' / 'paket-yamalari' / package
            manifest = json.loads((folder / 'manifest.json').read_text())
            sources = {}
            for item in manifest['files']:
                pristine = ROOT / '.packages' / package / '.indymat-patches' / 'pristine' / item['path']
                self.assertTrue(pristine.exists(), 'apply this package patch set before testing')
                sources[item['path']] = pristine.read_bytes()
                self.assertEqual(patch_packages.digest(sources[item['path']]), item['pristine_sha256'])
            for name in manifest['patches']:
                patch = (folder / name).read_text()
                target = patch.splitlines()[0][6:]
                _, sources[target] = patch_packages.unified(sources[target], patch)
            for item in manifest['files']:
                self.assertEqual(patch_packages.digest(sources[item['path']]), item['patched_sha256'])
                self.assertEqual((ROOT / '.packages' / package / item['path']).read_bytes(), sources[item['path']])

    def test_unchanged_core_forms_and_warm_overhead(self):
        output = self.run_octave(r"""
          for name = {'bandwidth','hamming','blackman','prctile','quantile'}
            name=name{1}; orig=__mf_toolbox_original__(name); wrapped=str2func(name);
            switch name
              case 'bandwidth', args={diag(ones(3,1))};
              case {'hamming','blackman'}, args={5,'periodic'};
              case 'prctile', args={[1 NaN 3 4 5], [20 70]};
              case 'quantile', args={[1 NaN 3 4 5], [.2 .7]};
            endswitch
            assert(isequaln(wrapped(args{:}),orig(args{:})));
            base=[]; extra=[];
            for j=1:3
              tic; for k=1:60, z=orig(args{:}); endfor; a=toc;
              tic; for k=1:60, z=wrapped(args{:}); endfor; b=toc;
              base(end+1)=a/60; extra(end+1)=(b-a)/60;
            endfor
            added=median(extra);
            assert(added<0.003, sprintf('%s overhead %.6f',name,added));
            fprintf('%s base_us=%.3f added_us=%.3f\n',name,1e6*median(base),1e6*added);
          endfor
          [l,u]=bandwidth([1 2 0;0 1 2;0 0 1]); assert(l==0 && u==1);
          assert(bandwidth(eye(2),'upper')==0);
        """)
        self.assertEqual(output.count('added_us='), 5)

    def test_percentile_methods_dimensions_and_integer_type(self):
        self.run_octave(r"""
          q=__mf_toolbox_original__('quantile'); p=__mf_toolbox_original__('prctile');
          x=reshape(1:24,3,4,2); ps=[.1 .4 .8];
          for pair={{'midpoint',5},{'inclusive',7},{'exclusive',6}}
            pair=pair{1};
            for dim=[1 2 3]
              assert(isequaln(quantile(x,ps,dim,'Method',pair{1}),q(x,ps,dim,pair{2})));
              assert(isequaln(prctile(x,100*ps,dim,'Method',pair{1}),p(x,100*ps,dim,pair{2})));
            endfor
          endfor
          for type={'int8','uint16','int64'}
            x=cast([1 2 4 8],type{1}); r=prctile(x,[25 50 75]);
            assert(isa(r,type{1}) && isequal(r,cast(p(double(x),[25 50 75]),type{1})));
          endfor
          assert(isequal(quantile([1 2 3 4],.5,'Method','midpoint'),2.5));
          assert(isequaln(quantile([],ps,'Method','midpoint'),q([],ps)));
          assert(isequaln(prctile([],100*ps,'Method','midpoint'),p([],100*ps)));
          assert(isa(quantile(int16([1 2 3 4]),.5,'Method','midpoint'),'int16'));
          assert(isequal(size(quantile([1 2 3], [.2;.8], 'Method','midpoint')),[2 1]));
          assert(isequaln(quantile([1 NaN 3],.5,'Method','midpoint'),2));
        """)

    def test_bandwidth_and_order_general_rational_models(self):
        self.run_octave(r"""
          for rate=[.01 1 100]
            for gain=[-3 .5 7]
              for drop=[-1 -3 -10]
                expected=rate*sqrt(10^(-drop/10)-1);
                assert(abs(bandwidth(tf(gain*rate,[1 rate]),drop)-expected)<1e-9*max(1,expected));
              endfor
            endfor
          endfor
          assert(isinf(bandwidth(tf(3)))); assert(bandwidth(tf(0))==0);
          assert(isnan(bandwidth(tf(1,[1 0]))));
          assert(order(tf([1 1],[1 3 2]))==2);
          assert(order(ss(diag([-1 -2 -3]),ones(3,1),ones(1,3),0))==3);
          assert(order(ss([],[],[],2))==0);
          failed=false; try, bandwidth(tf(1,[1 -.5],1)); catch e, failed=~isempty(strfind(e.message,'bandwidth:')); end; assert(failed);
        """)

    def test_window_empty_and_envelope_offset_invariance(self):
        self.run_octave(r"""
          for fun={@hamming,@blackman,@hann}
            assert(isequal(size(fun{1}(0)),[0 1]));
            assert(isequal(size(fun{1}(0,'periodic')),[0 1]));
          endfor
          x=[1 2 1 2 1]; [u,l]=envelope(x,3,'rms');
          [u2,l2]=envelope(x+10,3,'rms');
          assert(norm(u2-u-10)<1e-12 && norm(l2-l-10)<1e-12);
          assert(norm((u+l)/2-mean(x))<1e-12);
          [u,l]=envelope(ones(1,5)*3,2,'rms'); assert(isequal(u,l) && all(u==3));
        """)

    def test_package_numeric_invariants(self):
        self.run_octave(r"""
          for num={[1 2],[2 4 3],[0 0 1]}
            num=num{1}; den=[1 3 2]; [a,b,c,d]=tf2ss(num,den);
            assert(rows(a)==2 && isequal(b,[1;0]));
            for s=[1i 2+1i 10]
              assert(norm(c*((s*eye(2)-a)\b)+d-polyval(num,s)/polyval(den,s))<1e-12);
            endfor
          endfor
          for n=[4 10 20]
            w=.35; tau=(0:n)-n/2; raw=w*sinc(w*tau).*hamming(n+1).';
            assert(norm(fir1(n,w,'noscale')-raw)<1e-12);
            assert(abs(sum(fir1(n,w))-1)<1e-12);
          endfor
          for n=[3 4 7 10], assert(isequal(fir1(n,.35),fir1(int32(n),.35))); endfor
          x=[1 2 3 4]; h=[.25 .5 .25];
          [y,hout]=resample(x,2,1,h); assert(isequal(hout,h));
          assert(isequal(resample(x,1,1),x));
          for ratio={[2 1],[3 2],[2 3]}
            ratio=ratio{1}; [y,h]=resample(ones(200,1),ratio(1),ratio(2));
            assert(rows(y)==ceil(200*ratio(1)/ratio(2)));
            assert(abs(sum(h)-ratio(1))<1e-12);
            assert(norm(h-h(end:-1:1))<1e-12);
          endfor
          assert(isequaln(poisspdf([NaN -1 0 1 Inf],[0 0 0 0 0]),[NaN 0 1 0 0]));
          assert(isa(poisspdf(single([0 1]),single(0)),'single'));
          [m,s]=grpstats([1 NaN 3 5],[1 1 2 2],{'mean','sum'});
          assert(isequal(m,[1;4]) && isequal(s,[1;8]));
          assert(isequal(grpstats([1 2 3 5],[1 1 2 2],{'mean','std'}),[1.5;4]));
          g=tf([1 1],[1 3 2]); h=minreal(g); assert(abs(pole(h)+2)<1e-12);
          g=tf([1 1],[1 1+1e-6]); assert(order(minreal(g))==1 && order(minreal(g,1e-5))==0);
          g=tf(1,[1 1]); [m,p,w]=bode(g,[.1 1 10]); [re,im]=nyquist(g,[.1 1 10]);
          assert(isequal(size(m),[1 1 3]) && isequal(size(p),[1 1 3]) && iscolumn(w));
          assert(norm(re(:)+1i*im(:)-1./(1+1i*w))<1e-12);
          [gm,pm,wcg,wcp]=margin(tf(1,[1 2 1 0]));
          assert(abs(gm-2)<1e-6 && abs(wcg-1)<1e-6 && pm>0 && pm<90);
          g.Numerator={2}; assert(isequal(g.Numerator,{[0 2]}));
          [v,loc]=findpeaks([0 1 0 2 0 3 0],'MinPeakDistance',2); assert(isequal(loc,[2 6]));
        """)


if __name__ == '__main__':
    unittest.main()
