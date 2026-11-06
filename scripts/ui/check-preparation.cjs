// Actual Preparation component, with native capability results matching the production protocol.
const assert = require('node:assert/strict');
module.exports = async function checkPreparation(page, baseUrl) {
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    window.qa = {
      calls: [], startSuccess: false, startError: null, waitStart: false, readError: null,
      status: {installed: true, listening: false}, devices: ['Fixture microphone'],
      models: {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null, appleError: null},
    };
    let callbackId = 0;
    const callbacks = new Map(), listeners = new Map();
    window.qa.emit = (event, payload) => callbacks.get(listeners.get(event))?.({event, payload});
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: event => listeners.delete(event)};
    if (!new URL(location.href).searchParams.has('web')) window.__TAURI_INTERNALS__ = {
      transformCallback: callback => { callbacks.set(++callbackId, callback); return callbackId; },
      unregisterCallback: id => callbacks.delete(id),
      invoke: async (command, args) => {
        window.qa.calls.push(command);
        if (command === 'voice_models' && window.qa.readError) throw new Error(window.qa.readError);
        if (command === 'voice_models') return structuredClone(window.qa.models);
        if (command === 'voice_status') return structuredClone(window.qa.status);
        if (command === 'voice_devices') return structuredClone(window.qa.devices);
        if (command === 'plugin:event|listen') { listeners.set(args.event, args.handler); return args.handler; }
        return null;
      },
    };
  });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${baseUrl}/scripts/ui/fixtures/preparation.html`);
  const open = () => page.getByRole('button', {name: 'Open preparation', exact: true}).click();
  const panel = page.getByRole('dialog', {name: 'Prepare to start', exact: true});
  const refresh = async () => {
    await panel.getByRole('button', {name: 'Check again', exact: true}).click();
    await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'Check again' && !button.disabled));
  };
  const readOnly = async () => assert.equal(await page.evaluate(() => window.qa.calls.some(command => ['voice_start', 'voice_prepare', 'voice_select', 'model_download'].includes(command))), false);
  await open();
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'Start listening' && !button.disabled));
  await readOnly();
  assert.equal(await page.evaluate(() => window.qa.calls.includes('start')), false, 'opening and checking must not start capture');
  await panel.getByRole('button', {name: 'Start listening', exact: true}).click();
  await panel.getByRole('alert').filter({hasText: 'Listening did not start'}).waitFor();
  assert.equal(await panel.count(), 1);
  await page.evaluate(() => { window.qa.startSuccess = true; });
  await panel.getByRole('button', {name: 'Start listening', exact: true}).click();
  await panel.waitFor({state: 'detached'});

  await open();
  await page.evaluate(() => window.qa.setBoardReady(false));
  await refresh();
  await panel.getByText('Board generation is not ready.', {exact: false}).waitFor();
  await panel.getByRole('button', {name: 'Record transcript first', exact: true}).click();
  await panel.waitFor({state: 'detached'});
  await open();
  await page.evaluate(() => { window.qa.models.senseVoiceInstalled = false; window.qa.status.installed = false; });
  await refresh();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true);
  await panel.getByRole('button', {name: 'Prepare transcription', exact: true}).click();
  await page.getByRole('dialog', {name: 'Speech settings', exact: true}).waitFor();
  await page.keyboard.press('Escape');
  assert.equal(await panel.count(), 1, 'speech settings returns to preparation');

  await page.evaluate(() => {
    window.qa.status.installed = true;
    window.qa.models.selection = {provider: 'apple', locale: 'en-US'};
    window.qa.models.apple = {isApple: true, available: false, reason: 'os', defaultLocale: 'en-US', installedLocales: [], osVersion: '25', locales: []};
  });
  await refresh();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true, 'Apple installed flag alone is not readiness');
  await page.evaluate(() => {
    window.qa.models.apple.available = true;
    window.qa.models.apple.reason = null;
    window.qa.models.apple.locales = [{id: 'en-US', name: 'English', status: 'supported'}];
  });
  await refresh();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true, 'supported Apple language still needs assets');
  await page.evaluate(() => {
    window.qa.models.apple.locales[0].status = 'installed';
    window.qa.models.apple.installedLocales = ['en-US'];
    window.qa.emit('voice-models-changed');
  });
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'Record transcript first' && !button.disabled));
  await page.evaluate(() => {
    window.qa.models.selection.locale = 'auto';
    window.qa.models.autoLocales = ['en-US', 'zh-CN', 'ja-JP'];
    window.qa.models.apple.locales = window.qa.models.autoLocales.map(id => ({id, name: id, status: 'installed'}));
  });
  await refresh();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true, 'Apple Auto also needs SenseVoice detection');
  await page.evaluate(() => { window.qa.models.senseVoiceInstalled = true; });
  await refresh();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isEnabled(), true);

  await page.evaluate(() => { window.qa.readError = 'Capability unavailable'; });
  await refresh();
  await panel.getByRole('alert').filter({hasText: 'Capability unavailable'}).waitFor();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true);
  await page.evaluate(() => { window.qa.readError = null; window.qa.setMic('Unplugged microphone'); });
  await refresh();
  await panel.getByText('This microphone is no longer listed.', {exact: false}).waitFor();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isDisabled(), true);
  await panel.getByRole('combobox', {name: 'What to listen to', exact: true}).selectOption('system');
  assert.equal(await panel.getByRole('combobox', {name: 'Microphone', exact: true}).count(), 0);
  await panel.getByText('Input and permissions', {exact: true}).click();
  await panel.getByText('System audio may request', {exact: false}).waitFor();
  assert.equal(await panel.getByRole('button', {name: 'Record transcript first', exact: true}).isEnabled(), true);
  await page.evaluate(() => { window.qa.waitStart = true; });
  await panel.getByRole('button', {name: 'Record transcript first', exact: true}).click();
  await page.keyboard.press('Escape');
  assert.equal(await panel.count(), 1);
  await panel.getByRole('button', {name: 'Cancel starting audio', exact: true}).click();
  await panel.getByRole('button', {name: 'Record transcript first', exact: true}).waitFor();
  assert.equal(await panel.count(), 1, 'canceled start keeps preparation open');
  await readOnly();
  await page.setViewportSize({width: 720, height: 480});
  const startBounds = await panel.getByRole('button', {name: 'Record transcript first', exact: true}).boundingBox();
  assert(startBounds && startBounds.y >= 0 && startBounds.y + startBounds.height <= 480, 'start action remains visible while the checklist scrolls');
  await panel.getByRole('button', {name: 'Type first', exact: true}).click();
  await panel.waitFor({state: 'detached'});
  await page.waitForFunction(() => window.qa.calls.includes('type'));

  await page.goto(`${baseUrl}/scripts/ui/fixtures/preparation.html?web=1`);
  await page.setViewportSize({width: 480, height: 640});
  await open();
  await panel.getByText('You are using the browser version.', {exact: false}).waitFor();
  assert.equal(await panel.getByRole('button', {name: 'Start listening', exact: true}).count(), 0);
  assert.deepEqual(await page.evaluate(() => window.qa.calls), [], 'browser preparation must not invoke native APIs');
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await panel.getByRole('button', {name: 'Type first', exact: true}).click();
  await panel.waitFor({state: 'detached'});
  await page.waitForFunction(() => window.qa.calls.includes('type'));
  assert.deepEqual(errors, []);
  console.log('PASS: explicit start only, failure retry/cancel, transcript without board, missing speech/input, Apple capability/assets/Auto, capability refresh, browser typing, 480px layout.');
};
