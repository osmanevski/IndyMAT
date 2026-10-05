import tempfile
import time
import unittest
import sys
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.kernel import Kernel


ROOT = Path(__file__).resolve().parents[1]


class DebugInspectionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.workspace = Path(self.temporary.name) / 'workspace'
        self.workspace.mkdir()
        self.fixture = self.workspace / 'debug_inspection_fixture.m'
        self.fixture.write_text(
            'function out=debug_inspection_fixture(seed)\n'
            '  outer_local=seed+100;\n'
            '  out=middle(seed);\n'
            '  function mid=middle(value)\n'
            '    middle_local=[value,value+10];\n'
            '    mid=inner(value);\n'
            '  endfunction\n'
            '  function answer=inner(value)\n'
            '    inner_local=value+1;\n'
            '    answer=inner_local*2;\n'
            '  endfunction\n'
            'endfunction\n', encoding='utf-8')
        self.kernel = Kernel(ROOT, Path(self.temporary.name) / 'runtime', self.workspace)
        self.wait_idle()

    def tearDown(self):
        if self.kernel.snapshot()['status'] in ('running', 'paused', 'starting', 'stopping'):
            self.kernel.interrupt()
            self.wait_idle()
        self.kernel.close()
        self.temporary.cleanup()

    def wait(self, predicate, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if predicate(state):
                return state
            time.sleep(.03)
        raise AssertionError(self.kernel.snapshot())

    def wait_idle(self):
        return self.wait(lambda state: state['status'] in ('idle', 'dead'))

    def wait_paused(self, serial=-1):
        return self.wait(lambda state: state['status'] == 'paused' and state.get('debug', {}).get('ready') and state['debug']['serial'] != serial)

    def test_selected_frame_inspection_returns_detail_without_new_job_or_mutation(self):
        self.kernel.breakpoint(self.fixture, 10, True)
        self.assertFalse(self.wait_idle()['error'])
        self.kernel.submit('debug_inspection_result=debug_inspection_fixture(4);')
        state = self.wait_paused()
        job = state['job']
        generation = self.kernel.generation
        self.kernel.debug('frame', 2)
        state = self.wait_paused(state['debug']['serial'])
        self.assertIn('middle_local', {item['name'] for item in state['variables']})
        serial = state['debug']['serial']
        output = state['output']
        self.kernel.debug('inspect', 'middle_local')
        state = self.wait_paused(serial)
        self.assertEqual(state['job'], job)
        self.assertEqual(self.kernel.generation, generation)
        self.assertEqual(state['output'], output)
        self.assertEqual(state['debug']['frame'], 2)
        self.assertEqual(state['debug']['detail']['name'], 'middle_local')
        self.assertEqual(state['debug']['detail']['class'], 'double')
        self.assertEqual(state['debug']['detail']['rows'], [['4', '14']])
        self.assertNotIn('__mf_', {item['name'] for item in state['variables']})

    def test_inspection_accepts_only_a_variable_name_while_really_paused(self):
        with self.assertRaises(ValueError):
            self.kernel.debug('inspect', 'anything')
        self.kernel.breakpoint(self.fixture, 10, True)
        self.wait_idle()
        self.kernel.submit('debug_inspection_fixture(5);')
        self.wait_paused()
        for value in ('middle_local; dbquit', 'middle_local(1)', 'middle_local\n dbquit', '__mf_hidden'):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    self.kernel.debug('inspect', value)
                self.assertEqual(self.kernel.snapshot()['status'], 'paused')



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main(verbosity=2)
