"""Bounded discovery and validation of models from local assistant programs."""
import copy
import re
import subprocess
import threading
import time
from backend.codex_app_server import CodexAppServer
from backend.i18n import tr

CHOICE_PATTERN = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z')
CLAUDE_EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max']


def valid_choice(value):
    return isinstance(value, str) and (value == '' or bool(CHOICE_PATTERN.fullmatch(value)))


def parse_agy_models(output):
    result = []
    seen = set()
    for line in output.splitlines():
        parts = line.split('\t')
        if len(parts) != 2: continue
        identity, label = parts
        if not identity or not valid_choice(identity) or identity in seen: continue
        if not label.strip() or len(label) > 200 or any(ord(char) < 32 or ord(char) == 127 for char in label): continue
        seen.add(identity)
        result.append({'id': identity, 'label': label.strip(), 'efforts': []})
        if len(result) >= 200: break
    return result


def codex_models(entries):
    result = []
    seen = set()
    for entry in entries:
        identity, label = entry.get('id'), entry.get('displayName')
        if entry.get('hidden') or not identity or not valid_choice(identity) or identity in seen: continue
        if not isinstance(label, str) or not label.strip() or len(label) > 200 or any(ord(char) < 32 or ord(char) == 127 for char in label): continue
        supported = entry.get('supportedReasoningEfforts', [])
        if not isinstance(supported, list): continue
        efforts = list(dict.fromkeys(item['reasoningEffort'] for item in supported[:32]
            if isinstance(item, dict) and item.get('reasoningEffort') and valid_choice(item['reasoningEffort'])))
        item = {'id': identity, 'label': label.strip(), 'efforts': efforts, 'default': entry.get('isDefault') is True}
        if entry.get('defaultReasoningEffort') in efforts: item['default_effort'] = entry['defaultReasoningEffort']
        result.append(item)
        seen.add(identity)
    return result


class ModelCatalog:
    TTL = 600
    OUTPUT_LIMIT = 128 * 1024

    def __init__(self, owner):
        self.owner = owner
        self.cache = {}
        self.locks = {provider: threading.Lock() for provider in ('claude', 'codex', 'agy')}

    def models(self, provider):
        if not isinstance(provider, str) or provider not in self.locks:
            raise ValueError(tr('Invalid assistant provider or access mode.'))
        with self.locks[provider]:
            cached = self.cache.get(provider)
            if cached is None or time.monotonic() - cached[0] >= self.TTL:
                value = {'models': [{'id': '', 'label': 'Default', 'efforts': []}], 'efforts_separate': provider != 'agy'}
                try:
                    if provider == 'claude':
                        entries = [{'id': name, 'label': name.title(), 'efforts': list(CLAUDE_EFFORTS)} for name in ('fable', 'opus', 'sonnet', 'haiku')]
                        value['models'][0]['efforts'] = list(CLAUDE_EFFORTS)
                    else:
                        executable = self.owner.executable(provider)
                        if not executable: raise ValueError('Unavailable program')
                        entries = self._codex(executable) if provider == 'codex' else self._agy(executable)
                        if not entries: raise ValueError('Empty model list')
                        default = next((item for item in entries if item.get('default')), None)
                        if default:
                            value['models'][0]['efforts'] = list(default['efforts'])
                            if 'default_effort' in default: value['models'][0]['default_effort'] = default['default_effort']
                    value['models'].extend(entries)
                except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.TimeoutExpired):
                    # Do not relay provider stderr, credentials or untrusted errors into the page.
                    value['unavailable'] = True
                self.cache[provider] = (time.monotonic(), value)
            value = copy.deepcopy(self.cache[provider][1])
        if value.pop('unavailable', False): value['note'] = tr('Could not load models from the local program. Default uses its own setting; try again after ten minutes.')
        return value

    def validate(self, provider, model, effort):
        if not valid_choice(model) or not valid_choice(effort): raise ValueError(tr('Invalid assistant model or effort.'))
        if not model and not effort: return
        catalog = self.models(provider)
        chosen = next((item for item in catalog['models'] if item['id'] == model), None)
        if chosen is None or effort and effort not in chosen['efforts']:
            raise ValueError(tr('Invalid assistant model or effort.'))

    def _codex(self, executable):
        from backend.assistants import child_environment
        children = []
        def created(client):
            children.append(client)
            with self.owner.lock:
                if self.owner.closed: client.close()
                else: self.owner.model_clients.add(client)
        client = None
        try:
            client = CodexAppServer(executable, self.owner.workspace.current, child_environment(secret=self.owner.secret), lambda event: None, lambda kind, payload: 'cancel', on_created=created)
            return codex_models(client.list_models())
        finally:
            for child in children:
                child.close()
                with self.owner.lock: self.owner.model_clients.discard(child)

    def _agy(self, executable):
        from backend.assistants import child_environment
        proc = subprocess.Popen([executable, 'models'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=child_environment(secret=self.owner.secret), start_new_session=True)
        proc.assistant_kill_lock = threading.Lock()
        proc.assistant_killed = False
        output = []
        oversized = threading.Event()
        def read():
            try:
                data = proc.stdout.read(self.OUTPUT_LIMIT + 1)
                if len(data) > self.OUTPUT_LIMIT:
                    oversized.set()
                    self.owner._kill(proc)
                else: output.append(data)
            except (OSError, ValueError): pass
        try:
            with self.owner.lock:
                if self.owner.closed: raise ValueError('Closed')
                self.owner.probes.add(proc)
            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            deadline = time.monotonic() + 20
            proc.wait(timeout=20)
            reader.join(timeout=max(0, deadline - time.monotonic()))
            if reader.is_alive() or oversized.is_set() or proc.returncode: raise ValueError('Invalid model output')
            return parse_agy_models(b''.join(output).decode('utf-8', 'replace'))
        finally:
            self.owner._kill(proc)
            proc.stdout.close()
            with self.owner.lock: self.owner.probes.discard(proc)
