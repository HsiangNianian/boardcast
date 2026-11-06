#!/usr/bin/env python3
"""Check captured native deletion evidence; this does not drive or replay the UI."""
import argparse, datetime, hashlib, json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('evidence', type=Path)
root = parser.parse_args().evidence

def read(name):
    return json.loads((root / name).read_text())

def labels(name):
    return [e.get('label', '') for e in read(name + '.json')['elements']]

report = {'at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'checks': [], 'passed': False}

def check(condition, message):
    assert condition, message
    report['checks'].append(message)
    print('PASS:', message)

try:
    check(len(read('a-initial.json')['elements']) > 20, 'the initial native process exposes its accessibility tree after registry startup')
    for fixture, extra in [('r0', 'LATE-NATIVE-R0'), ('v1', 'Board 2'), ('r1', 'Board 3')]:
        state = labels(f'a-{fixture}-switch-blocked')
        check('Save failed' in state and 'Retry save' in state and 'Export a copy' in state,
              f'{fixture}: save failure remains recoverable after retry and New meeting')
        check(f'Meeting library Native Delete {fixture.upper()}' in state and extra in state
              and f'ORIGINAL-NATIVE-{fixture.upper()}' in state,
              f'{fixture}: the original meeting and its late edit remain visible')
    before = read('v1-before-delete.json')
    check(all(before.values()) and 'delete-v1.json' in before, 'legacy bytes and images were unchanged before deletion')
    check(read('r1-before-overlap.json')['sha256'] == read('fixture.json')['hashes']['v2/delete-r1.json'],
          'the revision-one original was unchanged before native contention')
    contention = read('r1-lock-contention.json')
    pids = [read(name)['pid'] for name in ('a-r1-original-loaded.json', 'b-r1-confirm.json')]
    check(len(set(pids)) == 2 and all(any('-> FLOCK' in line and f'WRITE {pid} ' in line
              and f':{contention["holder"]["inode"]} ' in line for line in contention['locks']) for pid in pids),
          'both independent native app PIDs waited on the captured shared file lock')
    check('Saved' in labels('a-r1-overlap-result') and 'Board 2' in labels('a-r1-overlap-result')
          and not any('Native Delete R1 Today' in label for label in labels('b-r1-overlap-result')),
          'after release, the queued save completed and the other native window deleted the original')
    storage = read('native-delete-storage-check.json')
    check(set(storage['deleted']) == {'delete-r0', 'delete-v1', 'delete-r1'}
          and all(set(namespaces) == {'.', 'v2'} for namespaces in storage['deleted'].values())
          and all(state == {'record': False, 'temporaryRecord': False, 'assets': False, 'markerBytes': 0}
                  for namespaces in storage['deleted'].values() for state in namespaces.values()),
          'all three deleted originals have no record, temporary record or assets in either namespace')
    cold = labels('c-cold-library')
    check(sum(label.startswith('Native Delete R1 (cont.) Today') for label in cold) == 2
          and not any(name in label for name in ('Native Delete R0', 'Native Delete V1', 'Native Delete R1 Today') for label in cold),
          'a fresh native process lists the two live continuation controls and none of the deleted originals')
    recovery = read('r0-offline-recovery.json')
    html = (root / 'r0-recovery.html').read_bytes()
    check(recovery['passed'] and recovery['offline'] and recovery['pageErrors'] == []
          and recovery['bytes'] == len(html) and recovery['sha256'] == hashlib.sha256(html).hexdigest(),
          'the actual native HTML matches the successful independent offline-browser check')
    data = recovery['data']
    check({'ORIGINAL-NATIVE-R0', 'LATE-NATIVE-R0'} <= {line['text'] for line in data['lines']}
          and any(node['label'] == 'Retained native board' for node in data['pages'][0]['graph']['nodes'])
          and data['pages'][0]['underlay'].startswith('data:image/svg+xml;base64,'),
          'the recovery file contains the original and late transcript, graph and embedded image')
    control = read('desktop-save-control.json')
    control_html = (root / 'desktop-save-control.html').read_bytes()
    check(control['dialogClosed'] and len(control['windows']['windows']) == 1
          and control['bytes'] == len(control_html) and control['sha256'] == hashlib.sha256(control_html).hexdigest()
          and any(label.startswith('Saved: ') and label.endswith('/boardcast-20261103-2251.html') for label in labels('c-desktop-save-final')),
          'the separate direct-desktop save control closes its dialog and reports the saved file')
    source = read('source-check.json')
    check(source['changedFiles'] == [] and source['checkedSourceFiles'] == 189
          and source['privateRustDifferences'] == [] and source['embeddedBundleFound'] and source['deleteFenceStringFound'],
          'the captured source, private Rust copy and embedded production bundle identify the tested binary')
    cleanup = read('cleanup.json')
    check(cleanup['remainingOwnedPids'] == [] and all(cleanup['knownPidsGone'].values()) and cleanup['privateDisplaySocketGone'],
          'all owned native, accessibility, display and helper processes were cleaned up')
    report.update(passed=True, bundle=source['bundle'], binarySha256=source['binarySha256'],
                  limitations=['The first saved dialog later remained mapped; its cause is unresolved.',
                               'The direct-desktop save control used a different process and meeting.',
                               'Offline HTML was opened in Chromium, not through the native Open file button.',
                               'These assertions check recorded evidence and do not replay native actions.'])
finally:
    (root / 'native-check.json').write_text(json.dumps(report, indent=2) + '\n')
