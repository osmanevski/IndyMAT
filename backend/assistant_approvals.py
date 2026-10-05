"""Review previews and conversation-bound decisions; never executes agent tools."""
import collections
import difflib
import hashlib
import json
import os
import shlex
from pathlib import Path
import stat
import threading
import time
import uuid
from backend.i18n import tr

DETAIL_BYTES = 64 * 1024
READ_BYTES = 1024 * 1024
WAIT_SECONDS = 3600
MAX_PENDING = 4
MAX_RETAINED = 128
FILE_TOOLS = ('Read', 'Write', 'Edit', 'MultiEdit', 'NotebookRead', 'NotebookEdit')


def bounded(text, limit=DETAIL_BYTES):
    data = text.encode('utf-8', 'replace')
    return data[:limit].decode('utf-8', 'ignore'), len(data) > limit


def file_path(value, folder):
    raw = value.get('file_path', value.get('notebook_path'))
    if not isinstance(raw, str) or not raw or len(raw) > 4096: return None
    path = Path(raw).expanduser()
    return (path if path.is_absolute() else Path(folder) / path).resolve()


def decision_key(tool, value, folder):
    """Exact tool + resolved file; Bash exact command; other tools exact name.

    A missing/malformed file or command never earns a reusable permission.
    """
    if tool in FILE_TOOLS:
        path = file_path(value, folder)
        return (tool, str(path)) if path else None
    if tool == 'Bash':
        command = value.get('command')
        return (tool, command) if isinstance(command, str) and command else None
    return (tool,)


def apply_edits(current, edits):
    for edit in edits:
        if not isinstance(edit, dict): raise ValueError
        old, new = edit.get('old_string'), edit.get('new_string')
        replace_all = edit.get('replace_all', False)
        if not isinstance(old, str) or not isinstance(new, str) or not old or not isinstance(replace_all, bool): raise ValueError
        count = current.count(old)
        if count == 0 or (not replace_all and count != 1): raise ValueError
        replacements = count if replace_all else 1
        if len(current) + replacements * (len(new) - len(old)) > READ_BYTES: raise ValueError
        current = current.replace(old, new, -1 if replace_all else 1)
    return current


def proposed_blocks(tool, value):
    if tool == 'Write' and isinstance(value.get('content'), str):
        return tr('New text:') + '\n' + ''.join('+' + line + '\n' for line in value['content'].splitlines())
    edits = value.get('edits') if tool == 'MultiEdit' else [value] if tool == 'Edit' else None
    if isinstance(edits, list) and edits and all(isinstance(edit, dict) and isinstance(edit.get('old_string'), str) and isinstance(edit.get('new_string'), str) for edit in edits):
        blocks = []
        for index, edit in enumerate(edits):
            blocks.append(tr('Proposed edit {number}', number=index + 1) + '\n' + tr('Old text:') + '\n' + ''.join('-' + line + '\n' for line in edit['old_string'].splitlines()) + tr('New text:') + '\n' + ''.join('+' + line + '\n' for line in edit['new_string'].splitlines()))
        return '\n'.join(blocks)
    return json.dumps(value, ensure_ascii=False, indent=2)


def read_current(path):
    # Resolve once for display, then refuse symlink races in every path component.
    directory = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        fd = os.open(path.name, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=directory)
        with os.fdopen(fd, 'rb') as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode): raise ValueError
            data = stream.read(READ_BYTES + 1)
        if len(data) > READ_BYTES: raise ValueError
        return data.decode('utf-8')
    finally:
        os.close(directory)


