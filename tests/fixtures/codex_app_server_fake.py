#!/usr/bin/env python3
"""Offline app-server fixture. Prompt text selects a deterministic turn script."""
import json
import os
import signal
import subprocess
import sys
import threading
import time

if '--version' in sys.argv:
    print('fixture app-server 1.0')
    sys.exit(0)

write_lock = threading.Lock()
threads = {}
turns = {}
requests = {}
serial = 0
initialized = False


def send(value):
    with write_lock:
        sys.stdout.write(json.dumps(value, ensure_ascii=False) + '\n')
        sys.stdout.flush()


def notify(method, params):
    send({'method': method, 'params': params})


def reply(message, value):
    send({'id': message['id'], 'result': value})


def finish(thread, turn, status='completed', error=None):
    notify('turn/completed', {'threadId': thread, 'turn': {'id': turn, 'items': [], 'status': status, 'error': error}})


def text(thread, turn, value):
    notify('item/agentMessage/delta', {'threadId': thread, 'turnId': turn, 'itemId': 'agent', 'delta': value})
    notify('item/completed', {'threadId': thread, 'turnId': turn, 'item': {'type': 'agentMessage', 'id': 'agent', 'text': value}})


def item(thread, turn, value, complete=False):
    notify('item/completed' if complete else 'item/started', {'threadId': thread, 'turnId': turn, 'startedAtMs': 0, 'item': value})


def ask(thread, turn, method, params):
    identity = 'server-' + turn
    done = threading.Event()
    state = {'done': done, 'response': None}
    requests[identity] = state
    send({'id': identity, 'method': method, 'params': {'threadId': thread, 'turnId': turn, 'itemId': 'approval', 'startedAtMs': 0, **params}})
    done.wait(20)
    return state['response']


def grandchild():
    child = subprocess.Popen([sys.executable, '-c', 'import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);print("ready",flush=True);time.sleep(60)'], stdout=subprocess.PIPE)
    child.stdout.readline()
    return child


