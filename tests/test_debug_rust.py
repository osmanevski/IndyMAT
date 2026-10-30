"""Debugger behaviour on the real indy-kernel, adapted from the Octave kernel's tests
(test_debug_dynamic, test_debug_inspection, test_breakpoint_relocation, test_debug_stop_recovery,
test_protocol_followups) where they are not about Octave's stdin transport. No Octave."""
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.kernel_rust import RustKernel
from tests.rust_engine_support import require_rust_kernel

ROOT = Path(__file__).resolve().parents[1]
FORMS = {'direct': 'keyboard', 'eval': "eval('keyboard')", 'base': "evalin('base','keyboard')",
         'caller': 'dynamic_caller()', 'handle': 'feval(@keyboard)'}


def setUpModule():
    require_rust_kernel()


class Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='debug-rust-')
        base = Path(self.temp.name).resolve()
        self.work = base / 'work'
        self.work.mkdir()
        self.k = RustKernel(ROOT, base / 'runtime', self.work)
        self.pid = self.k.session.proc.pid
        self.generation = self.k.generation

    def tearDown(self):
        self.k.close()
        self.temp.cleanup()

    def wait(self, predicate, seconds=10):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            state = self.k.snapshot()
            if predicate(state):
                return state
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))

    def idle(self, error=False):
        state = self.wait(lambda s: s['status'] in ('idle', 'dead'))
        self.assertEqual(state['status'], 'idle', state)
        if not error:
            self.assertFalse(state['error'], state)
        return state

    def paused(self, serial=None):
        return self.wait(lambda s: s['status'] == 'paused' and (s.get('debug') or {}).get('ready') and s['debug']['serial'] != serial)

    def code(self, code):
        self.k.submit(code)
        return self.idle()

    def preview(self, state, name):
        return next(v['preview'] for v in state['variables'] if v['name'] == name)

    def same_session(self):
        self.assertEqual(self.k.session.proc.pid, self.pid)
        self.assertEqual(self.k.generation, self.generation)


class DynamicPause(Base):
    def exercise(self, mode, form):
        (self.work / 'dynamic_caller.m').write_text("function dynamic_caller()\n evalin('caller','keyboard');\nendfunction\n")
        self.code('kept=37; kept_handle=@sin;')
        code = 'before_pause=19; ' + FORMS[form] + '; after_pause=23;'
        path = self.work / 'dynamic_entry.m'
        path.write_text(code)
        def start():
            job = self.k.submit(code if mode == 'code' else '', mode=mode, argument=str(path) if mode == 'file' else '')
            state = self.paused()
            self.assertEqual(state['job'], job)
            self.assertIn('before_pause', {v['name'] for v in state['variables']})
            with self.assertRaises(ValueError):
                self.k.input('arbitrary code')
            return state
        state = start()
        if mode == 'code' and form in ('direct', 'eval', 'base', 'handle'):
            self.assertEqual(state['debug']['file'], '')
            self.assertEqual(state['debug']['line'], 0)
        if mode == 'file':
            self.assertEqual(state['debug']['file'], str(path))
            self.assertEqual(state['debug']['line'], 1)
        self.k.debug('continue')
        self.idle()
        self.code('assert(after_pause==23); clear after_pause;')
        start()
        self.k.interrupt()
        self.k.interrupt()
        self.idle(error=True)
        self.same_session()
        self.code('assert(kept==37); assert(kept_handle(0)==0); assert(before_pause==19); assert(~exist("after_pause","var"));')
        start()
        self.k.debug('step')
        end = time.monotonic() + 10
        while time.monotonic() < end:
            state = self.k.snapshot()
            if state['status'] == 'idle':
                break
            if state['status'] == 'paused' and state['debug']['ready']:
                self.k.debug('continue')
                state = self.idle()
                break
            time.sleep(.01)
        self.assertEqual(state['status'], 'idle', state)
        self.assertFalse(state['error'], state)
        self.same_session()


for mode in ('code', 'file'):
    for form in FORMS:
        setattr(DynamicPause, f'test_{mode}_{form}', (lambda m, f: lambda self: self.exercise(m, f))(mode, form))


