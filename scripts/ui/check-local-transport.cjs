// Actual production App, blocked WebView fetch, controlled native IPC. Hardware is tested separately.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkLocalTransport(page, baseUrl, evidence) {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => {assert(ok, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.clear(); localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.autoupdate', 'off');
    const mode = new URL(location.href).searchParams.get('mode');
    const q = window.qaLocal = {calls: [], fetches: [], callbacks: {}, events: {}, pending: {}, meetings: {}, next: 0, port: 19888, failure: mode, timeout: []};
    const fetch = window.fetch.bind(window);
    window.fetch = (url, options) => {
      if (/^http:\/\/(127\.0\.0\.1|localhost):1988/.test(String(url))) {
        q.fetches.push(String(url)); return Promise.reject(new TypeError('Failed to fetch'));
      }
      return fetch(url, options);
    };
    const timeout = window.setTimeout.bind(window);
    window.setTimeout = (callback, ms, ...args) => {q.timeout.push(ms); return timeout(callback, mode === 'timeout' && ms === 180000 ? 80 : ms, ...args);};
    Object.defineProperty(AbortSignal, 'any', {value: undefined, configurable: true});
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => q.events[event] = (q.events[event] || []).filter(value => value !== id)};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'ready', url: `http://127.0.0.1:${q.port}`, model: 'Local fixture', device: 'Fixture GPU', gpus: []};
        if (command === 'model_select') {q.port++; q.failure = null; return;}
        if (command === 'remote_prepare') return String(++q.next);
        if (command === 'remote_cancel') {q.pending[args.requestId]?.reject('Request canceled'); delete q.pending[args.requestId]; return;}
        if (command === 'llm_chat') {
          if (q.failure === 'fail') throw 'HTTP 503: synthetic GPU inference error';
          if (q.failure === 'timeout' || q.failure === 'hold') return new Promise((resolve, reject) => q.pending[args.requestId] = {resolve, reject});
          return {choices: [{message: {content: 'N 「Native transport」'}}]};
        }
        if (command === 'voice_status') return {installed: true, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null};
        if (command === 'voice_devices') return [];
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') return null;
        if (command === 'meeting_save') {q.meetings[args.id] = JSON.parse(args.json); return;}
        if (command === 'meetings_list') return [];
        if (command === 'models_list') return {models: [{id:'local', label:'Local fixture', note:'Synthetic', kind:'llm', selected:true, installed:true, size:1, partial:0, recommended:true}], freeBytes:1e10};
        if (command === 'plugin:app|version') return '0.21.0-test';
        if (command === 'session_wayland') return false;
        return null;
      },
    };
  });
  const start = async mode => {
    await page.goto(`${baseUrl}/?mode=${mode}`);
    await page.locator('.splash').waitFor({state: 'hidden'});
  };
  await start('ready');
  await page.waitForFunction(() => window.qaLocal.calls.some(call => call.command === 'llm_chat'), null, {timeout: 5000});
  await page.waitForFunction(() => !document.querySelector('.chip.status')?.textContent.includes('warming'));
  check(await page.evaluate(() => window.qaLocal.fetches.length === 0), 'desktop warm-up and status do not use WebView HTTP');
  check(await page.evaluate(() => AbortSignal.any === undefined), 'warm-up does not require AbortSignal.any');
  await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('A synthetic transport check.');
  await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
  await page.waitForFunction(() => Object.values(window.qaLocal.meetings).some(meeting => meeting.graph.nodes.some(node => node.label === 'Native transport')));
  check(await page.evaluate(() => window.qaLocal.calls.filter(call => call.command === 'llm_chat').length === 2 && window.qaLocal.fetches.length === 0), 'a real App sentence generates and saves through native IPC while fetch is blocked');

  for (const mode of ['fail', 'timeout']) {
    await start(mode);
    await page.getByText(/The built-in model warm-up failed/).first().waitFor();
    check(await page.evaluate(() => window.qaLocal.fetches.length === 0), `${mode}: warm-up failure is shown without a browser retry`);
    if (mode === 'timeout') {
      await page.waitForFunction(() => Object.keys(window.qaLocal.pending).length === 0);
      check(await page.evaluate(() => window.qaLocal.timeout.includes(180000) && window.qaLocal.calls.some(call => call.command === 'remote_cancel')), 'the 180-second warm-up timeout cancels the native request');
    }
    await page.getByText(/The built-in model warm-up failed/).first().click();
    const dialog = page.getByRole('dialog', {name: 'Local models', exact: true});
    await dialog.waitFor();
    await dialog.getByRole('button', {name: 'Retry starting', exact: true}).click();
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => window.qaLocal.calls.filter(call => call.command === 'llm_chat').length >= 2);
    await page.waitForFunction(() => !document.body.textContent.includes('The built-in model warm-up failed') && !document.body.textContent.includes('warming up'));
    check(true, `${mode}: Manage models restart clears the error and warms the new server`);
  }

  await start('hold');
  await page.waitForFunction(() => Object.keys(window.qaLocal.pending).length === 1);
  await page.evaluate(() => { window.qaLocal.port++; window.qaLocal.failure = null; });
  await page.waitForFunction(() => Object.keys(window.qaLocal.pending).length === 0 && window.qaLocal.calls.filter(call => call.command === 'llm_chat').length === 2);
  check(await page.evaluate(() => window.qaLocal.calls.some(call => call.command === 'remote_cancel' && call.args.requestId === '1')), 'switching the server cancels the previous warm-up and starts a new one');
  check(report.pageErrors.length === 0, 'no uncaught browser errors');
  await fs.mkdir(evidence, {recursive: true});
  await page.screenshot({path: `${evidence}/local-transport.png`});
  await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2));
};
