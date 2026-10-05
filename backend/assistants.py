"""Optional local coding-agent CLIs. Optional per-conversation MCP session access; no shell or SDK.

Executable overrides (one executable path, never flags): INDYMAT_ASSISTANT_CLAUDE,
INDYMAT_ASSISTANT_CODEX, INDYMAT_ASSISTANT_AGY. Versions are probed on first listing.
Conversation IDs accepted by start are IndyMAT IDs; CLI IDs only come from streams.
"""
import collections
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import threading
import uuid
from backend.i18n import tr
from backend.assistant_codex import CodexConversations
from backend.assistant_models import ModelCatalog, valid_choice
from backend.assistant_approvals import ApprovalService
from backend.assistant_bridge_service import LEVELS, INSPECT_TOOLS, RUN_TOOLS

PROVIDERS = {'claude': 'Claude', 'codex': 'Codex', 'agy': 'Antigravity'}
MODES = ('read-only', 'edit', 'ask')
PROMPT_LIMIT = 128_000
LINE_LIMIT = 64_000
EVENT_LIMIT = 16_000
BUFFER_LIMIT = 512
BUFFER_BYTES = 1_000_000
ID_PATTERN = re.compile(r'[A-Za-z0-9_][A-Za-z0-9_-]{0,199}\Z')


def search_path(source=None):
    """PATH plus the per-user program folders: an app started from the Dock gets a short PATH without them."""
    source = os.environ if source is None else source
    home = source.get('HOME') or os.path.expanduser('~')
    parts = [part for part in source.get('PATH', '').split(os.pathsep) if part]
    for extra in (os.path.join(home, '.local', 'bin'), '/opt/homebrew/bin', '/usr/local/bin'):
        if extra not in parts: parts.append(extra)
    return os.pathsep.join(parts)