def preview(tool, value, folder):
    """Read regular files only, bounded before diffing. No model HTML is rendered."""
    path = file_path(value, folder) if tool in FILE_TOOLS else None
    warning = ''
    if path:
        outside = not path.is_relative_to(Path(folder).resolve())
        private = '.matlab-free' in path.parts
        shown = str(path) if outside or private else str(path.relative_to(Path(folder).resolve()))
        if outside or private: warning = tr('Warning: this file is outside the current folder or inside private app data.')
        current = ''
        readable = not private
        try:
            if private: raise ValueError
            current = read_current(path)
        except FileNotFoundError: pass
        except (OSError, ValueError, UnicodeError): readable = False
        try:
            if not readable: raise ValueError
            if tool == 'Write':
                updated = value['content']
                if not isinstance(updated, str): raise ValueError
            elif tool in ('Edit', 'MultiEdit'):
                edits = value.get('edits') if tool == 'MultiEdit' else [value]
                if not isinstance(edits, list) or not edits: raise ValueError
                updated = apply_edits(current, edits)
            else: raise ValueError
            text = ''.join(line if line.endswith('\n') else line + '\n\\ No newline at end of file\n' for line in difflib.unified_diff(current.splitlines(keepends=True), updated.splitlines(keepends=True), fromfile=shown, tofile=shown, lineterm='\n'))
        except (ValueError, KeyError):
            text = tr('Preview could not be applied to the current file; proposed old/new blocks follow.') + '\n' + proposed_blocks(tool, value)
        if private: text = tr('Private app data is not read for approval previews.') + '\n' + proposed_blocks(tool, value)
        summary = shown
        kind = 'diff'
    elif tool == 'Bash':
        summary = str(value.get('command', ''))
        text = str(value.get('description', '')) + '\n' + summary
        kind = 'command'
    else:
        summary = tool
        text = json.dumps(value, ensure_ascii=False, indent=2)
        kind = 'json'
    text, truncated = bounded(text)
    summary, _ = bounded(summary, 4096)
    return summary, {'kind': kind, 'text': text, 'warning': warning, 'truncated': truncated}


