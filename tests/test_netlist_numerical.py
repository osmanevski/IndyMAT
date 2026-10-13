"""Netlist acceptance against existing example numeric cores in real Octave."""
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


class NetlistNumericalTests(unittest.TestCase):
    def test_original_references_and_metrics(self):
        executable = shutil.which('octave-cli')
        self.assertIsNotNone(executable, 'octave-cli is required for numerical acceptance')
        packages = ROOT / '.packages'
        setup = ''
        if (packages / 'octave_packages').exists():
            setup = (f"pkg('prefix',{quote(packages)},{quote(packages / '.arch')}); "
                     f"pkg('local_list',{quote(packages / 'octave_packages')});\n")
        setup += (f"addpath({quote(ROOT / 'octave')}); "
                  f"addpath({quote(ROOT / 'tests')}); "
                  f"netlist_numerical({quote(ROOT)});\n")
        result = subprocess.run(
            [executable, '--quiet', '--no-init-file', '--no-history'],
            input=setup, text=True, capture_output=True, cwd=ROOT, timeout=180,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('error:', result.stderr.lower(), result.stderr)
        self.assertIn('NETLIST NUMERICAL OK:', result.stdout)
        print(result.stdout, end='')


if __name__ == '__main__':
    unittest.main()
