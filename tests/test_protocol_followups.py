"""Review follow-ups against real Octave: crash guard, staging failures, relocation edges, paused eval."""
import os
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class Base(unittest.TestCase):
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

    def alive(self):
        state = self.code('followup_alive=731;')
        self.assertEqual(next(v['preview'] for v in state['variables'] if v['name'] == 'followup_alive'), '731')

    @staticmethod
    def preview(state, name):
        return next(v['preview'] for v in state['variables'] if v['name'] == name)


class CommandLineFunctionGuardTests(Base):
    """dbclear on a command-line function kills Octave 11.3 (SIGSEGV); never reach it."""

    def make(self, name):
        source = self.work/f'{name}.m'
        source.write_text(f'function y={name}()\n y=1;\n y=y+1;\nendfunction\n')
        self.kernel.configure_breakpoint(source, 3, 'set')
        self.idle()
        self.code(f'function out={name}(); out=731; endfunction')
        return source

    def test_relocation_with_command_line_function_of_the_old_name(self):
        source = self.make('guard_old')
        target = self.work/'guard_new.m'
        source.rename(target)
        target.write_text('function y=guard_new()\n y=1;\n y=y+1;\nendfunction\n')
        self.kernel.relocate_file_breakpoints(str(source), str(target))
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['failed']), 1, report)
        self.assertIn('komut satırı', report['failed'][0]['message'])
        self.assertFalse(report['moved'], report)
        # The new location was not installed, and the map agrees with Octave.
        self.assertFalse(self.kernel.breakpoints)
        self.code("s=dbstatus('guard_new'); assert(isempty(s));")
        self.code('assert(guard_old()==731);')
        self.alive()

    def test_relocation_drop_reports_failed_engine_cleanup(self):
        source = self.make('guard_drop')
        source.unlink()
        self.kernel.relocate_file_breakpoints(str(source), None)
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['dropped']), 1, report)
        self.assertIn('temizlenemedi', report['dropped'][0]['message'])
        self.assertFalse(self.kernel.breakpoints)
        self.alive()

    def test_remove_and_clear_all_refuse_without_crashing(self):
        source = self.make('guard_remove')
        self.kernel.configure_breakpoint(source, 3, 'remove')
        state = self.idle(error=True)
        self.assertIn('komut satırı', state['error'])
        self.assertEqual(set(self.kernel.breakpoints[str(source.resolve())]), {3})
        self.kernel.clear_breakpoints()
        state = self.idle(error=True)
        self.assertIn('komut satırı', state['error'])
        self.assertTrue(self.kernel.breakpoints)
        self.alive()
        # Once the command-line function is gone, both operations work again.
        self.code('clear -f guard_remove')
        self.kernel.clear_breakpoints()
        self.idle()
        self.assertFalse(self.kernel.breakpoints)

    def test_run_to_cursor_refuses_command_line_name(self):
        runner = self.work/'guard_runner.m'
        runner.write_text('function y=guard_runner()\n y=1;\n y=y+1;\n y=y+2;\nendfunction\n')
        target = self.work/'guard_cmdline.m'
        target.write_text('function y=guard_cmdline()\n y=1;\n y=2;\nendfunction\n')
        self.code('function out=guard_cmdline(); out=731; endfunction')
        self.kernel.configure_breakpoint(runner, 2, 'set')
        self.idle()
        self.kernel.submit('guard_result=guard_runner();')
        state = self.wait(True)
        self.kernel.run_to_cursor(target, 3)
        state = self.wait(True, state['debug']['serial'])
        self.assertIn('komut satırı', state['output'])
        self.kernel.debug('quit')
        self.idle(error=True)
        self.alive()


