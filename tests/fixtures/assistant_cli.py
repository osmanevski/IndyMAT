#!/usr/bin/env python3
"""Recorded-style NDJSON fixture; selected by INDYMAT_ASSISTANT_<provider>.
No model call. The fixture infers provider from the actual production argv.
Prompts: wait (stop), flood (bounded events), long (bounded lines), fail.
"""
import json
import sys
import time
import os
import subprocess

if 'app-server' in sys.argv:
    import runpy
    from pathlib import Path
    runpy.run_path(str(Path(__file__).with_name('codex_app_server_fake.py')), run_name='__main__')
    sys.exit(0)
if '--version' in sys.argv:
    print('fixture 1.0')
    sys.exit(0)
provider = 'claude' if '--permission-mode' in sys.argv else 'agy'
attached = [value for value in sys.argv if value.startswith('-p=')]
prompt = attached[0][3:] if attached else sys.stdin.read()
full_prompt = prompt
# The application opens a conversation with its own context note; the fixture's commands are the user's part.
prompt = prompt.split('End of IndyMAT context.\n\n', 1)[-1]
if prompt.strip() == 'echo-context':
    print(json.dumps({'type': 'assistant', 'message': {'content': [{'type': 'text', 'text': full_prompt}]}}), flush=True)
def emit(value):
    print(json.dumps(value), flush=True)

if provider == 'claude': emit({'type': 'system', 'subtype': 'init', 'session_id': 'fixture-session'})
else: emit({'type': 'conversation', 'conversation_id': 'fixture-agy'})
if prompt.strip() == 'descendant':
    child = subprocess.Popen([sys.executable, '-c', "import pathlib,time; p=pathlib.Path('fixture-child-heartbeat');\nwhile True:\n p.write_text(str(time.monotonic_ns())); time.sleep(.02)"])
    emit({'type': 'fixture.child', 'pid': child.pid})
    time.sleep(30)
if prompt.strip() == 'wait':
    emit({'type': 'fixture.waiting'})
    time.sleep(30)
if prompt.strip() == 'stream':
    emit({'type': 'stream_event', 'event': {'delta': {'type': 'text_delta', 'text': 'First fragment'}}})
    time.sleep(.3)
if prompt.strip() == 'flood':
    for index in range(600): emit({'type': 'future.event', 'index': index})
if prompt.strip() == 'long': print('x' * 70000, flush=True)
if prompt.strip() == 'fail':
    emit({'type': 'error', 'message': 'Fixture error'})
    sys.exit(2)
# Bridge fixture uses only the production argv configuration, never a test API.
if prompt.strip() == 'bridge-run':
    config = json.load(open(sys.argv[sys.argv.index('--mcp-config') + 1]))['mcpServers']['indymat']
    bridge_argv = [config['command'], *config['args']]
    code = 'bridge_fixture_' + provider + ' = 7741; disp(7741);'
    emit({'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'name': 'mcp__indymat__run_code', 'input': {'code': code}}]}})
    bridge = subprocess.Popen(bridge_argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    output, _ = bridge.communicate(json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': 'run_code', 'arguments': {'code': code}}}) + '\n', timeout=20)
    reply = json.loads(output)['result']
    text = json.dumps(reply)
    emit({'type': 'assistant', 'message': {'content': [{'type': 'text', 'text': text}]}})
# Permission requests travel through the production stdio relay and obey its answer.
if provider == 'claude' and '--permission-prompt-tool' in sys.argv and prompt.strip().startswith('approval'):
    from pathlib import Path
    config = json.load(open(sys.argv[sys.argv.index('--mcp-config') + 1]))['mcpServers']['indymat']
    bridge = subprocess.Popen([config['command'], *config['args']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    repeats = 2 if prompt.strip() == 'approval-twice' else 1
    for index in range(repeats):
        value = {'file_path': str(Path('sample.m').resolve()), 'content': 'approved = ' + str(index + 42) + ';\n'}
        emit({'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'name': 'Write', 'input': value}]}})
        bridge.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': index + 1, 'method': 'tools/call', 'params': {'name': 'approve', 'arguments': {'tool_name': 'Write', 'input': value, 'tool_use_id': 'fixture-write-' + str(index)}}}) + '\n')
        bridge.stdin.flush()
        reply = json.loads(bridge.stdout.readline())['result']
        answer = json.loads(reply['content'][0]['text'])
        if answer.get('behavior') == 'allow':
            Path(answer['updatedInput']['file_path']).write_text(answer['updatedInput']['content'])
        emit({'type': 'assistant', 'message': {'content': [{'type': 'text', 'text': json.dumps(answer, ensure_ascii=False)}]}})
    bridge.stdin.close()
    bridge.wait(timeout=5)
# Tools and reasoning are intentionally different across providers.
text = 'Fixture answer\n```matlab\nx = 42;\n```\n'
emit({'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'name': 'Read', 'input': {'file_path': 'sample.m'}}, {'type': 'thinking', 'thinking': 'Fixture summary'}, {'type': 'text', 'text': text}]}})
if provider == 'claude': emit({'type': 'result', 'session_id': 'fixture-session', 'is_error': False})
else: emit({'type': 'conversation', 'conversation_id': 'fixture-agy'})
emit({'type': 'future.event', 'html': '<img src=x onerror=alert(1)>'})
# Incomplete/malformed terminal output must survive as a raw entry.
print('{unfinished', end='', flush=True)