class Inspection(Base):
    def setUp(self):
        super().setUp()
        self.fixture = self.work / 'debug_inspection_fixture.m'
        self.fixture.write_text(
            'function out=debug_inspection_fixture(seed)\n  outer_local=seed+100;\n  out=middle(seed);\n'
            '  function mid=middle(value)\n    middle_local=[value,value+10];\n    mid=inner(value);\n  endfunction\n'
            '  function answer=inner(value)\n    inner_local=value+1;\n    answer=inner_local*2;\n  endfunction\nendfunction\n')

    def test_selected_frame_inspection_without_new_job_or_mutation(self):
        self.k.breakpoint(self.fixture, 10, True)
        self.idle()
        self.k.submit('debug_inspection_result=debug_inspection_fixture(4);')
        state = self.paused()
        job = state['job']
        self.k.debug('frame', 2)
        state = self.paused(state['debug']['serial'])
        self.assertIn('middle_local', {item['name'] for item in state['variables']})
        serial, output = state['debug']['serial'], state['output']
        self.k.debug('inspect', 'middle_local')
        state = self.paused(serial)
        self.assertEqual(state['job'], job)
        self.same_session()
        self.assertEqual(state['output'], output)
        self.assertEqual(state['debug']['frame'], 2)
        self.assertEqual(state['debug']['detail']['name'], 'middle_local')
        self.assertEqual(state['debug']['detail']['class'], 'double')
        self.assertEqual(state['debug']['detail']['rows'], [['4', '14']])
        # An unknown variable reports an error and stays at the pause.
        serial = state['debug']['serial']
        self.k.debug('inspect', 'no_such_variable')
        state = self.paused(serial)
        self.assertTrue(state['error'])
        self.k.interrupt()
        self.idle(error=True)

    def test_inspection_accepts_only_a_variable_name_while_really_paused(self):
        with self.assertRaises(ValueError):
            self.k.debug('inspect', 'anything')
        self.k.breakpoint(self.fixture, 10, True)
        self.idle()
        self.k.submit('debug_inspection_fixture(5);')
        self.paused()
        for value in ('middle_local; dbquit', 'middle_local(1)', 'middle_local\n dbquit', '__mf_hidden'):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    self.k.debug('inspect', value)
                self.assertEqual(self.k.snapshot()['status'], 'paused')
        self.k.interrupt()
        self.idle(error=True)

    def test_workspace_and_new_jobs_are_refused_while_paused(self):
        self.k.breakpoint(self.fixture, 10, True)
        self.idle()
        self.k.submit('debug_inspection_fixture(5);')
        self.paused()
        for call in (lambda: self.k.submit('x=1'), lambda: self.k.workspace_action('workspace-clear-names', {'names': ['x']}),
                     lambda: self.k.configure_breakpoint(self.fixture, 10, 'remove'), lambda: self.k.clear_breakpoints()):
            with self.assertRaises(ValueError):
                call()
        self.assertEqual(self.k.snapshot()['status'], 'paused')
        self.k.debug('quit')
        state = self.idle()
        self.assertIsNone(state['debug'])


