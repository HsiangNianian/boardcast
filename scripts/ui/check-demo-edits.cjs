// Real bundled samples and App; desktop storage/model status use controlled IPC.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkDemoEdits(page, baseUrl, evidence) {
  const report = {checks: [], pageErrors: []};
  const check = (passed, name) => {report.checks.push({passed, name}); console.log(`${passed ? 'PASS' : 'FAIL'}: ${name}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('dialog', dialog => dialog.accept());
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.log', 'open');
    localStorage.setItem('boardcast.autoupdate', 'off'); localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19880');
    if (!new URL(location.href).searchParams.has('desktop')) return;
    const q = window.qaDemos = {calls: [], callbacks: {}, events: {}, meetings: JSON.parse(localStorage.getItem('qa.demo.meetings') || '{}'), next: 0};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => q.events[event] = (q.events[event] || []).filter(value => value !== id)};
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++q.next] = fn; return q.next;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push(command);
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'no-model', gpus: []};
        if (command === 'voice_status') return {installed: false, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: false, listening: false, preparing: false, autoLocales: [], apple: null};
        if (command === 'voice_devices') return [];
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') return args.storageVersion === 1 ? null : q.meetings[args.id] ? JSON.stringify(q.meetings[args.id]) : null;
        if (command === 'meeting_save') {q.meetings[args.id] = JSON.parse(args.json); localStorage.setItem('qa.demo.meetings', JSON.stringify(q.meetings)); return;}
        if (command === 'meetings_list') return args.storageVersion === 1 ? [] : Object.values(q.meetings).map(m => ({id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt, lines: m.lines.length, pages: m.pages?.length || 1}));
        if (command === 'models_list') return {models: [], freeBytes: 1e10};
        if (command === 'plugin:app|version') return '0.21.0-test';
        if (command === 'session_wayland') return false;
        return null;
      },
    };
  });
  await page.route('http://127.0.0.1:19880/**', route => route.fulfill({status: 503, body: 'No model'}));
  const rows = () => page.evaluate(async () => {
    if (window.qaDemos) return Object.values(window.qaDemos.meetings);
    if (!(await indexedDB.databases()).some(db => db.name === 'boardcast-v2')) return [];
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const all = await new Promise(yes => {const r = db.transaction('meetings').objectStore('meetings').getAll(); r.onsuccess = () => yes(r.result);}); db.close(); return all;
  });
  const fresh = async () => {await page.locator('.meeting-actions').getByRole('button', {name: 'New meeting', exact: true}).click(); await page.getByRole('button', {name: 'Play sample meeting', exact: true}).waitFor();};
  const open = async () => {
    await page.locator('.demo-pick select').selectOption('en-auth-correction');
    await page.getByRole('button', {name: 'Play sample meeting', exact: true}).click();
    await page.locator('.replay-dock').getByRole('button', {name: 'Exit replay', exact: true}).click();
    return page.evaluate(() => localStorage.getItem('boardcast.meeting'));
  };
  try {
    for (const runtime of ['web', 'desktop']) {
      await page.goto(`${baseUrl}/?${runtime === 'desktop' ? 'desktop=1' : ''}`); await page.locator('.splash').waitFor({state: 'hidden'});
      const available = await page.getByRole('button', {name: 'Play sample meeting', exact: true}).isEnabled({timeout: 3000}).catch(() => false);
      check(available, `${runtime}: samples are available without board or speech models`);
      if (!available) {await page.screenshot({path: `${evidence}/${runtime}-unavailable.png`}); continue;}
      await page.locator('.demo-note').getByRole('button').click();
      const settings = page.getByRole('dialog', {name: 'Settings', exact: true});
      check(await settings.getByRole('tab', {name: 'Board & models', exact: true}).getAttribute('aria-selected') === 'true', `${runtime}: the sample setup link opens board model settings`);
      await page.keyboard.press('Escape');
      for (const edit of ['rename', 'summary', 'free', 'glossary']) {
        const before = await rows();
        const id = await open();
        await page.locator('.meeting-library').click();
        const library = page.getByRole('dialog', {name: 'Meetings', exact: true});
        await library.getByRole('status').waitFor({state: 'hidden'});
        check((await rows()).length === before.length && await library.locator('.meeting-list > li').count() === before.length, `${runtime}/${edit}: viewing a sample preserves the existing library`);
        await page.keyboard.press('Escape');
        if (edit === 'rename') {
          await page.getByRole('tab', {name: 'Contents', exact: true}).click();
          await page.locator('#sidebar-contents-panel').getByRole('button', {name: 'Rename', exact: true}).first().click();
          await page.getByRole('textbox', {name: 'Name', exact: true}).fill('My sample decision'); await page.keyboard.press('Enter');
          // Immediate navigation exercises flush before the 500 ms autosave.
          await fresh();
        } else if (edit === 'summary') {
          await page.getByRole('tab', {name: 'Summary', exact: true}).click();
          await page.getByRole('button', {name: 'Edit summary', exact: true}).click();
          await page.getByRole('textbox', {name: 'Edit summary', exact: true}).fill('My sample summary');
        } else if (edit === 'free') {
          await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
          await page.locator('.pencil-stage[data-ready="true"]').waitFor();
          await page.getByRole('radio', {name: 'Rectangle', exact: true}).locator('..').click();
          const box = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
          const x = box.x + box.width * .68, y = box.y + box.height * .65;
          await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x + 70, y + 45, {steps: 5}); await page.mouse.up();
        } else {
          await page.getByRole('button', {name: 'Settings', exact: true}).click();
          await page.getByRole('tab', {name: 'Terminology & materials', exact: true}).click();
          await page.getByPlaceholder('Selecting a name on the board and clicking "Add to glossary" also adds it here').fill('MySampleTerm');
          await page.keyboard.press('Escape');
        }
        if (edit !== 'rename') await page.locator('.save-state.saved').waitFor({timeout: 5000}).catch(() => {});
        const after = await rows(), saved = after.find(row => row.id === id);
        const correct = saved && (edit === 'rename' ? saved.graph.nodes.some(n => n.label === 'My sample decision') : edit === 'summary' ? saved.summary === 'My sample summary' : edit === 'free' ? saved.scene?.elements.some(e => e.type === 'rectangle' && !e.isDeleted && !saved.scene.generated.some(g => g.id === e.id)) : saved.glossary === 'MySampleTerm');
        check(!!correct && after.length === before.length + 1, `${runtime}/${edit}: the first manual edit saves one personal meeting`);
        check(before.every(row => JSON.stringify(after.find(m => m.id === row.id)) === JSON.stringify(row)), `${runtime}/${edit}: earlier meetings remain byte-for-byte equivalent in storage`);
        if (edit !== 'rename') {
          await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
          check((await rows()).some(row => row.id === id) && await page.evaluate(() => localStorage.getItem('boardcast.meeting')) === id && await page.locator('.demo-pick').count() === 0, `${runtime}/${edit}: the edited sample restores on reload`);
          if (edit === 'free') await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
          await fresh();
        }
      }
      const existing = await rows();
      await open();
      await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
      await page.locator('.pencil-stage[data-ready="true"]').waitFor();
      await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      await fresh();
      check(JSON.stringify(await rows()) === JSON.stringify(existing), `${runtime}: switching sample views without editing neither saves the sample nor changes earlier meetings`);
      if (runtime === 'desktop') check(await page.evaluate(() => !window.qaDemos.calls.some(command => ['llm_chat', 'voice_start', 'model_download'].includes(command))), 'desktop: recorded samples do not infer, record, or download a model');
    }
    check(report.pageErrors.length === 0, 'no uncaught script errors');
    assert(report.checks.every(c => c.passed), 'sample entry or editing checks failed');
  } finally {await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n');}
};
