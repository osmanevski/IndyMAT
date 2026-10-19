"""Public input hooks execute through a file frame in a real Octave session."""
import tempfile
import time
import unittest
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class InputHookGuard(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='input-hooks-', dir=ROOT)
        self.work = Path(self.temp.name)
        for name in ('hook_alpha', 'hook_beta', 'hook_zeta'):
            (self.work/(name+'.m')).write_text(
                'function '+name+'(varargin)\n'
                'global hook_log hook_ids hook_mode;\n'
                'hook_log{end+1}={'+repr(name)+',varargin};\n'
                'remove_input_event_hook(hook_ids.'+name+');\n'
                "if strcmp(hook_mode,'insert') && strcmp('"+name+"','hook_alpha')\n"
                " hook_ids.hook_beta=add_input_event_hook('hook_beta','inserted');\n"
                'endif\n'
                "if strcmp(hook_mode,'error') && strcmp('"+name+"','hook_alpha')\n"
                " error('hook:test','Hook callback failure');\n"
                'endif\nendfunction\n')
        self.k = Kernel(ROOT, self.work/'runtime', self.work, executable='octave-cli')
        self.wait(lambda s: s['status']=='idle')

    def tearDown(self):
        self.k.close()
        self.temp.cleanup()

    def wait(self, predicate, seconds=10):
        end = time.monotonic()+seconds
        while time.monotonic()<end:
            state = self.k.snapshot()
            if predicate(state): return state
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))

    def run_code(self, code):
        self.k.submit(code)
        state = self.wait(lambda s:s['status']=='idle')
        self.assertFalse(state['error'], state)
        return state

    def test_builtin_eval_callback_has_file_frame(self):
        callback = "assignin('base','hook_stack',dbstack('-completenames')); remove_input_event_hook(evalin('base','hook_id'));"
        self.permitted_input('hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+');')
        state = self.run_code("assert(any(arrayfun(@(s) !isempty(s.file),hook_stack))); disp({hook_stack.name});")
        self.assertIn('__mf_input_hooks__', state['output'])


    def hooks(self, setup, assertions, next_reader=False):
        self.permitted_input("global hook_log hook_ids hook_mode; hook_log={}; hook_ids=struct(); hook_mode=''; "+setup)
        if next_reader:
            self.permitted_input()
        return self.run_code(assertions)

    def test_native_ids_replacement_order_data_and_removal(self):
        self.hooks(
            "hook_ids.hook_zeta=add_input_event_hook('hook_zeta',[]); "
            "hook_ids.hook_alpha=add_input_event_hook('hook_alpha','old'); "
            "replacement=add_input_event_hook('hook_alpha','new'); "
            "hook_ids.hook_beta=add_input_event_hook('hook_beta'); "
            "remove_input_event_hook(hook_ids.hook_beta); "
            "remove_input_event_hook('missing_hook'); "
            "assert(strcmp(replacement,'hook_alpha')); "
            "assert(strcmp(replacement,hook_ids.hook_alpha));",
            "assert(numel(hook_log)==2); "
            "assert(strcmp(hook_log{1}{1},'hook_alpha')); "
            "assert(isequal(hook_log{1}{2},{'new'})); "
            "assert(strcmp(hook_log{2}{1},'hook_zeta')); "
            "assert(isequal(hook_log{2}{2},{[]}));")

    def test_handle_id_and_no_data(self):
        self.hooks(
            "fh=@hook_alpha; native=builtin('add_input_event_hook',fh); "
            "builtin('remove_input_event_hook',native); "
            "hook_ids.hook_alpha=add_input_event_hook(fh); "
            "replacement=add_input_event_hook(fh); "
            "assert(strcmp(native,replacement)); "
            "assert(strcmp(native,hook_ids.hook_alpha));",
            "assert(numel(hook_log)==1); assert(isempty(hook_log{1}{2}));")

    def test_insertion_waits_until_next_native_tick(self):
        self.hooks(
            "hook_mode='insert'; "
            "hook_ids.hook_alpha=add_input_event_hook('hook_alpha'); "
            "hook_ids.hook_zeta=add_input_event_hook('hook_zeta');",
            "assert(numel(hook_log)==3); "
            "assert(strcmp(hook_log{1}{1},'hook_alpha')); "
            "assert(strcmp(hook_log{2}{1},'hook_zeta')); "
            "assert(strcmp(hook_log{3}{1},'hook_beta')); "
            "assert(isequal(hook_log{3}{2},{'inserted'}));")

    def test_callback_error_leaves_later_hook_registered(self):
        self.hooks(
            "hook_mode='error'; "
            "hook_ids.hook_alpha=add_input_event_hook('hook_alpha'); "
            "hook_ids.hook_zeta=add_input_event_hook('hook_zeta');",
            "assert(numel(hook_log)==2); "
            "assert(strcmp(hook_log{1}{1},'hook_alpha')); "
            "assert(strcmp(hook_log{2}{1},'hook_zeta'));", next_reader=True)

    def traced_writes(self):
        writes=[]
        original=self.k._write_command
        def observe(proc,command,**kwargs):
            writes.append(command)
            return original(proc,command,**kwargs)
        self.k._write_command=observe
        return writes

    def permitted_input(self, setup=''):
        # Native input invokes input-event hooks. Abort its unanswered wait
        # through ordinary Stop; the assertions run in the preserved session.
        self.k.submit(setup+" builtin('input','PERMITTED_INPUT:','s');")
        self.wait(lambda state:'PERMITTED_INPUT:' in state['output'])
        time.sleep(.3)
        self.k.interrupt(); self.k.interrupt()
        state=self.wait(lambda state:state['status'] in ('idle','dead'))
        self.assertEqual(state['status'],'idle',state)
        return state

    def root_hook_is_skipped(self, reader):
        self.run_code('preserved=127; preserved_handle=@sin;')
        pid=self.k.proc.pid; generation=self.k.generation
        callback=("if !isdebugmode() && !isempty(getenv('INDYMAT_DEBUG_CONTROL')); "
                  "remove_input_event_hook(hook_id); hook_root_frames=dbstack('-completenames'); "
                  "hook_reply="+reader+"; endif;")
        self.k.submit('hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); keyboard;')
        self.wait(lambda state:state['status']=='paused' and state['debug']['ready'])
        self.k.interrupt(); self.k.interrupt()
        state=self.wait(lambda state:state['status'] in ('idle','dead'))
        self.assertEqual(state['status'],'idle',state)
        self.assertEqual(self.k.proc.pid,pid)
        self.assertEqual(self.k.generation,generation)
        self.run_code("assert(preserved==127); assert(preserved_handle(0)==0); "
                      "assert(!exist('hook_reply','var')); assert(!exist('hook_root_frames','var')); "
                      "remove_input_event_hook(hook_id);")

    def test_hook_builtin_input_after_stop_dispatch(self):
        self.root_hook_is_skipped("builtin('input','Hook root input:','s')")

    def test_hook_public_input_after_stop_dispatch(self):
        self.root_hook_is_skipped("input('Hook root input:','s')")

    def test_public_hook_input_at_program_input(self):
        callback=("remove_input_event_hook(hook_id); "
                  "hook_reply=input('Hook program input:','s');")
        self.k.submit('hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+"); builtin('input','Program input:','s');")
        state=self.wait(lambda state:state['waiting_input'])
        self.assertIn('Hook program input:',state['output'])
        self.k.input('hook reply')
        time.sleep(.2)
        self.k.interrupt();self.k.interrupt()
        state=self.wait(lambda state:state['status'] in ('idle','dead'))
        self.assertEqual(state['status'],'idle',state)
        self.run_code("assert(strcmp(hook_reply,'hook reply'));")

    def test_clear_all_preserves_hook_registry_like_native(self):
        callback="setenv('INDYMAT_TEST_HOOK_TICKS',sprintf('%d',str2double(getenv('INDYMAT_TEST_HOOK_TICKS'))+1));"
        for registration in ("builtin('add_input_event_hook',@eval,", 'add_input_event_hook(@eval,'):
            with self.subTest(registration=registration):
                self.permitted_input("setenv('INDYMAT_TEST_HOOK_TICKS','0'); hook_id="+
                                     registration+self.k.quote(callback)+"); setenv('INDYMAT_TEST_HOOK_ID',hook_id);")
                self.run_code("assert(str2double(getenv('INDYMAT_TEST_HOOK_TICKS'))>0); "
                              "setenv('INDYMAT_TEST_HOOK_TICKS','0'); clear all; clear functions;")
                self.permitted_input()
                self.run_code("assert(str2double(getenv('INDYMAT_TEST_HOOK_TICKS'))>0); "
                              "remove_input_event_hook(getenv('INDYMAT_TEST_HOOK_ID')); "
                              "unsetenv('INDYMAT_TEST_HOOK_ID'); unsetenv('INDYMAT_TEST_HOOK_TICKS');")

    def test_native_argument_and_output_errors(self):
        forms = [
            ('add_input_event_hook()', "builtin('add_input_event_hook')"),
            ('add_input_event_hook(3)', "builtin('add_input_event_hook',3)"),
            ('add_input_event_hook(@sin,1,2)', "builtin('add_input_event_hook',@sin,1,2)"),
            ('remove_input_event_hook()', "builtin('remove_input_event_hook')"),
            ('remove_input_event_hook(3)', "builtin('remove_input_event_hook',3)"),
            ('remove_input_event_hook(1,2)', "builtin('remove_input_event_hook',1,2)"),
            ('[a,b]=add_input_event_hook(@sin)', "[a,b]=builtin('add_input_event_hook',@sin)"),
            ("a=remove_input_event_hook('missing')", "a=builtin('remove_input_event_hook','missing')"),
        ]
        for public, native in forms:
            with self.subTest(public=public):
                self.run_code(
                    'try; eval('+self.k.quote(public)+"); error('test:accepted','accepted'); catch; p=lasterror(); end_try_catch; "
                    'try; eval('+self.k.quote(native)+"); error('test:accepted','accepted'); catch; n=lasterror(); end_try_catch; "
                    "assert(strcmp(p.identifier,n.identifier)); assert(strcmp(p.message,n.message));")


if __name__=='__main__': unittest.main()
