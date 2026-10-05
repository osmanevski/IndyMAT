"""Kernel.relocate_file_breakpoints against real Octave: per-breakpoint outcomes."""
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class RelocationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name)/'work'
        self.work.mkdir()
        self.kernel = Kernel(ROOT, Path(self.temp.name)/'runtime', self.work, executable='octave-cli')
        self.idle()

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def idle(self, error=False):
        for _ in range(600):
            state = self.kernel.snapshot()
            if state['status'] in ('idle', 'dead'):
                break
            time.sleep(.03)
        self.assertEqual(state['status'], 'idle', state)
        if not error:
            self.assertFalse(state['error'], state)
        return state

    def code(self, code):
        self.kernel.submit(code)
        return self.idle()

    def test_moved_breakpoint_really_pauses_and_report_lists_it(self):
        source = self.work/'reloc_run_old.m'
        source.write_text('function y=reloc_run_old()\n y=1;\n y=y+1;\nendfunction\n')
        self.kernel.configure_breakpoint(source, 3, 'set')
        self.idle()
        target = self.work/'reloc_run_new.m'
        source.rename(target)
        target.write_text('function y=reloc_run_new()\n y=1;\n y=y+1;\nendfunction\n')
        self.kernel.relocate_file_breakpoints(str(source), str(target))
        report = self.idle()['breakpoint_relocation']
        self.assertEqual([row['line'] for row in report['moved']], [3], report)
        self.assertFalse(report['failed'] or report['dropped'], report)
        self.assertEqual(list(self.kernel.breakpoints), [str(target.resolve())])
        self.kernel.submit('moved_result=reloc_run_new();')
        for _ in range(300):
            state = self.kernel.snapshot()
            if state['status'] == 'paused' and state['debug']['ready']:
                break
            time.sleep(.03)
        self.assertEqual(state['status'], 'paused', state)
        self.assertEqual(state['debug']['line'], 3)
        self.kernel.debug('quit')
        self.idle(error=True)

    def test_reapply_failure_preserves_function_and_resolver(self):
        source = self.work/'relocate_failure_source.m'
        source.write_text('function out=relocate_failure_source()\n out=731;\nendfunction\n')
        self.code('handle_before=@relocate_failure_source; old_value=handle_before();')
        self.kernel.breakpoint(source, 2, True)
        self.idle()
        target = self.work/'relocate_failure_target.m'
        source.rename(target)
        target.write_text('function broken(\n syntax broken\n')
        self.kernel.relocate_file_breakpoints(str(source), str(target))
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['failed']), 1, report)
        self.assertEqual(report['failed'][0]['path'], str(target.resolve()))
        self.assertTrue(report['failed'][0]['message'])
        self.assertFalse(report['moved'], report)
        self.assertFalse(self.kernel.breakpoints)
        # Whether a handle to the renamed-away file still runs depends on when Octave next re-checks the file's
        # timestamp, so that is not asserted; the session must be alive and no autoload mapping may be left behind.
        self.code("mappings=autoload(); assert(isempty(mappings) || ~any(strcmp({mappings.function},'relocate_failure_source'))); assert(is_function_handle(handle_before));")

    def test_command_line_function_survives_relocation(self):
        source = self.work/'cmdline_fn.m'
        source.write_text('function y=cmdline_fn()\n y=1;\nendfunction\n')
        self.kernel.breakpoint(source, 2, True)
        self.idle()
        source.unlink()
        self.kernel.relocate_file_breakpoints(str(source), None)
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['dropped']), 1, report)
        self.assertFalse(self.kernel.breakpoints)
        self.code("function out=cmdline_fn(); out=731; endfunction; assert(cmdline_fn()==731);")
        self.code("assert(cmdline_fn()==731);")

    def test_invalid_and_missing_targets_are_dropped(self):
        for suffix in ['bad-name.m', 'bad.txt', 'folder', 'missing.m', 'if.m', 'ağ.m']:
            with self.subTest(target=suffix):
                source = self.work/'old_relocate.m'
                source.write_text('x=1;\ny=2;\n')
                self.kernel.breakpoint(source, 2, True)
                self.idle()
                target = self.work/suffix
                if suffix == 'folder':
                    source.unlink()
                    target.mkdir(exist_ok=True)
                else:
                    source.rename(target)
                    if suffix == 'missing.m':
                        target.unlink()
                self.kernel.relocate_file_breakpoints(str(source), str(target))
                result = self.idle()['breakpoint_relocation']
                self.assertEqual(len(result['dropped']), 1, result)
                self.assertFalse(self.kernel.breakpoints)
                self.code('relocation_alive=1;')

    def test_submission_failure_leaves_no_stale_paths(self):
        source = self.work/'submit_old.m'
        source.write_text('x=1;\ny=2;\n')
        self.kernel.breakpoint(source, 2, True)
        self.idle()
        target = self.work/'submit_new.m'
        source.rename(target)
        with patch.object(self.kernel, '_submit', side_effect=OSError('submission failed')):
            with self.assertRaises(OSError):
                self.kernel.relocate_file_breakpoints(str(source), str(target))
        self.assertFalse(self.kernel.breakpoints)
        self.assertEqual(len(self.kernel.snapshot()['breakpoint_relocation']['failed']), 1)

    def test_mixed_folder_results_and_existing_autoload(self):
        before = self.work/'before'
        before.mkdir()
        source = before/'folder_function.m'
        source.write_text('function y=folder_function()\n y=731;\nendfunction\n')
        self.code(f"addpath({self.kernel.quote(before)}); retained_folder_handle=@folder_function; original_folder_value=retained_folder_handle(); autoload('folder_function',{self.kernel.quote(source)});")
        self.kernel.configure_breakpoint(source, 2, 'set', '1 == 1')
        self.idle()
        self.kernel.configure_breakpoint(source, 3, 'set', enabled=False)
        self.idle()
        after = self.work/'after'
        before.rename(after)
        self.kernel.relocate_file_breakpoints(before, after)
        report = self.idle()['breakpoint_relocation']
        self.assertEqual(len(report['moved']), 2, report)
        self.assertFalse(report['failed'], report)
        self.assertNotIn(str(source.resolve()), self.kernel.breakpoints)
        self.assertIn(str((after/'folder_function.m').resolve()), self.kernel.breakpoints)
        self.code(f"mapping_after=autoload(); assert(strcmp(mapping_after(strcmp({{mapping_after.function}},'folder_function')).file,{self.kernel.quote(source)})); retained_folder_value=retained_folder_handle(); assert(retained_folder_value==731);")

    def test_rejected_while_running_reconciles_missing_source(self):
        source = self.work/'busy_source.m'
        source.write_text('function y=busy_source()\n y=731;\nendfunction\n')
        self.kernel.configure_breakpoint(source, 2, 'set')
        self.idle()
        self.kernel.configure_breakpoint(source, 3, 'set', enabled=False)
        self.idle()
        self.kernel.submit("fprintf('BUSY_LOOP_STARTED\\n'); while 1; end")
        for _ in range(200):
            if 'BUSY_LOOP_STARTED' in self.kernel.snapshot()['output']:
                break
            time.sleep(.02)
        else:
            self.fail('Çalışan kullanıcı döngüsü başlamadı.')
        target = self.work/'busy_target.m'
        source.rename(target)
        with self.assertRaises(ValueError):
            self.kernel.relocate_file_breakpoints(source, target)
        report = self.kernel.snapshot()['breakpoint_relocation']
        self.assertEqual(len(report['failed']), 1, report)
        self.assertEqual(len(report['moved']), 1, report)
        self.assertNotIn(str(source.resolve()), self.kernel.breakpoints)
        self.assertEqual(set(self.kernel.breakpoints[str(target.resolve())]), {3})
        self.kernel.interrupt()
        self.idle(error=True)



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main()
