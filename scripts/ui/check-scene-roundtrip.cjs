const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkSceneRoundtrip(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, { recursive: true });
  const errors = [];
  const report = { checkpoints: [], pageErrors: errors };
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19876');
  });
  await page.route('http://127.0.0.1:19876/**', route => route.fulfill({
    status: 200, contentType: 'application/json', headers: { 'access-control-allow-origin': '*' },
    body: JSON.stringify(route.request().method() === 'POST'
      ? { choices: [{ message: { content: 'R n1 「Model overwrite」\nN 「New increment」' } }] }
      : { status: 'ok', data: [{ id: 'test-model' }] }),
  }));
  const read = async () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => { r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets'); }; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    const id = localStorage.getItem('boardcast.meeting');
    const row = await new Promise((yes, no) => { const r = db.transaction('meetings').objectStore('meetings').get(id); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    db.close(); return row;
  });
  const saved = async (predicate, description) => {
    for (let i = 0; i < 80; i++) { const row = await read(); if (row && predicate(row)) return row; await page.waitForTimeout(100); }
    throw new Error(`Saved data did not satisfy: ${description}; ${JSON.stringify(await read())}`);
  };
  const checkpoint = async name => { console.log(name); report.checkpoints.push(name); await page.screenshot({ path: `${evidence}/scene-${name}.png` }); };
  const shape = row => row.scene?.elements.find(e => e.id === 'n1' && !e.isDeleted);
  const padSelect = async () => {
    const el = shape(await read()); assert.ok(el, 'generated body is present');
    const p = await page.locator('.ink > svg').evaluate((svg, el) => {
      const point = new DOMPoint(el.x + el.width / 2, el.y + el.height / 2).matrixTransform(svg.getScreenCTM());
      return { x: point.x, y: point.y };
    }, el);
    await page.mouse.click(p.x, p.y);
    await page.getByRole('button', { name: 'Rename', exact: true }).waitFor();
  };
  try {
    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.evaluate(async () => {
      const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => { r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets'); }; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
      const graph = { nodes: [{ id: 'n1', label: 'Original node', aliases: [], kind: 'node', marks: [], ink: 'ink' }], edges: [], groups: [], nextNode: 2, nextGroup: 1 };
      const row = { version: 2, storageRevision: 1, id: 'qa-scene-roundtrip', title: 'Scene roundtrip', titled: true, createdAt: Date.now(), updatedAt: Date.now(), graph, history: [], lines: [] };
      await new Promise((yes, no) => { const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error); });
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    await page.reload();
    await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.getByRole('radio', { name: 'Free edit', exact: true }).click();
    const canvas = page.locator('canvas.excalidraw__canvas.interactive');
    await canvas.waitFor();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.waitForTimeout(700);
    // A single generated node is fitted at the center of the editor viewport.
    const box = await canvas.boundingBox();
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    await page.mouse.dblclick(center.x, center.y);
    const text = page.locator('textarea.excalidraw-wysiwyg');
    await text.waitFor();
    await text.fill('PC choice');
    await page.keyboard.press('Escape');
    let row = await saved(m => m.graph.nodes[0]?.label === 'PC choice', 'PC rename reaches semantic graph');
    const beforeMove = shape(row);
    await checkpoint('pc-rename');
    await page.keyboard.press('v');
    await page.mouse.move(center.x, center.y + 15); await page.mouse.down();
    await page.mouse.move(center.x + 100, center.y + 55, { steps: 12 }); await page.mouse.up();
    row = await saved(m => shape(m)?.x !== beforeMove.x, 'PC drag reaches saved scene');
    assert.ok(Math.abs(shape(row).x - beforeMove.x) > 20);
    await page.getByRole('radio', { name: 'Hand-drawn', exact: true }).click();
    await page.locator('image.free-scene').waitFor();
    const movedX = shape(row).x;
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await saved(m => Math.abs(shape(m)?.x - beforeMove.x) < 1, 'one undo restores the whole pointer gesture');
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await saved(m => Math.abs(shape(m)?.x - movedX) < 1, 'redo restores the complete drag');
    await padSelect();
    await page.getByRole('button', { name: 'Rename', exact: true }).click();
    await page.getByRole('textbox', { name: 'Rename', exact: true }).fill('PAD choice');
    await page.keyboard.press('Enter');
    row = await saved(m => m.graph.nodes[0]?.label === 'PAD choice' && m.scene.elements.some(e => e.type === 'text' && e.originalText === 'PAD choice'), 'PAD rename replaces earlier PC text override');
    await checkpoint('pad-rename');
    const visualOwnership = m => Object.values(m.scene.overrides ?? {}).some(fields => Object.keys(fields).some(key => !['text', 'originalText', 'isDeleted'].includes(key)));
    assert.ok(visualOwnership(row));
    assert.equal(await page.getByRole('button', { name: 'Use model suggestion', exact: true }).count(), 0, 'no model candidate must not claim adoption');
    await page.getByRole('button', { name: 'Reset styling', exact: true }).click();
    await saved(m => m.graph.nodes[0]?.label === 'PAD choice' && !visualOwnership(m), 'style reset keeps semantic user text');
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await saved(m => m.graph.nodes[0]?.label === 'PAD choice' && visualOwnership(m), 'style reset undo restores owned visual fields');
    await page.getByRole('textbox', { name: 'What was said', exact: true }).fill('Please add another idea to this board.');
    await page.getByRole('button', { name: 'Put on the board', exact: true }).click();
    row = await saved(m => m.graph.nodes.length === 2, 'model increment accepted');
    assert.equal(row.graph.nodes.find(n => n.id === 'n1').label, 'PAD choice');
    assert.ok(row.scene.elements.some(e => e.type === 'text' && e.originalText === 'PAD choice'));
    await padSelect();
    await page.getByRole('button', { name: 'Use model suggestion', exact: true }).click();
    await saved(m => m.graph.nodes.find(n => n.id === 'n1')?.label === 'Model overwrite' && m.scene.elements.some(e => e.type === 'text' && e.originalText === 'Model overwrite'), 'explicit adoption uses the actual model candidate');
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await saved(m => m.graph.nodes.find(n => n.id === 'n1')?.label === 'PAD choice' && m.scene.modelCandidates?.n1?.label === 'Model overwrite', 'one undo restores the user content and candidate');
    await padSelect();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await saved(m => !m.graph.nodes.some(n => n.id === 'n1') && !m.scene.elements.some(e => e.customData?.boardcast?.id === 'n1' && !e.isDeleted), 'PAD deletion removes customized PC object');
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await saved(m => m.graph.nodes.some(n => n.id === 'n1' && n.label === 'PAD choice') && shape(m), 'undo restores graph and scene');
    await page.getByRole('button', { name: 'Redo', exact: true }).click();
    await saved(m => !m.graph.nodes.some(n => n.id === 'n1'), 'redo deletes again');
    await checkpoint('model-and-undo');
    await page.getByRole('button', { name: 'New board', exact: true }).click();
    await page.getByRole('radio', { name: 'Free edit', exact: true }).click();
    await canvas.waitFor();
    await page.getByRole('radio', { name: 'Rectangle', exact: true }).locator('..').click();
    await page.mouse.move(420, 300); await page.mouse.down(); await page.mouse.move(650, 430, { steps: 12 }); await page.mouse.up();
    row = await saved(m => m.page === 1 && m.scene?.elements.some(e => e.type === 'rectangle' && !e.isDeleted), 'scene-only board saves');
    const freeId = row.scene.elements.find(e => e.type === 'rectangle' && !e.isDeleted).id;
    await page.getByRole('radio', { name: 'Hand-drawn', exact: true }).click();
    await page.locator('.pages').getByRole('button', { name: 'Board 1', exact: true }).click();
    await page.locator('.pages').getByRole('button', { name: 'Board 2', exact: true }).click();
    await page.locator('image.free-scene').waitFor();
    await page.locator('.save-state.saved').waitFor();
    await page.reload();
    await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.locator('image.free-scene').waitFor();
    row = await read(); assert.ok(row.scene.elements.some(e => e.id === freeId && !e.isDeleted));
    await page.getByRole('button', { name: 'Replay', exact: true }).click();
    const range = page.locator('.replay-dock input[type="range"]');
    await range.waitFor(); await range.focus(); await page.keyboard.press('End');
    await page.locator('image.free-scene').waitFor();
    await page.locator('.replay-dock button.fork').click();
    row = await saved(m => m.pages?.length === 3 && m.page === 2, 'fork produces a third board');
    assert.ok(row.scene.elements.some(e => e.id === freeId && !e.isDeleted));
    assert.ok(row.pages[1].scene.elements.some(e => e.id === freeId && !e.isDeleted));
    await checkpoint('scene-only-fork');
    assert.deepEqual(errors, []);
    report.passed = true;
    report.generatedNodeEdit = { pcRename: true, pcDragSaved: true, dragUndoRedo: true, padRename: true, styleResetUndo: true, modelConflictProtected: true, explicitModelAdoptionUndo: true, padDeleteUndoRedo: true };
    report.sceneOnlyBoard = { pageSwitch: true, reload: true, replay: true, fork: true };
    await fs.rm(`${evidence}/scene-failure.png`, { force: true });
    return report;
  } catch (error) {
    report.error = String(error);
    await page.screenshot({ path: `${evidence}/scene-failure.png` });
    throw error;
  } finally {
    await fs.writeFile(`${evidence}/scene-roundtrip-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
