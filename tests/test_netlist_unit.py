"""Real Octave checks for the narrow IndyMAT netlist contract."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


class NetlistUnitTests(unittest.TestCase):
    def run_group(self, group):
        self.assertIsNotNone(OCTAVE, 'octave-cli is required for netlist acceptance')
        with tempfile.TemporaryDirectory(prefix='.netlist-unit-', dir=ROOT) as folder:
            quote = lambda value: str(value).replace("'", "''")
            command = (f"addpath('{quote(ROOT / 'octave')}'); "
                       f"addpath('{quote(ROOT / 'tests')}'); netlist_unit('{group}');")
            result = subprocess.run(
                [OCTAVE, '--quiet', '--no-init-file', '--no-site-file',
                 '--no-history', '--eval', command], cwd=folder,
                text=True, capture_output=True, timeout=90)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(f'NETLIST PASS {group}', result.stdout)

    def test_all_thirteen_blocks(self): self.run_group('numeric')
    def test_state_scheduler_and_step_boundaries(self): self.run_group('states')
    def test_exact_schema_and_time_grid(self): self.run_group('schema')
    def test_parameters_initial_state_and_modes(self): self.run_group('parameters')
    def test_graph_ports_vectors_and_disconnected_cycles(self): self.run_group('graph')
    def test_json_exact_keys_and_duplicates(self): self.run_group('json')
    def test_resource_budgets(self): self.run_group('limits')
    def test_runtime_nonfinite_signals_and_states(self): self.run_group('nonfinite')


if __name__ == '__main__':
    unittest.main()
