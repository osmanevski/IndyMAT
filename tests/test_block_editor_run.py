"""Block diagram Run code in a real persistent octave-cli Kernel.

The browser builds ordinary Octave code (frontend/block_model_utils.cjs runCode)
and sends it through the existing execute path. These tests run exactly that
code in the Kernel: the export equals im_example_model, only <stem>_result is
written, ans and other variables survive, one figure per Scope replaces only the
figure with exactly its name, quotes/Unicode/control characters survive the text
literal, a 200-block model runs, and a compile error names a block the editor
can select.
"""
import json
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / 'workspace' / 'examples' / 'pid_kapali_cevrim.imbd.json'

NODE = r"""
const u = require(process.argv[1] + '/frontend/block_model_utils.cjs');
const input = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const out = {};
for (const [key, job] of Object.entries(input)) {
  if (job.kind === 'code') {
    const read = u.readDiagram(job.text);
    if (!read.ok) throw new Error(JSON.stringify(read.error));
    const core = new u.DiagramCore(read.file);
    for (const [id, name, value] of job.edits || []) {
      const r = core.setParameter(id, name, value);
      if (r.error) throw new Error(JSON.stringify(r.error));
    }
    out[key] = { code: u.runCode(core.model(), job.name, { time: 'Time (s)' }), literal: u.compactModelJSON(core.model()), result: u.resultName(job.name) };
  } else if (job.kind === 'target') {
    out[key] = u.errorTarget(u.firstErrorLine(job.message), JSON.parse(job.model));
  }
}
process.stdout.write(JSON.stringify(out));
"""


def node(jobs):
    done = subprocess.run(['node', '-e', NODE, str(ROOT)], input=json.dumps(jobs), capture_output=True, text=True, check=True, timeout=60)
    return json.loads(done.stdout)


def octave_bytes(text):
    return 'char([' + ' '.join(str(b) for b in text.encode('utf-8')) + '])'


def diagram(model, layout):
    return json.dumps({'imbVersion': 1, 'model': model, 'layout': {'blocks': layout}})


class BlockEditorRunTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='.block-editor-run-', dir=ROOT)
        self.addCleanup(self.temp.cleanup)
        folder = Path(self.temp.name)
        work = folder / 'work'
        work.mkdir()
        self.kernel = Kernel(ROOT, folder / 'runtime', work, executable='octave-cli')
        self.addCleanup(self.kernel.close)
        self.idle()

    def idle(self, allow_error=False, timeout=60):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if state['status'] in ('idle', 'dead'):
                self.assertEqual(state['status'], 'idle', state)
                if not allow_error:
                    self.assertFalse(state['error'], state['error'])
                return state
            time.sleep(.025)
        self.fail('Kernel did not finish the job')

    def code(self, code, allow_error=False):
        self.kernel.submit(code)
        return self.idle(allow_error)

    def test_pid_run_writes_one_variable_and_replaces_only_its_figure(self):
        text = EXAMPLE.read_text(encoding='utf-8')
        jobs = node({
            'plain': {'kind': 'code', 'text': text, 'name': 'pid_kapali_cevrim.imbd.json'},
            'kp5': {'kind': 'code', 'text': text, 'name': 'pid_kapali_cevrim.imbd.json', 'edits': [['proportional', 'gain', 5]]},
        })
        plain = jobs['plain']
        self.assertEqual(plain['result'], 'pid_kapali_cevrim_result')
        self.assertNotIn('\n', plain['literal'])
        # Export equals the factory model exactly (decoded the same way).
        self.code(
            f"exported=(@jsondecode)({octave_string(plain['literal'])}); "
            "factory=(@jsondecode)((@jsonencode)((@im_example_model)('pid_kapali_cevrim'))); "
            "assert(isequal(exported,factory)); clear exported factory;"
        )
        figure_name = 'pid_kapali_cevrim.imbd.json: output'
        self.code(
            "keep_me=731; kept_handle=@(x)x+1; ans=42; "
            "(@figure)('Name','Other'); (@figure)('Name','PID_KAPALI_CEVRIM.imbd.json: output'); "
            f"(@figure)('Name','{figure_name}'); (@figure)('Name','{figure_name} '); "
            "names_before=sort(who);"
        )
        for run in range(2):
            self.code(jobs['kp5']['code'])
            self.code(
                "names_after=sort(who); "
                "added=setdiff(names_after,[names_before;{'names_before';'names_after'}]); "
                "assert(isequal(added,{'pid_kapali_cevrim_result'}), strjoin(added,',')); "
                "assert(ans==42); assert(keep_me==731 && kept_handle(1)==2); "
                "names=get(findobj(0,'type','figure'),'name'); "
                f"assert(sum(strcmp(names,'{figure_name}'))==1); "
                "assert(any(strcmp(names,'Other'))); "
                "assert(any(strcmp(names,'PID_KAPALI_CEVRIM.imbd.json: output'))); "
                f"assert(any(strcmp(names,'{figure_name} '))); "
                "assert(numel(names)==4); "
                "ref=(@im_simulate)((@im_example_model)('pid_kapali_cevrim',struct('gains',[5 1 1]))); "
                "assert(isequal(ref.t,pid_kapali_cevrim_result.t)); "
                "assert(isequal(ref.signals.output,pid_kapali_cevrim_result.signals.output)); "
                f"f=findobj(0,'type','figure','-regexp','name','^pid_kapali_cevrim\\.imbd\\.json: output$'); "
                "line=findobj(f,'type','line'); assert(numel(line)==1); "
                "assert(isequal(get(line,'ydata')(:),ref.signals.output)); "
                "assert(isequal(get(line,'xdata')(:),ref.t)); "
                "clear names_after added names ref f line;"
            )

    def test_quotes_unicode_and_control_characters_survive_the_literal(self):
        model = json.loads(EXAMPLE.read_text(encoding='utf-8'))
        weird = json.loads(json.dumps(model))
        solver = "ode45'ç\n\t\x01😀"
        weird['model']['simulation']['solver'] = solver
        name = "it's ç.imbd.json"
        jobs = node({
            'weird': {'kind': 'code', 'text': json.dumps(weird), 'name': name},
            'named': {'kind': 'code', 'text': json.dumps(model), 'name': name},
        })
        literal = jobs['weird']['literal']
        self.assertTrue(all(32 <= ord(c) < 127 for c in literal))
        self.assertEqual(jobs['weird']['result'], 'diagram_result')
        self.code(
            f"decoded=(@jsondecode)({octave_string(literal)}); "
            f"assert(isequal(double(decoded.simulation.solver),double({octave_bytes(solver)}))); clear decoded;"
        )
        state = self.code(jobs['weird']['code'], allow_error=True)
        self.assertIn('requires solver ode45', state['error'])
        self.code("assert(exist('diagram_result','var')==0);")
        self.code(jobs['named']['code'])
        self.code(
            f"names=get(findobj(0,'type','figure'),'name'); if ischar(names), names={{names}}; end; "
            f"assert(sum(strcmp(names,[{octave_bytes(name)} ': output']))==1); "
            "assert(isequal(size(diagram_result.signals.output),[4001 1])); clear names;"
        )

    def test_two_hundred_block_model_runs(self):
        blocks = [{'id': 'b0', 'type': 'Constant', 'params': {'value': 1}, 'sampleTime': 0}]
        connections, layout = [], {'b0': {'x': 0, 'y': 0}}
        for k in range(1, 199):
            blocks.append({'id': f'b{k}', 'type': 'Gain', 'params': {'gain': 1.001}, 'sampleTime': 0})
            connections.append({'from': {'block': f'b{k-1}', 'port': 1}, 'to': {'block': f'b{k}', 'port': 1}})
            layout[f'b{k}'] = {'x': (k % 20) * 160, 'y': k // 20 * 90}
        blocks.append({'id': 'out', 'type': 'Scope', 'params': {}, 'sampleTime': 0})
        connections.append({'from': {'block': 'b198', 'port': 1}, 'to': {'block': 'out', 'port': 1}})
        layout['out'] = {'x': 0, 'y': 1000}
        model = {'schemaVersion': 1, 'blocks': blocks, 'connections': connections,
                 'simulation': {'mode': 'continuous', 't0': 0, 'tf': 1, 'solver': 'ode45', 'outputDt': 0.1, 'relTol': 1e-6, 'absTol': 1e-8, 'maxStep': 0.1}}
        jobs = node({'big': {'kind': 'code', 'text': diagram(model, layout), 'name': 'chain200.imbd.json'}})
        self.assertGreater(len(jobs['big']['literal']), 20000)
        self.code(jobs['big']['code'])
        self.code(
            "assert(numel(chain200_result.t)==11); "
            "assert(max(abs(chain200_result.signals.out-1.001^198))<1e-12);"
        )

    def test_compile_error_names_a_block_the_editor_selects(self):
        model = json.loads(EXAMPLE.read_text(encoding='utf-8'))
        model['model']['connections'] = [c for c in model['model']['connections'] if not (c['to']['block'] == 'plant')]
        jobs = node({'broken': {'kind': 'code', 'text': json.dumps(model), 'name': 'broken.imbd.json'}})
        state = self.code(jobs['broken']['code'], allow_error=True)
        self.assertIn('Block plant input 1 has no driver.', state['error'])
        target = node({'t': {'kind': 'target', 'message': state['error'], 'model': json.dumps(model['model'])}})['t']
        self.assertEqual(target, {'kind': 'block', 'id': 'plant'})
        self.code("assert(exist('broken_result','var')==0);")


def octave_string(text):
    return "'" + text.replace("'", "''") + "'"


if __name__ == '__main__':
    unittest.main()
