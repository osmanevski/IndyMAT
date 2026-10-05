"""Wave 16 Y4: numeric invariants, unchanged paths and package reconstruction."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from scripts import compat, patch_packages

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class SignalStatisticsHelperTests(unittest.TestCase):
    def run_octave(self, body):
        result = subprocess.run(
            [OCTAVE, '--quiet', '--no-init-file', '--no-site-file', '--no-history',
             '--eval', compat.setup() + body], cwd=ROOT,
            text=True, capture_output=True, timeout=90)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_frequency_psd_matches_two_sided_fft_values_and_shape(self):
        self.run_octave("""
          for x = {[2 -1 4 3 1 0 -2 5], [1+2i 3-1i -2 0 5i 1 2 -3]}
            w=hamming(8); f=(0:7)'/8*4;
            [p,fo]=periodogram(x{1},w,f,4);
            expected=abs(fft(x{1}(:).*w)).^2/(4*sum(w.^2));
            assert(p,expected,2e-13); assert(fo,f);
            [p,fo]=periodogram(x{1},w,f.',4);
            assert(p,expected.',2e-13); assert(isequal(size(p),[1 8]));
            p=periodogram(x{1},[],(0:7)'/8*2*pi);
            assert(sum(p)*(2*pi/8),mean(abs(x{1}).^2),2e-13);
          endfor
          x=[1 2 3]; f=[.1 .23 .4];
          p=periodogram(x,[],f,1);
          expected=abs(1+2*exp(-2i*pi*f)+3*exp(-4i*pi*f)).^2/3;
          assert(p,expected,1e-13);
          p=periodogram(single(x),[],f,1); assert(isa(p,'single')); assert(double(p),expected,2e-6);
        """)

    def test_welch_frequency_values_are_segment_averages(self):
        self.run_octave("""
          x=sin((1:21)*.4)+.1*(1:21); f=[0 .13 .4 .77 1]; w=hamming(8);
          [p,fo]=pwelch(x,8,4,f,2); expected=zeros(size(f));
          for j=1:4:14, expected+=periodogram(x(j:j+7),w,f,2); endfor
          assert(p,expected/4,1e-13); assert(fo,f);
          [p,fo]=pwelch(x(:),w,0,f.',2);
          expected=(periodogram(x(1:8),w,f.',2)+periodogram(x(9:16),w,f.',2))/2;
          assert(p,expected,1e-13); assert(isequal(size(fo),[5 1]));
          try, pwelch(1:8); error('test:missing','missing error'); catch e,
            assert(strcmp(e.identifier,'signal:welchparse:NotEnoughSamplesForDefaultSettings'));
          end_try_catch
          pshort=pwelch(1:8,4,2,8,2); % Explicit windows remain valid for short records.
          p=pwelch(single(x),8,4,f,2); assert(isa(p,'single'));
          failed=false; try, p=pwelch(x,[],[],f,2); catch e,
            failed=~isempty(strfind(e.message,'requires an explicit window')); end_try_catch
          assert(failed);
        """)

    def test_peak_geometry_plateaus_negative_values_and_nested_peaks(self):
        self.run_octave("""
          [p,l,w,h]=findpeaks([0 2 4 2 0]);
          assert(p,4); assert(l,3); assert(w,2); assert(h,4);
          [p,l,w,h]=findpeaks([0 2 2 0]);
          assert(p,2); assert(l,2); assert(w,2); assert(h,2);
          [p,l,w,h]=findpeaks([-5 -2 -4 -1 -5],'MinPeakProminence',2);
          assert(p,[-2 -1]); assert(l,[2 4]); assert(h,[2 4]);
          [p,l,w,h]=findpeaks([0 4 2 3 1 0],'MinPeakProminence',2);
          assert(p,4); assert(l,2); assert(w,1.5); assert(h,4);
          [p,l]=findpeaks([0 1 0 3 0],'MinPeakProminence',0,'SortStr','descend','NPeaks',1);
          assert(p,3); assert(l,4);
          [p,l,w,h]=findpeaks(ones(5,1)); assert(isempty(p)&&isempty(l)&&isempty(w)&&isempty(h));
        """)

    def test_elliptic_prototypes_satisfy_ripple_stopband_and_stability(self):
        self.run_octave("""
          for n=[1 2 3 4 7 10]
            for spec={[.5 30],[1 20],[2 60]}
              rp=spec{1}(1); rs=spec{1}(2);
              [z,p,g]=__mf_ellip_prototype__(n,rp,rs);
              assert(numel(p)==n && numel(z)==n-rem(n,2)); assert(all(real(p)<0));
              h=g*prod(1i-z)/prod(1i-p); assert(20*log10(abs(h)),-rp,1e-8);
              m1=expm1(log(10)*rp/10)/expm1(log(10)*rs/10);
              kk=ellipke([m1;1-m1]); target=n*kk(1)/kk(2);
              m=fzero(@(v)ellipke(v)/ellipke(1-v)-target,[eps,1-eps]); ws=1/sqrt(m);
              h=g*prod(1i*ws-z)/prod(1i*ws-p); assert(20*log10(abs(h)),-rs,1e-7);
              [zd,pd,kd]=ellip(n,rp,rs,.35); q=exp(.35i*pi);
              h=kd*prod(q-zd)/prod(q-pd); assert(20*log10(abs(h)),-rp,1e-7);
              [b,a]=ellip(n,rp,rs,.35); assert(b,real(kd*poly(zd)),1e-12); assert(a,real(poly(pd)),1e-12);
            endfor
          endfor
        """)

    def test_scalar_order_cutoffs_touch_stopband_in_both_directions(self):
        self.run_octave("""
          for edges={[.15 .3],[.6 .4]}
            wp=edges{1}(1); ws=edges{1}(2); kind='low'; if(wp>ws),kind='high';endif
            [n,w]=buttord(wp,ws,1,30); [b,a]=butter(n,w,kind);
            assert(-20*log10(abs(polyval(b,exp(1i*ws*pi))/polyval(a,exp(1i*ws*pi)))),30,1e-8);
            assert(-20*log10(abs(polyval(b,exp(1i*wp*pi))/polyval(a,exp(1i*wp*pi))))<=1+1e-8);
            [n,w]=cheb2ord(wp,ws,1,30); assert(w,ws,1e-14);
            [b,a]=cheby2(n,30,w,kind);
            assert(-20*log10(abs(polyval(b,exp(1i*ws*pi))/polyval(a,exp(1i*ws*pi)))),30,1e-8);
            assert(-20*log10(abs(polyval(b,exp(1i*wp*pi))/polyval(a,exp(1i*wp*pi))))<=1+1e-8);
          endfor
          [n,w]=buttord(2,4,1,30,'s'); [b,a]=butter(n,w,'s');
          assert(-20*log10(abs(polyval(b,4i)/polyval(a,4i))),30,1e-8);
          [n,w]=cheb2ord(2,4,1,30,'s'); assert(w,4);
        """)

    def test_residue_expansion_reconstructs_transfer_function(self):
        self.run_octave("""
          for coefficients={{[1 2],[1 -.5 -.25]},{[2 0 1],[1 -.2 -.08]}}
            b=coefficients{1}{1}; a=coefficients{1}{2}; [r,p,k,m]=residuez(b,a);
            assert(all(diff(abs(p))<=0));
            for z=[1.3 2+1i -3]
              h=sum(r./(1-p/z).^m); if(!isempty(k)), h+=polyval(fliplr(k),1/z);endif
              expected=polyval(fliplr(b),1/z)/polyval(fliplr(a),1/z);
              assert(h,expected,1e-12);
            endfor
          endfor
        """)

    def test_fir2_default_symmetry_and_linearity(self):
        self.run_octave("""
          for n=[0 2 5 8 16]
            f=[0 .23 .65 1]; m=[1 .7 .2 0]; b=fir2(n,f,m);
            assert(numel(b)==n+1); assert(b,fliplr(b),1e-14);
            assert(fir2(n,f,2*m),2*b,1e-14);
          endfor
          b=fir2(8,[0 1],[1 1]); expected=zeros(1,9); expected(5)=1;
          assert(b,expected,1e-14);
        """)

    def test_dc_margin_boundary_and_nonzero_crossover(self):
        self.run_octave("""
          for tau=[.5 1 3]
            [gm,pm,wgm,wpm]=margin(tf(1,[tau 1]));
            assert(isinf(gm)&&isnan(wgm)&&pm==-180&&wpm==0);
          endfor
          [gm,pm,wgm,wpm]=margin(tf(2,[1 1]));
          assert(wpm,sqrt(3),1e-12); assert(pm,120,1e-10);
        """)

    def test_periodogram_pass_through_and_measured_overhead(self):
        output = self.run_octave("""
          orig=__mf_original_function__('periodogram'); x=sin(1:64);
          for args={{x},{x,[],64,4},{x,hamming(64),64,4,'twosided'},{x+1i*x,[],32}}
            [p,f]=periodogram(args{1}{:}); [po,fo]=orig(args{1}{:}); assert(isequaln(p,po)&&isequaln(f,fo));
          endfor
          tmp=periodogram(x,[],64,4); tmp=orig(x,[],64,4); extra=[]; direct=[]; wrapped=[];
          for trial=1:5
            tic; for k=1:1000, tmp=orig(x,[],64,4); endfor; a=toc/1000;
            tic; for k=1:1000, tmp=periodogram(x,[],64,4); endfor; b=toc/1000;
            direct(end+1)=a; wrapped(end+1)=b; extra(end+1)=b-a;
          endfor
          fprintf('PERIODOGRAM_US %.3f %.3f %.3f\\n',median(direct)*1e6,median(wrapped)*1e6,median(extra)*1e6);
          assert(median(extra)<.0005); % Catch accidental per-call path resolution.
        """)
        print(output.strip())

    def test_wave15_unchanged_package_forms(self):
        # Rebuild the wave-15 bytes from pinned originals and only old patches.
        with tempfile.TemporaryDirectory(prefix='y4-original-') as temp:
            temp = Path(temp)
            for package in ('signal-1.4.8', 'control-4.2.3'):
                folder = ROOT / 'octave' / 'paket-yamalari' / package
                base = ROOT / '.packages' / package
                manifest = json.loads((folder / 'manifest.json').read_text())
                files = {item['path']: (base / '.indymat-patches' / 'pristine' / item['path']).read_bytes()
                         for item in manifest['files']}
                for name in manifest['patches']:
                    if 'wave16' in name:
                        continue
                    patch = (folder / name).read_text()
                    target = patch.splitlines()[0][6:]
                    _, files[target] = patch_packages.unified(files[target], patch)
                (temp / 'private').mkdir(exist_ok=True)
                for dependency in (base / 'private').glob('*.m'):
                    shutil.copyfile(dependency, temp / 'private' / dependency.name)
                for name, data in files.items():
                    if '/' not in name:
                        (temp / name).write_bytes(data)
            output = self.run_octave("""
              previous=pwd(); cd('%s');
              oldfir2=str2func('fir2'); oldbuttord=str2func('buttord'); oldcheb2ord=str2func('cheb2ord');
              oldresiduez=str2func('residuez'); oldfindpeaks=str2func('findpeaks');
              oldpwelch=str2func('pwelch'); oldmargin=str2func('margin'); oldellip=str2func('ellip');
              cd(previous); addpath(fullfile('%s','private'),'-end');
              assert(fir2(8,[0 .3 .6 1],[1 1 0 0],64),oldfir2(8,[0 .3 .6 1],[1 1 0 0],64));
              assert(fir2(8,[0 .3 .3 1],[1 1 0 0]),oldfir2(8,[0 .3 .3 1],[1 1 0 0]));
              [a,b,c]=buttord(.2,.3,1,20); [ao,bo,co]=oldbuttord(.2,.3,1,20); assert(isequal(a,ao)&&isequal(b,bo)&&isequal(c,co));
              [a,b,c]=cheb2ord(.2,.3,1,20); [ao,bo,co]=oldcheb2ord(.2,.3,1,20); assert(isequal(a,ao)&&isequal(b,bo)&&isequal(c,co));
              [a,b,c,d]=residuez(1,[1 -.8 .16]); [ao,bo,co,dold]=oldresiduez(1,[1 -.8 .16]); assert(isequaln({a,b,c,d},{ao,bo,co,dold}));
              [a,b,c]=findpeaks([0 1 0 3 0]); [ao,bo,co]=oldfindpeaks([0 1 0 3 0]); assert(isequaln({a,b,c},{ao,bo,co}));
              [a,b]=pwelch(1:16,8,4,8,4); [ao,bo]=oldpwelch(1:16,8,4,8,4); assert(isequaln({a,b},{ao,bo}));
              [a,b,c,d]=margin(tf(2,[1 1])); [ao,bo,co,dold]=oldmargin(tf(2,[1 1])); assert(isequaln({a,b,c,d},{ao,bo,co,dold}));
              [a,b]=ellip(2,0,20,.4); [ao,bo]=oldellip(2,0,20,.4); assert(isequaln({a,b},{ao,bo}));
              function out=y4_order3(f), [n,p,out]=f(.2,.3,1,20); endfunction
              calls={@()fir2(8,[0 .3 .6 1],[1 1 0 0],64), @()oldfir2(8,[0 .3 .6 1],[1 1 0 0],64); ...
                     @()y4_order3(@buttord), @()y4_order3(oldbuttord); ...
                     @()y4_order3(@cheb2ord), @()y4_order3(oldcheb2ord); ...
                     @()residuez(1,[1 -.8 .16]), @()oldresiduez(1,[1 -.8 .16]); ...
                     @()findpeaks([0 1 0 3 0]), @()oldfindpeaks([0 1 0 3 0]); ...
                     @()pwelch(1:16,8,4,8,4), @()oldpwelch(1:16,8,4,8,4); ...
                     @()margin(tf(2,[1 1])), @()oldmargin(tf(2,[1 1])); ...
                     @()ellip(2,0,20,.4), @()oldellip(2,0,20,.4)};
              labels={'fir2','buttord','cheb2ord','residuez','findpeaks','pwelch','margin','ellip'};
              for j=1:rows(calls)
                a=[];b=[];
                for trial=1:5
                  tic; for k=1:200, tmp=calls{j,2}(); endfor; a(end+1)=toc/200;
                  tic; for k=1:200, tmp=calls{j,1}(); endfor; b(end+1)=toc/200;
                endfor
                fprintf('PACKAGE_US %%s %%.3f %%.3f %%.3f\\n',labels{j},median(a)*1e6,median(b)*1e6,median(b-a)*1e6);
              endfor
            """ % (str(temp).replace("'", "''"), str(temp).replace("'", "''")))
            print(output.strip())


if __name__ == '__main__':
    unittest.main()