class PausedEval(Base):
    def pause(self):
        (self.work / 'paused_inner.m').write_text('function y=paused_inner()\n y=-1;\n inner=20;\n keyboard;\n y=inner;\nendfunction\n')
        self.k.submit('paused_result=paused_inner();')
        return self.paused()

    def test_return_ends_the_typed_line(self):
        state = self.pause()
        self.k.debug('eval', 'inner=77; return; inner=88;')
        state = self.paused(state['debug']['serial'])
        self.assertEqual(self.preview(state, 'inner'), '77')
        self.k.debug('continue')
        self.assertEqual(self.preview(self.idle(), 'paused_result'), '77')

    def test_errors_multiple_statements_and_helperless_stack(self):
        state = self.pause()
        self.k.debug('eval', "names=dbstack(); seen=numel(names); a=1; b=a+1;")
        state = self.paused(state['debug']['serial'])
        self.assertEqual(self.preview(state, 'seen'), '1')
        self.assertEqual(self.preview(state, 'b'), '2')
        self.k.debug('eval', "error('plain_boom')")
        state = self.paused(state['debug']['serial'])
        self.assertIn('plain_boom', state['output'])
        self.k.debug('eval', 'disp(inner)')
        state = self.paused(state['debug']['serial'])
        self.assertIn('20', state['output'])
        self.k.debug('eval', 'dbcont')
        self.idle()

    def test_input_inside_a_paused_line_uses_the_input_bar(self):
        state = self.pause()
        self.k.debug('eval', "typed=input('? ');")
        self.wait(lambda s: s['waiting_input'])
        self.k.input('5')
        state = self.paused(state['debug']['serial'])
        self.assertFalse(state['waiting_input'])
        self.assertEqual(self.preview(state, 'typed'), '5')
        self.k.debug('continue')
        self.idle()

    def test_a_long_paused_line_can_be_stopped(self):
        state = self.pause()
        self.k.debug('eval', 'while true; end')
        self.wait(lambda s: s['status'] == 'running')
        time.sleep(.2)
        self.k.interrupt()
        self.k.interrupt()
        self.idle(error=True)
        self.same_session()


class Stop(Base):
    def test_stop_in_a_function_with_on_cleanup(self):
        (self.work / 'cleanup_stop.m').write_text("function cleanup_stop()\n c=onCleanup(@() disp('never'));\n keyboard;\nendfunction\n")
        self.code('preserved=127;')
        self.k.submit('cleanup_stop();')
        self.paused()
        self.k.interrupt()
        self.k.interrupt()
        self.assertEqual(self.idle(error=True)['status'], 'idle')
        self.same_session()
        self.code('assert(preserved==127);')
        self.k.interrupt()  # idle: no effect, and it never reaches the next eval
        self.code('assert(preserved==127);')

    def test_stop_right_after_a_resume(self):
        (self.work / 'spin.m').write_text('function spin()\n keyboard;\n while true; end\nendfunction\n')
        self.k.submit('spin();')
        self.paused()
        self.k.debug('continue')
        self.k.interrupt()
        self.idle(error=True)
        self.same_session()

    def test_stop_during_a_pending_run_to_cursor(self):
        path = self.work / 'pending_rtc.m'
        path.write_text('function pending_rtc()\n keyboard;\n a=1;\n a=2;\nendfunction\n')
        self.k.submit('pending_rtc();')
        self.paused()
        self.k.run_to_cursor(path, 4)
        self.k.interrupt()
        self.k.interrupt()
        self.idle(error=True)
        self.assertIsNone(self.k.run_to_cursor_state)

    def test_quit_has_no_error_and_keeps_variables(self):
        (self.work / 'qq.m').write_text('function qq()\n assignin("base","seen",1);\n keyboard;\n assignin("base","late",1);\nendfunction\n')
        self.k.submit('before=1; qq(); after=1;')
        self.paused()
        self.k.debug('quit')
        state = self.idle()
        names = {v['name'] for v in state['variables']}
        self.assertLessEqual({'before', 'seen'}, names)
        self.assertFalse({'after', 'late'} & names)

    def test_reset_while_paused_restores_breakpoints_in_a_new_child(self):
        path = self.work / 'rs.m'
        path.write_text('function rs()\n a=1;\n a=2;\nendfunction\n')
        self.k.breakpoint(path, 2, True)
        self.idle()
        self.k.configure_breakpoint(path, 3, 'set', 'a > 100', False)
        self.idle()
        self.k.submit('rs();')
        self.paused()
        self.k.reset()
        state = self.idle()
        self.assertIsNone(state['debug'])
        rows = {item['file']: item['breakpoints'] for item in state['breakpoints']}
        self.assertEqual(rows[str(path)], [{'line': 2, 'enabled': True, 'condition': ''}, {'line': 3, 'enabled': False, 'condition': 'a > 100'}])
        self.k.submit('rs();')
        self.assertEqual(self.paused()['debug']['line'], 2)
        self.k.debug('continue')
        self.idle()

    def test_child_killed_while_paused_is_a_dead_session(self):
        self.k.submit('keyboard;')
        self.paused()
        self.k.session.kill()
        state = self.wait(lambda s: s['status'] == 'dead')
        self.assertIsNone(state['debug'])
        self.assertTrue(state['error'])
        self.k.reset()
        self.idle()


