const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkPencilBatch(page, baseUrl, evidence = '/tmp/boardcast-pencil-batch') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: []};
  const check = (ok, name) => { report.checks.push({name, ok: !!ok}); assert.ok(ok, name); };
  page.on('pageerror', e => report.pageErrors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.mode', 'pencil');
    localStorage.setItem('boardcast.mode-hint', 'dismissed');
    localStorage.setItem('boardcast.log', 'closed');
  });
  const read = () => page.evaluate(async () => {
    const id = localStorage.getItem('boardcast.meeting');
    if (!id) return undefined;
    const db = await new Promise((resolve, reject) => {
      const r = indexedDB.open('boardcast-v2');
      r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);
    });
    try {
      return await new Promise((resolve, reject) => {
        const r = db.transaction('meetings').objectStore('meetings').get(id);
        r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);
      });
    } finally { db.close(); }
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) {
      const row = await read();
      if (row && predicate(row)) { await page.locator('.save-state.saved').waitFor(); return read(); }
      await page.waitForTimeout(100);
    }
    throw new Error('Edited meeting did not save');
  };
  const visible = row => row?.scene?.elements.filter(e => !e.isDeleted) ?? [];
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    if (await page.getByRole('button', {name: 'Got it', exact: true}).count()) await page.getByRole('button', {name: 'Got it', exact: true}).click();
    for (const tool of ['Rectangle', 'Draw']) {
      await page.getByRole('radio', {name: tool, exact: true}).locator('..').click();
      const before = await read(), oldIds = visible(before).map(e => e.id), stepsBefore = before?.steps?.length ?? 0;
      await page.mouse.move(600, 300); await page.mouse.down();
      for (let i = 1; i <= 24; i++) {
        await page.mouse.move(600 + i * 8, 300 + i * 5);
        if (i < 24) await page.waitForTimeout(25);
      }
      await page.mouse.up();
      const row = await saved(m => visible(m).some(e => !oldIds.includes(e.id)));
      const shape = visible(row).find(e => !oldIds.includes(e.id));
      const steps = row.steps.slice(stepsBefore);
      await fs.writeFile(`${evidence}/${tool.toLowerCase()}.json`, JSON.stringify({before, after: row, shape, steps}, null, 2) + '\n');
      check(steps.length === 1, `${tool}: one complete pointer gesture produces one replay step`);
      const recorded = steps[0].scene.elements.find(e => e.id === shape.id);
      assert.deepEqual(recorded, shape); check(true, `${tool}: the recorded step has the final saved geometry`);
      await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      await page.getByRole('button', {name: 'Undo', exact: true}).click();
      await saved(m => !visible(m).some(e => e.id === shape.id));
      check(true, `${tool}: one global undo removes the whole gesture`);
      await page.getByRole('button', {name: 'Redo', exact: true}).click();
      const restored = await saved(m => visible(m).some(e => e.id === shape.id));
      assert.deepEqual(visible(restored).find(e => e.id === shape.id), shape);
      check(true, `${tool}: global redo restores the final shape`);
      await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
      await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    }
    const beforeReload = await read();
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    assert.deepEqual(visible(await read()), visible(beforeReload)); check(true, 'both completed gestures survive reload');
    check(report.pageErrors.length === 0, 'no browser script errors');
    await page.screenshot({path: `${evidence}/final.png`});
  } catch (error) {
    report.error = String(error);
    await page.screenshot({path: `${evidence}/failure.png`});
    throw error;
  } finally { await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n'); }
  return report;
};
