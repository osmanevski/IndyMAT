"""Server-side capability policy and bounded live-session jobs for assistants."""
import base64
import collections
import json
import os
from pathlib import Path
import secrets
import sys
import tempfile
import threading
import time
from backend.i18n import tr
from backend.assistant_approvals import ApprovalService

LEVELS = ('none', 'inspect', 'run')
INSPECT_TOOLS = ('session_status', 'workspace_variables', 'variable_value', 'figure_image')
RUN_TOOLS = INSPECT_TOOLS + ('run_code', 'job_result')
CODE_BYTES = 16_000
TEXT_LIMIT = 16_000
VARIABLE_LIMIT = 500
IMAGE_BYTES = 4_000_000
WAIT_SECONDS = 10
JOB_LIMIT = 32
TOTAL_JOB_LIMIT = 64


def tool_definitions():
    definitions = [
        ('session_status', 'Show live session status, folder, engine and access level.', {}),
        ('workspace_variables', 'List live variables by name, size and class.', {}),
        ('variable_value', 'Read a bounded typed variable preview while idle.', {'name': {'type': 'string'}}),
        ('figure_image', 'Show an open figure as a bounded PNG image.', {'index': {'type': 'integer', 'minimum': 1}}),
        ('run_code', 'Run code visibly in the user\'s live Octave session while idle.', {'code': {'type': 'string'}}),
        ('job_result', 'Retrieve the outcome of a submitted session job.', {'job': {'type': 'string'}}),
    ]
    return [{'name': name, 'description': description, 'inputSchema': {'type': 'object', 'properties': properties, 'additionalProperties': False, **({'required': list(properties)} if name in ('variable_value', 'run_code', 'job_result') else {})}} for name, description, properties in definitions]


