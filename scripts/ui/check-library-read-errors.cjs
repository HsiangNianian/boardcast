const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkLibraryReadErrors(page, baseUrl, evidence) {
  const report = {checks: [], pageErrors: []};
  const check = (value, label) => {assert(value, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await fs.mkdir(evidence, {recursive: true});
  await page.addInitScript(() => {
    localStorage.clear(); localStorage.setItem('boardcast.autoupdate', 'off');
    const mode = new URL(location.href).searchParams.get('mode');
    localStorage.setItem('boardcast.lang', mode === 'zh' ? 'zh' : 'en');
    const make = (id, title, version = 2) => ({version, id, title, titled: true, createdAt: 1, updatedAt: 2, history: [], lines: [{text: `${title} transcript`, ms: 1}], graph: {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1}});
    const q = window.qaLibrary = {calls: [], callbacks: {}, failure: mode === 'directory', records: {
      1: {'bad-v1': '{truncated', 'current-good': '{truncated', rescue: make('rescue', 'Recoverable older meeting', 1)},
      2: {'bad-v2': '{truncated', rescue: '{truncated', 'current-good': make('current-good', 'Healthy current record'), good: make('good', 'Healthy meeting')},
    }};
    if (mode === 'empty' || mode === 'directory') q.records = {1: {}, 2: {}};
    if (mode === 'all-bad') q.records = {1: {}, 2: {'bad-v2': '{truncated'}};
    q.original = structuredClone(q.records);
    q.make = make;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: () => {}};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') return args.handler;
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'no-model', gpus: []};
        if (command === 'voice_status') return {installed: false, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: false, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null};
        if (command === 'voice_devices') return [];
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') {
          const value = q.records[args.storageVersion][args.id];
          return value === undefined ? null : typeof value === 'string' ? value : JSON.stringify(value);
        }
        if (command === 'meeting_save') {
          if (q.failWrites) throw 'Synthetic recovery write failure';
          const previous = q.records[args.storageVersion][args.id];
          if (typeof previous === 'string') throw 'Cannot overwrite a damaged record';
          if ((previous?.storageRevision || 0) !== args.expectedRevision) throw 'Storage revision mismatch';
          q.records[args.storageVersion][args.id] = JSON.parse(args.json); return;
        }
        if (command === 'meetings_list') {
          if (q.failure) throw 'Synthetic meeting directory read failure';
          return Object.entries(q.records[args.storageVersion]).map(([id, value]) => typeof value === 'string'
            ? {id, title: `${id}.json`, createdAt: 0, updatedAt: 0, lines: 0, pages: 0, readError: `Could not read meeting /synthetic/${'long-folder-name-'.repeat(12)}/v${args.storageVersion}/${id}.json: unexpected EOF`}
            : {id, title: value.title, createdAt: value.createdAt, updatedAt: value.updatedAt, lines: value.lines.length, pages: 1});
        }
        if (command === 'plugin:app|version') return '0.21.0-test';
        if (command === 'session_wayland') return false;
        return null;
      },
    };
  });
  const library = page.getByRole('dialog', {name: 'Meetings', exact: true});
  const warning = library.locator('.meeting-read-errors');
  const rows = library.locator('.meeting-list > li');
  const open = async mode => {
    await page.goto(`${baseUrl}/?mode=${mode}`);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.meeting-library').click();
  };
  const search = async text => {
    await library.getByRole('searchbox').fill(text);
    if (text.trim()) await library.getByText('Searching…', {exact: true}).waitFor();
    await library.getByText('Searching…', {exact: true}).waitFor({state: 'hidden'});
  };
  await open('mixed');
  await warning.getByRole('alert').getByText('4 meeting files could not be read.', {exact: true}).waitFor();
  check(await rows.count() === 3 && await warning.locator('details').getAttribute('open') === null, 'four unreadable files are reported while three healthy meetings remain usable');
  check(await rows.filter({hasText: /bad-v[12]/}).count() === 0, 'unreadable files have no open, rename or delete actions');
  await warning.locator('summary').focus(); await page.keyboard.press('Enter');
  check(await warning.locator('details[open] li').count() === 4, 'keyboard opens the per-file read errors');
  await page.setViewportSize({width: 720, height: 600});
  await page.evaluate(() => document.documentElement.style.zoom = '2');
  check(await library.evaluate(element => element.scrollWidth <= element.clientWidth + 1 && element.getBoundingClientRect().right <= innerWidth + 1), 'long error paths wrap within the dialog at 720 pixels and 200 percent zoom');
  await page.screenshot({path: `${evidence}/narrow-errors.png`});
  await page.evaluate(() => document.documentElement.style.zoom = '');
  await page.setViewportSize({width: 1280, height: 800});
  await warning.locator('summary').click();
  await search('Healthy current');
  check(await rows.count() === 1 && await warning.isVisible(), 'search finds the healthy current version without reading its broken older file');
  await search('missing phrase');
  await library.getByText('Nothing found.', {exact: true}).waitFor();
  check(await warning.isVisible(), 'no search matches remains distinct from unreadable files');
  await search('Recoverable older');
  await page.evaluate(() => window.qaLibrary.failWrites = true);
  await rows.locator('.open').click();
  await library.getByRole('alert').getByText('Synthetic recovery write failure', {exact: true}).waitFor();
  check(await page.evaluate(() => JSON.stringify(window.qaLibrary.records) === JSON.stringify(window.qaLibrary.original)) && await library.isVisible(), 'a failed recovery save keeps the library open and preserves all originals');
  await page.evaluate(() => window.qaLibrary.failWrites = false);
  await rows.locator('.open').click();
  await library.waitFor({state: 'hidden'});
  await page.waitForFunction(() => Object.values(window.qaLibrary.records[2]).some(value => typeof value === 'object' && value.title.includes('older version copy')));
  check(await page.evaluate(() => {
    const q = window.qaLibrary;
    return [1, 2].every(version => Object.entries(q.original[version]).every(([id, value]) => JSON.stringify(q.records[version][id]) === JSON.stringify(value))) && Object.keys(q.records[2]).some(id => !Object.hasOwn(q.original[2], id));
  }), 'opening the older version saves a new meeting and preserves every original record');
  await page.locator('.meeting-library').click();
  await warning.waitFor();
  await search('');
  await page.evaluate(() => { const q = window.qaLibrary; q.records[2]['bad-v2'] = q.make('bad-v2', 'Restored meeting'); });
  await warning.getByRole('button', {name: 'Reload list', exact: true}).click();
  await warning.getByText('3 meeting files could not be read.', {exact: true}).waitFor();
  await rows.filter({hasText: 'Restored meeting'}).waitFor();
  check(true, 'Reload list exposes a restored file and updates the remaining failures');
  await page.screenshot({path: `${evidence}/recovered-library.png`});

  await open('all-bad');
  await warning.getByText('1 meeting file could not be read.', {exact: true}).waitFor();
  check(await rows.count() === 0 && await library.getByText(/^No saved meetings yet/).count() === 0 && await library.getByText('Nothing found.', {exact: true}).count() === 0, 'a library containing only broken files is not described as empty');
  await open('directory');
  await library.getByRole('alert').getByText('Synthetic meeting directory read failure', {exact: true}).waitFor();
  check(await library.getByText(/^No saved meetings yet/).count() === 0, 'a directory read failure is not described as an empty library');
  await page.evaluate(() => window.qaLibrary.failure = false);
  await library.getByRole('button', {name: 'Reload list', exact: true}).click();
  await library.getByText(/^No saved meetings yet/).waitFor();
  check(await library.getByRole('alert').count() === 0, 'a successful retry clears the directory error and shows the true empty state');
  await open('zh');
  const chinese = page.getByRole('dialog', {name: '会议', exact: true});
  await chinese.getByRole('alert').getByText('4 个会议文件读取失败。', {exact: true}).waitFor();
  check(await chinese.getByText('查看读取失败的文件', {exact: true}).count() === 1 && await chinese.getByRole('button', {name: '重新读取列表', exact: true}).count() === 1, 'Chinese shows the warning, details and retry action');
  check(report.pageErrors.length === 0, 'no uncaught browser errors');
  await page.screenshot({path: `${evidence}/chinese-errors.png`});
  await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2));
};
