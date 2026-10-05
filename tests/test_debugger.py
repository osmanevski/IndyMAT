import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from backend.kernel import Kernel
from backend.files import Workspace
from app import Handler


class DebuggerTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.workspace=Path(self.temp.name)/'workspace'
        self.workspace.mkdir()
        self.fixture=self.workspace/'debugger_nested_fixture.m'
        self.fixture.write_text(
            'function out=debugger_nested_fixture(seed)\n'
            '  outer_local=seed+100;\n'
            '  out=middle(seed);\n'
            '  function mid=middle(value)\n'
            '    middle_local=value+10;\n'
            '    mid=inner(value);\n'
            '  endfunction\n'
            '  function answer=inner(value)\n'
            '    inner_local=value+1;\n'
            '    answer=inner_local*2;\n'
            '  endfunction\n'
            'endfunction\n',encoding='utf-8')
        self.other=self.workspace/'debugger_other_fixture.m'
        self.other.write_text('function y=debugger_other_fixture(x)\n\n  y=x+1;\nendfunction\n',encoding='utf-8')
        self.slow=self.workspace/'debugger_slow_fixture.m'
        self.slow.write_text('function y=debugger_slow_fixture(x)\n  marker=x;\n  while true; end\n  y=marker+1;\nendfunction\n',encoding='utf-8')
        self.kernel=Kernel(ROOT,Path(self.temp.name)/'runtime',self.workspace)
        self.wait_idle()

    def tearDown(self):
        state=self.kernel.snapshot()
        if state['status'] in ('running','paused','starting'):
            self.kernel.interrupt()
            try:self.wait_idle(10)
            except AssertionError:pass
        self.kernel.close()
        self.temp.cleanup()

    def wait_idle(self,seconds=15):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            state=self.kernel.snapshot()
            if state['status'] in ('idle','dead'):return state
            time.sleep(.03)
        raise AssertionError(self.kernel.snapshot())

    def wait_paused(self,serial=None,seconds=15):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            state=self.kernel.snapshot()
            debug=state.get('debug') or {}
            if state['status']=='paused' and debug.get('ready') and (serial is None or debug.get('serial')!=serial):return state
            time.sleep(.03)
        raise AssertionError(self.kernel.snapshot())

    def set_breakpoint(self,path,line,condition=''):
        self.kernel.breakpoint(path,line,True,condition)
        state=self.wait_idle()
        self.assertFalse(state['error'],state)

    def test_stack_is_newest_first_and_frame_switch_changes_locals(self):
        self.set_breakpoint(self.fixture,10)
        self.kernel.submit('stack_result=debugger_nested_fixture(4);')
        state=self.wait_paused()
        stack=state['debug']['stack']
        self.assertGreaterEqual(len(stack),3,stack)
        self.assertEqual([frame['name'].split('>')[-1] for frame in stack[:3]],['inner','middle','debugger_nested_fixture'])
        self.assertTrue(stack[0]['current'])
        self.assertIn('inner_local',{item['name'] for item in state['variables']})
        with self.assertRaises(ValueError):self.kernel.debug('dbcont')
        with self.assertRaises(ValueError):self.kernel.debug('frame','2; dbquit')
        self.assertEqual(self.kernel.snapshot()['status'],'paused')
        serial=state['debug']['serial']
        self.kernel.debug('frame',2)
        state=self.wait_paused(serial)
        self.assertTrue(stack[0]['file'].endswith('debugger_nested_fixture.m'))
        self.assertEqual(next(frame['index'] for frame in state['debug']['stack'] if frame['current']),2,state)
        self.assertIn('middle_local',{item['name'] for item in state['variables']})
        self.assertNotIn('inner_local',{item['name'] for item in state['variables']})
        self.kernel.interrupt()
        self.kernel.interrupt()
        self.assertEqual(self.wait_idle()['status'],'idle')

    def test_conditional_breakpoint_false_true_and_error_semantics(self):
        self.set_breakpoint(self.fixture,10,'value == 3')
        self.kernel.submit('conditional_false=debugger_nested_fixture(2);')
        state=self.wait_idle()
        self.assertFalse(state['error'],state)
        self.kernel.submit('conditional_true=debugger_nested_fixture(3);')
        state=self.wait_paused()
        self.assertEqual(state['debug']['line'],10)
        self.kernel.debug('continue')
        self.assertFalse(self.wait_idle()['error'])
        self.kernel.configure_breakpoint(self.fixture,10,'set','missing_debug_value > 0')
        self.wait_idle()
        self.kernel.submit('debugger_nested_fixture(3);')
        state=self.wait_paused()
        self.assertIn('Error evaluating breakpoint condition',state['output'])
        self.kernel.debug('quit')
        self.wait_idle()
        self.kernel.configure_breakpoint(self.fixture,10,'set','error("debug-condition-boom")')
        self.wait_idle()
        self.kernel.submit('debugger_nested_fixture(3);')
        state=self.wait_paused()
        self.assertIn('debug-condition-boom',state['output'])
        self.kernel.debug('quit')
        self.wait_idle()

    def test_condition_validation_and_literal_quoting(self):
        for condition in ('value > 0\ndisp(1)','value > 0\x00disp(1)',"strcmp(value, 'x)",'"missing', 'x'*1001):
            with self.subTest(condition=condition[:20]):
                with self.assertRaises(ValueError):self.kernel.configure_breakpoint(self.fixture,10,'set',condition)
        condition="strcmp(label, 'x'); system('trusted-side-effect')"
        checked=self.kernel._condition(condition)
        command=self.kernel._set_breakpoint_command(self.fixture,10,checked)
        self.assertIn("'strcmp(label, ''x''); system(''trusted-side-effect'')'",command)
        self.assertNotIn("if strcmp(label, 'x')",command)

    def test_conditions_and_disabled_breakpoints_survive_reset(self):
        self.set_breakpoint(self.fixture,10,'value == 7')
        self.kernel.configure_breakpoint(self.other,2,'set','x > 0',False)
        self.wait_idle()
        self.kernel.reset()
        state=self.wait_idle()
        rows={item['file']:item['breakpoints'] for item in state['breakpoints']}
        self.assertEqual(rows[str(self.fixture.resolve())],[{'line':10,'enabled':True,'condition':'value == 7'}])
        self.assertEqual(rows[str(self.other.resolve())],[{'line':2,'enabled':False,'condition':'x > 0'}])
        self.kernel.submit('reset_restored=debugger_nested_fixture(7);')
        self.assertEqual(self.wait_paused()['debug']['line'],10)
        self.kernel.debug('continue')
        self.wait_idle()

    def test_conditional_breakpoint_survives_real_file_operation_rename(self):
        source=(self.workspace/'debugger_relocate_old.m').resolve()
        source.write_text(
            'relocate_total=0;\n'
            'for relocate_value=1:3\n'
            '  relocate_total+=relocate_value;\n'
            'endfor\n',encoding='utf-8')
        self.set_breakpoint(source,3,'relocate_value == 2')
        workspace=Workspace(self.workspace,self.workspace)
        handler=object.__new__(Handler)
        handler.server=SimpleNamespace(app=SimpleNamespace(kernel=self.kernel,workspace=workspace,file_lock=threading.Lock()))
        handler.send=lambda status,data:(status,data)
        status,result=handler.post('/api/file-operation',{'operation':'rename','source':str(source),'name':'debugger_relocate_new.m'})
        self.assertEqual(status,200);self.assertTrue(result['breakpoint_job'])
        state=self.wait_idle();self.assertFalse(state['error'],state)
        destination=Path(result['path'])
        rows={item['file']:item['breakpoints'] for item in state['breakpoints']}
        self.assertNotIn(str(source),rows)
        self.assertEqual(rows[str(destination)],[{'line':3,'enabled':True,'condition':'relocate_value == 2'}])
        self.kernel.submit(mode='file',argument=str(destination))
        state=self.wait_paused()
        self.assertEqual(state['debug']['file'],str(destination))
        self.assertEqual(state['debug']['line'],3)
        self.assertEqual(next(item['preview'] for item in state['variables'] if item['name']=='relocate_value'),'2')
        self.kernel.debug('continue');state=self.wait_idle();self.assertFalse(state['error'],state)
        self.assertEqual(next(item['preview'] for item in state['variables'] if item['name']=='relocate_total'),'6')
        old_path=self.kernel.quote(source.resolve())
        self.kernel.submit(f"old_status=dbstatus(); assert(isempty(old_status) || ~any(strcmp({{old_status.file}},{old_path}))); clear old_status;")
        state=self.wait_idle();self.assertFalse(state['error'],state)

    def test_run_to_cursor_hit_and_job_end_remove_ephemeral_breakpoint(self):
        self.set_breakpoint(self.fixture,9)
        self.set_breakpoint(self.fixture,10,'value == 99')
        self.kernel.submit('rtc_hit=debugger_nested_fixture(4);')
        self.assertEqual(self.wait_paused()['debug']['line'],9)
        self.kernel.run_to_cursor(self.fixture,10)
        state=self.wait_paused()
        self.assertEqual(state['debug']['line'],10)
        self.assertNotIn('run_to_cursor',state)
        self.kernel.debug('continue')
        self.wait_idle()
        self.kernel.configure_breakpoint(self.fixture,9,'remove')
        self.wait_idle()
        self.kernel.submit('rtc_restored_false=debugger_nested_fixture(5);')
        self.assertEqual(self.wait_idle()['status'],'idle')
        self.kernel.submit('rtc_restored_true=debugger_nested_fixture(99);')
        self.assertEqual(self.wait_paused()['debug']['line'],10)
        self.kernel.debug('continue')
        self.wait_idle()
        self.set_breakpoint(self.fixture,9)
        self.kernel.submit('rtc_end=debugger_nested_fixture(5);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        state=self.wait_idle()
        self.assertNotIn('run_to_cursor',state)
        self.kernel.submit('rtc_other=debugger_other_fixture(9);')
        state=self.wait_idle()
        self.assertFalse(state['error'],state)

    def test_run_to_cursor_in_file_created_after_engine_start(self):
        # HTTP/UI create fixtures after the server starts. Timestamp checking
        # used to attach the temporary breakpoint to a reparsed, inactive body.
        self.kernel.submit('recent_time_stamp=ignore_function_time_stamp();')
        self.wait_idle()
        for frame in (1,2,3):
            with self.subTest(frame=frame):
                stem=f'debugger_recent_fixture_{frame}'
                recent=self.workspace/(stem+'.m')
                recent.write_text(self.fixture.read_text().replace('debugger_nested_fixture',stem))
                self.set_breakpoint(recent,9,'value == 3')
                self.kernel.submit(f'recent_false={stem}(2);')
                self.assertFalse(self.wait_idle()['error'])
                job=self.kernel.submit(f'recent_true={stem}(3);')
                state=self.wait_paused()
                self.assertEqual(state['debug']['line'],9)
                pid=self.kernel.proc.pid
                generation=self.kernel.generation
                if frame!=1:
                    self.kernel.debug('frame',frame)
                    state=self.wait_paused(state['debug']['serial'])
                    self.assertEqual(state['debug']['frame'],frame)
                self.kernel.run_to_cursor(recent,10)
                state=self.wait_paused(state['debug']['serial'])
                self.assertEqual(state['debug']['line'],10,state)
                self.assertEqual(state['debug']['frame'],1)
                self.assertEqual(state['job'],job)
                self.assertEqual(self.kernel.proc.pid,pid)
                self.assertEqual(self.kernel.generation,generation)
                self.assertNotIn('run_to_cursor',state)
                self.kernel.debug('continue')
                self.assertFalse(self.wait_idle()['error'])
                self.kernel.submit(f"assert(strcmp(ignore_function_time_stamp(),recent_time_stamp)); recent_cleanup={stem}(2);")
                state=self.wait_idle()
                self.assertEqual(state['status'],'idle')
                self.assertFalse(state['error'],state)
                self.kernel.submit(f'recent_preserved={stem}(3);')
                self.assertEqual(self.wait_paused()['debug']['line'],9)
                self.kernel.interrupt()
                self.kernel.interrupt()
                self.assertEqual(self.wait_idle()['status'],'idle')
                self.kernel.clear_breakpoints()
                self.wait_idle()

    def test_run_to_cursor_cleanup_on_quit_and_stop_from_selected_frame(self):
        self.set_breakpoint(self.fixture,9)
        self.set_breakpoint(self.fixture,10)
        self.kernel.submit('rtc_quit=debugger_nested_fixture(2);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        state=self.wait_paused()
        self.assertIn('run_to_cursor',state)
        self.kernel.debug('quit')
        self.wait_idle()
        self.kernel.submit('rtc_stop=debugger_nested_fixture(3);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        state=self.wait_paused()
        self.assertTrue(any(frame['current'] for frame in state['debug']['stack']),state)
        self.assertGreaterEqual(len(state['debug']['stack']),2,state)
        self.kernel.debug('frame',2)
        state=self.wait_paused(state['debug']['serial'])
        self.assertEqual(next(frame['index'] for frame in state['debug']['stack'] if frame['current']),2,state)
        self.kernel.interrupt()
        self.kernel.interrupt()
        state=self.wait_idle()
        self.assertNotIn('run_to_cursor',state)
        self.kernel.submit('rtc_reset=debugger_nested_fixture(4);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        state=self.wait_paused()
        self.assertIn('run_to_cursor',state)
        self.kernel.reset()
        state=self.wait_idle()
        self.assertNotIn('run_to_cursor',state)
        self.assertEqual(state['breakpoints'][0]['lines'],[9,10])
        self.kernel.clear_breakpoints()
        self.wait_idle()
        self.kernel.submit('rtc_cleanup_probe=debugger_other_fixture(1);')
        self.assertEqual(self.wait_idle()['status'],'idle')

    def test_recent_loop_removes_temporary_breakpoint_from_executing_body(self):
        recent=self.workspace/'debugger_recent_loop.m'
        recent.write_text('function out=debugger_recent_loop(seed)\n  out=0;\n  for iteration=1:2\n    out+=seed;\n    marker=out;\n  endfor\nendfunction\n')
        self.set_breakpoint(recent,4,'iteration == 1')
        self.set_breakpoint(recent,5,'iteration == 99')
        self.kernel.submit('recent_loop_result=debugger_recent_loop(3);')
        state=self.wait_paused()
        self.assertEqual(state['debug']['line'],4)
        self.kernel.run_to_cursor(recent,5)
        state=self.wait_paused(state['debug']['serial'])
        self.assertEqual(state['debug']['line'],5)
        self.assertNotIn('run_to_cursor',state)
        self.kernel.debug('continue')
        state=self.wait_idle()
        self.assertEqual(state['status'],'idle')
        self.assertFalse(state['error'],state)
        self.assertEqual(next(v['preview'] for v in state['variables'] if v['name']=='recent_loop_result'),'6')

    def test_run_to_cursor_cleanup_when_stopped_while_running(self):
        self.set_breakpoint(self.slow,2)
        self.kernel.submit('rtc_running_stop=debugger_slow_fixture(3);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        deadline=time.monotonic()+3
        while time.monotonic()<deadline:
            state=self.kernel.snapshot()
            if state.get('run_to_cursor',{}).get('continued'):break
            time.sleep(.01)
        self.assertTrue(self.kernel.snapshot().get('run_to_cursor',{}).get('continued'))
        self.kernel.interrupt()
        state=self.wait_idle(10)
        self.assertEqual(state['status'],'idle',state)
        self.kernel.interrupt()
        self.assertNotIn('run_to_cursor',state)
        self.kernel.submit('rtc_setup_stop=debugger_slow_fixture(4);')
        self.wait_paused()
        self.kernel.run_to_cursor(self.other,2)
        self.kernel.interrupt()
        self.kernel.interrupt()
        state=self.wait_idle(10)
        self.assertEqual(state['status'],'idle',state)
        self.assertNotIn('run_to_cursor',state)
        self.kernel.clear_breakpoints()
        self.wait_idle()
        self.kernel.submit('rtc_after_stop=debugger_other_fixture(2);')
        self.assertEqual(self.wait_idle()['status'],'idle')

    def test_debug_commands_rejected_unless_really_paused(self):
        for command,code in [('continue',''),('step',''),('in',''),('out',''),('quit',''),('eval','x=1'),('frame',1)]:
            with self.subTest(command=command):
                with self.assertRaises(ValueError):self.kernel.debug(command,code)
        with self.assertRaises(ValueError):self.kernel.run_to_cursor(self.fixture,10)
        self.kernel.submit('pause(1);')
        time.sleep(.1)
        with self.assertRaises(ValueError):self.kernel.debug('continue')
        with self.assertRaises(ValueError):self.kernel.run_to_cursor(self.fixture,10)
        self.kernel.interrupt()
        self.wait_idle()



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
