import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class CoreMetadataHelperTests(unittest.TestCase):
    def run_octave(self, body, fixtures=None):
        with tempfile.TemporaryDirectory(prefix='y1-metadata-') as folder:
            folder = Path(folder)
            for name, content in (fixtures or {}).items():
                target = folder / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content)
            command = ("warning('off','Octave:shadowed-function'); "
                       "addpath('%s'); addpath('%s'); %s") % (
                           ROOT / 'octave' / 'compat', folder, body)
            result = subprocess.run(
                [OCTAVE, '--quiet', '--no-init-file', '--no-site-file',
                 '--no-history', '--eval', command], cwd=folder,
                text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return result.stdout

    def test_caller_arity_with_bare_and_explicit_queries(self):
        self.run_octave(r"""
          a = y1_inspect(1, 2); assert(isequal(a, [2 2 1 1]));
          [a, b] = y1_inspect(); assert(isequal(a, [0 0 2 2]));
          y1_inspect(1, 2, 3);
        """, {'y1_inspect.m': """function varargout = y1_inspect(varargin)
          counts = [nargin nargin() nargout nargout()];
          assert(isequal(counts, [builtin('nargin') builtin('nargin') builtin('nargout') builtin('nargout')]));
          varargout = repmat({counts}, 1, builtin('nargout'));
        end
        """})

    def test_user_functions_anonymous_and_variadic_metadata_pass_through(self):
        self.run_octave(r"""
          for f = {@y1_pair, @y1_variadic, @(x,y)x+y, @(varargin)numel(varargin)}
            assert(nargin(f{1}) == builtin('nargin', f{1}));
            assert(nargout(f{1}) == builtin('nargout', f{1}));
          endfor
          for name = {'y1_pair', 'y1_variadic'}
            assert(nargin(name{1}) == builtin('nargin', name{1}));
            assert(nargout(name{1}) == builtin('nargout', name{1}));
          endfor
        """, {'y1_pair.m': 'function [a,b]=y1_pair(x,y), a=x; b=y; end\n',
               'y1_variadic.m': 'function varargout=y1_variadic(x,varargin), varargout=varargin; end\n'})

    def test_user_function_with_builtin_name_keeps_its_metadata(self):
        self.run_octave(r"""
          assert(nargin('sin') == 3 && nargin(@sin) == 3);
          assert(nargout('sum') == 2 && nargout(@sum) == 2);
        """, {'sin.m': 'function out=sin(a,b,c), out=a+b+c; end\n',
               'sum.m': 'function [a,b]=sum(x), a=x; b=x; end\n'})

    def test_metadata_errors_retain_native_identifier_and_message(self):
        self.run_octave(r"""
          for name = {'nargin', 'nargout'}
            for args = {{'cos'}, {'y1_missing'}, {'('}, {42}, {{}}, {'sin', 2}}
              try, builtin(name{1}, args{1}{:}); error('Y1:expected', 'expected failure');
              catch native, end_try_catch
              assert(!strcmp(native.identifier, 'Y1:expected'));
              try, feval(name{1}, args{1}{:}); error('Y1:expected', 'expected failure');
              catch wrapped, end_try_catch
              assert(strcmp(native.identifier, wrapped.identifier));
              assert(strcmp(native.message, wrapped.message));
            endfor
          endfor
        """)

    def test_exist_queries_use_caller_variables_and_preserve_literal_bytes(self):
        self.run_octave(r"""
          out=1; varargin=2; code=3; metadata=4; k=5;
          for name = {'out', 'varargin', 'code', 'metadata', 'k', 'sin', 'missing'}
            assert(exist(name{1}) == builtin('exist', name{1}));
            for type = {'var', 'file', 'builtin', 'dir', 'class'}
              assert(exist(name{1}, type{1}) == builtin('exist', name{1}, type{1}));
            endfor
          endfor
          for name = {'', 'a''b', sprintf('a\nb'), char([0 65]), char('out','bad'), ...
                      char(zeros(0,2)), char(zeros(2,0)), ...
                      'x''); error(''Y1:injected'', ''bad''); %'}
            assert(exist(name{1}) == builtin('exist', name{1}));
          endfor
          y1_scope();
        """, {'y1_scope.m': """function y1_scope()
          code = 1;
          assert(exist('code','var') == 1);
          assert(exist('out','var') == 0 && exist('varargin','var') == 0);
          assert(exist('metadata','var') == 0 && exist('k','var') == 0);
        end
        """})

    def test_qualified_classes_are_not_confused_with_namespace_functions(self):
        self.run_octave(r"""
          assert(exist('containers.Map','class') == 8);
          assert(exist('y1.nested.Widget','class') == 8);
          assert(exist('y1.plain','class') == 0);
          assert(exist('y1.nested.Missing','class') == 0);
          assert(exist('y1..Widget','class') == 0);
          assert(exist('y1.plain','file') == builtin('exist','y1.plain','file'));
        """, {'+y1/+nested/Widget.m': 'classdef Widget\nend\n',
               '+y1/plain.m': 'function x=plain(), x=1; end\n'})

    def test_exist_invalid_forms_retain_native_errors(self):
        self.run_octave(r"""
          for args = {{}, {42}, {'x', 42}, {'x', 'bad'}, {'x','var',1}}
            try, builtin('exist', args{1}{:}); error('Y1:expected', 'expected failure');
            catch native, end_try_catch
            assert(!strcmp(native.identifier, 'Y1:expected'));
            try, exist(args{1}{:}); error('Y1:expected', 'expected failure');
            catch wrapped, end_try_catch
            assert(strcmp(native.identifier, wrapped.identifier));
            assert(strcmp(native.message, wrapped.message));
          endfor
        """)

    def test_unchanged_path_overhead_stays_below_one_millisecond(self):
        output = self.run_octave(r"""
          n=3000; f=@y1_pair; x=1;
          for name = {'nargin','nargout','exist'}
            samples=zeros(3,2);
            for repeat=1:3
              tic;
              if strcmp(name{1},'exist')
                for j=1:n, builtin('exist','x','var'); endfor
              else
                for j=1:n, builtin(name{1},f); endfor
              endif
              samples(repeat,1)=toc/n;
              tic;
              if strcmp(name{1},'exist')
                for j=1:n, exist('x','var'); endfor
              elseif strcmp(name{1},'nargin')
                for j=1:n, nargin(f); endfor
              else
                for j=1:n, nargout(f); endfor
              endif
              samples(repeat,2)=toc/n;
            endfor
            native=median(samples(:,1)); wrapped=median(samples(:,2));
            assert(wrapped-native < .001);
            printf('OVERHEAD %s native=%.3f us wrapped=%.3f us delta=%.3f us\n', ...
                   name{1}, native*1e6, wrapped*1e6, (wrapped-native)*1e6);
          endfor
        """, {'y1_pair.m': 'function [a,b]=y1_pair(x,y), a=x; b=y; end\n'})
        print(output.strip())


if __name__ == '__main__':
    unittest.main()
