"""Netlist work in a real persistent octave-cli Kernel, without HTTP/figures."""
from pathlib import Path
import tempfile
import time
import unittest

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class NetlistKernelTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='.netlist-kernel-', dir=ROOT)
        self.addCleanup(self.temp.cleanup)
        folder = Path(self.temp.name)
        work = folder / 'work'
        work.mkdir()
        self.kernel = Kernel(ROOT, folder / 'runtime', work, executable='octave-cli')
        self.addCleanup(self.kernel.close)
        self.idle()

    def idle(self, allow_error=False):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if state['status'] in ('idle', 'dead'):
                self.assertEqual(state['status'], 'idle', state)
                if not allow_error:
                    self.assertFalse(state['error'], state)
                self.assertFalse(state.get('figures'), state)
                return state
            time.sleep(.025)
        self.fail('Kernel did not finish the netlist job')

    def code(self, code):
        self.kernel.submit(code)
        return self.idle()

    def long_job(self):
        self.kernel.submit(
            "slow=(@im_example_model)('kutle_yay_sonum'); "
            "slow.simulation.maxStep=1e-8; "
            "fprintf('NETLIST_STARTED\\n'); fflush(stdout); "
            "interrupted_result=(@im_simulate)(slow);"
        )
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if 'NETLIST_STARTED' in state['output']:
                self.assertEqual(state['status'], 'running', state)
                return state
            time.sleep(.025)
        self.fail('Netlist job did not reach the start marker')

    def test_workspace_handles_errors_and_idempotent_stop(self):
        self.code(
            "sentinel=731; kept_handle=@(x)x+sentinel; "
            "im_compile=11; im_simulate=12; ode45=13; "
            "model=(@im_example_model)('ayrik_kontrol',struct('Ts',.5)); "
            "result=(@im_simulate)(model); "
            "assert(isequal(size(result.signals.output),[17 1])); "
            "assert(abs(result.signals.output(end)-2/3)<1e-10);"
        )
        self.code(
            "bad=model; bad.simulation.mode='mixed'; "
            "try; result=(@im_simulate)(bad); error('test:missing','No rejection'); "
            "catch failure; assert(strcmp(failure.identifier,'im:model:Mode')); end; "
            "assert(result.signals.output(1)==0); "
            "assert(sentinel==731 && kept_handle(2)==733); "
            "assert(im_compile==11 && im_simulate==12 && ode45==13);"
        )
        self.long_job()
        self.kernel.interrupt()
        self.kernel.interrupt()
        self.idle(allow_error=True)
        self.code(
            "assert(exist('interrupted_result','var')==0); "
            "assert(sentinel==731 && kept_handle(2)==733); "
            "assert(result.signals.output(1)==0); after_stop=1;"
        )

    def test_reset_during_netlist_rejects_old_job_result(self):
        old_epoch = self.kernel.generation
        old_job = self.long_job()['job']
        self.kernel.reset()
        self.idle()
        self.assertGreater(self.kernel.generation, old_epoch)
        state = self.code(
            "assert(exist('interrupted_result','var')==0); "
            "assert(exist('slow','var')==0); "
            "fresh=(@im_simulate)((@im_example_model)('ayrik_kontrol',struct('Ts',.5))); "
            "assert(isequal(size(fresh.signals.output),[17 1]));"
        )
        self.assertNotEqual(state['job'], old_job)


if __name__ == '__main__':
    unittest.main()
