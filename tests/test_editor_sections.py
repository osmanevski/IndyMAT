"""Extract sections with the frontend lexer, then evaluate in isolated Octave."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which("octave-cli")
NODE = shutil.which("node")


@unittest.skipUnless(OCTAVE and NODE, "octave-cli and Node are required")
class EditorSectionExecutionTests(unittest.TestCase):
    def extracted(self, source, cursor_text, through_end=False):
        script = """
const fs = require('node:fs');
const { sectionRange } = require('./frontend/editor_command_utils.cjs');
const { source, cursor, throughEnd } = JSON.parse(fs.readFileSync(0, 'utf8'));
const range = sectionRange(source, source.indexOf(cursor), throughEnd);
process.stdout.write(source.slice(range.from, range.to));
"""
        result = subprocess.run(
            [NODE, "-e", script], cwd=ROOT, text=True, capture_output=True,
            input=json.dumps({"source": source, "cursor": cursor_text, "throughEnd": through_end}),
            check=True, timeout=10,
        )
        return result.stdout

    def assert_octave(self, code, assertion):
        result = subprocess.run(
            [OCTAVE, "--quiet", "--no-init-file", "--no-history", "--eval", code + "\n" + assertion],
            cwd=ROOT, text=True, capture_output=True, timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_reviewer_example_preserves_opening_comment(self):
        source = "%{\n%% fake\n%}\nx = 17;\n%% real"
        extracted = self.extracted(source, "x =")
        self.assertTrue(extracted.startswith("%{\n"))
        # x follows %}, so this example must still execute the active assignment.
        self.assert_octave(extracted, "assert(x == 17);")

    def test_commented_assignment_cannot_escape_through_fake_section(self):
        source = "%{\n%% fake\nx = 17;\n%}\n%% real\ny = 23;"
        extracted = self.extracted(source, "x =")
        self.assertTrue(extracted.startswith("%{\n"))
        self.assert_octave(extracted, "assert(exist('x', 'var') == 0); assert(exist('y', 'var') == 0);")

    def test_run_to_end_keeps_comments_and_runs_following_real_sections(self):
        source = "%% first\n%{\n%% fake\nx = 17;\n%}\n%% real\ny = 23;"
        extracted = self.extracted(source, "x =", through_end=True)
        self.assertTrue(extracted.startswith("%% first\n%{\n"))
        self.assert_octave(extracted, "assert(exist('x', 'var') == 0); assert(y == 23);")



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == "__main__":
    unittest.main()
