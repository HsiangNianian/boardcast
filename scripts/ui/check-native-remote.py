#!/usr/bin/env python3
"""Check evidence captured through Cua from the isolated native remote-settings run."""
import argparse, datetime, hashlib, json, pathlib, stat

parser = argparse.ArgumentParser()
parser.add_argument('root', type=pathlib.Path)
args = parser.parse_args()
root = args.root.resolve()
if not root.name.startswith('boardcast-g2-') or not (root / 'source-hashes.json').is_file():
    raise SystemExit('Expected the explicitly owned native verification directory')

def read(name):
    return json.loads((root / name).read_text())

def labels(name):
    return '\n'.join(e.get('label', '') for e in read(name + '.json')['elements'])

report = {'date': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'method': 'Real Linux Tauri/WebKit, Cua native input, Rust IPC and files; local non-forwarding proxy fixture',
    'checks': [], 'sourceHashes': read('source-hashes.json'),
    'binarySha256': (root / 'binary-sha256.txt').read_text().strip()}

def check(condition, message):
    assert condition, message
    report['checks'].append(message)
    print('PASS:', message)

try:
    rows = read('remote-requests-stage1.json')
    check(all(r['host'] in ('boardcast-a.test', 'boardcast-b.test') and r['path'] == '/v1/chat/completions' for r in rows), 'all observed native requests use the named synthetic targets')
    check(all(r['keyMatches'] for r in rows), 'native requests consistently use the synthetic saved credential')
    check(all(r['fixedProbe'] and not r['containsMeeting'] for r in rows if r['probe']), 'every connection probe contains only the fixed probe')
    check('Connected:' in labels('test-only') and 'In use' not in labels('test-only') and 'Save and enable' in labels('test-only'), 'successful test remains separate from enabling')
    check('Once enabled, this is sent to boardcast-a.test:' in labels('consent-a') and 'Save and enable' in labels('cancelled-a'), 'initial consent names A and cancellation returns to the draft')
    check('Base URL http://boardcast-a.test/v1' in labels('cancelled-a') and 'Model native-fixture' in labels('cancelled-a'), 'cancelled initial consent preserves URL and model draft')
    check('Enabled:' in labels('enabled-a') and 'Saved nat…2a71' in labels('enabled-a'), 'enable displays success and a saved credential hint')
    remote = read('meeting-remote.json')
    check(remote['version'] == 2 and remote['storageRevision'] == 1 and remote['graph']['nodes'][0]['label'] == 'Native verified' and remote['lines'][0]['via'] == 'Remote native-fixture', 'the first native request produces a saved v2 transcript and board')
    local = read('meeting-local-failed.json')
    check(local['localOnly'] and len(local['lines']) == 2 and local['lines'][-1]['failed'] and local['lines'][-1]['boardFailure'] == 'unavailable', 'local-only with no installed model preserves the original line as unavailable')
    check(not any(r['phase'] == 'local-only' for r in rows), 'local-only input makes no remote request')
    check('Retry this line' in labels('local-sent') and 'Configure board model' in labels('local-sent'), 'the unavailable line has actionable native recovery controls')
    check('HTTP 401 Unauthorized: Invalid credential: [key]' in labels('native-401'), 'native HTTP errors redact the echoed credential')
    timeout = [r for r in rows if r['phase'] == 'timeout']
    check(len(timeout) == 1 and timeout[0]['clientClosed'] and 28000 < timeout[0]['closedAfterMs'] < 35000 and 'Request timed out.' in labels('native-timeout'), 'the native 30-second body timeout closes the socket and shows a recoverable error')
    check('Model native-fixture' in labels('native-waiting-readonly') and 'ignored-while-testing' not in labels('native-waiting-readonly'), 'typing during a connection test does not mutate the model draft')
    check('Connected:' in labels('timeout-recovered'), 'connection testing succeeds again after a timeout')
    check('Once enabled, this is sent to boardcast-b.test:' in labels('consent-b') and 'Base URL http://boardcast-b.test/v1' in labels('cancelled-b') and not any(r['phase'] == 'cancel-b' for r in rows), 'changing domains asks again and cancelled B consent sends no request')
    check('Permission denied' in labels('native-save-denied') and 'Base URL http://boardcast-b.test/v1' in labels('native-save-denied') and 'Enabled:' in labels('native-save-recovered'), 'native file permission failure preserves the draft and retry succeeds')
    check('Save and enable' in labels('disabled-b') and 'Save and enable' in labels('restarted-remote') and 'Saved nat…2a71' in labels('restarted-remote'), 'disabled configuration and saved credential hint survive normal quit and restart')
    check('NATIVE-PRIVATE-ONE' in labels('restarted') and 'NATIVE-PRIVATE-TWO' in labels('restarted') and 'Retry this line' in labels('restarted'), 'both saved transcript lines and their recovery state survive restart')
    recovered = read('meeting-recovered.json')
    check(not recovered.get('localOnly') and len(recovered['lines']) == 2 and not recovered['lines'][-1].get('failed') and recovered['lines'][-1]['id'] == local['lines'][-1]['id'] and recovered['lines'][-1]['via'] == 'Remote native-fixture', 'releasing local-only and retrying updates the same saved line without duplication')
    chat = [r for r in rows if not r['probe']]
    check(len(chat) == 2 and [(r['phase'], r['host']) for r in chat] == [('meeting-one', 'boardcast-a.test'), ('retry-b', 'boardcast-b.test')] and all(r['containsMeeting'] for r in chat), 'only the two explicitly sent or retried lines reach their approved destinations')
    config = next((root / 'config-headless').rglob('remote.json'))
    saved = json.loads(config.read_text())
    check(stat.S_IMODE(config.stat().st_mode) == 0o600 and hashlib.sha256(saved['apiKey'].encode()).hexdigest() == read('remote-server.json')['keySha256'], 'native configuration retains the credential with 0600 permissions')
    ui_files = [p for p in root.glob('*.json') if 'elements' in json.loads(p.read_text())]
    check(all(saved['apiKey'] not in p.read_text() for p in ui_files), 'the raw synthetic credential is absent from all captured accessibility snapshots')
    report.update(passed=True, requests=rows, timeoutMs=timeout[0]['closedAfterMs'], uiSnapshotsChecked=len(ui_files))
finally:
    (root / 'native-remote-check.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