class ApprovalService:
    def __init__(self, assistants):
        self.assistants = assistants
        self.condition = threading.Condition(assistants.lock)
        self.entries = collections.OrderedDict()
        self.rules = {}
        self.waiters = {}

    @staticmethod
    def answer(entry):
        if entry['decision'] in ('allow', 'allow-conversation'):
            return {'behavior': 'allow', 'updatedInput': entry['input']}
        return {'behavior': 'deny', 'message': entry['message'] or tr('The user denied this action.')}

    def _resolve(self, entry, decision, message=''):
        if entry['decision'] is not None: return
        entry.update(decision=decision, message=message)
        session = self.assistants.sessions.get(entry['session'])
        if session and session['turn'] == entry['turn']:
            self.assistants._emit(session, {'type': 'approval-resolved', 'id': entry['id'], 'decision': decision})
        self.condition.notify_all()

    def cancel(self, identity, message):
        with self.condition:
            for entry in self.entries.values():
                if entry['session'] == identity:
                    if entry['decision'] is None: entry['cancelled'] = True
                    self._resolve(entry, 'deny', message)

    def forget(self, identity):
        with self.condition:
            self.rules.pop(identity, None)
            for key in list(self.entries):
                if self.entries[key]['session'] == identity: del self.entries[key]

    def request(self, grant, arguments):
        # Bound blocked HTTP threads too, including retries of the same tool use.
        identity = grant['identity']
        with self.condition:
            if self.waiters.get(identity, 0) >= MAX_PENDING:
                return {'behavior': 'deny', 'message': tr('Too many approvals are pending in this conversation.')}
            self.waiters[identity] = self.waiters.get(identity, 0) + 1
        try:
            return self._request(grant, arguments)
        finally:
            with self.condition:
                self.waiters[identity] -= 1
                if not self.waiters[identity]: del self.waiters[identity]

    def _request(self, grant, arguments):
        if not isinstance(arguments, dict) or len(json.dumps(arguments, ensure_ascii=False).encode('utf-8')) > 150_000 or set(arguments) - {'tool_name', 'input', 'tool_use_id'} or not isinstance(arguments.get('tool_name'), str) or not 0 < len(arguments['tool_name']) <= 128 or not isinstance(arguments.get('input'), dict) or not isinstance(arguments.get('tool_use_id'), str) or len(arguments['tool_use_id']) > 200:
            raise ValueError(tr('Invalid approval request.'))
        identity = grant['identity']
        tool, value = arguments['tool_name'], arguments['input']
        with self.condition:
            session = self.assistants.sessions.get(identity)
            if not grant['active'] or not grant.get('approvals') or not session or session['mode'] != 'ask' or not session['running'] or session['stopped'] or self.assistants.closed:
                return {'behavior': 'deny', 'message': tr('The assistant turn is no longer running.')}
            turn = session['turn']
            key = decision_key(tool, value, session['folder'])
            if key is not None and key in self.rules.get(identity, set()): return {'behavior': 'allow', 'updatedInput': value}
            # Retries of a tool use join the existing wait, never open duplicate cards.
            entry = next((item for item in self.entries.values() if item['session'] == identity and item['turn'] == turn and item['tool_use_id'] == arguments['tool_use_id']), None)
            if entry and (entry['tool_name'] != tool or entry['input'] != value): raise ValueError(tr('Invalid approval request.'))
            if entry is None:
                if sum(item['session'] == identity and item['decision'] is None for item in self.entries.values()) >= MAX_PENDING:
                    return {'behavior': 'deny', 'message': tr('Too many approvals are pending in this conversation.')}
                # Keep slow file I/O outside the shared conversation lock.
                folder = session['folder']
        if entry is None:
            summary, detail = preview(tool, value, folder)
            summary, _ = bounded(self.assistants._clean(summary), 4096)
            detail['text'], clipped = bounded(self.assistants._clean(detail['text']))
            detail['truncated'] = detail['truncated'] or clipped
            with self.condition:
                if not grant['active'] or session['turn'] != turn or not session['running'] or session['stopped'] or self.assistants.closed:
                    return {'behavior': 'deny', 'message': tr('The assistant turn is no longer running.')}
                entry = next((item for item in self.entries.values() if item['session'] == identity and item['turn'] == turn and item['tool_use_id'] == arguments['tool_use_id']), None)
                if entry and (entry['tool_name'] != tool or entry['input'] != value): raise ValueError(tr('Invalid approval request.'))
                if entry is None:
                    if sum(item['session'] == identity and item['decision'] is None for item in self.entries.values()) >= MAX_PENDING:
                        return {'behavior': 'deny', 'message': tr('Too many approvals are pending in this conversation.')}
                    owned = [item for item in self.entries.values() if item['session'] == identity]
                    retained = len(owned)
                    for old in owned:
                        if retained < MAX_RETAINED: break
                        if old['decision'] is not None:
                            del self.entries[old['id']]
                            retained -= 1
                    entry = {'id': uuid.uuid4().hex, 'session': identity, 'turn': turn, 'tool_name': tool, 'tool_use_id': arguments['tool_use_id'], 'input': value, 'key': key, 'decision': None, 'message': '', 'summary': summary, 'detail': detail}
                    self.entries[entry['id']] = entry
                    self.assistants._emit(session, {'type': 'approval', 'id': entry['id'], 'tool': tool, 'summary': summary, 'detail': detail})
        with self.condition:
            deadline = time.monotonic() + WAIT_SECONDS
            while entry['decision'] is None:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    self._resolve(entry, 'deny', tr('Approval expired after one hour; the action was denied.'))
                    break
                self.condition.wait(remaining)
            return self.answer(entry)

    def request_prepared(self, identity, turn, kind, payload):
        """Codex supplies its diff: never read files or synthesize Claude inputs."""
        if kind not in ('command', 'file') or not isinstance(payload, dict): raise ValueError(tr('Invalid approval request.'))
        raw = str(payload.get('command') or '')
        summary = raw
        if kind == 'command':
            try:
                words = shlex.split(raw)
                if len(words) == 3 and words[0] in ('/bin/zsh', '/bin/bash', '/bin/sh') and words[1] == '-lc': summary = words[2]
            except ValueError: pass
            text = str(payload.get('reason') or '') + '\n' + raw
            tool = 'Shell'
            key = ('codex-command', hashlib.sha256(raw.encode('utf-8', 'replace')).hexdigest()) if raw else None
        else:
            paths = [item.get('path', '') for item in payload.get('paths', [])[:256] if isinstance(item, dict)]
            folder = Path(self.assistants.sessions[identity]['folder']).resolve() if identity in self.assistants.sessions else None
            def shown_path(raw):
                # Same rule as Claude's cards: relative inside the current folder, absolute (and visible) outside it.
                try: return str(Path(raw).resolve().relative_to(folder)) if folder and Path(raw).is_absolute() else str(raw)
                except ValueError: return str(raw)
            shown = [shown_path(path) for path in paths]
            summary = ', '.join(shown)
            text = str(payload.get('reason') or '') + '\n' + '\n'.join(shown) + '\n' + str(payload.get('diff') or '')
            tool = 'Edit'
            key = ('codex-file', hashlib.sha256(json.dumps(paths, ensure_ascii=False).encode('utf-8', 'replace')).hexdigest()) if paths else None
        summary, _ = bounded(self.assistants._clean(summary), 4096)
        text, clipped = bounded(self.assistants._clean(text))
        detail = {'kind': 'command' if kind == 'command' else 'diff', 'text': text, 'warning': '', 'truncated': clipped or bool(payload.get('truncated'))}
        request_id = 'codex:' + hashlib.sha256(json.dumps(payload.get('request_id'), ensure_ascii=False).encode('utf-8', 'replace')).hexdigest()
        with self.condition:
            def active():
                session = self.assistants.sessions.get(identity)
                return session and session['turn'] == turn and session['mode'] == 'ask' and session['running'] and not session['stopped'] and not self.assistants.closed
            if not active(): return 'cancel'
            if self.waiters.get(identity, 0) >= MAX_PENDING: return 'decline'
            if key is not None and key in self.rules.get(identity, set()): return 'accept_session'
            entry = next((item for item in self.entries.values() if item['session'] == identity and item['turn'] == turn and item['tool_use_id'] == request_id), None)
            if entry is None:
                if sum(item['session'] == identity and item['decision'] is None for item in self.entries.values()) >= MAX_PENDING: return 'decline'
                owned = [item for item in self.entries.values() if item['session'] == identity]
                retained = len(owned)
                for old in owned:
                    if retained < MAX_RETAINED: break
                    if old['decision'] is not None:
                        del self.entries[old['id']]
                        retained -= 1
                entry = {'id': uuid.uuid4().hex, 'session': identity, 'turn': turn, 'tool_name': tool, 'tool_use_id': request_id, 'input': None, 'key': key, 'decision': None, 'message': '', 'summary': summary, 'detail': detail}
                self.entries[entry['id']] = entry
                self.assistants._emit(self.assistants.sessions[identity], {'type': 'approval', 'id': entry['id'], 'tool': tool, 'summary': summary, 'detail': detail})
            self.waiters[identity] = self.waiters.get(identity, 0) + 1
            try:
                deadline = time.monotonic() + WAIT_SECONDS
                while entry['decision'] is None:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        self._resolve(entry, 'deny', tr('Approval expired after one hour; the action was denied.'))
                        break
                    self.condition.wait(remaining)
                if entry.get('cancelled') or not active(): return 'cancel'
                return {'allow': 'accept', 'allow-conversation': 'accept_session', 'deny': 'decline'}[entry['decision']]
            finally:
                self.waiters[identity] -= 1
                if not self.waiters[identity]: del self.waiters[identity]

    def pending(self, identity):
        with self.condition:
            return [{'type': 'approval', 'id': entry['id'], 'turn': entry['turn'], 'tool': entry['tool_name'], 'summary': entry['summary'], 'detail': entry['detail']} for entry in self.entries.values() if entry['session'] == identity and entry['decision'] is None]

    def decide(self, request):
        if not isinstance(request, dict) or set(request) - {'session', 'id', 'decision', 'message'} or not {'session', 'id', 'decision'} <= set(request) or not isinstance(request['session'], str) or not isinstance(request['id'], str) or request['decision'] not in ('allow', 'allow-conversation', 'deny') or not isinstance(request.get('message', ''), str) or len(request.get('message', '').encode('utf-8')) > 1000:
            raise ValueError(tr('Invalid approval decision.'))
        with self.condition:
            entry = self.entries.get(request['id'])
            if not entry or entry['session'] != request['session']: raise ValueError(tr('Approval does not belong to this conversation.'))
            if entry['decision'] is None:
                session = self.assistants.sessions.get(entry['session'])
                if not session or session['turn'] != entry['turn'] or session['stopped'] or not session['running'] or self.assistants.closed:
                    self._resolve(entry, 'deny', tr('The assistant turn is no longer running.'))
                else:
                    if request['decision'] == 'allow-conversation' and entry['key'] is not None:
                        rules = self.rules.setdefault(entry['session'], set())
                        if len(rules) >= MAX_RETAINED and entry['key'] not in rules:
                            raise ValueError(tr('Conversation approval limit reached; use Allow for this action.'))
                        rules.add(entry['key'])
                    self._resolve(entry, request['decision'], request.get('message', ''))
            return {'ok': True, 'id': entry['id'], 'decision': entry['decision']}
