"""Tiny stdio MCP transport. Each tool operation makes one loopback HTTP request.

Only argv[1], a private capability-file path, is passed by the agent CLI. Session
policy and job submission are exclusively in the running application's service.
"""
import http.client
import json
import math
import sys
from urllib.parse import urlsplit

HTTP_TIMEOUT = 15


def transport_request(credentials, route, payload):
    target = urlsplit(credentials['url'])
    if target.scheme != 'http' or target.hostname != '127.0.0.1' or not target.port or target.path not in ('', '/') or target.query or target.fragment or target.username: raise ValueError('Invalid bridge server address.')
    connection = http.client.HTTPConnection('127.0.0.1', target.port, timeout=3660 if route == 'call' and payload.get('name') == 'approve' else HTTP_TIMEOUT)
    try:
        connection.request('POST', '/api/bridge/' + route, json.dumps(payload).encode(), {'X-IndyMAT-Bridge': credentials['token'], 'Content-Type': 'application/json'})
        response = connection.getresponse()
        data = response.read(8_000_001)
        if len(data) > 8_000_000: raise ValueError('Bridge response exceeds the transport limit.')
        result = json.loads(data)
        if response.status != 200: raise ValueError(result.get('error', 'Bridge request refused.'))
        return result
    finally: connection.close()


def serve(credentials):
    def reject_constant(value):
        raise ValueError('Invalid JSON constant.')
    def send(value):
        sys.stdout.write(json.dumps(value, ensure_ascii=True) + '\n')
        sys.stdout.flush()
    while True:
        line = sys.stdin.buffer.readline(150_001)
        if not line: return
        identity = None
        try:
            if len(line) > 150_000:
                while line and not line.endswith(b'\n'): line = sys.stdin.buffer.readline(150_001)
                raise ValueError('Request exceeds the transport limit.')
            request = json.loads(line, parse_constant=reject_constant)
        except (ValueError, UnicodeError, RecursionError):
            send({'jsonrpc': '2.0', 'id': None, 'error': {'code': -32700, 'message': 'Parse error'}})
            continue
        if not isinstance(request, dict) or request.get('jsonrpc') != '2.0' or not isinstance(request.get('method'), str) or ('id' in request and (isinstance(request['id'], bool) or not isinstance(request['id'], (str, int, float, type(None))) or (isinstance(request['id'], float) and not math.isfinite(request['id'])))):
            send({'jsonrpc': '2.0', 'id': None, 'error': {'code': -32600, 'message': 'Invalid Request'}})
            continue
        identity = request.get('id')
        method = request['method']
        params = request.get('params', {})
        if 'id' not in request: continue
        response = {'jsonrpc': '2.0', 'id': identity}
        if not isinstance(params, dict): response['error'] = {'code': -32602, 'message': 'Invalid params'}
        elif method == 'initialize': response['result'] = {'protocolVersion': params.get('protocolVersion', '2024-11-05'), 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'indymat', 'version': '1'}}
        elif method == 'ping': response['result'] = {}
        elif method in ('tools/list', 'tools/call'):
            # Real clients add protocol bookkeeping such as _meta (progress token) and a list cursor;
            # only the tool name and its arguments are the application's business.
            if method == 'tools/call' and isinstance(params, dict): params = {'name': params.get('name'), 'arguments': params.get('arguments') or {}}
            elif method == 'tools/list': params = {}
            try: response['result'] = transport_request(credentials, 'tools' if method == 'tools/list' else 'call', params)
            except Exception as error:
                response['result'] = {'content': [{'type': 'text', 'text': str(error)[:2000]}], 'isError': True}
        else: response['error'] = {'code': -32601, 'message': 'Method not found'}
        send(response)


if __name__ == '__main__':
    try:
        with open(sys.argv[1], encoding='utf-8') as stream: credentials = json.load(stream)
    except (OSError, ValueError, IndexError):
        credentials = {}
    serve(credentials)