class StagingFailureTests(Base):
    def test_write_failure_leaves_kernel_idle(self):
        original = Path.write_text
        def failing(path, *args, **kwargs):
            if path.name == 'preapply.txt':
                raise OSError(28, 'No space left on device')
            return original(path, *args, **kwargs)
        with patch.object(Path, 'write_text', failing):
            with self.assertRaises(ValueError) as caught:
                self.kernel.submit('never_runs=1;')
        self.assertIn('hazırlanamadı', str(caught.exception))
        self.assertEqual(self.kernel.snapshot()['status'], 'idle')
        self.assertEqual([p for p in self.kernel.runtime.iterdir() if p.is_dir() and not (p/'result.json').exists()], [])
        state = self.code('after_failure=1;')
        self.assertNotIn('never_runs', [v['name'] for v in state['variables']])

    def test_read_failure_ends_job_with_turkish_error(self):
        original = Path.write_text
        def unreadable(path, *args, **kwargs):
            result = original(path, *args, **kwargs)
            if path.name == 'preapply.txt':
                os.chmod(path, 0)
            return result
        with patch.object(Path, 'write_text', unreadable):
            self.kernel.submit('never_runs=1;')
            state = self.idle(error=True)
        self.assertIn('Kesme noktaları uygulanamadı', state['error'])
        self.assertNotIn('never_runs', [v['name'] for v in state['variables']])
        self.alive()


class RelocationPathTests(Base):
    def moved_report(self, stem):
        source = self.work/f'{stem}_old.m'
        source.write_text(f'function y={stem}_old()\n y=1;\n y=y+1;\nendfunction\n')
        self.kernel.configure_breakpoint(source, 3, 'set')
        self.idle()
        return source

    def test_first_relocation_after_application_path_removed(self):
        for index, removal in enumerate([f"rmpath({Kernel.quote(ROOT/'octave')});", 'restoredefaultpath;']):
            with self.subTest(removal=removal):
                source = self.moved_report(f'pathrm{index}')
                self.code(removal)
                target = self.work/f'pathrm{index}_new.m'
                source.rename(target)
                target.write_text(f'function y=pathrm{index}_new()\n y=1;\n y=y+1;\nendfunction\n')
                self.kernel.relocate_file_breakpoints(str(source), str(target))
                report = self.idle()['breakpoint_relocation']
                self.assertEqual(len(report['moved']), 1, report)
                self.assertFalse(report['failed'], report)
                self.assertIn(str(target.resolve()), self.kernel.breakpoints)
                self.kernel.clear_breakpoints()
                self.idle()


class EmptiedPathTests(Base):
    def test_path_emptied_by_user_ends_the_job_instead_of_hanging(self):
        self.kernel.submit("path('');")
        state = self.idle(error=True)
        self.assertEqual(state['status'], 'idle')
        self.kernel.reset()
        self.idle()
        self.alive()


class PausedEvalTests(Base):
    def pause(self):
        fixture = self.work/'paused_inner.m'
        fixture.write_text('function y=paused_inner()\n y=-1;\n inner=20;\n keyboard;\n y=inner;\nendfunction\n')
        self.kernel.submit('paused_result=paused_inner();')
        return self.wait(True)

    def test_return_stops_the_line_like_the_prompt(self):
        state = self.pause()
        # Same as the prompt itself: the statement after return never runs, the
        # snapshot still arrives, and Continue then leaves the function (y stays -1).
        self.kernel.debug('eval', 'inner=77; return; inner=88;')
        state = self.wait(True, state['debug']['serial'])
        self.assertEqual(self.preview(state, 'inner'), '77')
        self.kernel.debug('continue')
        state = self.idle()
        self.assertEqual(self.preview(state, 'paused_result'), '-1')

    def test_no_helper_frame_error_report_and_multiple_statements(self):
        state = self.pause()
        self.kernel.debug('eval', "names=dbstack(); frames_seen=sum(strncmp({names.name},'__mf_debug',10)); a=1; b=a+1;")
        state = self.wait(True, state['debug']['serial'])
        self.assertEqual(self.preview(state, 'frames_seen'), '0')
        self.assertEqual(self.preview(state, 'b'), '2')
        self.kernel.debug('eval', "try; error('inner_boom'); catch caught; stack_has_helper=any(strncmp({caught.stack.name},'__mf_debug',10)); end")
        state = self.wait(True, state['debug']['serial'])
        self.assertEqual(self.preview(state, 'stack_has_helper'), 'false')
        self.kernel.debug('eval', "error('plain_boom')")
        state = self.wait(True, state['debug']['serial'])
        self.assertIn('plain_boom', state['output'])
        self.kernel.debug('eval', "warning('off','all'); lasterror_has_helper=any(strncmp({lasterror().stack.name},'__mf_debug',10));")
        state = self.wait(True, state['debug']['serial'])
        self.assertEqual(self.preview(state, 'lasterror_has_helper'), 'false')
        self.kernel.debug('continue')
        self.idle()



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main()