def script(thread, turn, prompt):
    full_prompt = prompt
    prompt = prompt.split('End of IndyMAT context.\n\n')[-1].split('End attached editor context.\n\n')[-1].strip()
    if prompt == 'echo-context':
        text(thread, turn, full_prompt)
        finish(thread, turn)
        return
    if prompt == 'bridge-run':
        config = threads[thread]['config']['mcp_servers']['indymat']
        code = 'bridge_fixture_codex = 7741; disp(7741);'
        item(thread, turn, {'id': 'bridge', 'type': 'mcpToolCall', 'server': 'indymat', 'tool': 'run_code', 'arguments': {'code': code}, 'status': 'inProgress'})
        bridge = subprocess.Popen([config['command'], *config['args']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        output, _ = bridge.communicate(json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': 'run_code', 'arguments': {'code': code}}}) + '\n', timeout=20)
        text(thread, turn, output)
        finish(thread, turn)
        return
    if prompt in ('hang', 'hold', 'wait'):
        return
    if prompt == 'crash':
        time.sleep(.05)
        os._exit(7)
    if prompt == 'crash-grandchild':
        child = grandchild()
        text(thread, turn, str(child.pid))
        time.sleep(.05)
        os._exit(9)
    if prompt == 'bad-delta':
        notify('item/agentMessage/delta', {'threadId': thread, 'turnId': turn, 'itemId': 'bad', 'delta': []})
        return
    if prompt == 'malformed':
        with write_lock:
            sys.stdout.write('{broken\n')
            sys.stdout.flush()
        return
    if prompt == 'oversized':
        with write_lock:
            sys.stdout.write('x' * (2 * 1024 * 1024 + 2) + '\n')
            sys.stdout.flush()
        return
    if prompt == 'grandchild':
        # Ignore TERM so close must escalate for both leader and descendant.
        child = grandchild()
        text(thread, turn, str(child.pid))
        return
    if prompt in ('approval-command', 'approval-file', 'approval-command-write'):
        if prompt.endswith('file'):
            item(thread, turn, {'id': 'approval', 'type': 'fileChange', 'status': 'inProgress',
                                'changes': [{'path': 'answer.m', 'kind': {'type': 'add'}, 'diff': '--- /dev/null\n+++ answer.m\n+answer=42;\n'}]})
        response = ask(thread, turn, 'item/commandExecution/requestApproval' if 'command' in prompt else 'item/fileChange/requestApproval',
                       {'command': "/bin/zsh -lc 'printf approved > sample.m'" if prompt.endswith('write') else 'touch answer.m', 'cwd': os.getcwd(), 'reason': 'Write a file.'} if 'command' in prompt else {'reason': 'Write a file.'})
        if response and response.get('result', {}).get('decision') in ('accept', 'acceptForSession'):
            from pathlib import Path
            Path('sample.m' if prompt.endswith('write') else 'answer.m').write_text('approved = 42;\n')
        text(thread, turn, json.dumps(response))
        finish(thread, turn)
        return
    if prompt.startswith('unsupported:'):
        response = ask(thread, turn, prompt.split(':', 1)[1], {})
        text(thread, turn, json.dumps(response))
        finish(thread, turn)
        return
    if prompt == 'flood':
        for index in range(600): notify('future.event', {'index': index})
    if prompt == 'long':
        text(thread, turn, 'x' * 100000)
    if prompt == 'unknown':
        notify('future/notification', {'value': 'ü' * 40_000})
    elif prompt in ('file', 'big-diff'):
        diff = '--- a/test.m\n+++ b/test.m\n@@ -1 +1 @@\n-old\n+new\n'
        if prompt == 'big-diff':
            diff += 'ğ' * 90_000
        item(thread, turn, {'id': 'file', 'type': 'fileChange', 'status': 'inProgress',
                            'changes': [{'path': 'test.m', 'kind': {'type': 'update', 'move_path': 'renamed.m'}, 'diff': diff}]})
        notify('item/fileChange/patchUpdated', {'threadId': thread, 'turnId': turn, 'itemId': 'file',
                                               'changes': [{'path': 'test.m', 'kind': {'type': 'update'}, 'diff': diff}]})
        notify('turn/diff/updated', {'threadId': thread, 'turnId': turn, 'diff': diff})
    elif prompt == 'events':
        item(thread, turn, {'id': 'reason', 'type': 'reasoning', 'summary': ['Think first.']}, True)
        notify('item/reasoning/summaryTextDelta', {'threadId': thread, 'turnId': turn, 'itemId': 'stream-reason', 'summaryIndex': 0, 'delta': 'Streamed thought.'})
        item(thread, turn, {'id': 'stream-reason', 'type': 'reasoning', 'summary': ['Streamed thought.']}, True)
        item(thread, turn, {'id': 'command', 'type': 'commandExecution', 'command': 'pwd', 'cwd': os.getcwd(), 'commandActions': [], 'status': 'inProgress'})
        item(thread, turn, {'id': 'command', 'type': 'commandExecution', 'command': 'pwd', 'status': 'completed'}, True)
        item(thread, turn, {'id': 'mcp', 'type': 'mcpToolCall', 'server': 'indymat', 'tool': 'session_status', 'arguments': {}, 'status': 'inProgress'})
        item(thread, turn, {'id': 'web', 'type': 'webSearch', 'query': 'Octave'})
        notify('thread/tokenUsage/updated', {'threadId': thread})
        notify('error', {'threadId': thread, 'turnId': turn, 'willRetry': True, 'error': {'message': 'Retrying.'}})
    elif prompt in ('failed', 'fail'):
        finish(thread, turn, 'failed', {'message': 'Model failed.'})
        return
    elif prompt == 'stderr':
        os.write(sys.stderr.fileno(), b'z' * 100_000)
    if prompt not in ('unknown', 'file', 'big-diff', 'events', 'stderr'):
        item(thread, turn, {'id': 'read', 'type': 'commandExecution', 'command': 'list files', 'status': 'inProgress'})
        item(thread, turn, {'id': 'reason', 'type': 'reasoning', 'summary': ['Fixture summary']}, True)
        notify('future.event', {'html': '<img src=x onerror=alert(1)>'})
        text(thread, turn, 'Fixture answer\n```matlab\nx = 42;\n```\n')
    else:
        text(thread, turn, 'done')
    finish(thread, turn)


for line in sys.stdin:
    message = json.loads(line)
    if 'method' not in message:
        state = requests.get(message.get('id'))
        if state:
            state['response'] = message
            state['done'].set()
        continue
    method, params = message['method'], message.get('params', {})
    if method == 'initialize':
        if 'fake_hang_initialize=true' in sys.argv:
            continue
        if 'fake_bad_initialize=true' in sys.argv:
            sys.stdout.write('not-json\n')
            sys.stdout.flush()
            continue
        reply(message, {'userAgent': 'fake-codex', 'codexHome': os.getcwd(), 'platformFamily': 'unix', 'platformOs': sys.platform})
    elif method == 'initialized':
        initialized = True
    elif not initialized:
        send({'id': message['id'], 'error': {'code': -32600, 'message': 'Not initialized'}})
    elif method == 'thread/start':
        serial += 1
        thread = 'thread-' + str(serial)
        threads[thread] = params
        notify('thread/started', {'thread': {'id': thread}})
        reply(message, {'thread': {'id': thread}})
    elif method == 'thread/resume':
        if params['threadId'] == 'hang':
            continue
        if params['threadId'] == 'rpc-error':
            send({'id': message['id'], 'error': {'code': -32000, 'message': 'Cannot resume'}})
        elif params['threadId'] == 'bad-result':
            reply(message, {})
        else:
            threads.setdefault(params['threadId'], {'resumed': True})
            reply(message, {'thread': {'id': params['threadId']}})
    elif method == 'turn/start':
        serial += 1
        thread, turn = params['threadId'], 'turn-' + str(serial)
        turns[thread] = turn
        prompt = params['input'][0]['text']
        if prompt == 'grandchild':
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        if prompt == 'hang-call':
            continue
        notify('turn/started', {'threadId': thread, 'turn': {'id': turn, 'status': 'inProgress', 'items': [], 'error': None}})
        if prompt == 'early':
            script(thread, turn, 'hello')
            reply(message, {'turn': {'id': turn}})
            continue
        reply(message, {'turn': {'id': turn}})
        if prompt.split('End of IndyMAT context.\n\n')[-1].strip() == 'config':
            text(thread, turn, json.dumps({'thread': threads[thread], 'argv': sys.argv[1:], 'env': dict(os.environ), 'input': prompt}))
            finish(thread, turn)
        else:
            threading.Thread(target=script, args=(thread, turn, prompt), daemon=True).start()
    elif method == 'turn/steer':
        assert params['expectedTurnId'] == turns[params['threadId']]
        reply(message, {'turnId': params['expectedTurnId']})
        text(params['threadId'], params['expectedTurnId'], params['input'][0]['text'])
    elif method == 'turn/interrupt':
        reply(message, {})
        finish(params['threadId'], params['turnId'], 'interrupted')
    else:
        send({'id': message['id'], 'error': {'code': -32601, 'message': 'Unknown method'}})
