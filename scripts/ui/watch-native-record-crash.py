#!/usr/bin/env python3
"""Kill only an owned G2 fixture process when its production .tmp record opens.

This is a process-crash check, not power-loss testing or an app write failpoint.
"""
import argparse, ctypes, hashlib, json, os, pathlib, select, signal, struct, time

p = argparse.ArgumentParser()
p.add_argument('root', type=pathlib.Path)
p.add_argument('pid', type=int)
p.add_argument('meeting')
p.add_argument('--expected-line', required=True)
p.add_argument('--timeout', type=float, default=120)
args = p.parse_args()
root = args.root.resolve()
expected = str(root / 'target/debug/boardcast')
if not root.name.startswith('boardcast-g2-') or not (root / 'source-hashes.json').is_file():
    raise SystemExit('Expected a source-identified disposable G2 run')
if not args.meeting.replace('-', '').isalnum(): raise SystemExit('Invalid fixture id')
if os.readlink(f'/proc/{args.pid}/exe') != expected: raise SystemExit('Not the owned fixture app')
directory = root / 'data/games.mmstudio.boardcast.g2/meetings/v2'
record = directory / (args.meeting + '.json')
temp = directory / (args.meeting + '.json.tmp')
original_bytes = record.read_bytes()
if b'asset:' in original_bytes:
    raise SystemExit('This record-crash fixture must be resource-free; asset preparation needs a separate check')
original = json.loads(original_bytes)
before = hashlib.sha256(original_bytes).hexdigest()
libc = ctypes.CDLL(None, use_errno=True)
fd = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
if fd < 0: raise OSError(ctypes.get_errno(), 'inotify_init1')
if libc.inotify_add_watch(fd, os.fsencode(directory), 0x100 | 0x2) < 0:
    raise OSError(ctypes.get_errno(), 'inotify_add_watch')
deadline = time.monotonic() + args.timeout
print('watching owned fixture temporary file', temp, 'pid', args.pid, flush=True)
result = dict(pid=args.pid, meeting=args.meeting, originalSha256=before, killed=False)
try:
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], min(1, max(0, deadline - time.monotonic())))
        if not ready: continue
        events = os.read(fd, 65536)
        offset = 0
        while offset + 16 <= len(events):
            _, mask, _, length = struct.unpack_from('iIII', events, offset)
            name = events[offset + 16:offset + 16 + length].split(b'\0')[0].decode()
            offset += 16 + length
            if name != temp.name: continue
            if os.readlink(f'/proc/{args.pid}/exe') != expected: raise SystemExit('Target identity changed')
            try: size = temp.stat().st_size
            except FileNotFoundError: size = None
            os.kill(args.pid, signal.SIGKILL)
            result.update(killed=True, observedEvent=mask, tempSizeAtKill=size, killedAt=time.time())
            raise StopIteration
except StopIteration:
    pass
finally:
    os.close(fd)
    if result['killed']:
        for _ in range(100):
            try:
                stat = pathlib.Path(f'/proc/{args.pid}/stat').read_text().rsplit(')', 1)[1].split()
                if stat[0] == 'Z': break
            except FileNotFoundError: break
            time.sleep(.05)
        else: raise RuntimeError('Owned process did not exit after SIGKILL; no stable disk assertion')
    after_bytes = record.read_bytes()
    after = json.loads(after_bytes)
    result['recordSha256AfterKill'] = hashlib.sha256(after_bytes).hexdigest()
    try: result['temporaryBytes'] = temp.stat().st_size
    except FileNotFoundError: result['temporaryBytes'] = None
    result['temporaryExists'] = result['temporaryBytes'] is not None
    result['recordParsesAfterKill'] = isinstance(after, dict)
    old = result['recordSha256AfterKill'] == before
    new = isinstance(after, dict) and all([
        after.get('id') == original['id'],
        after.get('storageRevision') == original['storageRevision'] + 1,
        after.get('lines', [])[:-1] == original['lines'],
        len(after.get('lines', [])) == len(original['lines']) + 1,
        bool(after.get('lines')) and after['lines'][-1].get('text') == args.expected_line,
        after.get('history', [])[:-1] == original['history'],
        [page['graph'] for page in after.get('pages', [])] == [page['graph'] for page in original['pages']],
        all(after.get(key) == original.get(key) for key in ('title', 'summary', 'voices', 'glossary', 'corrections', 'localOnly')),
    ])
    result['recovered'] = 'exact-old-record' if old else 'complete-new-revision' if new else 'INVALID'
    (root / (args.meeting + '-crash.json')).write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2), flush=True)
if not result['killed']: raise SystemExit('No observed temporary write; nothing was killed')
if result['recovered'] == 'INVALID': raise SystemExit('Record is neither the exact original nor the expected complete new revision')
