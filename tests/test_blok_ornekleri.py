"""Run example computations in fresh octave-cli jobs without graphics objects."""
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
MARKER = '%% Grafikler — sayısal test bu işaretten önce durur'
EXAMPLES = {
    'pid_kapali_cevrim.m': ('PID kapalı çevrim', 3),
    'kutle_yay_sonum.m': ('kütle-yay-sönüm', 1),
    'ayrik_kontrol.m': ('ayrık kontrol', 2),
    'doyum_antiwindup.m': ('doyum ve anti-windup', 2),
    'sinyal_suzme.m': ('sinyal süzme', 2),
}


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


class BlockExampleTests(unittest.TestCase):
    def test_numerical_cores(self):
        exe = os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave-cli')
        self.assertIsNotNone(exe, 'octave-cli is required for numerical acceptance')
        packages = ROOT / '.packages'
        setup = ''
        if (packages / 'octave_packages').exists():
            setup = (f"pkg('prefix',{quote(packages)},{quote(packages / '.arch')}); "
                     f"pkg('local_list',{quote(packages / 'octave_packages')});\n")
        for filename, (label, number_lines) in EXAMPLES.items():
            with self.subTest(example=filename):
                source = (ROOT / 'workspace' / 'examples' / filename).read_text()
                self.assertEqual(source.count(MARKER), 1)
                core, plots = source.split(MARKER)
                self.assertIn("figure('Name'", plots)
                self.assertNotRegex(core, r'\b(?:figure|plot|subplot|close)\s*\(')
                result = subprocess.run(
                    [exe, '--quiet', '--no-init-file', '--no-history'],
                    input=setup + core, text=True, capture_output=True, cwd=ROOT,
                    timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertNotIn('error:', result.stderr.lower())
                lines = result.stdout.strip().splitlines()
                self.assertEqual(lines[-1], 'REFERANS OK: ' + label)
                self.assertEqual(len(lines), number_lines + 1, result.stdout)
                self.assertNotRegex(result.stdout, r'\b(?:NaN|Inf)\b')


if __name__ == '__main__':
    unittest.main()
