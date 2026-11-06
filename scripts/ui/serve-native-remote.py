#!/usr/bin/env python3
"""Local-only HTTP proxy fixture for the real, isolated Tauri remote-settings UI."""
import argparse, hashlib, http.server, json, os, pathlib, threading, time, urllib.parse

parser = argparse.ArgumentParser()
parser.add_argument('root', type=pathlib.Path)
args = parser.parse_args()
root = args.root.resolve()
if not root.name.startswith('boardcast-g2-') or not (root / 'source-hashes.json').is_file():
    raise SystemExit('Expected an explicitly owned native test directory')
key = 'native-remote-fixture-4b2a71'
mode_file = root / 'remote-mode.json'
if not mode_file.exists(): mode_file.write_text('{"mode":"ok","phase":"initial"}\n')
requests, lock = [], threading.Lock()

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *_): pass

    def do_CONNECT(self):
        self.send_error(502, 'This fixture never forwards network traffic')

    def do_GET(self):
        self.reply(200, '{}')

    def reply(self, status, text):
        data = text.encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Connection', 'close')
        self.end_headers()
        try: self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError): pass
        self.close_connection = True

    def do_POST(self):
        target = urllib.parse.urlsplit(self.path)
        host = target.hostname or self.headers.get('Host', '').split(':')[0]
        if host not in ('boardcast-a.test', 'boardcast-b.test', '127.0.0.1', 'localhost'):
            self.reply(400, '{"error":{"message":"Unknown synthetic target"}}'); return
        try:
            body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
            messages = body['messages']
            assert isinstance(messages, list) and messages
            control = json.loads(mode_file.read_text())
        except (ValueError, KeyError, AssertionError):
            self.reply(400, '{"error":{"message":"Invalid fixture request"}}'); return
        mode = control.get('mode', 'ok')
        probe = body.get('max_tokens') == 8 and len(messages) == 1
        row = dict(at=time.time(), host=host, path=target.path, mode=mode, phase=control.get('phase'),
            probe=probe, fixedProbe=probe and messages[0].get('content') in ('只回复 OK 两个字母。', 'Reply with just the two letters OK.'),
            model=body.get('model'), keyMatches=self.headers.get('Authorization') == f'Bearer {key}',
            containsMeeting='NATIVE-PRIVATE-' in json.dumps(messages))
        with lock:
            requests.append(row)
            (root / 'remote-requests.json').write_text(json.dumps(requests, indent=2)+'\n')
        if mode == 'hold':
            for _ in range(900):
                time.sleep(.05)
                mode = json.loads(mode_file.read_text()).get('mode', 'ok')
                if mode != 'hold': break
        if mode == 'stall-body':
            self.send_response(200); self.send_header('Content-Length', '1000'); self.end_headers()
            self.wfile.write(b'{"choices":['); self.wfile.flush()
            self.connection.settimeout(40)
            try: closed = self.connection.recv(1) == b''
            except (ConnectionResetError, BrokenPipeError): closed = True
            except TimeoutError: closed = False
            with lock:
                row['closedAfterMs'] = round((time.time()-row['at'])*1000); row['clientClosed'] = closed
                (root / 'remote-requests.json').write_text(json.dumps(requests, indent=2)+'\n')
            self.close_connection = True; return
        if mode == '401':
            credential = self.headers.get('Authorization', '').removeprefix('Bearer ')
            self.reply(401, json.dumps({'error': {'message': f'Invalid credential: {credential}'}})); return
        if mode == '503': self.reply(503, '{"error":{"message":"Synthetic unavailable"}}'); return
        summary = any('helping someone' in str(m.get('content')) or '帮人' in str(m.get('content')) for m in messages)
        content = 'OK' if probe else '### Conclusions\n- Native verification summary.' if summary else 'N 「Native verified」'
        self.reply(200, json.dumps({'choices': [{'message': {'content': content}}]}))

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
(root / 'remote-server.json').write_text(json.dumps({'pid':os.getpid(),'port':server.server_port,'keySha256':hashlib.sha256(key.encode()).hexdigest()},indent=2)+'\n')
print(f'Isolated remote fixture listening on 127.0.0.1:{server.server_port}', flush=True)
try: server.serve_forever()
finally: server.server_close()
