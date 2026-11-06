const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkLifecycle(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, { recursive: true });
  const errors = [];
  const report = { llm: [], summary: [], import: [], pageErrors: errors };
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19877');
    const original = window.fetch.bind(window);
    window.qaLifecycle = { requests: [], imports: [] };
    // Deliberately ignore AbortSignal, as a native transport may already be running.
    window.fetch = (input, options) => {
      if (!String(input).startsWith('http://127.0.0.1:19877')) return original(input, options);
      if (options?.method !== 'POST') return Promise.resolve(new Response('{}', { status: 200 }));
      return new Promise((resolve, reject) => window.qaLifecycle.requests.push({ resolve, reject, body: JSON.parse(options.body) }));
    };
    const originalText = File.prototype.text;
    File.prototype.text = function () {
      if (!this.name.startsWith('qa-late-')) return originalText.call(this);
      return new Promise((resolve, reject) => window.qaLifecycle.imports.push({ resolve, reject, name: this.name }));
    };
  });
  const count = () => page.evaluate(() => window.qaLifecycle.requests.length);
  const requestAfter = async (n, action) => { await action(); await page.waitForFunction(n => window.qaLifecycle.requests.length > n, n); return n; };
  const respond = (index, content, fail = false) => page.evaluate(({ index, content, fail }) => {
    const request = window.qaLifecycle.requests[index];
    if (fail) request.reject(new Error(content));
    else request.resolve(new Response(JSON.stringify({ choices: [{ message: { content } }] }), { status: 200, headers: { 'content-type': 'application/json' } }));
  }, { index, content, fail });
  const speak = async text => requestAfter(await count(), async () => {
    await page.getByRole('textbox', { name: 'What was said', exact: true }).fill(text);
    await page.getByRole('button', { name: 'Put on the board', exact: true }).click();
  });
  const nextMeeting = async () => {
    await page.getByRole('button', { name: 'New meeting', exact: true }).click();
    await page.getByRole('heading', { name: 'Just talk. The whiteboard draws itself.', exact: true }).waitFor();
  };
  const read = async () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    const result = await new Promise((yes, no) => { const r = db.transaction('meetings').objectStore('meetings').getAll(); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    db.close(); const id = localStorage.getItem('boardcast.meeting'); return { current: result.find(m => m.id === id), all: result };
  });
  const saved = async predicate => {
    for (let i = 0; i < 80; i++) { const state = await read(); if (state.current && predicate(state.current)) return state; await page.waitForTimeout(100); }
    throw new Error('Expected meeting snapshot was not saved');
  };
  try {
    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({ state: 'hidden' });
    for (const fail of [false, true]) {
      const oldText = `Old retained speech ${fail ? 'failure' : 'success'}.`;
      const old = await speak(oldText);
      await nextMeeting();
      const fresh = await speak('Current meeting statement.');
      await respond(old, fail ? 'late old model error' : 'N 「Wrong old board」', fail);
      await respond(fresh, 'N 「Correct current board」');
      const state = await saved(m => m.graph.nodes.some(n => n.label === 'Correct current board'));
      assert.ok(!state.current.graph.nodes.some(n => n.label === 'Wrong old board'));
      assert.ok(!state.current.lines.some(l => l.text === oldText));
      assert.ok(state.all.some(m => m.lines.some(l => l.text === oldText && l.failed)), 'canceled speech remains in its original meeting');
      report.llm.push({ lateResult: fail ? 'failure' : 'success', isolated: true, originalSpeechRetained: true });
      await nextMeeting();
    }
    for (const fail of [false, true]) {
      const seed = await speak('Source for the old summary.'); await respond(seed, 'N 「Old source」');
      await saved(m => m.graph.nodes.length === 1);
      const old = await requestAfter(await count(), () => page.getByRole('button', { name: 'Summary', exact: true }).click());
      await nextMeeting();
      const fresh = await speak('Source for the current summary.'); await respond(fresh, 'N 「Current source」');
      await saved(m => m.graph.nodes.length === 1);
      const current = await requestAfter(await count(), () => page.getByRole('button', { name: 'Summary', exact: true }).click());
      await respond(old, fail ? 'late old summary error' : 'OLD SUMMARY MUST NOT APPEAR', fail);
      await page.waitForTimeout(150);
      assert.equal(await page.getByRole('button', { name: 'Stop', exact: true }).count(), 1, 'old finally must not clear the current summary task');
      assert.ok(!(await page.locator('body').innerText()).includes('OLD SUMMARY MUST NOT APPEAR'));
      assert.ok(!(await page.locator('body').innerText()).includes('late old summary error'));
      await respond(current, '### Conclusions\n- CURRENT SUMMARY');
      await saved(m => m.summary?.includes('CURRENT SUMMARY'));
      report.summary.push({ lateResult: fail ? 'failure' : 'success', isolated: true, currentFinallyPreserved: true });
      await nextMeeting();
    }
    for (const fail of [false, true]) {
      const n = await page.evaluate(() => window.qaLifecycle.imports.length);
      await page.locator('input[type="file"]').setInputFiles({ name: `qa-late-${fail ? 'failure' : 'success'}.excalidraw`, mimeType: 'application/json', buffer: Buffer.from('{}') });
      await page.waitForFunction(n => window.qaLifecycle.imports.length > n, n);
      await page.getByRole('dialog', {name:'Import materials',exact:true}).getByRole('button',{name:'Cancel import',exact:true}).click();
      await nextMeeting();
      await page.evaluate(({ n, fail }) => {
        const request = window.qaLifecycle.imports[n];
        if (fail) request.reject(new Error('late import error'));
        else request.resolve(JSON.stringify({ type: 'excalidraw', elements: [{ id: 'old-free-object', type: 'rectangle', x: 0, y: 0, width: 100, height: 70, isDeleted: false }], files: {} }));
      }, { n, fail });
      await page.waitForTimeout(200);
      assert.equal(await page.getByRole('heading', { name: 'Just talk. The whiteboard draws itself.', exact: true }).count(), 1);
      assert.ok(!(await page.locator('body').innerText()).includes('late import error'));
      report.import.push({ lateResult: fail ? 'failure' : 'success', isolated: true });
    }
    assert.deepEqual(errors, []);
    report.passed = true;
    await page.screenshot({ path: `${evidence}/lifecycle-isolated.png` });
    return report;
  } catch (error) {
    report.error = String(error);
    await page.screenshot({ path: `${evidence}/lifecycle-failure.png` });
    throw error;
  } finally {
    await fs.writeFile(`${evidence}/lifecycle-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
