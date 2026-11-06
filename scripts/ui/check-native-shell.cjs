// Actual App with a controlled native bridge. No real files, microphones or app exits are used.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkNativeShell(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, {recursive: true});
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.hotkey', 'Ctrl+Shift+Space');
    localStorage.setItem('boardcast.meeting', 'native-fixture-meeting');
    const params = new URL(location.href).searchParams;
    if (params.has('updateChecks')) {
      localStorage.setItem('boardcast.lang', params.get('lang') || 'en');
      localStorage.setItem('boardcast.autoupdate', params.has('auto') ? 'on' : 'off');
    }
    const startup = params.has('nativeStartup');
    const toggleStartup = params.has('nativeToggle');
    const holdLoad = startup || toggleStartup || params.has('earlyToggle');
    const seed = {version: 2, id: 'native-fixture-meeting', title: 'Native fixture', createdAt: 1, updatedAt: 1, storageRevision: 1,
      graph: {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1}, history: [], lines: holdLoad ? [{text: 'Previously saved meeting', ms: 0}] : []};
    if (params.has('libraryArchive')) {
      localStorage.removeItem('boardcast.meeting');
      seed.titled = true;
      seed.lines = [{text: 'Saved archive transcript', ms: 1}];
      if (params.get('archiveContext') === 'separate') seed.history = [{id: 'old-context', text: 'Prior model-only context', speaker: 'Alice'}];
      seed.graph.nodes = [{id: 'n1', kind: 'node', label: 'Saved archive board', ink: 'ink', aliases: [], marks: []}];
      seed.graph.nextNode = 2;
      seed.pages = [{graph: seed.graph}, {graph: {...seed.graph, nodes: [{...seed.graph.nodes[0], label: 'Archived second board'}]}}];
      seed.page = 0;
    }
    const q = window.qaShell = {calls: [], acks: [], callbacks: {}, events: {}, meetings: {[seed.id]: seed}, listening: false, installed: true,
      shortcut: null, shortcutHold: false, shortcutResolvers: [], shortcutReject: new URL(location.href).searchParams.has('shortcutStartup') ? 'Ctrl+Shift+Space' : undefined,
      togglePending: toggleStartup, saveFail: false, saveHold: false, saveResolvers: [], finalOnStop: null, stopFail: false, next: 100, pending: startup ? {id: 77, reason: 'quit'} : null};
    q.emit = (name, payload) => {
      if (name === 'mic-toggle' && !q.pending) q.togglePending = !q.togglePending;
      if (name === 'shell-exit-request') q.togglePending = false;
      [...(q.events[name] || [])].forEach(id => q.callbacks[id]?.({event: name, id, payload}));
    };
    q.requestExit = reason => {
      if (q.pending) return q.pending.id;
      q.pending = {id: ++q.next, reason};
      q.emit('shell-exit-request', {...q.pending});
      return q.pending.id;
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => {q.events[event] = (q.events[event] || []).filter(value => value !== id);}};
    let callbackId = 0, loadHeld = false;
    window.__TAURI_INTERNALS__ = {
      transformCallback: callback => {q.callbacks[++callbackId] = callback; return callbackId;},
      unregisterCallback: id => {delete q.callbacks[id];}, convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {
          (q.events[args.event] ||= []).push(args.handler);
          if (args.event === 'mic-toggle' && params.has('listenToggle')) q.emit('mic-toggle');
          return args.handler;
        }
        if (command === 'plugin:event|unlisten') return;
        if (command === 'shell_take_toggle') {
          const pending = q.togglePending; q.togglePending = false;
          return pending && !q.pending;
        }
        if (command === 'shortcut_set') {
          if (q.shortcutHold) await new Promise(resolve => q.shortcutResolvers.push({accel: args.accel, finish: resolve}));
          if (q.shortcutReject === args.accel) throw new Error('Shortcut is occupied by another application');
          q.shortcut = args.accel; return;
        }
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'ready', url: 'http://127.0.0.1:19888', model: 'Fixture model', device: 'Fixture', gpus: []};
        if (command === 'llm_chat') return {choices: [{message: {content: 'N 「Native undo board」'}}]};
        if (command === 'voice_status') return {installed: q.installed, listening: q.listening};
        if (command === 'voice_start') {
          if (q.startFail) throw new Error('Injected capture start failure');
          q.listening = true; q.emit('voice-status', {installed: true, listening: true}); return;
        }
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: q.listening, preparing: false, autoLocales: [], activeLocale: null, apple: null, appleError: null};
        if (command === 'voice_devices') return ['Fixture microphone'];
        if (command === 'voice_input_check_start') {
          q.inputPreview = args.id;
          q.emit('voice-input-check', {id: args.id, state: 'active', device: 'Synthetic native input', level: 0.2}); return;
        }
        if (command === 'voice_input_check_stop') {
          if (q.inputPreview && (!args.id || args.id === q.inputPreview)) {
            q.emit('voice-input-check', {id: q.inputPreview, state: 'stopped', reason: 'cancelled'});
            q.inputPreview = null;
          }
          return;
        }
        if (command === 'voice_stop') {
          if (q.stopFail) throw new Error('Injected stop failure');
          if (q.finalOnStop) {
            q.emit('voice-final', {id: q.finalId || 'final-before-exit', revision: 0, sources: ['mic'], text: q.finalOnStop, start: 0, end: 1});
            q.finalOnStop = null;
          }
          q.listening = false; q.emit('voice-status', {installed: true, listening: false}); return;
        }
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'remote_prepare') return String(++q.next);
        if (command === 'remote_cancel') { (q.canceledRequests ||= []).push(args.requestId); return; }
        if (command === 'arxiv_pdf') return new Promise((resolve, reject) => {
          // Deliberately retain canceled replies to exercise the App's late-result guard.
          (q.arxivRequests ||= []).push({args, resolve, reject});
        });
        if (command === 'meeting_load') {
          if (params.has('slowArchive')) await new Promise(resolve => setTimeout(resolve, 20));
          if (holdLoad && !loadHeld && args.storageVersion === 2) {
            loadHeld = true;
            await new Promise(resolve => {q.finishLoad = resolve;});
            q.calls.push({command: 'load-complete'});
          }
          return args.storageVersion === 2 && q.meetings[args.id] ? JSON.stringify(q.meetings[args.id]) : null;
        }
        if (command === 'meeting_save') {
          if (q.saveFail) {q.calls.push({command: 'save-failed'}); throw new Error('Injected disk full');}
          if (q.saveHold) await new Promise(resolve => q.saveResolvers.push(resolve));
          const existing = q.meetings[args.id];
          if ((existing?.storageRevision ?? 0) !== args.expectedRevision) throw new Error('Injected revision conflict');
          q.meetings[args.id] = JSON.parse(args.json);
          q.calls.push({command: 'save-complete', id: args.id}); return;
        }
        if (command === 'meetings_list') return args.storageVersion === 2 ? Object.values(q.meetings).map(m => ({id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt, lines: m.lines.length})) : [];
        if (command === 'shell_pending_exit') return q.pending ? {...q.pending} : null;
        if (command === 'shell_resolve_exit') {
          q.acks.push({...args, transition: !!document.querySelector('.transition-state')});
          if (q.pending?.id !== args.id) return false;
          q.pending = null; return true;
        }
        if (command === 'plugin:app|version') return '0.21.0-fixture';
        if (command === 'update_check') {
          (q.updateTimes ||= []).push(performance.now());
          if (q.updateError || params.has('auto')) throw q.updateError || 'UPDATE_CHECK_TIMEOUT';
          return q.updateInfo || null;
        }
        if (command === 'models_list') return {models: q.models || [], freeBytes: 10000000000};
        if (command === 'model_delete') {q.models.find(model => model.id === args.id).installed = false; return;}
        if (command === 'session_wayland') return false;
        if (command === 'diagnostics_info') return {os: 'fixture', arch: 'fixture', log: null};
        return null;
      },
    };
  });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const emit = (event, payload) => page.evaluate(({event, payload}) => window.qaShell.emit(event, payload), {event, payload});
  const menu = action => emit('shell-menu-action', action);
  const current = () => page.evaluate(() => window.qaShell.meetings[localStorage.getItem('boardcast.meeting')]);
  const waitSaved = () => page.locator('.save-state.saved').waitFor();
  const waitGraph = count => page.waitForFunction(count => window.qaShell.meetings[localStorage.getItem('boardcast.meeting')]?.graph.nodes.length === count, count);
  const write = async text => {
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text);
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
  };
  const report = {archiveOpen: [], startupPending: false, failedSaveBlocksExit: false, finalLineSavedBeforeAck: false, editHistory: {}, pageErrors: errors};
  try {
    await page.goto(`${baseUrl}/?titleChecks=1`);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.meeting-title').click();
    const titleDialog = page.getByRole('dialog', {name: 'Rename meeting', exact: true});
    const titleInput = titleDialog.getByRole('textbox', {name: 'Meeting title', exact: true});
    await titleInput.fill('Native title before speaking');
    await page.evaluate(() => {window.qaShell.saveHold = true; window.qaShell.calls = [];});
    await titleInput.press('Enter');
    await page.waitForFunction(() => window.qaShell.saveResolvers.length === 1);
    assert(await titleDialog.getByRole('button', {name: 'Save title', exact: true}).isDisabled());
    assert(await titleDialog.getByRole('button', {name: 'Cancel', exact: true}).isDisabled());
    await page.keyboard.press('Enter'); await page.keyboard.press('Escape'); await menu('new-meeting');
    assert(await titleDialog.isVisible(), 'saving keeps the title dialog open until native persistence completes');
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'meeting_save').length), 1);
    await page.evaluate(() => {window.qaShell.saveHold = false; window.qaShell.saveResolvers.splice(0).forEach(resolve => resolve());});
    await titleDialog.waitFor({state: 'hidden'}); await waitSaved();
    assert.equal((await current()).title, 'Native title before speaking');
    assert.equal((await current()).lines.length, 0);
    assert(await page.locator('.meeting-title').evaluate(element => element === document.activeElement));
    report.titleSaveBarrier = true;
    report.arxivChecks = [];
    await page.goto(`${baseUrl}/?arxivChecks=1`);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await write('Retained while importing a paper');
    await waitGraph(1); await waitSaved();
    const originalImportMeeting = await current();
    const importDialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
    const importTrigger = page.getByRole('button', {name: 'Import', exact: true}).first();
    await importTrigger.focus(); await page.keyboard.press('Enter');
    await page.getByPlaceholder('arxiv.org/abs/… or 2401.12345').fill('not-an-id');
    await page.getByPlaceholder('arxiv.org/abs/… or 2401.12345').press('Enter');
    await importDialog.getByRole('alert').waitFor();
    await page.keyboard.press('Escape'); await importDialog.waitFor({state: 'hidden'});
    assert(await importTrigger.evaluate(element => element === document.activeElement), 'canceling an invalid keyboard import must return focus to Import');
    assert.deepEqual(await current(), originalImportMeeting);
    assert.equal(await page.evaluate(() => window.qaShell.arxivRequests?.length ?? 0), 0);
    report.arxivChecks.push('Invalid keyboard import returns focus to Import without a request or saved changes');
    const startArxiv = async () => {
      await importTrigger.click();
      await page.getByPlaceholder('arxiv.org/abs/… or 2401.12345').fill('1706.03762v7');
      await page.getByPlaceholder('arxiv.org/abs/… or 2401.12345').press('Enter');
      await importDialog.waitFor();
    };
    await startArxiv();
    await page.waitForFunction(() => window.qaShell.arxivRequests?.length === 1);
    await importDialog.getByRole('button', {name: 'Cancel import', exact: true}).click();
    await importDialog.waitFor({state: 'hidden'});
    await page.waitForFunction(() => window.qaShell.canceledRequests?.includes(window.qaShell.arxivRequests[0].args.requestId));
    assert(await importTrigger.evaluate(element => element === document.activeElement), 'canceling a download must restore focus to Import');
    assert.deepEqual(await current(), originalImportMeeting);
    report.arxivChecks.push('Cancel forwards the matching native request ID and retains the meeting');
    await startArxiv();
    await page.waitForFunction(() => window.qaShell.arxivRequests?.length === 2);
    await page.evaluate(() => window.qaShell.arxivRequests[0].reject('late canceled failure'));
    assert.match(await importDialog.textContent(), /Downloading 1706\.03762v7/);
    assert.equal(await importDialog.getByRole('alert').count(), 0);
    report.arxivChecks.push('Late canceled failure cannot replace the next import');
    await page.evaluate(() => window.qaShell.arxivRequests[1].reject('Download failed: held connection closed'));
    await importDialog.getByRole('alert').waitFor();
    await importDialog.getByRole('button', {name: 'Try importing again', exact: true}).click();
    await page.waitForFunction(() => window.qaShell.arxivRequests?.length === 3);
    const requests = await page.evaluate(() => window.qaShell.arxivRequests.map(request => request.args));
    assert.deepEqual(requests.map(request => request.id), ['1706.03762v7', '1706.03762v7', '1706.03762v7']);
    assert.equal(new Set(requests.map(request => request.requestId)).size, 3);
    report.arxivChecks.push('Retry retains the paper and registers a fresh request');
    const pdf = require('./check-import-task.cjs').scannedPdf(1).toString('base64');
    await page.evaluate(pdf => window.qaShell.arxivRequests[2].resolve(pdf), pdf);
    await importDialog.getByText('1 board is ready. Choose where to add them.', {exact: true}).waitFor();
    assert.deepEqual(await current(), originalImportMeeting);
    await importDialog.getByRole('button', {name: 'Cancel import', exact: true}).click();
    await importDialog.waitFor({state: 'hidden'});
    assert.deepEqual(await current(), originalImportMeeting);
    assert(await importTrigger.evaluate(element => element === document.activeElement), 'canceling a preview must restore focus to Import');
    report.arxivChecks.push('Recovered native PDF is parsed for preview and canceled without writing');
    report.updateChecks = [];
    for (const lang of ['en', 'zh']) {
      const zh = lang === 'zh';
      await page.goto(`${baseUrl}/?updateChecks=1&lang=${lang}`);
      await page.locator('.splash').waitFor({state: 'hidden'});
      const original = await current();
      await page.getByRole('button', {name: zh ? '设置' : 'Settings', exact: true}).click();
      const settings = page.getByRole('dialog', {name: zh ? '设置' : 'Settings', exact: true});
      await settings.getByRole('tab', {name: zh ? '维护与高级' : 'Maintenance & advanced', exact: true}).click();
      const check = settings.getByRole('button', {name: zh ? '检查更新' : 'Check for updates', exact: true});
      await page.evaluate(() => {window.qaShell.updateError = 'UPDATE_CHECK_TIMEOUT';});
      await check.click();
      await settings.getByText(zh ? /检查更新超时.*重试/ : /Checking for updates timed out.*try again/).waitFor();
      assert(await check.isEnabled());
      await page.evaluate(() => {window.qaShell.updateError = null;});
      await check.click();
      await settings.getByText(zh ? '已经是最新版' : 'You are on the latest version', {exact: true}).waitFor();
      await page.evaluate(() => {window.qaShell.updateInfo = {version: '9.0.0-fixture', current: '0.21.0', notes: 'Fixture release notes'};});
      await check.click();
      const update = page.getByRole('dialog', {name: zh ? '有新版本' : 'New version available', exact: true});
      await update.getByText('Fixture release notes', {exact: true}).waitFor();
      await update.getByRole('button', {name: zh ? '以后再说' : 'Later', exact: true}).click();
      await page.evaluate(() => {window.qaShell.updateError = 'UPDATE_CHECK_TIMEOUT';});
      await menu('check-updates');
      await page.getByText(zh ? /检查更新超时.*重试/ : /Checking for updates timed out.*try again/).waitFor();
      await page.evaluate(() => {window.qaShell.updateError = null;});
      await menu('check-updates');
      await update.getByText('Fixture release notes', {exact: true}).waitFor();
      await update.getByRole('button', {name: zh ? '以后再说' : 'Later', exact: true}).click();
      const calls = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
      assert.equal(calls.filter(command => command === 'update_check').length, 5);
      assert(!calls.some(command => ['update_download', 'update_install', 'voice_start', 'voice_stop'].includes(command)));
      assert.deepEqual(await current(), original, 'checking and postponing updates preserve the meeting');
      report.updateChecks.push({lang, timeoutRetry: true, latest: true, notesAndLater: true, nativeMenuRetry: true, meetingPreserved: true});
    }
    await page.goto(`${baseUrl}/?updateChecks=1&auto=1`);
    await page.waitForFunction(() => window.qaShell.updateTimes?.length === 1, null, {timeout: 15000});
    assert((await page.evaluate(() => window.qaShell.updateTimes[0])) >= 8000, 'automatic checks wait until after startup');
    assert.equal(await page.getByText(/Checking for updates timed out/).count(), 0, 'automatic failures stay quiet');
    assert.equal(await page.getByRole('dialog', {name: 'New version available', exact: true}).count(), 0);
    report.updateChecks.push({automaticAfterStartup: true, failureQuiet: true});

    for (const context of ['empty', 'separate']) for (const slow of [false, true]) {
      await page.goto(`${baseUrl}/?libraryArchive=1&archiveContext=${context}${slow ? '&slowArchive=1' : ''}`);
      await page.locator('.splash').waitFor({state: 'hidden'});
      const original = await page.evaluate(() => structuredClone(window.qaShell.meetings['native-fixture-meeting']));
      await page.locator('.meeting-library').click();
      const library = page.getByRole('dialog', {name: 'Meetings', exact: true});
      await library.getByRole('button', {name: /^Native fixture .*1 line/}).click();
      await library.waitFor({state: 'hidden'});
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.locator('.save-state.saved').count(), 1, 'opening an archive must not leave a stale unsaved status');
      const opening = await page.evaluate(() => {
        const event = new Event('beforeunload', {cancelable: true}); window.dispatchEvent(event);
        return {protected: event.defaultPrevented, saves: window.qaShell.calls.filter(call => call.command === 'meeting_save').length};
      });
      assert.deepEqual(opening, {protected: false, saves: 0}, 'read-only opening neither rewrites the archive nor activates unsaved protection');
      assert.deepEqual(await current(), original);
      await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
      const viewing = await page.evaluate(async () => {
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const event = new Event('beforeunload', {cancelable: true}); window.dispatchEvent(event);
        return {protected: event.defaultPrevented, saved: !!document.querySelector('.save-state.saved')};
      });
      assert.deepEqual(viewing, {protected: false, saved: true}, 'read-only browsing must not temporarily report unsaved edits or block leaving');
      await waitSaved();
      await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
      await waitSaved();
      assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'meeting_save').length), 0, 'browsing legacy boards must not save synthesized replay steps');
      assert.deepEqual(await current(), original);
      const exit = await page.evaluate(() => {window.qaShell.saveFail = true; return window.qaShell.requestExit('quit');});
      await page.waitForFunction(id => window.qaShell.acks.some(ack => ack.id === id), exit);
      assert.equal(await page.evaluate(id => window.qaShell.acks.find(ack => ack.id === id).allow, exit), true, 'an unedited archive must close even when the disk cannot be written');
      assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'meeting_save').length), 0, 'read-only closing must not rewrite model context');
      assert.deepEqual(await current(), original, 'read-only close preserves the exact saved archive');
      await page.evaluate(() => {window.qaShell.saveFail = false;});
      await write('New words after opening the archive.'); await waitGraph(2); await waitSaved();
      assert.deepEqual((await current()).lines.map(line => line.text), ['Saved archive transcript', 'New words after opening the archive.']);
      assert.equal((await current()).pages[1].steps[0].d.nodes.set[0].label, 'Archived second board', 'an actual edit keeps the original board available for replay');
      report.archiveOpen.push({context, slow, readOnlyBrowsing: true, readOnlyExitAllowed: true, originalPreserved: true, editSaved: true});
    }

    // Native closes during the initial meeting load must wait for restored data.
    await page.goto(`${baseUrl}/?nativeStartup=1&nativeToggle=1`);
    await page.waitForFunction(() => typeof window.qaShell.finishLoad === 'function');
    assert.equal(await page.evaluate(() => window.qaShell.acks.length), 0);
    await page.evaluate(() => window.qaShell.finishLoad());
    await page.waitForFunction(() => window.qaShell.acks.some(ack => ack.id === 77 && ack.allow));
    const startupCommands = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
    assert(startupCommands.indexOf('load-complete') < startupCommands.indexOf('shell_pending_exit'));
    const startupWrite = startupCommands.indexOf('save-complete');
    assert(startupWrite < 0 || startupWrite < startupCommands.indexOf('shell_resolve_exit'));
    assert.equal((await current()).lines[0].text, 'Previously saved meeting');
    assert.equal(startupCommands.includes('voice_start'), false, 'a pending quit must suppress startup capture');
    report.startupPending = true;

    // CLI/tray requests received before restoration must be consumed exactly once afterwards.
    for (const startup of ['nativeToggle=1', 'earlyToggle=1', 'earlyToggle=1&cancelToggle=1']) {
      await page.goto(`${baseUrl}/?${startup}`);
      await page.waitForFunction(() => typeof window.qaShell.finishLoad === 'function');
      if (startup.startsWith('earlyToggle')) await emit('mic-toggle');
      if (startup.includes('cancelToggle')) await emit('mic-toggle');
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'voice_start')), false, 'capture must wait for meeting restoration');
      await page.evaluate(() => window.qaShell.finishLoad());
      await page.waitForFunction(() => window.qaShell.calls.some(call => call.command === 'shell_take_toggle'), null, {timeout: 2500});
      if (!startup.includes('cancelToggle')) await page.waitForFunction(() => window.qaShell.listening);
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const calls = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
      assert(calls.indexOf('load-complete') < calls.indexOf('shell_take_toggle'));
      assert.equal(calls.filter(call => call === 'voice_start').length, startup.includes('cancelToggle') ? 0 : 1);
      assert.equal(calls.filter(call => call === 'voice_stop').length, 0);
      assert.equal((await current()).lines[0].text, 'Previously saved meeting');
      await page.getByText('Previously saved meeting', {exact: true}).first().waitFor();
      if (startup === 'nativeToggle=1') {
        await emit('voice-final', {id: 'startup-line', revision: 0, sources: ['mic'], text: 'New speech after startup', start: 0, end: 1});
        await page.waitForFunction(() => window.qaShell.meetings['native-fixture-meeting'].lines.some(line => line.text === 'New speech after startup'));
        assert.equal((await current()).lines[0].text, 'Previously saved meeting', 'startup speech must append to the restored meeting');
      }
    }
    await page.goto(`${baseUrl}/?listenToggle=1`);
    await page.waitForFunction(() => window.qaShell.listening);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_start').length), 1, 'listener delivery and startup consumption must not apply the same toggle twice');
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_stop').length), 0);
    report.startupToggleRecovery = true;

    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.waitForFunction(() => window.qaShell.shortcut === 'Ctrl+Shift+Space');
    await menu('settings');
    const registrationSettings = page.getByRole('dialog', {name: 'Settings', exact: true});
    const registration = registrationSettings.getByRole('textbox', {name: 'Global shortcut', exact: true});
    await page.evaluate(() => window.qaShell.shortcutReject = 'Ctrl+Shift+K');
    await registration.press('Control+Shift+K');
    await page.waitForFunction(() => window.qaShell.calls.some(call => call.command === 'shortcut_set' && call.args.accel === 'Ctrl+Shift+K'));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await registration.inputValue(), 'Ctrl+Shift+Space', 'a rejected shortcut must retain the active combination');
    assert.equal(await page.evaluate(() => localStorage.getItem('boardcast.hotkey')), 'Ctrl+Shift+Space');
    await registrationSettings.getByRole('alert').filter({hasText: 'Still using Ctrl+Shift+Space'}).waitFor();
    const rejectedCalls = await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'shortcut_set').length);
    await registration.dispatchEvent('keydown', {key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true, repeat: true});
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'shortcut_set').length), rejectedCalls, 'holding a rejected combination must not queue repeated registration attempts');
    await registrationSettings.getByRole('radio', {name: '中文', exact: true}).click();
    await page.getByRole('dialog', {name: '设置', exact: true}).getByRole('alert').filter({hasText: '仍使用 Ctrl+Shift+Space'}).waitFor();
    await page.getByRole('radio', {name: 'English', exact: true}).click();
    await page.evaluate(() => window.qaShell.shortcutReject = undefined);
    await registration.press('Control+Shift+K');
    await page.waitForFunction(() => window.qaShell.shortcut === 'Ctrl+Shift+K' && localStorage.getItem('boardcast.hotkey') === 'Ctrl+Shift+K');
    assert.equal(await registration.inputValue(), 'Ctrl+Shift+K');
    assert.equal(await registrationSettings.getByRole('alert').count(), 0);

    await page.evaluate(() => {window.qaShell.shortcutHold = true; window.qaShell.calls = [];});
    await registration.press('Control+Shift+L');
    await page.waitForFunction(() => window.qaShell.shortcutResolvers.length === 1);
    assert.equal(await registration.getAttribute('aria-busy'), 'true');
    assert.equal(await page.evaluate(() => localStorage.getItem('boardcast.hotkey')), 'Ctrl+Shift+K', 'pending changes must not be persisted');
    await registration.press('Control+Shift+M');
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'shortcut_set').length), 1, 'registration requests must not overlap');
    await page.evaluate(() => window.qaShell.shortcutResolvers.shift().finish());
    await page.waitForFunction(() => window.qaShell.shortcutResolvers[0]?.accel === 'Ctrl+Shift+M');
    assert.equal(await page.evaluate(() => localStorage.getItem('boardcast.hotkey')), 'Ctrl+Shift+L');
    await page.evaluate(() => {window.qaShell.shortcutReject = 'Ctrl+Shift+M'; window.qaShell.shortcutResolvers.shift().finish();});
    await registrationSettings.getByRole('alert').filter({hasText: 'Still using Ctrl+Shift+L'}).waitFor();
    assert.equal(await registration.inputValue(), 'Ctrl+Shift+L');
    assert.equal(await registration.getAttribute('aria-busy'), 'false');

    await registration.press('Control+Shift+N');
    await page.waitForFunction(() => window.qaShell.shortcutResolvers[0]?.accel === 'Ctrl+Shift+N');
    await registration.press('Backspace');
    assert.equal(await page.evaluate(() => window.qaShell.shortcutResolvers.length), 1);
    await page.evaluate(() => window.qaShell.shortcutResolvers.shift().finish());
    await page.waitForFunction(() => window.qaShell.shortcutResolvers.length === 1 && window.qaShell.shortcutResolvers[0].accel === null);
    await page.evaluate(() => window.qaShell.shortcutResolvers.shift().finish());
    await page.waitForFunction(() => window.qaShell.shortcut === null && localStorage.getItem('boardcast.hotkey') === '');
    assert.equal(await registration.inputValue(), '');
    assert.equal(await registrationSettings.getByRole('alert').count(), 0);

    await page.goto(`${baseUrl}/?shortcutStartup=1`);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await menu('settings');
    await registrationSettings.getByRole('alert').filter({hasText: 'No global shortcut is active'}).waitFor();
    assert.equal(await registration.inputValue(), '');
    assert.equal(await page.evaluate(() => localStorage.getItem('boardcast.hotkey')), 'Ctrl+Shift+Space', 'a startup conflict must not overwrite the last saved preference');
    await registration.press('Backspace');
    await page.waitForFunction(() => localStorage.getItem('boardcast.hotkey') === '');
    assert.equal(await registrationSettings.getByRole('alert').count(), 0, 'an explicit Off must work even when the field is already empty after startup failure');
    report.shortcutRegistrationRecovery = true;

    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.waitForFunction(() => window.qaShell.events['mic-toggle']?.length === 1);
    await menu('settings');
    const shortcutSettings = page.getByRole('dialog', {name: 'Settings', exact: true});
    const shortcut = shortcutSettings.getByRole('textbox', {name: 'Global shortcut', exact: true});
    await shortcut.click();
    assert(await page.evaluate(() => document.hasFocus()));
    await page.evaluate(() => {window.qaShell.calls = []; window.qaShell.emit('mic-toggle');});
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_start' || call.command === 'voice_stop').length), 0, 'recording a global shortcut must not start or stop capture');
    assert.equal(await shortcut.inputValue(), 'Ctrl+Shift+Space');
    assert.equal(await page.getByRole('dialog').count(), 1);
    await page.evaluate(() => {
      window.qaShell.listening = true;
      window.qaShell.emit('voice-status', {installed: true, listening: true});
    });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await emit('mic-toggle');
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_stop').length), 0, 'recording a global shortcut must preserve an existing capture');
    await page.evaluate(() => {
      window.qaShell.listening = false;
      window.qaShell.emit('voice-status', {installed: true, listening: false});
    });
    await shortcutSettings.getByRole('button', {name: 'Close', exact: true}).click();
    await emit('mic-toggle');
    await page.waitForFunction(() => window.qaShell.listening);
    await page.getByRole('button', {name: 'Stop listening', exact: true}).waitFor();
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'shell_show_window')), false, 'successful background capture must not raise the window');
    await emit('mic-toggle');
    await page.getByRole('button', {name: 'Start listening', exact: true}).waitFor();
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'shell_show_window')), false, 'successful stop must not raise the window');
    for (const failure of ['start', 'stop', 'missing-model']) {
      await page.evaluate(failure => {
        const q = window.qaShell;
        q.startFail = failure === 'start'; q.stopFail = failure === 'stop'; q.installed = failure !== 'missing-model';
        q.listening = failure === 'stop'; q.emit('voice-status', {installed: q.installed, listening: q.listening});
      }, failure);
      await page.getByRole('button', {name: failure === 'stop' ? 'Stop listening' : 'Start listening', exact: true}).waitFor();
      await page.evaluate(() => {window.qaShell.calls = []; window.qaShell.emit('mic-toggle');});
      await page.waitForFunction(() => window.qaShell.calls.some(call => call.command === 'shell_show_window'));
      assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'shell_show_window').length), 1);
      if (failure === 'missing-model') {
        await page.getByRole('dialog', {name: 'Prepare speech recognition', exact: true}).waitFor();
        assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'voice_start')), false);
      } else {
        await page.getByText(failure === 'start' ? 'Injected capture start failure' : 'Injected stop failure', {exact: true}).waitFor();
      }
    }
    report.globalCaptureFocusAndRecovery = true;

    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.waitForFunction(() => window.qaShell.events['shell-exit-request']?.length === 1);
    await page.evaluate(() => {
      window.qaShell.calls = []; window.qaShell.acks = []; window.qaShell.stopFail = true; window.qaShell.listening = true;
      window.qaShell.emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
      window.qaShell.requestExit('close');
    });
    await page.waitForFunction(() => window.qaShell.acks.length === 1);
    assert.equal(await page.evaluate(() => window.qaShell.acks[0].allow), false, 'failed capture finalization must reject exit');
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'meeting_save')), false, 'must not proceed to save after failed stop');
    report.failedStopBlocksExit = true;
    await page.evaluate(() => {window.qaShell.stopFail = false; window.qaShell.saveFail = true;});
    await write('Retained content before closing.');
    await page.locator('.save-error').waitFor();
    await page.evaluate(() => {
      window.qaShell.calls = []; window.qaShell.acks = [];
      window.qaShell.listening = true;
      window.qaShell.emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
      const id = window.qaShell.requestExit('quit');
      window.qaShell.emit('shell-exit-request', {id, reason: 'quit'});
    });
    await page.waitForFunction(() => window.qaShell.acks.length === 1);
    assert.equal(await page.evaluate(() => window.qaShell.acks[0].allow), false);
    const failedCommands = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
    assert(failedCommands.indexOf('voice_stop') < failedCommands.indexOf('save-failed'));
    assert(failedCommands.indexOf('save-failed') < failedCommands.indexOf('shell_resolve_exit'));
    assert.equal(await page.getByText('Retained content before closing.', {exact: true}).count(), 1);
    report.failedSaveBlocksExit = true;

    await page.evaluate(() => {
      window.qaShell.saveFail = false; window.qaShell.saveHold = true; window.qaShell.calls = []; window.qaShell.acks = [];
      window.qaShell.listening = true; window.qaShell.finalOnStop = 'Final words before exit';
      window.qaShell.emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
      window.qaShell.requestExit('close');
    });
    await page.waitForFunction(() => window.qaShell.saveResolvers.length > 0);
    assert.equal(await page.evaluate(() => window.qaShell.acks.length), 0, 'native ack must wait for the write to resolve');
    await page.evaluate(() => {window.qaShell.saveHold = false; window.qaShell.saveResolvers.splice(0).forEach(resolve => resolve());});
    await page.waitForFunction(() => window.qaShell.acks.some(ack => ack.allow));
    const successCommands = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
    assert(successCommands.indexOf('voice_stop') < successCommands.indexOf('save-complete'));
    assert(successCommands.indexOf('save-complete') < successCommands.indexOf('shell_resolve_exit'));
    assert.equal(await page.evaluate(() => window.qaShell.acks[0].transition), true, 'transition remains locked at ack');
    assert((await current()).lines.some(line => line.id === 'final-before-exit' && line.text === 'Final words before exit'));
    report.finalLineSavedBeforeAck = true;

    // IME candidate keys must not start capture, open help, or dismiss a menu.
    const startsBefore = await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_start').length);
    await page.evaluate(() => {
      for (const [key, code] of [['?', 'Slash'], [' ', 'Space']]) document.body.dispatchEvent(new KeyboardEvent('keydown', {key, code, bubbles: true, isComposing: true}));
    });
    assert.equal(await page.getByRole('dialog').count(), 0);
    assert.equal(await page.evaluate(() => window.qaShell.calls.filter(call => call.command === 'voice_start').length), startsBefore);
    await page.getByRole('button', {name: 'Import', exact: true}).click();
    await page.getByRole('menu').evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true, isComposing: true})));
    assert.equal(await page.getByRole('menu').count(), 1);
    await page.keyboard.press('Escape');
    assert.equal(await page.getByRole('menu').count(), 0);
    report.compositionIsolation = true;

    // One live caption, with an explicit retained opt-out for transcript reading.
    await emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
    await emit('voice-partial', 'A provisional caption');
    await page.locator('.dock .caption').filter({hasText: 'A provisional caption'}).waitFor();
    assert.equal(await page.locator('.dock .caption').getAttribute('aria-live'), 'polite');
    assert.equal(await page.locator('.line.partial').getAttribute('aria-live'), 'off');
    const announcements = page.getByRole('checkbox', {name: 'Announce live transcript', exact: true});
    await announcements.uncheck();
    assert.equal(await page.locator('.dock .caption').getAttribute('aria-live'), 'off');
    assert.equal(await page.evaluate(() => localStorage.getItem('boardcast.announce-transcript')), 'off');
    await announcements.check();
    await emit('voice-status', {installed: true, listening: false});
    report.transcriptAnnouncements = true;

    // Text undo must still work inside Settings, before the modal shortcut guard.
    await menu('settings');
    const settings = page.getByRole('dialog', {name: 'Settings', exact: true});
    await settings.getByRole('tab', {name: 'Remote & privacy', exact: true}).click();
    const input = settings.getByPlaceholder('https://api.openai.com/v1');
    await input.click(); await input.pressSequentially('https://fixture.invalid/v1');
    await menu('undo'); assert.equal(await input.inputValue(), '');
    await menu('redo'); assert.equal(await input.inputValue(), 'https://fixture.invalid/v1');
    assert.equal(await input.evaluate(element => document.activeElement === element), true);
    await menu('select-all');
    assert.deepEqual(await input.evaluate(element => [element.selectionStart, element.selectionEnd]), [0, 'https://fixture.invalid/v1'.length]);
    await page.keyboard.press('Escape');
    await page.getByRole('button', {name: 'Discard connection changes', exact: true}).click();
    report.editHistory.textInDialog = true;

    await settings.waitFor({state: 'detached'});
    await menu('new-meeting');
    const preparation = page.getByRole('dialog', {name: 'Prepare to start', exact: true});
    await preparation.waitFor();
    await preparation.getByRole('button', {name: 'Check input', exact: true}).click();
    await preparation.getByRole('meter').waitFor();
    const previewExit = await page.evaluate(() => { window.qaShell.calls = []; return window.qaShell.requestExit('close'); });
    await page.waitForFunction(id => window.qaShell.acks.some(ack => ack.id === id && ack.allow), previewExit);
    assert.equal(await page.evaluate(() => window.qaShell.inputPreview), null);
    const previewCommands = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
    assert(previewCommands.indexOf('voice_input_check_stop') >= 0 && previewCommands.indexOf('voice_input_check_stop') < previewCommands.indexOf('shell_resolve_exit'));
    report.previewStoppedBeforeExit = true;
    await page.keyboard.press('Escape');
    await write('Build a board for native history.');
    await waitGraph(1); await waitSaved();
    await menu('undo'); await waitGraph(0);
    await menu('redo'); await waitGraph(1);
    report.editHistory.handDrawn = true;
    await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
    await page.locator('.excalidraw').waitFor();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await waitSaved();
    const before = new Set(((await current()).scene?.elements || []).map(element => element.id));
    async function rectangle(x, y) {
      const count = ((await current()).scene?.elements || []).filter(element => element.type === 'rectangle' && !element.isDeleted && !element.customData?.boardcast).length;
      await page.getByRole('radio', {name: 'Rectangle', exact: true}).locator('..').click();
      await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x + 90, y + 60, {steps: 8}); await page.mouse.up();
      await page.waitForFunction(count => window.qaShell.meetings[localStorage.getItem('boardcast.meeting')]?.scene?.elements.filter(element => element.type === 'rectangle' && !element.isDeleted && !element.customData?.boardcast).length === count + 1, count);
      await waitSaved();
    }
    await rectangle(520, 300);
    let scene = (await current()).scene.elements;
    const first = scene.find(element => element.type === 'rectangle' && !element.isDeleted && !element.customData?.boardcast && !before.has(element.id));
    assert(first);
    before.add(first.id);
    await rectangle(650, 400);
    scene = (await current()).scene.elements;
    const second = scene.find(element => element.type === 'rectangle' && !element.isDeleted && !element.customData?.boardcast && !before.has(element.id));
    assert(second);
    await menu('undo');
    await page.waitForFunction(id => !window.qaShell.meetings[localStorage.getItem('boardcast.meeting')]?.scene.elements.some(element => element.id === id && !element.isDeleted), second.id);
    assert((await current()).scene.elements.some(element => element.id === first.id && !element.isDeleted), 'one menu Undo must not undo twice');
    await menu('redo');
    await page.waitForFunction(id => window.qaShell.meetings[localStorage.getItem('boardcast.meeting')]?.scene.elements.some(element => element.id === id && !element.isDeleted), second.id);
    report.editHistory.freeEditUndoOnceAndRedo = true;
    await menu('select-all');
    await page.keyboard.press('Delete');
    await page.waitForFunction(ids => {
      const elements = window.qaShell.meetings[localStorage.getItem('boardcast.meeting')].scene.elements;
      return ids.every(id => !elements.some(element => element.id === id && !element.isDeleted));
    }, [first.id, second.id]);
    await menu('undo');
    await page.waitForFunction(ids => {
      const elements = window.qaShell.meetings[localStorage.getItem('boardcast.meeting')].scene.elements;
      return ids.every(id => elements.some(element => element.id === id && !element.isDeleted));
    }, [first.id, second.id]);
    report.editHistory.selectAllInDialogAndCanvas = true;

    // Ending into review uses the same stop/save barrier and retains failure recovery.
    await page.evaluate(() => {
      window.qaShell.stopFail = true; window.qaShell.listening = true;
      window.qaShell.emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
    });
    await page.getByRole('button', {name: 'End and review', exact: true}).click();
    const review = page.getByRole('dialog', {name: 'End and review', exact: true});
    await review.getByRole('button', {name: 'End and review', exact: true}).click();
    await review.getByRole('alert').waitFor();
    assert.equal(await page.locator('.review-summary').count(), 0);
    await page.evaluate(() => {window.qaShell.stopFail = false; window.qaShell.saveFail = true; window.qaShell.finalOnStop = 'Final words before review';});
    await review.getByRole('button', {name: 'End and review', exact: true}).click();
    await review.getByRole('alert').filter({hasText: 'Injected disk full'}).waitFor();
    assert.equal(await page.locator('.review-summary').count(), 0);
    assert.equal(await review.getByRole('button', {name: 'Export a copy', exact: true}).isVisible(), true);
    await page.evaluate(() => {window.qaShell.saveFail = false; window.qaShell.saveHold = true; window.qaShell.calls = [];});
    await review.getByRole('button', {name: 'End and review', exact: true}).click();
    await page.waitForFunction(() => window.qaShell.saveResolvers.length > 0);
    assert.equal(await page.locator('.review-summary').count(), 0, 'review must wait for durable save');
    await page.evaluate(() => {window.qaShell.saveHold = false; window.qaShell.saveResolvers.splice(0).forEach(resolve => resolve());});
    await page.locator('.review-summary').waitFor();
    assert((await current()).lines.some(line => line.text === 'Final words before review'));
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'save-complete')), true);
    report.reviewSaveBarrier = true;

    // Removing model files must use the real App stop/tail/save boundary before IPC.
    await page.evaluate(() => {
      const q = window.qaShell;
      q.models = [{id: 'speech-pack', label: 'Fixture speech pack', kind: 'voice', size: 1e9, partial: 0, installed: true, selected: false, recommended: false, note: 'Controlled native fixture'}];
      q.listening = true; q.stopFail = true; q.calls = [];
      q.emit('voice-status', {installed: true, listening: true, device: 'Fixture microphone'});
    });
    await menu('settings');
    await settings.getByRole('tab', {name: 'Audio & transcription', exact: true}).click();
    await settings.getByRole('button', {name: 'Manage models', exact: true}).click();
    const modelsPanel = page.getByRole('dialog', {name: 'Local models', exact: true});
    await modelsPanel.getByRole('button', {name: 'Delete', exact: true}).click();
    const removing = page.getByRole('dialog', {name: 'Delete Fixture speech pack?', exact: true});
    const remove = removing.getByRole('button', {name: 'Stop, save and delete', exact: true});
    await remove.click(); await removing.getByRole('alert').waitFor();
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'model_delete')), false);
    await page.evaluate(() => {window.qaShell.stopFail = false; window.qaShell.saveFail = true; window.qaShell.finalOnStop = 'Last words before model removal'; window.qaShell.finalId = 'final-before-model-delete';});
    await remove.click(); await removing.getByRole('alert').filter({hasText: 'Injected disk full'}).waitFor();
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'model_delete')), false);
    await page.evaluate(() => {window.qaShell.saveFail = false; window.qaShell.saveHold = true; window.qaShell.calls = [];});
    await remove.click(); await page.waitForFunction(() => window.qaShell.saveResolvers.length > 0);
    assert.equal(await page.evaluate(() => window.qaShell.calls.some(call => call.command === 'model_delete')), false);
    await page.evaluate(() => {window.qaShell.saveHold = false; window.qaShell.saveResolvers.splice(0).forEach(resolve => resolve());});
    await removing.waitFor({state: 'hidden'});
    const removalCommands = await page.evaluate(() => window.qaShell.calls.map(call => call.command));
    assert(removalCommands.indexOf('save-complete') >= 0 && removalCommands.indexOf('save-complete') < removalCommands.indexOf('model_delete'));
    assert((await current()).lines.some(line => line.text === 'Last words before model removal'));
    report.modelRemovalSaveBarrier = true;
    assert.deepEqual(errors, []);
    report.passed = true;
    await fs.rm(`${evidence}/native-shell-failure.png`, {force: true});
    console.log('PASS: startup pending exit, stop/save-failure veto, final speech→save→ack order, duplicate request, text/PAD/Excal undo+redo.');
  } catch (error) {
    report.error = String(error);
    report.nativeState = await page.evaluate(() => ({calls: window.qaShell.calls.slice(-60), acks: window.qaShell.acks, current: localStorage.getItem('boardcast.meeting'), events: window.qaShell.events}));
    await page.screenshot({path: `${evidence}/native-shell-failure.png`, animations: 'disabled'});
    throw error;
  } finally {
    await fs.writeFile(`${evidence}/native-shell-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
