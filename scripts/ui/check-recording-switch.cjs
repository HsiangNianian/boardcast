// Production App with controlled native capture/storage; no actual microphone or meeting directory.
const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkRecordingSwitch(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => {assert.ok(ok, label); report.checks.push(label);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.clear(); localStorage.setItem('boardcast.lang', new URL(location.href).searchParams.get('lang') || 'en'); localStorage.setItem('boardcast.meeting', 'current');
    const make = (id, title) => {
      const graph = {nodes: [{id: 'n1', label: `${title} content`, kind: 'node', aliases: [], marks: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
      return {version: 2, storageRevision: 1, id, title, titled: true, createdAt: 1, updatedAt: 2, graph, history: [], lines: [{id: `${id}-line`, text: `${title} transcript`, speaker: 'A', ms: 1, page: 0}], page: 0, pages: [{graph, steps: [{t: 1, line: 1, d: {nodes: {set: graph.nodes}, nextNode: 2}}]}]};
    };
    const q = window.qaSwitch = {meetings: {current: make('current', 'Current meeting'), target: make('target', 'Target meeting')}, calls: [], events: {}, callbacks: {}, listening: false, saveFail: false, saveHold: false, saveResolvers: [], stopFail: false, stopHold: false, startHold: false, tail: null, acks: []};
    q.originalTarget = JSON.stringify(q.meetings.target);
    q.emit = (event, payload) => (q.events[event] || []).forEach(id => q.callbacks[id]?.({event, id, payload}));
    q.listen = listening => {q.listening = listening; q.emit('voice-status', {installed: true, listening, device: 'Fixture microphone'});};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => {q.events[event] = (q.events[event] || []).filter(value => value !== id);}};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'ready', url: 'http://127.0.0.1:19882', model: 'Fixture', device: 'Fixture', gpus: []};
        if (command === 'remote_prepare') return crypto.randomUUID();
        if (command === 'llm_chat') return {choices: [{message: {content: '-'}}]};
        if (command === 'voice_status') return {installed: true, listening: q.listening};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: q.listening, preparing: false, autoLocales: [], activeLocale: null, apple: null};
        if (command === 'voice_devices') return ['Fixture microphone'];
        if (command === 'voice_start') {if (q.startHold) await new Promise(resolve => {q.releaseStart = resolve;}); q.listen(true); return;}
        if (command === 'voice_stop') {
          if (q.stopFail) throw new Error('Injected stop failure');
          if (q.stopHold) await new Promise(resolve => {q.releaseStop = resolve;});
          if (q.tail) {q.emit('voice-final', {id: 'final-tail', revision: 0, sources: ['mic'], text: q.tail, start: 1, end: 2}); q.tail = null;}
          q.listen(false); return;
        }
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') return args.storageVersion === 2 && q.meetings[args.id] ? JSON.stringify(q.meetings[args.id]) : null;
        if (command === 'meeting_save') {
          if (q.saveFail) throw new Error('Injected disk full');
          if (q.saveHold) await new Promise(resolve => q.saveResolvers.push(resolve));
          if ((q.meetings[args.id]?.storageRevision ?? 0) !== args.expectedRevision) throw new Error('Unexpected stale revision');
          q.meetings[args.id] = JSON.parse(args.json); q.calls.push({command: 'save-complete', id: args.id}); return;
        }
        if (command === 'meetings_list') return args.storageVersion === 2 ? Object.values(q.meetings).map(m => ({id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt, lines: m.lines.length, pages: m.pages?.length || 1})) : [];
        if (command === 'models_list') return {models: [], freeBytes: 1000000};
        if (command === 'plugin:app|version') return '0.21.0-fixture';
        if (command === 'session_wayland') return false;
        if (command === 'shell_resolve_exit') {q.acks.push(args); return true;}
        return null;
      },
    };
  });
  const prompt = () => page.getByRole('alertdialog', {name: 'Stop input before switching?', exact: true});
  const library = () => page.getByRole('dialog', {name: 'Meetings', exact: true});
  const row = () => library().locator('.meeting-list li').filter({has: page.getByText('Target meeting', {exact: true})});
  const currentId = () => page.evaluate(() => localStorage.getItem('boardcast.meeting'));
  const count = command => page.evaluate(command => window.qaSwitch.calls.filter(call => call.command === command).length, command);
  const targetLoads = () => page.evaluate(() => window.qaSwitch.calls.filter(call => call.command === 'meeting_load' && call.args.id === 'target').length);
  const load = async (listening = true, suffix = '') => {
    await page.goto(`${baseUrl}/${suffix}`); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.waitForFunction(() => window.qaSwitch.events['voice-final']?.length && window.qaSwitch.events['shell-exit-request']?.length);
    await page.evaluate(listening => {window.qaSwitch.listen(listening); window.qaSwitch.calls = [];}, listening);
  };
  const openLibrary = () => page.locator('.meeting-library').click();
  const stay = async () => {await prompt().getByRole('button', {name: 'Stay here', exact: true}).click(); await prompt().waitFor({state: 'hidden'});};
  const confirm = () => prompt().getByRole('button', {name: 'Stop and switch', exact: true}).click();
  const newMeeting = () => page.getByRole('button', {name: 'New meeting', exact: true}).click();
  try {
    await fs.mkdir(evidence, {recursive: true});
    await load();
    report.productionScripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
    const draft = page.getByRole('textbox', {name: 'What was said', exact: true});
    await draft.fill('Keep this unsent draft.'); await newMeeting();
    await prompt().waitFor();
    check(await prompt().getByRole('button', {name: 'Stay here', exact: true}).evaluate(button => document.activeElement === button), 'confirmation defaults keyboard focus to staying');
    check((await prompt().innerText()).includes('Current meeting') && await count('voice_stop') === 0, 'new meeting asks before stopping or switching');
    await page.evaluate(() => {
      window.qaSwitch.emit('voice-final', {id: 'while-confirming', revision: 0, text: 'Words while deciding', start: 0, end: 1, sources: ['mic']});
      window.qaSwitch.emit('shell-menu-action', 'new-meeting');
    });
    await page.waitForFunction(() => window.qaSwitch.meetings.current.lines.some(line => line.id === 'while-confirming'));
    check(await prompt().count() === 1 && await count('voice_stop') === 0, 'capture and autosave continue while a single confirmation is open');
    await stay();
    check(await currentId() === 'current' && await draft.inputValue() === 'Keep this unsent draft.' && await page.evaluate(() => window.qaSwitch.listening), 'staying preserves the live meeting, capture and composer draft');
    check(await page.getByRole('button', {name: 'New meeting', exact: true}).evaluate(button => document.activeElement === button), 'cancel restores focus to the new-meeting trigger');
    await page.evaluate(() => window.qaSwitch.emit('shell-menu-action', 'new-meeting'));
    await prompt().waitFor(); await page.keyboard.press('Escape'); await prompt().waitFor({state: 'hidden'});
    check(await currentId() === 'current' && await count('voice_stop') === 0 && await page.getByRole('dialog').count() === 0, 'Escape cancels the native New action without opening preparation');

    await openLibrary(); await row().locator('.open').click(); await stay();
    check(await library().isVisible() && await library().getByRole('alert').count() === 0 && await currentId() === 'current', 'canceling library Open keeps the library without a false failure');
    await page.waitForFunction(() => document.activeElement?.matches('.meeting-list .open'));
    check(await row().locator('.open').evaluate(button => document.activeElement === button), 'cancel restores focus to the library action');
    for (const action of ['Replay', 'Export', 'Continue on it']) {
      await row().getByRole('button', {name: action, exact: true}).click(); await stay();
      check(await currentId() === 'current' && await count('voice_stop') === 0 && await library().getByRole('alert').count() === 0, `library ${action} shares the cancelable recording guard`);
    }
    await page.evaluate(() => {const q = window.qaSwitch; q.stopHold = true; q.saveHold = true; q.tail = 'Final tail before target'; q.calls = [];});
    await row().locator('.open').click();
    await prompt().getByRole('button', {name: 'Stop and switch', exact: true}).evaluate(button => {button.click(); button.click();});
    await page.waitForFunction(() => typeof window.qaSwitch.releaseStop === 'function');
    check(await currentId() === 'current' && await count('voice_stop') === 1 && await targetLoads() === 0, 'duplicate approval stops only once and does not load the target before finalization');
    await page.evaluate(() => {window.qaSwitch.stopHold = false; window.qaSwitch.releaseStop();});
    await page.waitForFunction(() => window.qaSwitch.saveResolvers.length > 0);
    check(await currentId() === 'current' && await targetLoads() === 0, 'switch waits for the final transcript save acknowledgment');
    await page.evaluate(() => {window.qaSwitch.saveHold = false; window.qaSwitch.saveResolvers.splice(0).forEach(resolve => resolve());});
    await page.waitForFunction(() => localStorage.getItem('boardcast.meeting') === 'target'); await library().waitFor({state: 'hidden'});
    check(await page.evaluate(() => {
      const q = window.qaSwitch, calls = q.calls;
      return q.meetings.current.lines.filter(line => line.id === 'final-tail').length === 1 && q.meetings.current.lines.some(line => line.id === 'while-confirming') && !q.meetings.target.lines.some(line => line.id === 'final-tail') && calls.findIndex(call => call.command === 'save-complete' && call.id === 'current') < calls.findIndex(call => call.command === 'meeting_load' && call.args.id === 'target');
    }), 'both the decision-time speech and final tail belong only to the saved original meeting');

    await load(); await page.evaluate(() => {window.qaSwitch.stopFail = true;});
    await openLibrary(); await row().locator('.open').click(); await confirm();
    await library().getByRole('alert').waitFor();
    check(await currentId() === 'current' && await page.evaluate(() => window.qaSwitch.listening) && await targetLoads() === 0, 'stop failure leaves capture and the original meeting intact');
    await page.evaluate(() => {window.qaSwitch.stopFail = false; window.qaSwitch.saveFail = true; window.qaSwitch.tail = 'Retained final words after save failure';});
    await row().locator('.open').click(); await confirm(); await library().getByText(/Injected disk full/).waitFor();
    check(await currentId() === 'current' && !await page.evaluate(() => window.qaSwitch.listening) && await targetLoads() === 0, 'save failure prevents the target load after capture has stopped');
    const stops = await count('voice_stop');
    await page.evaluate(() => {window.qaSwitch.saveFail = false;});
    await row().locator('.open').click(); await library().waitFor({state: 'hidden'});
    check(await currentId() === 'target' && await prompt().count() === 0 && await count('voice_stop') === stops && await page.evaluate(() => window.qaSwitch.meetings.current.lines.some(line => line.text === 'Retained final words after save failure')), 'retry after a failed save preserves the tail and does not ask to stop already-stopped input');

    await load(); await openLibrary();
    await row().getByRole('button', {name: 'Continue on it', exact: true}).click(); await confirm(); await library().waitFor({state: 'hidden'});
    await page.waitForFunction(() => {const q = window.qaSwitch, id = localStorage.getItem('boardcast.meeting'); return id !== 'current' && id !== 'target' && q.meetings[id];});
    check(await page.evaluate(() => {const q = window.qaSwitch, m = q.meetings[localStorage.getItem('boardcast.meeting')]; return JSON.stringify(q.meetings.target) === q.originalTarget && m.pages[0].base.nodes[0].label === 'Target meeting content';}), 'confirmed Continue creates a new meeting and preserves the source board');

    await load();
    const drawing = JSON.stringify({type: 'excalidraw', elements: [{id: 'switch-shape', type: 'rectangle', x: 20, y: 20, width: 120, height: 80, isDeleted: false}], files: {}});
    await page.locator('input[type=file]').setInputFiles({name: 'switch.excalidraw', mimeType: 'application/json', buffer: Buffer.from(drawing)});
    const importer = page.getByRole('dialog', {name: 'Import materials', exact: true});
    await importer.getByRole('button', {name: 'New meeting', exact: true}).waitFor();
    await importer.getByRole('button', {name: 'New meeting', exact: true}).click(); await stay();
    check(await importer.isVisible() && await importer.getByRole('alert').count() === 0 && await count('voice_stop') === 0 && await currentId() === 'current', 'canceling a fresh import keeps the ready preview without a false save error');
    await importer.getByRole('button', {name: 'New meeting', exact: true}).click(); await confirm(); await importer.waitFor({state: 'hidden'});
    await page.waitForFunction(() => localStorage.getItem('boardcast.meeting') !== 'current');
    check(await count('voice_stop') === 1, 'retrying the ready import commits only after explicit stop approval');

    await load(false); await page.evaluate(() => {window.qaSwitch.startHold = true;});
    await page.getByRole('button', {name: 'Start listening', exact: true}).click(); await page.waitForFunction(() => typeof window.qaSwitch.releaseStart === 'function');
    await newMeeting(); await stay();
    check(await count('voice_stop') === 0 && await currentId() === 'current', 'staying while input starts does not cancel the pending start');
    await newMeeting(); await confirm();
    check(await currentId() === 'current' && await count('voice_stop') === 0, 'approved switch waits for a pending input start to finish cancellation');
    await page.evaluate(() => {window.qaSwitch.startHold = false; window.qaSwitch.releaseStart();});
    await page.waitForFunction(() => localStorage.getItem('boardcast.meeting') !== 'current');
    check(await count('voice_start') === 1 && await count('voice_stop') === 1 && !await page.evaluate(() => window.qaSwitch.listening), 'pending start is stopped exactly once before the new meeting');

    await load(); await newMeeting(); await prompt().waitFor();
    await page.evaluate(() => {window.qaSwitch.tail = 'Final words before native close'; window.qaSwitch.emit('shell-exit-request', {id: 77, reason: 'close'});});
    await page.waitForFunction(() => window.qaSwitch.acks.some(ack => ack.id === 77 && ack.allow));
    check(await prompt().count() === 0 && await currentId() === 'current' && await page.evaluate(() => window.qaSwitch.meetings.current.lines.some(line => line.text === 'Final words before native close')), 'native close cancels the pending switch and saves the original before its acknowledgment');

    await load(false); await newMeeting(); await page.waitForFunction(() => localStorage.getItem('boardcast.meeting') !== 'current');
    check(await prompt().count() === 0 && await count('voice_stop') === 0, 'idle new meeting remains a direct action');
    await load(true, '?lang=zh'); await page.setViewportSize({width: 720, height: 480});
    await clickMeetingAction(page, '新会议');
    const chinese = page.getByRole('alertdialog', {name: '停止收音后切换会议？', exact: true}); await chinese.waitFor();
    const button = chinese.getByRole('button', {name: '留在当前', exact: true}); const bounds = await button.boundingBox();
    check(bounds.y >= 0 && bounds.y + bounds.height <= 480 && (await chinese.innerText()).includes('Current meeting'), 'Chinese confirmation remains readable and cancelable at 720 by 480');
    await page.screenshot({path: `${evidence}/recording-switch-zh.png`, animations: 'disabled'}); await button.click();
    check(report.pageErrors.length === 0, 'recording-switch flows have no browser script errors'); report.passed = true;
  } catch (error) {
    report.error = error.stack || String(error);
    report.failureState = await page.evaluate(() => ({id: localStorage.getItem('boardcast.meeting'), listening: window.qaSwitch.listening, pendingSaves: window.qaSwitch.saveResolvers.length, calls: window.qaSwitch.calls.map(call => ({command: call.command, id: call.args?.id ?? call.id}))})).catch(() => null);
    throw error;
  }
  finally {await fs.writeFile(`${evidence}/recording-switch-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
