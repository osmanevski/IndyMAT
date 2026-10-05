"""Protocol immunity to name shadowing, against real Octave through the real Kernel.

User VARIABLES, in the base workspace or as locals of a paused function frame,
with the name of any identifier the internal protocol uses (including the reserved
entry name) must not disturb any application operation or lose their values.

Documented limits, deliberately not tested: the user emptying the whole load path
(path('')) breaks Octave's own functions, and user FUNCTION FILES (or command-line
functions) that shadow built-ins such as fprintf.m or rehash.m, break Octave
itself for the user and are not defended against.
"""
import re
import tempfile
import time
import unittest
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]
KEYWORDS = set('''__FILE__ __LINE__ break case catch classdef continue do else elseif end
 end_try_catch end_unwind_protect endclassdef endenumeration endevents endfor endfunction
 endif endmethods endparfor endproperties endswitch endwhile enumeration events for function
 global if methods otherwise parfor persistent properties return switch try until
 unwind_protect unwind_protect_cleanup while arguments endarguments spmd endspmd'''.split())
# Identifiers written to Octave's stdin by the kernel, plus every name the helper
# files call or reference (a variable of any of these names must be harmless).
PROTOCOL = '''addpath rehash fprintf fflush stdout builtin dbstop dbclear dbstatus dbquit dbup
 dbdown dbcont dbstep autoload clear whos run eval evalin assignin subsasgn who feval
 __mf_execute__ __mf_marker__ __mf_debug_snapshot__ __mf_debug_inspect__ __mf_debug_eval__
 __mf_run_to_cursor__ __mf_run_to_cursor_cleanup__ __mf_breakpoint_eval__ __mf_breakpoint_ops__
 __mf_workspace__ __mf_workspace_base__ __mf_variable__ __mf_publish__ __mf_bp__ __mf_bp_err__
 __mf_debug_err__ __mf_code__ __mf_job__ __mf_folder__'''.split()
HELPER_CALLS = {name for file in (ROOT/'octave').glob('__mf_*.m')
                for name in re.findall(r'\b([A-Za-z_]\w*)\s*\(', file.read_text(), re.ASCII)}
NAMES = sorted((set(PROTOCOL) | {name for name in HELPER_CALLS if len(name) > 1}) - KEYWORDS
               - {'ans', 'nargin', 'nargout', 'varargin', 'varargout', 'result', 'e', 'i', 'j', 'I', 'J'})
LOCAL_NAMES = [name for name in NAMES if name not in ('nargin', 'nargout')]


class ProtocolShadowingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name)/'work'
        self.work.mkdir()
        self.kernel = Kernel(ROOT, Path(self.temp.name)/'runtime', self.work, executable='octave-cli')
        self.idle()

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def wait(self, paused=False, serial=None):
        for _ in range(600):
            state = self.kernel.snapshot()
            if paused:
                debug = state.get('debug') or {}
                if state['status'] == 'paused' and debug.get('ready') and debug.get('serial') != serial:
                    return state
            elif state['status'] in ('idle', 'dead'):
                return state
            time.sleep(.03)
        self.fail(str(self.kernel.snapshot()))

    def idle(self, error=False):
        state = self.wait()
        self.assertEqual(state['status'], 'idle', state)
        if not error:
            self.assertFalse(state['error'], state)
        return state

    def code(self, code):
        self.kernel.submit(code)
        return self.idle()

    def action(self, mode, request):
        self.kernel.workspace_action(mode, request)
        return self.idle()

    @staticmethod
    def preview(state, name):
        return next(v['preview'] for v in state['variables'] if v['name'] == name)

    def check_leaks(self, expected=0):
        # The application's own __mf_ variables must never remain; the only ones
        # allowed are those the test itself created.
        state = self.code("leak_count=(@numel)((@who)('-regexp','^__mf_'));")
        self.assertEqual(self.preview(state, 'leak_count'), str(expected), state)

    def check_values(self, names=NAMES):
        state = self.code('shadow_preserved=' + ' && '.join(f'{name}=={index+101}' for index, name in enumerate(names)) + ';')
        self.assertEqual(self.preview(state, 'shadow_preserved'), 'true')

    def assign(self, names=NAMES):
        return ' '.join(f'{name}={index+101};' for index, name in enumerate(names))

    def debugger(self, locals=False):
        assign = self.assign(LOCAL_NAMES) if locals else ''
        assign += ' local_shadow_preserved=0;'
        file = self.work/'shadow_debug_fixture.m'
        file.write_text('function zz_out=shadow_debug_fixture(zz_seed)\n ' + assign + '\n outer_local=zz_seed+1;\n zz_out=zz_inner(zz_seed);\n function zz_y=zz_inner(zz_value)\n  ' + assign + '\n  inner_local=zz_value+2;\n  zz_y=inner_local+1;\n  zz_y+=1;\nendfunction\nendfunction\n')
        self.kernel.configure_breakpoint(file, 8, 'set', 'zz_value == 4')
        self.idle()
        self.kernel.configure_breakpoint(file, 8, 'disable')
        self.idle()
        self.kernel.configure_breakpoint(file, 8, 'enable')
        self.idle()
        self.kernel.submit('debug_result=shadow_debug_fixture(4);')
        state = self.wait(True)
        if locals:
            self.kernel.debug('eval', 'local_shadow_preserved=' + ' && '.join(f'{name}=={index+101}' for index, name in enumerate(LOCAL_NAMES)) + ';')
            state = self.wait(True, state['debug']['serial'])
            self.assertEqual(self.preview(state, 'local_shadow_preserved'), 'true')
            self.assertFalse(any(v['name'].startswith('__mf_') and v['name'] not in LOCAL_NAMES for v in state['variables']))
        self.kernel.debug('inspect', 'inner_local')
        state = self.wait(True, state['debug']['serial'])
        self.assertFalse(state['error'], state)
        self.assertEqual(state['debug']['detail']['name'], 'inner_local')
        self.kernel.debug('frame', 2)
        state = self.wait(True, state['debug']['serial'])
        self.assertIn('outer_local', {v['name'] for v in state['variables']})
        self.kernel.debug('frame', 1)
        state = self.wait(True, state['debug']['serial'])
        self.kernel.debug('step')
        state = self.wait(True, state['debug']['serial'])
        line = state['debug']['line']
        self.kernel.run_to_cursor(file, line + 1)
        state = self.wait(True, state['debug']['serial'])
        self.kernel.interrupt()
        self.idle(error=True)
        # A second pause: leave through quit, then step out and continue.
        self.kernel.submit('debug_result=shadow_debug_fixture(4);')
        state = self.wait(True)
        self.kernel.debug('quit')
        self.idle(error=True)
        self.kernel.submit('debug_result=shadow_debug_fixture(4);')
        state = self.wait(True)
        self.kernel.debug('in')
        state = self.wait(True, state['debug']['serial'])
        self.kernel.debug('out')
        state = self.wait(True, state['debug']['serial'])
        self.kernel.debug('continue')
        self.idle()
        # Stop while paused and while running a busy loop.
        self.kernel.submit('debug_result=shadow_debug_fixture(4);')
        self.wait(True)
        self.kernel.interrupt()
        self.kernel.interrupt()
        self.idle(error=True)
        self.kernel.clear_breakpoints()
        self.idle()
        self.kernel.submit('while 1; end')
        time.sleep(.15)
        self.kernel.interrupt()
        self.kernel.interrupt()
        self.idle(error=True)

    def operations(self):
        self.code('matrix_value=[1,2]; scalar_value=7;')
        script = self.work/'shadow_script.m'
        script.write_text('file_value=42;\n')
        self.kernel.submit(mode='file', argument=str(script))
        self.idle()
        self.kernel.submit(mode='inspect', argument='matrix_value')
        self.idle()
        self.debugger()
        self.action('workspace-rename', {'old_name': 'scalar_value', 'new_name': 'renamed_value'})
        self.action('workspace-assign-scalar', {'name': 'renamed_value', 'class': 'double', 'value': 9})
        target = self.work/'saved.mat'
        self.action('workspace-save', {'names': ['renamed_value'], 'all': False, 'path': str(target)})
        info = self.action('workspace-load-inspect', {'path': str(target)})['workspace_action']
        self.action('workspace-load', {'path': str(target), 'inventory': info['variables'], 'replacements': ['renamed_value']})
        page = self.action('variable-read', {'name': 'matrix_value', 'path': [], 'row': 1, 'column': 1, 'rows': 2, 'columns': 2, 'slices': [], 'epoch': self.kernel.generation})['variable_action']
        self.action('variable-write', {'name': 'matrix_value', 'path': [], 'row': 1, 'column': 1, 'height': 1, 'width': 1, 'slices': [], 'epoch': self.kernel.generation, 'read_job': 'a'*32, 'class': 'double', 'size': page['size'], 'values': [{'type': 'number', 'value': 8}]})
        report = self.work/'shadow_report.m'
        report.write_text('%% Rapor\n% Çevrimdışı\n2+2\n')
        self.kernel.submit(mode='publish', argument=str(report))
        self.idle()
        self.action('workspace-clear-names', {'names': ['file_value']})
        self.kernel.package('unload', 'signal')
        self.idle()
        self.kernel.package('load', 'signal')
        self.idle()

    def test_every_base_variable_is_preserved(self):
        self.code(self.assign())
        self.operations()
        self.check_values()
        # The test itself created every __mf_ name in NAMES; nothing else may exist.
        self.check_leaks(sum(name.startswith('__mf_') for name in NAMES))
        self.kernel.reset()
        self.idle()
        self.code('after_reset=73;')

    def test_every_debug_local_is_harmless(self):
        self.operations()
        self.debugger(locals=True)
        self.check_leaks()

    def test_reserved_entry_variable_is_harmless(self):
        self.code('__mf_execute__=1; kept_handle=@(x) x+1; kept_value=[4 5 6];')
        self.operations()
        for code in ['restoredefaultpath;', f"rmpath({self.kernel.quote(ROOT/'octave')});"]:
            self.code(code)
            state = self.code('reserved_kept=(__mf_execute__==1 && kept_handle(2)==3 && isequal(kept_value,[4 5 6]));')
            self.assertEqual(self.preview(state, 'reserved_kept'), 'true')
        self.debugger()
        # Only the user's own reserved-name variable exists.
        self.check_leaks(1)
        self.kernel.submit(mode='inspect', argument='kept_value')
        self.assertEqual(self.idle()['detail']['name'], 'kept_value')

    def test_shadowed_run_publish_and_input_names(self):
        self.code("run=1; whos=2; clear=3; evalin=4; builtin=5; pkg=6; builtin_run_value=7;")
        script = self.work/'shadowed_run_script.m'
        script.write_text('ran_value=731;\n')
        self.kernel.submit(mode='file', argument=str(script))
        self.assertEqual(self.preview(self.idle(), 'ran_value'), '731')
        self.kernel.submit(mode='profile', argument=str(script))
        self.idle()
        report = self.work/'shadowed_report.m'
        report.write_text('%% Rapor\n% Gerçek temel çalışma alanı\nshadow_publish_value=731;\nshadow_publish_value\n')
        self.kernel.submit(mode='publish', argument=str(report))
        state = self.idle()
        self.assertIn('731', (self.kernel.runtime/state['job']/'publish'/'shadowed_report.html').read_text())

    def test_error_trace_text_of_failing_script_is_unchanged(self):
        script = self.work/'trace_script.m'
        script.write_text("error('boom');\n")
        self.kernel.submit(mode='file', argument=str(script))
        state = self.idle(error=True)
        self.assertIn('boom', state['output'] + str(state['error']))
        self.assertIn('trace_script:1', state['error'])
        self.assertIn('run:78', state['error'])



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main()