class Relocation(Base):
    def test_moved_breakpoint_really_pauses_and_report_lists_it(self):
        source = self.work / 'reloc_run_old.m'
        source.write_text('function y=reloc_run_old()\n y=1;\n y=y+1;\nendfunction\n')
        self.k.configure_breakpoint(source, 3, 'set')
        self.idle()
        target = self.work / 'reloc_run_new.m'
        source.rename(target)
        target.write_text('function y=reloc_run_new()\n y=1;\n y=y+1;\nendfunction\n')
        self.k.relocate_file_breakpoints(str(source), str(target))
        report = self.idle()['breakpoint_relocation']
        self.assertEqual([row['line'] for row in report['moved']], [3], report)
        self.assertFalse(report['failed'] or report['dropped'], report)
        self.assertEqual(list(self.k.breakpoints), [str(target.resolve())])
        self.k.submit('moved_result=reloc_run_new();')
        state = self.paused()
        self.assertEqual(state['debug']['line'], 3)
        self.k.debug('quit')
        self.idle()

    def test_unparsable_target_fails_per_breakpoint(self):
        source = self.work / 'relocate_failure_source.m'
        source.write_text('function out=relocate_failure_source()\n out=731;\nendfunction\n')
        self.k.breakpoint(source, 2, True)
        self.idle()
        target = self.work / 'relocate_failure_target.m'
        source.rename(target)
        target.write_text('function broken(\n syntax broken\n')
        self.k.relocate_file_breakpoints(str(source), str(target))
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['failed']), 1, report)
        self.assertEqual(report['failed'][0]['path'], str(target.resolve()))
        self.assertTrue(report['failed'][0]['message'])
        self.assertFalse(report['moved'], report)
        self.assertFalse(self.k.breakpoints)

    def test_deleted_and_invalid_targets_are_dropped(self):
        for suffix in ['bad.txt', 'folder', 'missing.m', None]:
            with self.subTest(target=suffix):
                source = self.work / 'old_relocate.m'
                source.write_text('x=1;\ny=2;\n')
                self.k.breakpoint(source, 2, True)
                self.idle()
                target = self.work / suffix if suffix else None
                if suffix == 'folder':
                    source.unlink()
                    target.mkdir(exist_ok=True)
                elif suffix:
                    source.rename(target)
                    if suffix == 'missing.m':
                        target.unlink()
                else:
                    source.unlink()
                self.k.relocate_file_breakpoints(str(source), str(target) if target else None)
                result = self.idle()['breakpoint_relocation']
                self.assertEqual(len(result['dropped']), 1, result)
                self.assertFalse(self.k.breakpoints)
                self.code('relocation_alive=1;')

    def test_submission_failure_leaves_no_stale_paths(self):
        source = self.work / 'submit_old.m'
        source.write_text('x=1;\ny=2;\n')
        self.k.breakpoint(source, 2, True)
        self.idle()
        target = self.work / 'submit_new.m'
        source.rename(target)
        with patch.object(self.k, '_launch', side_effect=OSError('submission failed')):
            with self.assertRaises(OSError):
                self.k.relocate_file_breakpoints(str(source), str(target))
        self.assertFalse(self.k.breakpoints)
        self.assertEqual(len(self.k.snapshot()['breakpoint_relocation']['failed']), 1)

    def test_folder_move_mixed_enabled_and_disabled(self):
        before = self.work / 'before'
        before.mkdir()
        source = before / 'folder_function.m'
        source.write_text('function y=folder_function()\n y=731;\nendfunction\n')
        self.k.configure_breakpoint(source, 2, 'set', '1 == 1')
        self.idle()
        self.k.configure_breakpoint(source, 3, 'set', enabled=False)
        self.idle()
        after = self.work / 'after'
        before.rename(after)
        self.k.relocate_file_breakpoints(before, after)
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['moved']), 2, report)
        self.assertFalse(report['failed'], report)
        self.assertNotIn(str(source.resolve()), self.k.breakpoints)
        self.assertIn(str((after / 'folder_function.m').resolve()), self.k.breakpoints)
        self.code(f"addpath({self.k.quote(after)});")
        self.k.submit('folder_function();')
        self.assertEqual(self.paused()['debug']['line'], 2)
        self.k.debug('continue')
        self.idle()

    def test_rejected_while_running_reconciles_missing_source(self):
        source = self.work / 'busy_source.m'
        source.write_text('function y=busy_source()\n y=731;\nendfunction\n')
        self.k.configure_breakpoint(source, 2, 'set')
        self.idle()
        self.k.configure_breakpoint(source, 3, 'set', enabled=False)
        self.idle()
        self.k.submit("fprintf('BUSY_LOOP_STARTED\\n'); while 1; end")
        self.wait(lambda s: 'BUSY_LOOP_STARTED' in s['output'])
        target = self.work / 'busy_target.m'
        source.rename(target)
        with self.assertRaises(ValueError):
            self.k.relocate_file_breakpoints(source, target)
        report = self.k.snapshot()['breakpoint_relocation']
        self.assertEqual(len(report['failed']), 1, report)
        self.assertEqual(len(report['moved']), 1, report)
        self.assertEqual(set(self.k.breakpoints[str(target.resolve())]), {3})
        self.k.interrupt()
        self.idle(error=True)