def child_environment(source=None, secret=''):
    source = os.environ if source is None else source
    source = {**source, 'PATH': search_path(source)}
    allowed = {'PATH', 'HOME', 'USER', 'LOGNAME', 'SHELL', 'LANG', 'TMPDIR', 'TERM', 'SYSTEMROOT', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR'}
    return {key: value for key, value in source.items()
            if (key in allowed or key.startswith(('LC_', 'XDG_')))
            and '.matlab-free' not in value and (not secret or secret not in value)}


def command(provider, mode, executable, folder, conversation=None, prompt='', session_access='none', bridge=None, model='', effort=''):
    if provider not in ('claude', 'agy') or mode not in MODES:
        raise ValueError(tr('Invalid assistant provider or access mode.'))
    if mode == 'ask' and provider == 'agy': raise ValueError(tr('Approvals for this program arrive in a later step'))
    if mode == 'ask' and not bridge: raise ValueError(tr('Session bridge is not available.'))
    if conversation is not None and (not isinstance(conversation, str) or not ID_PATTERN.fullmatch(conversation)):
        raise ValueError(tr('Invalid assistant conversation.'))
    if session_access not in LEVELS or (session_access != 'none' and (provider == 'agy' or not bridge)):
        raise ValueError(tr('Invalid assistant session access.'))
    if not valid_choice(model) or not valid_choice(effort) or provider == 'agy' and effort:
        raise ValueError(tr('Invalid assistant model or effort.'))
    if provider == 'claude':
        argv = [executable, '-p', '--output-format', 'stream-json', '--verbose', '--permission-mode', 'plan' if mode == 'read-only' else 'default' if mode == 'ask' else 'acceptEdits']
        if session_access != 'none' or mode == 'ask':
            argv[argv.index('--permission-mode') + 1] = 'default' if mode in ('read-only', 'ask') else 'acceptEdits'
            names = INSPECT_TOOLS if session_access == 'inspect' else RUN_TOOLS if session_access == 'run' else ()
            allowed = (['Read', 'Glob', 'Grep'] if mode == 'read-only' else []) + ['mcp__indymat__' + name for name in names]
            argv += ['--mcp-config', bridge['config']]
            if allowed: argv += ['--allowedTools', ','.join(allowed)]
            # --allowedTools allows tools; approve is exclusively the permission prompt handler.
            if mode == 'ask': argv += ['--permission-prompt-tool', 'mcp__indymat__approve']
        if conversation: argv += ['--resume', conversation]
    else:
        # agy takes the prompt only as the value of -p (measured 5 Oct 2026: a bare -p swallows the next flag and
        # stdin is not read). One argv element in the attached form, so prompt text can never be read as a flag.
        argv = [executable, '-p=' + prompt, '--output-format', 'stream-json', '--mode', 'plan' if mode == 'read-only' else 'accept-edits', '--sandbox']
        if conversation: argv += ['--conversation', conversation]
    if model: argv += ['--model', model]
    if effort: argv += ['--effort', effort]
    return argv


def environment_note(mode, folder, home, version='', packages=(), session_access='none', editor=(), first=True):
    """Server-written note that opens a conversation: where the agent is and what it can rely on.

    Only facts the application knows; the folder is JSON-quoted so a folder name cannot pose as instructions.
    """
    try: shown = '~/' + str(Path(folder).relative_to(home)) if Path(folder) != Path(home) else '~'
    except ValueError: shown = str(folder)
    names = [name for name in packages if isinstance(name, str) and re.fullmatch(r'[A-Za-z][A-Za-z0-9_-]{0,40}', name)][:24]
    lines = [
        'Context from IndyMAT (written by the application, not by the user):',
        '- You are running inside IndyMAT, a local scientific IDE in which the user writes and runs MATLAB-style .m files.',
        '- The engine is GNU Octave' + (' ' + version if re.fullmatch(r'[0-9][0-9A-Za-z.+-]{0,20}', version or '') else '') + ', not MATLAB. Loaded packages: ' + (', '.join(names) if names else 'none') + '.',
        '- Write code that runs in Octave while keeping MATLAB syntax (end, single-quoted char, % comments). Do not assume a MATLAB-only function or toolbox exists; when unsure whether Octave has something, say so instead of guessing.',
        '- Current folder of the IDE, which is your working directory: ' + json.dumps(shown, ensure_ascii=False) + '.',
        '- Access mode chosen by the user: ' + ('read-only: you may read files here but must not change anything.' if mode == 'read-only' else 'you may edit files inside the current folder only.'),
        '- You cannot reach the user\'s live Octave session from here: no variables, figures or command execution. The user runs code with Run (F5) or Run Section. A separate octave-cli process, if your tools allow one, does not share that session\'s state.',
        '- Answer in the language the user writes in.',
        'End of IndyMAT context.',
    ]
    if session_access != 'none':
        if mode == 'read-only': lines[5] = '- File access mode chosen by the user: read-only: you may read files here but must not change files.'
        lines[6] = '- The indymat MCP tools can inspect your live session: session_status, workspace_variables, variable_value (bounded typed preview while idle), and figure_image (open-figure PNG). ' + ('Code cannot be run in the session at this access level.' if session_access == 'inspect' else "run_code runs code in the user's LIVE Octave session, visibly in their Command Window, and changes their variables. job_result checks unfinished jobs. Prefer small steps; never clear all, close all or exit unless the user asked. The user can Stop your job.")
    if mode == 'ask': lines[5] = '- You may propose any change; each action that needs permission is shown to the user for approval, so prefer small, reviewable edits and explain in one line what an edit is for before making it.'
    if mode != 'read-only': lines.insert(6, '- Change files with small targeted edits rather than rewriting whole files: the user watches the open file update live in the editor.')
    if not first: lines = [lines[0], lines[-1]]
    lines[-1:-1] = list(editor)
    if len(lines) == 2: return ''
    return '\n'.join(lines) + '\n\n'


def compact_input(name, value):
    """Tool arguments for the transcript: file contents become line counts so an edit reads as an edit, not as a dump."""
    if not isinstance(value, dict): return value
    lines = lambda text: text.count('\n') + (1 if text and not text.endswith('\n') else 0)
    result = {}
    if name in ('Edit', 'MultiEdit') or 'old_string' in value or 'edits' in value:
        edits = value.get('edits') if isinstance(value.get('edits'), list) else [value]
        result['added'] = sum(lines(item.get('new_string', '')) for item in edits if isinstance(item, dict) and isinstance(item.get('new_string', ''), str))
        result['removed'] = sum(lines(item.get('old_string', '')) for item in edits if isinstance(item, dict) and isinstance(item.get('old_string', ''), str))
    for key, item in value.items():
        if key in ('old_string', 'new_string', 'edits'): continue
        if isinstance(item, str) and (len(item) > 400 or key in ('content', 'CodeContent', 'new_source', 'ReplacementContent')): result[key + '_lines'] = lines(item)
        elif isinstance(item, (str, int, float, bool)) or item is None: result[key] = item
        else: result[key] = json.dumps(item, ensure_ascii=False)[:400]
    return result


def ide_lines(ide, folder, resolve):
    """Lines describing the editor state the page reported; every path is re-derived by the server and JSON-quoted."""
    if ide is None: return []
    if not isinstance(ide, dict) or set(ide) - {'active', 'dirty', 'open'}: raise ValueError(tr('Invalid assistant context.'))
    def shown(path):
        if not isinstance(path, str) or not path or len(path) > 4096: raise ValueError(tr('Invalid assistant context.'))
        target = resolve(path)
        if '.matlab-free' in target.parts: raise PermissionError(tr('Assistant context is outside the current folder.'))
        return json.dumps(str(target.relative_to(folder)) if target.is_relative_to(folder) else str(target), ensure_ascii=False)
    active, others = ide.get('active'), ide.get('open', [])
    if not isinstance(others, list) or len(others) > 30 or not isinstance(ide.get('dirty', False), bool): raise ValueError(tr('Invalid assistant context.'))
    if active is None: return ['- Editor: no file is open.']
    result = ['- Active editor file (what the user is looking at; when they name no file, they mean this one): ' + shown(active) + ('. It has unsaved changes in the editor that are not on disk yet.' if ide.get('dirty') else '.')]
    rest = [shown(path) for path in others if path != active][:20]
    if rest: result.append('- Other open editor tabs: ' + ', '.join(rest) + '.')
    return result


def raw_event(value):
    return {'type': 'raw', 'text': json.dumps(value, ensure_ascii=False) if not isinstance(value, str) else value}


def blocks(content):
    events = []
    if not isinstance(content, list): return [raw_event(content)]
    for block in content:
        if not isinstance(block, dict): events.append(raw_event(block)); continue
        kind = block.get('type')
        if kind == 'text' and isinstance(block.get('text'), str): events.append({'type': 'text', 'text': block['text']})
        elif kind in ('thinking', 'reasoning'):
            if str(block.get('thinking', block.get('text', ''))).strip(): events.append({'type': 'reasoning', 'text': str(block.get('thinking', block.get('text', '')))})
        elif kind == 'tool_use':
            events.append({'type': 'tool', 'name': str(block.get('name', 'tool')).replace('mcp__indymat__', 'indymat · '), 'text': json.dumps(compact_input(str(block.get('name', '')), block.get('input', {})), ensure_ascii=False)})
        else: events.append(raw_event(block))
    return events


def parse_claude(event):
    kind = event.get('type')
    if kind == 'system' and event.get('subtype') == 'init':
        # The program names the model it really runs (measured: alias haiku -> claude-haiku-4-5-...); models misreport themselves.
        named = event.get('model')
        return [{'type': 'conversation', 'conversation': event.get('session_id')}] + ([{'type': 'model', 'text': named[:80]}] if isinstance(named, str) and re.fullmatch(r'[A-Za-z0-9._:\-\[\]]{1,80}', named) else [])
    if kind == 'assistant' and isinstance(event.get('message'), dict): return blocks(event['message'].get('content'))
    if kind == 'result':
        result = [{'type': 'conversation', 'conversation': event.get('session_id')}]
        if event.get('is_error'): result.append({'type': 'error', 'text': str(event.get('result') or event.get('errors') or tr('Assistant turn failed.'))})
        return result
    if kind == 'stream_event' and isinstance(event.get('event'), dict):
        delta = event['event'].get('delta', {})
        if delta.get('type') == 'text_delta': return [{'type': 'text', 'text': str(delta.get('text', ''))}]
        if delta.get('type') == 'thinking_delta': return [{'type': 'reasoning', 'text': str(delta.get('thinking', ''))}]
    if kind == 'error': return [{'type': 'error', 'text': str(event.get('message', event))}]
    # Bookkeeping the transcript has no use for (measured stream, 5 Oct 2026); anything else unknown stays visible.
    if kind == 'rate_limit_event' or (kind == 'system' and event.get('subtype') in ('hook_started', 'hook_response', 'thinking_tokens')): return []
    if kind == 'user' and isinstance(event.get('message'), dict) and isinstance(event['message'].get('content'), list) and all(isinstance(block, dict) and block.get('type') == 'tool_result' for block in event['message']['content']): return []
    return [raw_event(event)]


def parse_agy(event):
    # Antigravity's help documents NDJSON, but not its schema. Recognise the
    # Claude-style envelope and explicit text/tool events; retain everything else.
    # Measured stream (5 Oct 2026): {"event": name, name: {...}} with init, step_update and result.
    name = event.get('event')
    body = event.get(name) if isinstance(name, str) and isinstance(event.get(name), dict) else None
    if name == 'init': return [{'type': 'conversation', 'conversation': event.get('conversation_id')}]
    if name == 'step_update' and body is not None:
        step = body.get('step_type')
        if step == 'agent_response': return [{'type': 'text', 'text': body['text_delta']}] if isinstance(body.get('text_delta'), str) else []
        if step == 'user_input': return []
        if step == 'tool':
            if body.get('state') != 'ACTIVE': return []
            info = body.get('tool_info') if isinstance(body.get('tool_info'), dict) else {}
            return [{'type': 'tool', 'name': str(body.get('tool_name', 'tool')), 'text': json.dumps(compact_input(str(body.get('tool_name', '')), info.get('parameters', {})), ensure_ascii=False)}]
    if name == 'result' and body is not None:
        result = [{'type': 'conversation', 'conversation': body.get('conversation_id')}]
        denied = body.get('denied_actions')
        if isinstance(denied, list) and denied: result.append({'type': 'error', 'text': tr('Actions denied by the access mode: {names}', names=', '.join(sorted({str(item.get('display_name', item.get('action', '?'))) if isinstance(item, dict) else str(item) for item in denied})))})
        if body.get('status') != 'SUCCESS': result.append({'type': 'error', 'text': str(body.get('error') or body.get('status') or tr('Assistant turn failed.'))})
        return result
    kind = event.get('type')
    if kind in ('system', 'assistant', 'result', 'stream_event', 'error'): return parse_claude(event)
    if kind in ('text', 'reasoning') and isinstance(event.get('text'), str): return [{'type': kind, 'text': event['text']}]
    if kind == 'tool' and isinstance(event.get('name'), str): return [{'type': 'tool', 'name': event['name'], 'text': str(event.get('arguments', ''))}]
    if kind in ('conversation', 'init'): return [{'type': 'conversation', 'conversation': event.get('conversation_id')}]
    return [raw_event(event)]

PARSERS = {'claude': parse_claude, 'agy': parse_agy}


class Assistants(CodexConversations):
    MAX_RUNNING = 3
    MAX_SESSIONS = 24

    def __init__(self, workspace, secret='', describe=None, bridge=None):
        self.workspace = workspace
        self.secret = secret
        self.describe = describe
        self.bridge = bridge
        self.lock = threading.RLock()
        self.sessions = {}
        self.versions = {}
        self.probes = set()
        self.model_clients = set()
        self.model_catalog = ModelCatalog(self)
        self.closed = False
        self._approvals = None

    @property
    def approvals(self):
        if self.bridge: return self.bridge.approvals
        with self.lock:
            if self._approvals is None: self._approvals = ApprovalService(self)
            return self._approvals

    def executable(self, provider):
        override = os.environ.get('INDYMAT_ASSISTANT_' + provider.upper())
        return shutil.which(override or provider, path=search_path())

    def models(self, provider):
        return self.model_catalog.models(provider)

    def providers(self):
        result = []
        for provider, label in PROVIDERS.items():
            executable = self.executable(provider)
            version = None
            if executable:
                with self.lock:
                    cached = self.versions.get(executable)
                    if cached is None: self.versions[executable] = ''
                if cached is None:
                    # agy has no documented version flag: report unknown, never
                    # accidentally invoke its interactive mode for a version probe.
                    cached = ''
                    if provider != 'agy':
                        proc = None
                        try:
                            proc = subprocess.Popen([executable, '--version'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=child_environment(secret=self.secret), start_new_session=True)
                            proc.assistant_kill_lock = threading.Lock()
                            proc.assistant_killed = False
                            with self.lock:
                                if self.closed:
                                    self._kill(proc)
                                    raise ValueError(tr('Assistants are closed.'))
                                self.probes.add(proc)
                            reader = threading.Thread(target=self._version_read, args=(proc,), daemon=True)
                            reader.start()
                            proc.wait(timeout=2)
                            reader.join(timeout=.2)
                            cached = getattr(proc, 'assistant_version', '')
                        except (OSError, subprocess.TimeoutExpired): pass
                        finally:
                            if proc:
                                self._kill(proc)
                                if proc.stdout: proc.stdout.close()
                                with self.lock: self.probes.discard(proc)
                    with self.lock: self.versions[executable] = cached
                version = cached or None
            result.append({'id': provider, 'name': label, 'available': bool(executable), 'version': version, 'reason': '' if executable else tr('Program not installed or not executable.')})
        return result

    @staticmethod
    def _version_read(proc):
        try: proc.assistant_version = proc.stdout.read(256).decode('utf-8', 'replace').strip()
        except (OSError, ValueError): pass

    def _clean(self, text):
        value = text.replace(self.secret, '[private]') if self.secret else text
        return value.encode('utf-8', 'replace').decode('utf-8')

    def _emit(self, session, event):
        with self.lock:
            if event.get('type') == 'error' and not event.get('will_retry'): session['failed'] = True
            limits = {'summary': 4096, 'name': 128, 'text': 512 if event.get('type') == 'tool' else EVENT_LIMIT}
            bounded = {key: self._clean(value)[:limits.get(key, EVENT_LIMIT)] if isinstance(value, str) else value for key, value in event.items()}
            clipped = any(isinstance(value, str) and len(value) > limits.get(key, EVENT_LIMIT) for key, value in event.items())
            if len(session['events']) == BUFFER_LIMIT:
                session['bytes'] -= len(json.dumps(session['events'][0]).encode())
            session['seq'] += 1
            session['events'].append({**bounded, 'id': event['id'] if event.get('type') in ('approval', 'approval-resolved') else session['seq'], 'seq': session['seq'], 'turn': session['turn']})
            session['bytes'] += len(json.dumps(session['events'][-1]).encode())
            if clipped:
                if len(session['events']) == BUFFER_LIMIT:
                    session['bytes'] -= len(json.dumps(session['events'][0]).encode())
                session['seq'] += 1
                session['events'].append({'type': 'notice', 'text': tr('Assistant event was truncated.'), 'id': session['seq'], 'turn': session['turn']})
                session['bytes'] += len(json.dumps(session['events'][-1]).encode())
            while session['bytes'] > BUFFER_BYTES and len(session['events']) > 1:
                session['bytes'] -= len(json.dumps(session['events'].popleft()).encode())

    def _prompt(self, prompt, context, folder):
        if not isinstance(prompt, str) or not prompt.strip(): raise ValueError(tr('Enter an assistant prompt.'))
        if not isinstance(context, dict) or set(context) - {'path', 'selection', 'dirty', 'include_unsaved', 'content'}:
            raise ValueError(tr('Invalid assistant context.'))
        prefix = ''
        if context:
            path = context.get('path')
            if not isinstance(path, str) or Path(path).is_absolute(): raise ValueError(tr('Attach a file relative to the current folder.'))
            target = self.workspace.path(str(folder / path))
            if not target.is_relative_to(folder) or '.matlab-free' in target.parts: raise PermissionError(tr('Assistant context is outside the current folder.'))
            for flag in ('dirty', 'include_unsaved'):
                if flag in context and not isinstance(context[flag], bool): raise ValueError(tr('Invalid assistant context.'))
            # JSON length framing makes arbitrary file text clearly distinguishable
            # from the user's instructions, without a delimiter injection ambiguity.
            attachment = {'path': str(target.relative_to(folder))}
            if context.get('dirty') and not context.get('include_unsaved'):
                attachment['note'] = tr('File has unsaved changes; unsaved text was not included.')
            else:
                if 'selection' in context: attachment['selection'] = context['selection']
                if context.get('dirty') and context.get('include_unsaved'): attachment['unsaved_content'] = context.get('content', '')
            if any(not isinstance(value, str) for value in attachment.values()): raise ValueError(tr('Invalid assistant context.'))
            payload = json.dumps(attachment, ensure_ascii=False)
            prefix = f'Attached editor context (JSON, {len(payload.encode())} bytes):\n{payload}\nEnd attached editor context.\n\n'
        result = prefix + prompt
        if len(result.encode('utf-8')) > PROMPT_LIMIT: raise ValueError(tr('Assistant prompt exceeds 128 KB.'))
        return self._clean(result)

    def start(self, request, facts=None):
        if not isinstance(request, dict) or set(request) - {'provider', 'mode', 'prompt', 'context', 'conversation', 'session_access', 'ide', 'model', 'effort'}:
            raise ValueError(tr('Invalid assistant request.'))
        provider = request.get('provider')
        # A follow-up turn that names no mode or access level continues with the conversation's own.
        with self.lock: earlier = self.sessions.get(request.get('conversation')) if isinstance(request.get('conversation'), str) else None
        mode = request.get('mode', earlier['mode'] if earlier else 'ask' if (provider == 'codex' or provider == 'claude' and self.bridge) else 'read-only')
        level = request.get('session_access', earlier.get('session_access', 'none') if earlier else 'none')
        if level not in LEVELS: raise ValueError(tr('Invalid assistant session access.'))
        if level != 'none' and provider == 'agy': raise ValueError(tr('Antigravity needs a one-time MCP registration; not available yet'))
        if mode == 'ask' and provider == 'agy': raise ValueError(tr('Approvals for this program arrive in a later step'))
        if (level != 'none' or mode == 'ask' and provider == 'claude') and self.bridge is None: raise ValueError(tr('Session bridge is not available.'))
        if not isinstance(provider, str) or provider not in PROVIDERS or mode not in MODES:
            raise ValueError(tr('Invalid assistant provider or access mode.'))
        model = request.get('model', earlier.get('model', '') if earlier else '')
        effort = request.get('effort', earlier.get('effort', '') if earlier else '')
        self.model_catalog.validate(provider, model, effort)
        with self.lock:
            if self.closed: raise ValueError(tr('Assistants are closed.'))
            folder = self.workspace.folder(self.workspace.current)
            if '.matlab-free' in folder.parts: raise PermissionError(tr('Assistant context is outside the current folder.'))
            prompt = self._prompt(request.get('prompt'), request.get('context', {}), folder)
            identity = request.get('conversation')
            session = self.sessions.get(identity) if isinstance(identity, str) else None
            if identity is not None and session is None: raise ValueError(tr('Invalid assistant conversation.'))
            if session and (session['provider'] != provider or session['mode'] != mode or session.get('session_access', 'none') != level or session['folder'] != str(folder)):
                raise ValueError(tr('Start a new conversation when changing provider, mode, or folder.'))
            if session and (session.get('model', '') != model or provider != 'codex' and session.get('effort', '') != effort):
                raise ValueError(tr('Start a new conversation when changing provider, mode, or folder.'))
            steering = bool(session and session['running'] and provider == 'codex')
            if steering and session.get('effort', '') != effort:
                raise ValueError(tr('Change effort after the current turn ends.'))
            if steering and session['stopped']: raise ValueError(tr('The assistant turn is no longer running.'))
            if session and session['running'] and not steering: raise ValueError(tr('This assistant conversation is already running.'))
            if not steering and sum(s['running'] for s in self.sessions.values()) >= self.MAX_RUNNING: raise ValueError(tr('Too many assistant turns are running.'))
            if session is None and len(self.sessions) >= self.MAX_SESSIONS: raise ValueError(tr('Assistant conversation limit reached for this app run.'))
            if session and not session['conversation'] and provider != 'codex': raise ValueError(tr('Provider did not return a conversation ID; start a new conversation.'))
            executable = self.executable(provider)
            if not executable: raise ValueError(tr('Program not installed or not executable.'))
            # Every turn tells the agent which file the user has in front of them; the first turn also
            # carries the application's description of itself.
            editor = ide_lines(request.get('ide'), folder, self.workspace.path)
            home = self.workspace.home if hasattr(self.workspace, 'home') else Path.home()
            if session is None:
                try: facts = facts if facts is not None else self.describe() if self.describe else {}
                except Exception: facts = {}
                loaded = [item.get('name') for item in facts.get('packages', []) if isinstance(item, dict) and item.get('loaded')]
                note = environment_note(mode, folder, home, str(facts.get('version', '')), loaded, level, () if provider == 'codex' else editor)
                # Measured with codex-cli 0.160.0 (5 Oct 2026): with this line an edit in ask mode arrives as a
                # file-change approval carrying a diff; without it Codex writes through a shell command and the
                # user is shown that command instead.
                if provider == 'codex' and mode == 'ask': note = note.replace('End of IndyMAT context.', '- The user approves each change in the IDE and sees it as a diff: change files ONLY with the apply_patch tool (one small patch per edit), never with shell redirection, sed -i, python or other commands that write files.\nEnd of IndyMAT context.')
                prompt = (environment_note(mode, folder, home, session_access=level, editor=editor, first=False) if provider == 'codex' else note) + prompt
            else:
                prompt = environment_note(mode, folder, home, session_access=level, editor=editor, first=False) + prompt
            if len(prompt.encode('utf-8')) > PROMPT_LIMIT + 16384: raise ValueError(tr('Assistant prompt exceeds 128 KB.'))
            created = session is None
            if created:
                identity = uuid.uuid4().hex
                session = {'provider': provider, 'mode': mode, 'session_access': level, 'model': model, 'effort': effort, 'folder': str(folder), 'conversation': None, 'events': collections.deque(maxlen=BUFFER_LIMIT), 'seq': 0, 'bytes': 0}
                if level != 'none' or mode == 'ask' and provider == 'claude': session['bridge'] = self.bridge.create(identity, provider, level, approvals=mode == 'ask' and provider == 'claude')
            if provider == 'codex':
                session['effort'] = effort
                if created:
                    session.update(identity=identity, developer_instructions=self._clean(note), codex_rpc_lock=threading.Lock(), codex_client=None, codex_generation=None)
                    self.sessions[identity] = session
                if not steering: session.update(turn=uuid.uuid4().hex, running=True, stopped=False, failed=False, codex_turn=None)
                self._codex_launch(identity, session, executable, prompt, steering)
                return {'session': identity, 'turn': session['turn'], 'steered': steering}
            argv = command(provider, mode, executable, folder, session['conversation'], prompt if provider == 'agy' else '', level, session.get('bridge'), model, effort)
            try:
                proc = subprocess.Popen(argv, cwd=folder, env=child_environment(secret=self.secret), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True, bufsize=0)
            except Exception:
                if created and self.bridge: self.bridge.revoke(identity)
                raise
            proc.assistant_kill_lock = threading.Lock()
            proc.assistant_killed = False
            if created: self.sessions[identity] = session
            session.update(turn=uuid.uuid4().hex, running=True, stopped=False, failed=False, proc=proc)
            threading.Thread(target=self._run, args=(session, proc, prompt), daemon=True).start()
            return {'session': identity, 'turn': session['turn']}

    def _line(self, session, line):
        try:
            value = json.loads(line)
            events = PARSERS[session['provider']](value) if isinstance(value, dict) else [raw_event(value)]
        except (ValueError, TypeError, KeyError, AttributeError, RecursionError, OverflowError): events = [raw_event(line)]
        for event in events:
            if event['type'] == 'conversation':
                value = event.get('conversation')
                if isinstance(value, str) and ID_PATTERN.fullmatch(value) and (not self.secret or self.secret not in value):
                    with self.lock: session['conversation'] = value
                else: self._emit(session, raw_event(line))
            else: self._emit(session, event)

    def _read(self, session, pipe, parse=True):
        pending = b''
        dropping = False
        while True:
            chunk = pipe.read(4096)
            if not chunk: break
            for index, part in enumerate(chunk.split(b'\n')):
                if index:
                    if not dropping and pending: self._line(session, pending.decode('utf-8', 'replace')) if parse else self._emit(session, raw_event(pending.decode('utf-8', 'replace')))
                    pending = b''
                    dropping = False
                if dropping: continue
                pending += part
                if len(pending) > LINE_LIMIT:
                    self._emit(session, raw_event(pending[:LINE_LIMIT].decode('utf-8', 'replace')))
                    self._emit(session, {'type': 'notice', 'text': tr('Assistant output line was truncated.')})
                    pending = b''
                    dropping = True
        if pending and not dropping: self._line(session, pending.decode('utf-8', 'replace')) if parse else self._emit(session, raw_event(pending.decode('utf-8', 'replace')))

    def _run(self, session, proc, prompt):
        def write():
            try: proc.stdin.write(prompt.encode('utf-8')); proc.stdin.close()
            except (OSError, ValueError): pass
        def reap():
            proc.wait()
            self._kill(proc)
        writer = threading.Thread(target=write, daemon=True)
        stderr = threading.Thread(target=self._read, args=(session, proc.stderr, False), daemon=True)
        writer.start()
        stderr.start()
        threading.Thread(target=reap, daemon=True).start()
        try:
            self._read(session, proc.stdout)
            proc.wait()
            stderr.join(timeout=2)
            if proc.returncode and not session['stopped']: self._emit(session, {'type': 'error', 'text': tr('Assistant exited with status {status}.', status=proc.returncode)})
        except (OSError, ValueError) as exc:
            self._emit(session, {'type': 'error', 'text': str(exc)})
        finally:
            self._kill(proc)
            for pipe in (proc.stdin, proc.stdout, proc.stderr):
                try: pipe.close()
                except (OSError, ValueError): pass
            with self.lock:
                state = 'stopped' if session['stopped'] else 'failed' if proc.returncode or session['failed'] else 'completed'
                self._emit(session, {'type': 'turn-end', 'state': state, 'conversation': session['conversation']})
                session['running'] = False
                if self.bridge:
                    identity = next((key for key, item in self.sessions.items() if item is session), None)
                    if identity: self.bridge.approvals.cancel(identity, tr('The assistant turn is no longer running.'))

    def events(self, identity, after=0):
        if isinstance(after, bool) or not isinstance(after, int) or after < 0: raise ValueError(tr('Invalid assistant event cursor.'))
        with self.lock:
            session = self.sessions.get(identity)
            if not session: raise ValueError(tr('Invalid assistant conversation.'))
            events = [event.copy() for event in session['events'] if event.get('seq', event['id']) > after]
            truncated = bool(session['events'] and after < session['events'][0].get('seq', session['events'][0]['id']) - 1)
            if truncated: events.insert(0, {'type': 'notice', 'text': tr('Earlier assistant events were truncated.'), 'id': session['events'][0].get('seq', session['events'][0]['id']) - 1, 'turn': session['turn']})
            return {'events': events, 'after': session['seq'], 'running': session['running'], 'turn': session['turn'], 'truncated': truncated, 'approvals': self.approvals.pending(identity)}

    @staticmethod
    def _kill(proc):
        # Stop, EOF cleanup and shutdown can race. Signal each process group
        # once, so a delayed reaper never signals a recycled process identity.
        with proc.assistant_kill_lock:
            if proc.assistant_killed: return
            def signal_group(sig):
                try: os.killpg(proc.pid, sig)
                except ProcessLookupError: pass
                except PermissionError:
                    # The macOS sandbox can report EPERM for a vanished group.
                    # A still-running process's permission error remains visible.
                    if proc.poll() is None: raise
            signal_group(signal.SIGTERM)
            try: proc.wait(timeout=1)
            except subprocess.TimeoutExpired: pass
            # Even an exited leader may leave a child holding output pipes open.
            signal_group(signal.SIGKILL)
            try: proc.wait(timeout=2)
            except subprocess.TimeoutExpired: pass
            proc.assistant_killed = True

    def stop(self, identity):
        with self.lock:
            session = self.sessions.get(identity)
            if not session: raise ValueError(tr('Invalid assistant conversation.'))
            if not session['running']: return {'ok': True}
            session['stopped'] = True
            self.approvals.cancel(identity, tr('The assistant was stopped; the action was denied.'))
            proc = session.get('proc')
        if session['provider'] == 'codex': self._codex_stop(session)
        elif proc: self._kill(proc)
        return {'ok': True}

    def remove(self, identity):
        self.stop(identity)
        with self.lock: session = self.sessions.pop(identity, None)
        if session and session['provider'] == 'codex': self._codex_discard(identity, session, session.get('codex_generation'))
        self.approvals.forget(identity)
        if self.bridge: self.bridge.revoke(identity)
        return {'ok': True}

    def close(self):
        with self.lock:
            self.closed = True
            identities = list(self.sessions)
            probes = list(self.probes)
            model_clients = list(self.model_clients)
        for client in model_clients: client.close()
        for proc in probes: self._kill(proc)
        for identity in identities:
            self.stop(identity)
            with self.lock: session = self.sessions.get(identity)
            if session and session['provider'] == 'codex':
                self._codex_discard(identity, session, session.get('codex_generation'))
                self._codex_finish(identity, session, 'stopped')
            if self.bridge: self.bridge.revoke(identity)