class BridgeService:
    """Capabilities are independent of the browser token; lock order: grant, kernel.

    Completion capture runs under kernel.lock before another job can replace its
    state. Retained results and browser command entries are bounded and epoch-bound.
    """
    def __init__(self, app):
        self.app = app
        self.directory = app.runtime / 'bridge'
        self.lock = threading.RLock()
        self.grants = {}
        self.jobs = collections.OrderedDict()
        self._approvals = None

    @property
    def approvals(self):
        with self.lock:
            if self._approvals is None: self._approvals = ApprovalService(self.app.assistants)
            return self._approvals

    def _write(self, path, value):
        self.directory.mkdir(mode=0o700, exist_ok=True)
        if self.directory.is_symlink(): raise ValueError(tr('Invalid bridge directory.'))
        os.chmod(self.directory, 0o700)
        fd, temporary = tempfile.mkstemp(prefix='.bridge-', dir=self.directory)
        try:
            with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                os.fchmod(stream.fileno(), 0o600)
                json.dump(value, stream)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
        finally:
            try: os.unlink(temporary)
            except FileNotFoundError: pass

    def create(self, identity, provider, level, approvals=False):
        capability = self.directory / (identity + '.json')
        config = self.directory / (identity + '.mcp.json')
        grant = {'identity': identity, 'provider': provider, 'level': level, 'token': secrets.token_urlsafe(32), 'active': True, 'approvals': approvals, 'lock': threading.RLock(), 'files': (capability, config)}
        try:
            self._write(capability, {'url': self.app.base, 'token': grant['token']})
            self._write(config, {'mcpServers': {'indymat': {'command': sys.executable, 'args': [str(Path(__file__).with_name('assistant_bridge.py').resolve()), str(capability)]}}})
        except Exception:
            for path in grant['files']: path.unlink(missing_ok=True)
            raise
        with self.lock: self.grants[identity] = grant
        return {'capability': str(capability), 'config': str(config), 'script': str(Path(__file__).with_name('assistant_bridge.py').resolve()), 'python': sys.executable}

    def revoke(self, identity):
        with self.lock: grant = self.grants.pop(identity, None)
        if grant:
            with grant['lock']:
                grant['active'] = False
                for path in grant['files']: path.unlink(missing_ok=True)
            self.approvals.cancel(identity, tr('The conversation was removed or the app closed; the action was denied.'))
            self.approvals.forget(identity)
            # Keep bounded browser command entries after removal, so a fast
            # completed turn cannot vanish before the next /api/state poll.

    def authenticate(self, token):
        if not isinstance(token, str) or not token.isascii() or secrets.compare_digest(token, self.app.token): raise PermissionError(tr('Invalid assistant session capability.'))
        with self.lock:
            for grant in self.grants.values():
                if secrets.compare_digest(token, grant['token']): return grant
        raise PermissionError(tr('Invalid assistant session capability.'))

    @staticmethod
    def text(value, error=False):
        text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)
        if len(text) > TEXT_LIMIT: text = text[:TEXT_LIMIT] + '\n' + tr('Bridge output was truncated.')
        return {'content': [{'type': 'text', 'text': text}], 'isError': error}

    def capture(self, kernel):
        # Caller holds kernel.lock. Avoid snapshot here: PublishKernel calls us.
        state = kernel.state
        with self.lock:
            for job, entry in self.jobs.items():
                if entry['epoch'] != kernel.generation:
                    entry['result'] = self.text(tr('The Octave session changed; this job is no longer available.'), True)
                    continue
                if entry.get('result') is not None: continue
                if state.get('job') == job and state['status'] in ('idle', 'dead'):
                    entry['result'] = self.outcome(job, state)
                    if entry['code'] is None:
                        entry['result'] = self.text({'job': job, 'value': state.get('variable_action'), 'error': state.get('error'), 'note': tr('Preview is limited to 10 rows, 10 columns and the first N-dimensional slice.')}, bool(state.get('error')))
                    entry['output'] = self.bounded(state.get('output', ''))
                    entry['error'] = self.bounded(state.get('error') or '')
                    entry['elapsed'] = state.get('elapsed', 0)
                    entry['status'] = state['status']

    @classmethod
    def bounded(cls, text, limit=TEXT_LIMIT):
        return text if len(text) <= limit else text[:limit] + '\n' + tr('Bridge output was truncated.')

    @staticmethod
    def figures(state):
        return [{'number': figure.get('number'), 'index': index + 1} for index, figure in enumerate(state.get('figures', [])[:100])]

    def outcome(self, job, state):
        # Preserve job/error/figure metadata even when output needs truncation.
        value = {'job': job, 'status': state['status'], 'error': self.bounded(state.get('error') or '', 4096), 'figures': self.figures(state), 'output': state.get('output', '')}
        if len(state.get('figures', [])) > 100: value['figures_note'] = tr('Figure list was truncated.')
        if len(json.dumps(value, ensure_ascii=False)) > TEXT_LIMIT:
            value['note'] = tr('Bridge output was truncated.')
            original = value['output']
            low, high = 0, len(original)
            while low < high:
                middle = (low + high + 1) // 2
                value['output'] = original[:middle]
                if len(json.dumps(value, ensure_ascii=False)) <= TEXT_LIMIT: low = middle
                else: high = middle - 1
            value['output'] = original[:low]
        return self.text(value, bool(state.get('error')))

    def visible_jobs(self, kernel):
        self.capture(kernel)
        with self.lock:
            return [{key: entry.get(key) for key in ('job', 'epoch', 'provider', 'code', 'output', 'error', 'elapsed', 'status')} for entry in self.jobs.values() if entry.get('code') is not None and entry['epoch'] == kernel.generation]

    def _track(self, grant, job, epoch, code=None):
        with self.lock:
            owned = [key for key, entry in self.jobs.items() if entry['identity'] == grant['identity']]
            while len(owned) >= JOB_LIMIT: del self.jobs[owned.pop(0)]
            while len(self.jobs) >= TOTAL_JOB_LIMIT: self.jobs.popitem(last=False)
            self.jobs[job] = {'identity': grant['identity'], 'job': job, 'epoch': epoch, 'provider': grant['provider'], 'code': code, 'result': None, 'status': 'running'}

    def _wait(self, grant, job, seconds=WAIT_SECONDS):
        deadline = time.monotonic() + seconds
        while True:
            with grant['lock']:
                if not grant['active']: return self.text(tr('Invalid assistant session capability.'), True)
                kernel = self.app.kernel
                with kernel.lock:
                    kernel.snapshot()  # App completion fence captures before any subsequent job.
                    self.capture(kernel)
                    with self.lock:
                        entry = self.jobs.get(job)
                        if not entry or entry['identity'] != grant['identity']: return self.text(tr('This assistant job is no longer available.'), True)
                        if entry['result'] is not None: return entry['result']
                        state = kernel.state
                        status = state['status'] if state.get('job') == job else 'running'
            if time.monotonic() >= deadline or status == 'paused' or state.get('waiting_input'):
                return self.text({'job': job, 'status': status, 'message': tr('Job is still running; use job_result to check its outcome.')})
            time.sleep(.025)

    def tools(self, grant):
        names = RUN_TOOLS if grant['level'] == 'run' else INSPECT_TOOLS if grant['level'] == 'inspect' else ()
        tools = [tool for tool in tool_definitions() if tool['name'] in names]
        if grant.get('approvals'):
            tools.append({'name': 'approve', 'description': 'Answer a permission prompt through the IndyMAT user approval panel.', 'inputSchema': {'type': 'object', 'properties': {'tool_name': {'type': 'string'}, 'input': {'type': 'object'}, 'tool_use_id': {'type': 'string'}}, 'required': ['tool_name', 'input', 'tool_use_id'], 'additionalProperties': False}})
        return {'tools': tools}

    def call(self, grant, request):
        try:
            if not isinstance(request, dict) or set(request) - {'name', 'arguments'}: raise ValueError(tr('Invalid bridge tool request.'))
            name = request.get('name')
            arguments = request.get('arguments', {})
            if name == 'approve' and grant.get('approvals'):
                # Do not truncate the permission JSON: updatedInput must remain exact.
                answer = self.approvals.request(grant, arguments)
                return {'content': [{'type': 'text', 'text': json.dumps(answer, ensure_ascii=False)}]}
            allowed = RUN_TOOLS if grant['level'] == 'run' else INSPECT_TOOLS if grant['level'] == 'inspect' else ()
            if name not in allowed: raise ValueError(tr('This tool is not allowed by the session access level.'))
            properties = {'variable_value': {'name'}, 'figure_image': {'index'}, 'run_code': {'code'}, 'job_result': {'job'}}.get(name, set())
            if not isinstance(arguments, dict) or set(arguments) - properties or (name != 'figure_image' and set(arguments) != properties): raise ValueError(tr('Invalid bridge tool arguments.'))
            kernel = self.app.kernel
            with grant['lock']:
                if not grant['active']: raise ValueError(tr('Invalid assistant session capability.'))
                with kernel.lock:
                    state = kernel.snapshot()
                    if name == 'session_status':
                        folder = Path(state.get('cwd') or self.app.workspace.current)
                        home = self.app.workspace.home
                        shown = '~' if folder == home else '~/' + str(folder.relative_to(home)) if folder.is_relative_to(home) else str(folder)
                        return self.text({'status': 'waiting for input' if state.get('waiting_input') else 'busy' if state['status'] in ('starting', 'running', 'stopping') else state['status'], 'current_folder': shown, 'octave_version': state.get('version'), 'loaded_packages': [p['name'] for p in state.get('packages', []) if p.get('loaded')], 'access_level': grant['level']})
                    if name == 'workspace_variables':
                        variables = state.get('variables', [])
                        return self.text({'variables': [{key: item.get(key) for key in ('name', 'size', 'class')} for item in variables[:VARIABLE_LIMIT]], 'note': tr('Variable list was truncated.') if len(variables) > VARIABLE_LIMIT else ''})
                    if name == 'figure_image':
                        figures = state.get('figures', [])
                        index = arguments.get('index', 1)
                        if isinstance(index, bool) or not isinstance(index, int) or index < 1 or index > len(figures): raise ValueError(tr('No open figure at this index.'))
                        figure = figures[index - 1]
                        import re
                        if not re.fullmatch(r'[a-f0-9]{32}', figure.get('job', '')) or not re.fullmatch(r'figure-\d+\.png', figure.get('file', '')): raise ValueError(tr('Invalid figure.'))
                        with (kernel.runtime / figure['job'] / figure['file']).open('rb') as stream: data = stream.read(IMAGE_BYTES + 1)
                        if len(data) > IMAGE_BYTES: return self.text(tr('Figure image exceeds the bridge size limit.'))
                        return {'content': [{'type': 'image', 'mimeType': 'image/png', 'data': base64.b64encode(data).decode('ascii')}], 'isError': False}
                    if name == 'job_result':
                        job = arguments['job']
                        if not isinstance(job, str): raise ValueError(tr('Invalid bridge tool arguments.'))
                    else:
                        if state['status'] != 'idle' or state.get('waiting_input'): raise ValueError(tr('Session must be idle; finish the running job, debugger pause or input request first.'))
                        if name == 'variable_value':
                            from backend.workspace_actions import variable_read_request
                            variable = arguments['name']
                            request = variable_read_request({'action': 'read', 'name': variable, 'row': 1, 'column': 1, 'rows': 10, 'columns': 10, 'epoch': kernel.generation})
                            job = kernel.workspace_action('variable-read', request)
                            self._track(grant, job, kernel.generation)
                        elif name == 'run_code':
                            code = arguments['code']
                            if not isinstance(code, str) or not code.strip() or len(code.encode('utf-8')) > CODE_BYTES: raise ValueError(tr('Bridge code must be nonempty and at most 16 KB.'))
                            job = kernel.submit(code, 'code')
                            self._track(grant, job, kernel.generation, code)
            if name == 'run_code': self.app.assistants.bridge_activity(grant['identity'], code)
            result = self._wait(grant, job, 0 if name == 'job_result' else WAIT_SECONDS)
            return result
        except (ValueError, TypeError, KeyError, OSError, UnicodeError) as error:
            return self.text(str(error), True)