class Features(Base):
    def test_state_does_not_list_the_debugger_as_unsupported(self):
        unsupported = self.k.snapshot()['unsupported']
        for feature in ('debugger', 'breakpoints', 'run_to_cursor'):
            self.assertNotIn(feature, unsupported)

    def test_bad_breakpoint_reports_and_restores_the_map(self):
        path = self.work / 'nocode.m'
        path.write_text('function nocode()\n a=1;\nendfunction\n')
        self.k.breakpoint(path, 500, True)
        state = self.idle(error=True)
        self.assertIn('No executable code', state['error'])
        self.assertEqual(state['breakpoints'], [])

    def test_blank_line_breakpoint_stops_at_the_next_statement(self):
        path = self.work / 'blank.m'
        path.write_text('function y=blank(x)\n\n  y=x+1;\nendfunction\n')
        self.k.breakpoint(path, 2, True)
        state = self.idle()
        self.assertEqual(state['breakpoints'][0]['lines'], [3])
        self.k.submit('blank(1);')
        self.assertEqual(self.paused()['debug']['line'], 3)
        self.k.debug('continue')
        self.idle()
        # The moved breakpoint is removed from the line that now shows it.
        self.k.configure_breakpoint(path, 3, 'remove')
        self.assertEqual(self.idle()['breakpoints'], [])
        self.k.submit('blank(1);')
        self.idle()

    def test_steps_change_the_paused_line_and_the_stack(self):
        path = self.work / 'stepper.m'
        path.write_text('function y=stepper(x)\n a=x;\n b=helper_fn(a);\n y=b;\nendfunction\nfunction r=helper_fn(v)\n r=v+1;\nendfunction\n')
        self.k.breakpoint(path, 2, True)
        self.idle()
        self.k.submit('stepper(1);')
        state = self.paused()
        self.assertEqual(state['debug']['line'], 2)
        self.k.debug('step')
        state = self.paused(state['debug']['serial'])
        self.assertEqual(state['debug']['line'], 3)
        self.k.debug('in')
        state = self.paused(state['debug']['serial'])
        self.assertEqual((state['debug']['line'], state['debug']['stack'][0]['name']), (7, 'helper_fn'))
        self.assertEqual(state['debug']['stack'][1]['line'], 3)
        self.k.debug('out')
        state = self.paused(state['debug']['serial'])
        self.assertEqual((state['debug']['line'], state['debug']['stack'][0]['name']), (4, 'stepper'))
        self.assertIn('a', {v['name'] for v in state['variables']})
        self.k.debug('continue')
        self.idle()


if __name__ == '__main__':
    unittest.main(verbosity=2)
