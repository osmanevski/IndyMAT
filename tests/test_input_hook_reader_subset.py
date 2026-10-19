"""Input-event hooks are limited to native program input waits."""
import time
import unittest

from tests import test_input_hook_guard as support


class HookReaderSubset(unittest.TestCase):
    def setUp(self):
        self.fixture = support.InputHookGuard('test_builtin_eval_callback_has_file_frame')
        self.fixture.setUp()
        self.addCleanup(self.fixture.tearDown)
        self.k = self.fixture.k

    def test_idle_root_kbhit_hook_cannot_consume_next_command(self):
        callback = "hook_entered=true; got=kbhit(); fprintf('KBYTE=%d\\n',double(got));"
        self.fixture.run_code('preserved=91; hook_entered=false; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+');')
        time.sleep(.35)
        state = self.fixture.run_code('assert(preserved==91); assert(!hook_entered); remove_input_event_hook(hook_id);')
        self.assertNotIn('KBYTE=', state['output'])

    def test_retained_kbhit_hook_still_skips_root_after_clear(self):
        callback = "hook_entered=true; got=kbhit(); fprintf('KBYTE=%d\\n',double(got));"
        self.fixture.run_code('hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); clear all; clear functions;')
        time.sleep(.35)
        state = self.fixture.run_code("ordinary=97; assert(ordinary==97); assert(!exist('hook_entered','var'));")
        self.assertNotIn('KBYTE=', state['output'])
        # The retained callback deliberately remains registered until close;
        # its id was part of the workspace that clear all removed.

    def test_public_and_builtin_input_hooks_skip_root_and_debugger(self):
        for reader in ("input('FORBIDDEN_CALLBACK_INPUT:','s')",
                       "builtin('input','FORBIDDEN_CALLBACK_INPUT:','s')"):
            with self.subTest(reader=reader):
                callback = 'hook_entered=true; hook_reply='+reader+';'
                self.fixture.run_code('hook_entered=false; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+');')
                time.sleep(.2)
                self.k.submit('assert(!hook_entered); keyboard;')
                state = self.fixture.wait(lambda state:state['status']=='paused' and state['debug']['ready'])
                time.sleep(.2)
                self.assertFalse(state['waiting_input'], state)
                self.assertNotIn('FORBIDDEN_CALLBACK_INPUT:', state['output'])
                self.k.debug('eval', 'assert(!hook_entered);')
                self.fixture.wait(lambda state:state['status']=='paused' and state['debug']['ready'])
                self.k.interrupt(); self.k.interrupt()
                state = self.fixture.wait(lambda state:state['status'] in ('idle','dead'))
                self.assertEqual(state['status'], 'idle', state)
                self.assertNotIn('FORBIDDEN_CALLBACK_INPUT:', state['output'])
                self.fixture.run_code("assert(!hook_entered); assert(!exist('hook_reply','var')); remove_input_event_hook(hook_id);")

    def test_warning_once_survives_clear_all_and_functions(self):
        state = self.fixture.run_code(
            "lastwarn('',''); hook_id=add_input_event_hook(@sin,0); "
            "[msg,id]=lastwarn(); assert(strcmp(id,'IndyMAT:inputEventHook:limited')); "
            "assert(strcmp(msg,'Input event hooks run only while a program waits for input().')); "
            "remove_input_event_hook(hook_id); lastwarn('',''); "
            "hook_id=add_input_event_hook(@sin,0); [msg,id]=lastwarn(); "
            "assert(isempty(msg)); remove_input_event_hook(hook_id); "
            "clear all; clear functions; lastwarn('',''); "
            "hook_id=add_input_event_hook(@sin,0); [msg,id]=lastwarn(); "
            "assert(!strcmp(id,'IndyMAT:inputEventHook:limited')); remove_input_event_hook(hook_id);"
        )
        # Octave may refresh its own shadowed-function warning after clear
        # functions. Count this contract warning separately from native ones.
        self.assertEqual(state['output'].count('warning: Input event hooks run only while a program waits for input().'), 1)

    def test_native_input_runs_callback_with_frame_below_wrapper(self):
        callback = (
            "hook_frames=dbstack('-completenames'); hook_entered=true; "
            "remove_input_event_hook(hook_id);"
        )
        self.fixture.permitted_input('hook_entered=false; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+');')
        self.fixture.run_code(
            "assert(hook_entered); names={hook_frames.name}; "
            "index=find(strcmp(names,'__mf_input_hooks__'),1); assert(!isempty(index)); "
            "assert(index<numel(hook_frames));"
        )

    def test_public_input_file_poll_retains_existing_no_native_ticks(self):
        callback = "hook_entered=true; remove_input_event_hook(hook_id);"
        self.k.submit('hook_entered=false; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+"); reply=input('Public input:','s');")
        self.fixture.wait(lambda state:state['waiting_input'])
        time.sleep(.3)
        self.k.input('reply')
        state = self.fixture.wait(lambda state:state['status']=='idle')
        self.assertFalse(state['error'], state)
        self.fixture.run_code("assert(strcmp(reply,'reply')); assert(!hook_entered); remove_input_event_hook(hook_id);")


if __name__ == '__main__':
    unittest.main()
