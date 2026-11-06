#!/usr/bin/env python3
"""Check captured native diagnostics evidence; this does not drive or replay the UI."""
import argparse, datetime, hashlib, json, zipfile
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
    with zipfile.ZipFile(root / 'before.zip') as before:
        raw = before.read('llama-server.log').decode()
        check(before.testzip() is None and all(s in raw for s in ('NATIVE-DIAG-PRIVATE-EN', 'NATIVE-DIAG-CONTINUATION', 'NATIVE-DIAG-PRIVATE-JSON')), 'the original native ZIP reproduces the English, continuation and JSON content leak')
    with zipfile.ZipFile(root / 'final-clean.zip') as saved:
        check(saved.testzip() is None, 'the final native ZIP passes CRC validation')
        check(saved.namelist() == ['info.json', 'timing.json', 'errors.json', 'llama-server.json'], 'the ZIP contains only the four previewed JSON files')
        text = {name: saved.read(name).decode() for name in saved.namelist()}
    joined = '\n'.join(text.values())
    check(not any(s in joined for s in ('NATIVE-PRIVATE-', 'NATIVE-DIAG-', '私人会议内容', 'private meeting sentence', 'private request', 'native-diag-token-fixture-2141', 'synthetic-user', 'hsiangnianian')), 'the final ZIP excludes all seeded content, credential and username samples')
    info, timing, errors, log = (json.loads(text[n]) for n in text)
    check(info['version'] == '0.21.0' and info['platform'] == {'os': 'linux', 'arch': 'x86_64'}, 'app version and actual native platform remain available')
    check(info['model']['state'] == 'no-model' and '/home/[user]/' in info['model']['modelsDir'], 'model state remains available while the user path is anonymized')
    check(timing['lines'] == 2 and timing['modelMs']['max'] == 375 and timing['spoken'] == 0, 'numeric meeting timing remains available without transcript text')
    check(errors == [], 'the native run exports an empty error array when no error was captured')
    check(log['lines'] == 11 and log['contextTokens'] == [4096] and log['timings'] == [{'stage': 'prompt eval', 'ms': 12, 'tokens': 4}], 'model-log summary retains context size and numeric timing')
    check(text['llama-server.json'] in labels('final2-expanded'), 'the actual saved model-log summary exactly matches its expanded native preview')
    summaries = [f'{name} {len(value)} characters' for name, value in text.items()]
    check(all(s in labels('final2-preview') for s in summaries), 'all four native preview names and character counts match the saved files')
    cancelled = labels('final2-cancelled')
    check('Export zip' in cancelled and all(s in cancelled for s in summaries) and not any('Diagnostics saved:' in s for s in cancelled), 'canceling the native save keeps the preview and does not announce success')
    failed = read('clean-permission-result.json')['elements']
    check(any('Could not export diagnostics: Save failed: Permission denied (os error 13)' in e.get('label', '') for e in failed) and any(e.get('label') == 'Export zip' and e.get('enabled') for e in failed), 'native EACCES is visible and export remains available for retry')
    locked = (root / 'locked.zip').read_bytes()
    filesystem = read('filesystem-check.json')['lockedTarget']
    check(locked == b'PRESERVE-ORIGINAL-DIAGNOSTIC' and filesystem['mode'] == 0o400 and filesystem['sha256'] == hashlib.sha256(locked).hexdigest(), 'the read-only target retains its original bytes and recorded native permissions')
    final = labels('clean-saved')
    check(any(s.endswith('/diagnostics/final-clean.zip') and s.startswith('Diagnostics saved:') for s in final) and 'Export diagnostics' not in final, 'retry saves the native file, announces its path and closes the preview')
    check(any('NATIVE-PRIVATE-ONE' in s for s in final) and any('NATIVE-PRIVATE-TWO' in s for s in final), 'both synthetic meeting lines remain in the app after export')
    check((root / 'final-clean.zip').read_bytes() == (root / 'final2.zip').read_bytes(), 'the repeated native save produces identical diagnostic bytes')
    source = read('source-check.json')
    check(source['changedFiles'] == [] and source['checkedSourceFiles'] == 192 and source['embeddedBundleFound'] and source['privateRustMatches'], 'the recorded final source and embedded bundle identity match the tested build')
    report.update(passed=True, bundle=source['bundle'], binarySha256=source['binarySha256'], zipSha256=hashlib.sha256((root / 'final-clean.zip').read_bytes()).hexdigest())
finally:
    (root / 'native-check.json').write_text(json.dumps(report, indent=2) + '\n')
