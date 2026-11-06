const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkPencilConcurrent(page, baseUrl, evidence = '/tmp/boardcast-pencil-concurrent') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: []};
  const check = (ok, name) => { report.checks.push({name, ok: !!ok}); assert.ok(ok, name); };
  let pending, hold = false;
  page.on('pageerror', e => report.pageErrors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.mode', 'pencil');
    localStorage.setItem('boardcast.mode-hint', 'dismissed');
    localStorage.setItem('boardcast.log', 'closed');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19876');
  });
  const response = text => ({status: 200, contentType: 'application/json', headers: {'access-control-allow-origin': '*'}, body: JSON.stringify({choices: [{message: {content: text}}]})});
  await page.route('http://127.0.0.1:19876/**', route => {
    if (route.request().method() !== 'POST') return route.fulfill({status: 200, contentType: 'application/json', headers: {'access-control-allow-origin': '*'}, body: JSON.stringify({status: 'ok', data: [{id: 'fixture'}]})});
    if (hold) { pending = route; return; }
    return route.fulfill(response('N 「Initial topic」'));
  });
  const read = () => page.evaluate(async () => {
    const id = localStorage.getItem('boardcast.meeting');
    if (!id) return null;
    const db = await new Promise((resolve, reject) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);});
    try { return await new Promise((resolve, reject) => {const r = db.transaction('meetings').objectStore('meetings').get(id); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);}); }
    finally { db.close(); }
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) { const row = await read(); if (row && predicate(row)) { await page.locator('.save-state.saved').waitFor(); return read(); } await page.waitForTimeout(100); }
    throw new Error('Expected meeting content was not saved');
  };
  const send = async text => { await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text); await page.getByRole('button', {name: 'Put on the board', exact: true}).click(); };
  const free = row => row?.scene?.elements.filter(e => !e.isDeleted && !e.customData?.boardcast) ?? [];
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await send('Establish an initial discussion topic.');
    await saved(m => m.graph.nodes.length === 1 && m.lines.at(-1).ms !== undefined);
    hold = true;
    for (const [index, tool] of ['Rectangle', 'Draw'].entries()) {
      await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
      await page.locator('.pencil-stage[data-ready="true"]').waitFor();
      const before = await read();
      await send('Add concurrent topic ' + index + '.');
      for (let i = 0; i < 100 && !pending; i++) await page.waitForTimeout(20);
      assert.ok(pending, 'the actual App inference request is held');
      await page.getByRole('radio', {name: tool, exact: true}).locator('..').click();
      const zoom = Number((await page.locator('.excalidraw button').filter({hasText: /^\d+%$/}).textContent()).replace('%', '')) / 100;
      await page.mouse.move(790, 300); await page.mouse.down();
      await page.mouse.move(880, 360, {steps: 10});
      await pending.fulfill(response('N 「Concurrent topic ' + index + '」')); pending = null;
      await page.waitForTimeout(900);
      const during = await read();
      check(during.lines.length === before.lines.length + 1 && during.lines.at(-1).ms === undefined, `${tool}: accepted transcript saves while the gesture remains down`);
      check(during.graph.nodes.length === before.graph.nodes.length, `${tool}: the returned model result waits for the direct edit`);
      await page.screenshot({path: `${evidence}/${tool.toLowerCase()}-during.png`});
      await page.mouse.move(1090, 500, {steps: 20}); await page.mouse.up();
      const after = await saved(m => m.graph.nodes.length === before.graph.nodes.length + 1 && m.lines.at(-1).ms !== undefined && free(m).length > free(before).length);
      const shape = free(after).find(e => !free(before).some(old => old.id === e.id));
      const steps = after.steps.slice(before.steps.length);
      await fs.writeFile(`${evidence}/${tool.toLowerCase()}.json`, JSON.stringify({before, during, after, shape, steps, zoom}, null, 2) + '\n');
      check(Math.abs(shape.width - 300 / zoom) < 2 / zoom && Math.abs(shape.height - 200 / zoom) < 2 / zoom, `${tool}: the entire drag reaches the final pointer position after model arrival`);
      check(steps.length === 2, `${tool}: one user gesture and one model increment produce exactly two steps`);
      const appearances = steps.flatMap(s => s.scene?.elements.filter(e => e.id === shape.id && !e.isDeleted) ?? []);
      check(appearances.length > 0 && appearances.every(e => e.width === shape.width && e.height === shape.height && JSON.stringify(e.points) === JSON.stringify(shape.points)), `${tool}: model arrival and autosave never record an unfinished gesture`);
      await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      await page.getByRole('button', {name: 'Undo', exact: true}).click();
      const undone = await saved(m => JSON.stringify(m.graph) !== JSON.stringify(after.graph) || JSON.stringify(free(m)) !== JSON.stringify(free(after)));
      check(free(undone).some(e => e.id === shape.id) ? JSON.stringify(free(undone).find(e => e.id === shape.id)) === JSON.stringify(shape) : true, `${tool}: Undo never restores a partial gesture`);
      await page.getByRole('button', {name: 'Undo', exact: true}).click();
      const original = await saved(m => m.graph.nodes.length === before.graph.nodes.length && free(m).length === free(before).length);
      check(original.graph.nodes.length === before.graph.nodes.length, `${tool}: exactly two undo actions remove the model increment and the whole gesture`);
      await page.getByRole('button', {name: 'Redo', exact: true}).click();
      await page.getByRole('button', {name: 'Redo', exact: true}).click();
      const restored = await saved(m => m.graph.nodes.length === after.graph.nodes.length && free(m).some(e => e.id === shape.id));
      assert.deepEqual(free(restored).find(e => e.id === shape.id), shape);
      check(true, `${tool}: two redo actions restore the complete user and model edits`);
    }
    const beforeReload = await read();
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    assert.deepEqual(free(await read()), free(beforeReload));
    assert.deepEqual((await read()).graph, beforeReload.graph);
    check(true, 'completed concurrent edits survive reload');
    check(!report.pageErrors.length, 'no browser script errors');
  } catch (error) { report.error = String(error); await fs.writeFile(`${evidence}/failure-meeting.json`, JSON.stringify(await read(), null, 2) + '\n'); await page.screenshot({path: `${evidence}/failure.png`}); throw error; }
  finally { if (pending) await pending.abort().catch(() => {}); await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n'); }
  return report;
};
