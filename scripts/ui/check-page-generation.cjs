const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const {click: meetingAction} = require('./meeting-action.cjs');

module.exports = async function checkPageGeneration(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: [], requests: []};
  const check = (value, label) => { assert(value, label); report.checks.push(label); };
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({lang: 'en', log: 'open', 'mode-hint': 'dismissed', localLlm: 'http://127.0.0.1:19881'})) localStorage.setItem('boardcast.' + key, value);
    const fetch = window.fetch.bind(window);
    window.qaPageRequests = [];
    window.fetch = (input, options) => {
      if (!String(input).startsWith('http://127.0.0.1:19881')) return fetch(input, options);
      if (options?.method !== 'POST') return Promise.resolve(new Response('{}', {status: 200}));
      // Keep late replies deliverable even after cancellation, as native IPC can do.
      return new Promise((resolve, reject) => window.qaPageRequests.push({body: JSON.parse(options.body), resolve, reject}));
    };
  });
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const rows = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').getAll(); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return {all: rows, current: rows.find(row => row.id === localStorage.getItem('boardcast.meeting'))};
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) {const state = await read(); if (state.current && predicate(state.current, state.all)) return state; await page.waitForTimeout(75);}
    throw Error('Expected page generation state was not saved');
  };
  const send = async text => {await page.locator('.write textarea').fill(text); await page.locator('.write textarea').press('Enter');};
  const requests = async n => page.waitForFunction(n => window.qaPageRequests.length >= n, n);
  const reply = (index, text, failure = false) => page.evaluate(({index, text, failure}) => {
    const request = window.qaPageRequests[index];
    if (failure) request.reject(Error(text));
    else request.resolve(new Response(JSON.stringify({choices: [{message: {content: text}}]}), {status: 200, headers: {'content-type': 'application/json'}}));
  }, {index, text, failure});
  const go = async n => {await page.locator('.pages').getByRole('button', {name: `Board ${n}`, exact: true}).click(); await page.locator(`.pages [aria-current=page][aria-label="Board ${n}"]`).waitFor();};
  const labels = graph => graph.nodes.map(node => node.label);
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const pages = ['Alpha original', 'Bravo original', 'Charlie original'].map(label => ({graph: {nodes: [{id: 'n1', kind: 'node', label, aliases: [], marks: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1}}));
      const row = {version: 2, storageRevision: 1, id: 'qa-generating-pages', title: 'Navigation during generation', titled: true, createdAt: 1, updatedAt: 2, graph: pages[0].graph, pages, page: 0, lines: [], history: []};
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await send('First request belongs to Alpha.'); await requests(1);
    await send('Queued request also belongs to Alpha.');
    await page.locator('.line.pending').first().waitFor();
    report.navigationEnabled = await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).isEnabled();
    check(report.navigationEnabled, 'completed boards remain reachable while generation is pending');
    await go(2); await send('Bravo request can finish independently.'); await requests(2);
    await saved(m => m.page === 1 && m.lines.length === 3 && m.lines.every(l => l.ms === undefined));
    check(true, 'navigation preserves and saves pending original lines with their board');
    await reply(0, 'N 「Alpha generated」'); await requests(3);
    let state = await saved(m => labels(m.pages[0].graph).includes('Alpha generated') && m.lines[0].ms !== undefined);
    check(state.current.page === 1 && !labels(state.current.graph).includes('Alpha generated'), 'late Alpha output stays on Alpha without changing the viewed Bravo board');
    const body = await page.evaluate(() => window.qaPageRequests[2].body);
    check(JSON.stringify(body).includes('Alpha generated') && !JSON.stringify(body).includes('Bravo original'), 'the next queued request reads its own updated board after navigation');
    await reply(2, 'N 「Alpha queued result」');
    await reply(1, 'Fixture Bravo request failed', true);
    state = await saved(m => m.lines[1].ms !== undefined && m.lines[2].failed);
    check(state.current.lines.map(l => l.page).join() === '0,0,1', 'completion and failure preserve each original page assignment');
    await go(3);
    await page.getByRole('button', {name: 'Retry this line', exact: true}).click(); await requests(4);
    await go(2); await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
    const item = page.locator('.content-list li').filter({has: page.getByText('Bravo original', {exact: true})});
    await item.getByRole('button', {name: 'Rename', exact: true}).click(); await item.locator('input').fill('Bravo edited while waiting'); await item.getByRole('button', {name: 'Save name', exact: true}).click();
    await reply(3, 'N 「Bravo recovered」');
    state = await saved(m => labels(m.graph).includes('Bravo recovered'));
    check(labels(state.current.graph).includes('Bravo edited while waiting') && !state.current.lines[2].failed && state.current.lines.filter(l => !l.manual).length === 3, 'returning to an active page preserves manual edits and retries its original line once');
    await page.locator('#board-panel').focus(); await page.keyboard.press('Control+z');
    state = await saved(m => !labels(m.graph).includes('Bravo recovered'));
    check(labels(state.current.graph).includes('Bravo edited while waiting'), 'one undo removes the model round while retaining the preceding manual edit');
    await page.keyboard.press('Control+Shift+z'); await saved(m => labels(m.graph).includes('Bravo recovered'));
    check(true, 'redo restores that model round once');
    await send('Bravo remains active during navigation.'); await requests(5);
    await page.locator('.content-page select').selectOption('2');
    await page.locator('.content-page select').waitFor();
    check(await page.locator('.content-page select').inputValue() === '2', 'the contents page picker also works during generation');
    await page.locator('#board-panel').focus(); await page.keyboard.press('PageUp');
    await page.locator('.pages [aria-current=page][aria-label="Board 2"]').waitFor();
    check(true, 'keyboard page navigation works while another request is pending');
    await send('下一页'); await page.locator('.pages [aria-current=page][aria-label="Board 3"]').waitFor();
    check(await page.evaluate(() => window.qaPageRequests.length) === 5, 'spoken navigation uses the same page change without a model request');
    await page.locator('.pages .new').click(); await page.locator('.pages [aria-current=page][aria-label="Board 4"]').waitFor();
    await send('New fourth board request.'); await requests(6);
    await reply(4, 'N 「Bravo second result」'); await reply(5, 'N 「Fourth result」');
    state = await saved(m => m.pages.length === 4 && labels(m.graph).includes('Fourth result') && m.lines.every(l => l.ms !== undefined));
    check(state.current.page === 3 && labels(state.current.pages[1].graph).includes('Bravo second result') && !labels(state.current.graph).includes('Bravo second result'), 'new-page generation and prior-page completion remain isolated');
    check(new Set(state.current.lines.filter(l => !l.manual).map(l => l.id)).size === state.current.lines.filter(l => !l.manual).length, 'all completed and navigation transcript IDs remain unique');
    report.completed = state.current;
    await page.screenshot({path: evidence + '/completed.png'});
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    state = await saved(m => m.page === 3);
    check(JSON.stringify(state.current) === JSON.stringify(report.completed), 'cold browser reload preserves all four saved pages and pending outcomes');
    await send('Canceled fourth-page request.'); await requests(1); await go(1);
    await send('Canceled first-page request.'); await requests(2);
    await meetingAction(page, 'New meeting'); await page.locator('.hero').waitFor();
    await reply(0, 'N 「Wrong old fourth result」'); await reply(1, 'N 「Wrong old first result」');
    await send('New meeting owns this request.'); await requests(3); await reply(2, 'N 「Fresh meeting result」');
    state = await saved(m => labels(m.graph).includes('Fresh meeting result'));
    const old = state.all.find(m => m.id === 'qa-generating-pages');
    check(old.lines.slice(-2).every(l => l.failed && l.boardFailure === 'cancelled'), 'switching meetings cancels every page worker and retains unfinished original lines');
    check(state.current.lines.length === 1 && state.current.graph.nodes.every(n => !n.label.startsWith('Wrong old')), 'late success from two old pages cannot change the new meeting');
    check(report.pageErrors.length === 0, 'the complete multi-page flow has no uncaught errors');
    report.passed = true;
  } catch (error) {
    report.error = String(error); await page.screenshot({path: evidence + '/failure.png'}); throw error;
  } finally {
    report.requests = await page.evaluate(() => window.qaPageRequests?.map(r => r.body) ?? []).catch(() => []);
    await fs.writeFile(evidence + '/report.json', JSON.stringify(report, null, 2) + '\n');
  }
};
