#!/usr/bin/env python3
"""Check recorded native export evidence, without replaying UI or changing the archive."""
import argparse, base64, hashlib, json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('evidence', type=Path)
root = parser.parse_args().evidence
report = {'checks': [], 'passed': False}

def read(name):
    return json.loads((root / name).read_text())

def labels(name):
    return [e.get('label', '') for e in read(name)['elements']]

def check(condition, message):
    assert condition, message
    report['checks'].append(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

try:
    run = read('run.json')
    source = read('source-check.json')
    build = read('build-source-check.json')
    check(source['checkedSourceFiles'] == len(read('source-hashes.json')) == 189
          and source['changedFiles'] == [] and source['recheckedAtEnd']
          and source['binarySha256'] == build['binarySha256'] == read('fixture.json')['binarySha256']
          and source['embeddedBundleFound'] and build['privateRustDifferences'] == [],
          'the same source-identified native binary was used throughout the controls and final flow')
    check(digest(root / 'run-native-isolated-display.py') == source['helperSha256'],
          'the archived display helper matches the helper actually run')
    diagnostic = read('before/handoff-launch-diagnostic.json')
    check(diagnostic['exit'] == 0 and 'Failed to initialize Wayland platform' in diagnostic['stderr'],
          'the original launcher exited successfully despite Chromium failing to initialize Wayland')
    env = read('headless-env.json')
    check(env['XDG_SESSION_TYPE'] == 'x11' and env['XDG_CURRENT_DESKTOP'] == 'Openbox'
          and env['DESKTOP_SESSION'] == 'openbox' and env['XDG_DATA_HOME'] == run['root'] + '/data',
          'the corrected private display publishes its actual desktop identity and owned data path')
    check(read('mime-defaults.json') == {'text/html': 'chromium.desktop', 'application/pdf': 'chromium.desktop',
                                      'inode/directory': 'thunar.desktop'},
          'the tested default applications are explicitly recorded for the private configuration')

    html = root / run['canonicalHtml']
    content = html.read_text()
    marker = 'window.__boardcastData = '
    data, _ = json.JSONDecoder().raw_decode(content[content.index(marker) + len(marker):])
    check(data == read('export-content.json') and data['title'] == 'Native Export Control'
          and len(data['pages']) == 1 and [line['text'] for line in data['lines']] == ['EXPORT-NATIVE-CONTROL'],
          'the actual native HTML contains the fixture title, page and transcript')
    page = data['pages'][0]
    check(any(node['label'] == 'Retained native board' for node in page['graph']['nodes'])
          and base64.b64decode(page['underlay'].split(',', 1)[1]) == (root / 'fixture-image.svg').read_bytes(),
          'the saved replay preserves the semantic node and exact embedded background bytes')
    exports = [entry for entry in read('export-inventory.json') if entry['name'] != 'locked-control.html']
    check(len(exports) == 8 and all(entry['bytes'] == html.stat().st_size == 850121
          and entry['sha256'] == digest(html) for entry in exports),
          'all eight observed HTML outputs match the archived final artifact byte for byte')

    for prefix, filename, input_file, result_file in [
        ('b-keyboard', 'keyboard-control.html', 'b-keyboard-input-action.json', 'b-keyboard-save-result.json'),
        ('b-keyboard-retry', 'keyboard-retry.html', 'b-keyboard-retry-input.json', 'b-keyboard-retry-result.json'),
    ]:
        inputs = read(input_file)
        before = read(prefix + '-before-save.json')
        saved = read(prefix + '-save-action.json')
        check([entry['tool'] for entry in inputs] == ['hotkey', 'type_text']
              and all(entry['exit'] == 0 and entry['args']['target']['kind'] == 'desktop' for entry in inputs)
              and inputs[0]['args']['keys'] == ['ctrl', 'a']
              and inputs[1]['args']['text'] == run['root'] + '/exports/' + filename
              and all(not observation['exists'] for observation in before)
              and before[-1]['atNs'] - before[0]['atNs'] >= 2_500_000_000,
              f'{filename}: real keyboard input leaves the target absent for the full observation period')
        check(saved['exit'] == 0 and saved['existsBefore'] is False
              and saved['startedNs'] <= saved['file']['mtimeNs'] <= saved['observations'][-1]['atNs']
              and saved['file']['sha256'] == digest(html)
              and saved['file']['bytes'] == html.stat().st_size,
              f'{filename}: the file is created only after the recorded Save click starts')
        main = read(result_file)
        window = hex(main['window_id'])
        check(saved['observations'][-1]['fileExists']
              and saved['observations'][-1]['clientWindows'].strip().endswith('window id # ' + window)
              and 'Saved: ' + run['root'] + '/exports/' + filename in labels(result_file)
              and 'EXPORT-NATIVE-CONTROL' in labels(result_file),
              f'{filename}: the save dialog closes and the same meeting reports the real output path')

    failed = read('b-keyboard-permission-result.json')
    check(any('Permission denied (os error 13)' in e.get('label', '') for e in failed['elements'])
          and any(e.get('label') == 'Export again' and e.get('enabled') for e in failed['elements'])
          and 'EXPORT-NATIVE-CONTROL' in labels('b-keyboard-permission-result.json'),
          'a real filesystem permission failure retains the meeting and an enabled retry action')
    locked = read('b-permission-file-final.json')
    check(locked == read('b-permission-file-before.json') and locked['mode'] == 0o400
          and locked['sha256'] == digest(root / 'exports/locked-control.html')
          and (root / 'exports/locked-control.html').read_bytes() == b'PRESERVE-ORIGINAL-EXPORT',
          'the read-only destination retains its original bytes and recorded filesystem permissions')
    retry = read('b-keyboard-retry-action.json')
    retry_dialog = read('b-keyboard-retry-dialog.json')
    check(retry['exit'] == 0 and json.loads(retry['stdout'])['route'] == 'global_input'
          and any(w['pid'] == failed['pid'] and w['title'] == 'Save File' for w in retry_dialog['windows'])
          and read('b-keyboard-retry-result.json')['pid'] == failed['pid'],
          'the visible retry button opens a new native picker and succeeds in the same process')

    cancelled = labels('b-cancel-result.json')
    check(read('b-cancel-files-before.json') == read('b-cancel-files-after.json')
          and 'EXPORT-NATIVE-CONTROL' in cancelled
          and not any(label.startswith('Saved: ') for label in cancelled) and 'Open file' not in cancelled,
          'native cancellation preserves the existing files and meeting without a success message')
    opened = read('b-final-open-browser.json')
    windows = read('b-final-open-windows.json')['windows']
    check(any(w['app_name'] == 'Chromium' and w['pid'] == opened['pid']
              and w['window_id'] == opened['window_id'] and w['is_on_screen']
              and 'Native Export Control' in w['title'] for w in windows)
          and opened['screenshot_frame_valid'] and (root / 'b-final-open-browser.png').stat().st_size > 10_000
          and 'Asked the system to open: ' + run['root'] + '/exports/keyboard-retry.html'
              in labels('b-final-handoff-result.json'),
          'the final native Open file request has a matching real browser window and reviewed screenshot')
    folder = read('b-reveal-folder.json')
    check(any(e.get('value') == run['root'] + '/exports/' for e in folder['elements'])
          and 'native-control.html' in labels('b-reveal-folder.json')
          and any(w['app_name'] == 'Thunar' and w['pid'] == folder['pid'] and w['is_on_screen']
                  for w in read('b-reveal-windows.json')['windows']),
          'the native folder request reaches the correct path and exported filename in a real file manager')
    check(any('The exported file is no longer available' in label for label in labels('b-missing-open-result.json'))
          and 'Open file' in labels('b-missing-open-result.json')
          and any(w['app_name'] == 'Chromium' and 'Native Export Control' in w['title']
                  for w in read('b-restored-open-windows.json')['windows']),
          'a moved export fails visibly and opens again after its original path is restored')

    check(any(entry.get('alreadyWrittenBeforeSaveClick') for entry in exports if entry['run'] == 'before')
          and read('b-retry-early-save.json')['saveClickIssued'] is False
          and run['excludedFromCleanSaveEvidence'],
          'premature filename commits are retained as counterevidence and excluded from clean Save acceptance')
    modal = read('b-untitled-modal.json')
    check('already exists' in modal['tree_markdown'] and 'Replace' in labels('b-untitled-modal.json')
          and '_NET_WM_STATE:  not found.' in read('b-after-open-x11.json')['save']['stdout']
          and read('b-untitled-cancel-action.json')['exit'] == 0
          and 'Export scope' in labels('b-after-modal-cancel.json'),
          'the hidden overwrite prompt is identified and cancelling it restores the same native process')
    for prefix in ['', 'before/']:
        check(any(state.get('state') == 'Z' and state.get('exitCodeField52') == 0 for state in read(prefix + 'app-quit.json'))
              and all(read(prefix + 'cleanup.json')['knownPidsGone'].values())
              and read(prefix + 'cleanup.json')['privateDisplaySocketGone'],
              f'{prefix or "final/"}: the application exits normally and its private display processes are removed')
    check(not read('cleanup.json')['remainingOwnedPids'] and all(not pids for pids in read('owned-process-scan.json').values()),
          'neither retained run has any remaining process with its private data environment')
    report.update(passed=True, bundle=source['bundle'], binarySha256=source['binarySha256'],
                  limitations=run['remaining'], scope='Recorded evidence checks, not UI replay or full C45/G2 completion.')
finally:
    print(json.dumps(report, indent=2, ensure_ascii=False))
