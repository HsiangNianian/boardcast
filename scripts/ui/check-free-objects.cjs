const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkFreeObjects(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  await page.setViewportSize({width: 1600, height: 1000});
  const report = {checks: [], pageErrors: []};
  const check = (ok, name) => { report.checks.push({name, ok: !!ok}); assert.ok(ok, name); console.log('PASS:', name); };
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    if (window.top !== window) return;
    // Exercise the standard file input used by WebKit; Chromium's native picker has no Playwright event.
    delete window.showOpenFilePicker;
    for (const [key, value] of Object.entries({lang: 'en', log: 'closed', 'mode-hint': 'dismissed', localLlm: 'http://127.0.0.1:19876'})) localStorage.setItem('boardcast.' + key, value);
    if (!localStorage.getItem('boardcast.mode')) localStorage.setItem('boardcast.mode', 'pencil');
  });
  let replies = 0;
  await page.route('http://127.0.0.1:19876/**', route => route.fulfill({
    status: 200, contentType: 'application/json', headers: {'access-control-allow-origin': '*'},
    body: JSON.stringify(route.request().method() === 'POST' ? {choices: [{message: {content: `N 「Generated after free editing ${++replies}」`}}]} : {status: 'ok'}),
  }));
  const read = () => page.evaluate(async () => {
    if (!(await indexedDB.databases()).some(db => db.name === 'boardcast-v2')) return null;
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    try {return await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});}
    finally {db.close();}
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) {const row = await read(); if (row && predicate(row)) {await page.locator('.save-state.saved').waitFor(); return read();} await page.waitForTimeout(100);}
    throw Error('Expected free object edit was not saved');
  };
  const live = scene => (scene?.elements ?? []).filter(e => !e.isDeleted);
  const fields = e => JSON.parse(JSON.stringify(Object.fromEntries(Object.entries(e).filter(([key]) => !['version', 'versionNonce', 'updated', 'index'].includes(key)))));
  const retained = (row, before) => before.every(old => {
    const current = live(row.scene ?? row).find(e => e.id === old.id);
    assert.deepEqual(current && fields(current), fields(old), `${old.type} ${old.id} keeps its fields`);
    return true;
  });
  const mode = async name => {
    await page.getByRole('radio', {name, exact: true}).click();
    if (name === 'Free edit') await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    else await page.locator('image.free-scene').waitFor();
  };
  const drag = async (x, y, endX, endY) => {await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(endX, endY, {steps: 12}); await page.mouse.up();};
  const record = async name => {
    const row = await read();
    await fs.writeFile(`${evidence}/${name}.json`, JSON.stringify(row, null, 2) + '\n');
    await page.screenshot({path: `${evidence}/${name}.png`});
    return row;
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    const canvas = page.locator('canvas.excalidraw__canvas.interactive');
    const initialBounds = await canvas.boundingBox();
    for (const [name, type, start, end] of [
      ['Rectangle', 'rectangle', [350, 210], [500, 310]],
      ['Ellipse', 'ellipse', [700, 210], [850, 310]],
      ['Diamond', 'diamond', [1050, 210], [1200, 310]],
      ['Arrow', 'arrow', [502, 260], [698, 260]],
      ['Line', 'line', [350, 440], [500, 540]],
      ['Draw', 'freedraw', [700, 440], [850, 540]],
    ]) {
      await page.getByRole('radio', {name, exact: true}).locator('..').click();
      await drag(...start, ...end);
      const row = await saved(m => live(m.scene).some(e => e.type === type));
      check(true, `${type}: a real pointer gesture saves the object`);
      if (type === 'rectangle') {
        const shape = live(row.scene).find(e => e.type === type);
        check(shape.width === 150 && shape.height === 100, 'the first drag retains its full 150 by 100 size');
        check((await canvas.boundingBox()).x === initialBounds.x, 'the first object does not move the canvas under the pointer');
        await page.keyboard.press('v'); await page.mouse.click(425, 210); await page.keyboard.press('Delete');
        await saved(m => live(m.scene).length === 0);
        check((await canvas.boundingBox()).x === initialBounds.x, 'deleting the last free object does not move the canvas');
        await page.getByRole('button', {name: 'Undo', exact: true}).first().click();
        await saved(m => live(m.scene).some(e => e.id === shape.id));
      }
    }
    const arrow = live((await read()).scene).find(e => e.type === 'arrow');
    check(arrow.startBinding && arrow.endBinding, 'the drawn arrow binds to both original shapes');
    await page.getByRole('radio', {name: 'Text', exact: true}).locator('..').click();
    await page.mouse.click(1050, 440); await page.locator('textarea.excalidraw-wysiwyg').fill('自由文字 / Notes'); await page.keyboard.press('Escape');
    await saved(m => live(m.scene).some(e => e.type === 'text' && e.originalText === '自由文字 / Notes'));
    const choose = page.waitForEvent('filechooser');
    await page.getByRole('radio', {name: 'Insert image', exact: true}).locator('..').click();
    await (await choose).setFiles({name: 'free-picture.svg', mimeType: 'image/svg+xml', buffer: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><rect width="160" height="100" fill="#1871c2"/><circle cx="80" cy="50" r="32" fill="#ffb81c"/></svg>')});
    await drag(350, 650, 510, 750);
    await saved(m => live(m.scene).some(e => e.type === 'image' && m.scene.files[e.fileId]));
    check(true, 'an image inserted through the editor file chooser saves its attachment');
    await page.getByRole('button', {name: 'More tools', exact: true}).click();
    await page.getByRole('button', {name: /^Frame tool/}).click();
    await drag(1010, 180, 1260, 540);
    let row = await saved(m => live(m.scene).some(e => e.type === 'frame'));
    const frame = live(row.scene).find(e => e.type === 'frame');
    check(live(row.scene).filter(e => e.frameId === frame.id).length === 2, 'the frame captures the diamond and free text');
    await page.getByRole('button', {name: 'More tools', exact: true}).click();
    await page.getByRole('button', {name: 'Web Embed', exact: true}).click();
    await drag(1050, 650, 1250, 800);
    await page.getByRole('textbox', {name: 'Type or paste your link here', exact: true}).fill('https://example.invalid/boardcast-free-object');
    await page.keyboard.press('Enter');
    row = await saved(m => live(m.scene).some(e => e.type === 'embeddable' && e.link === 'https://example.invalid/boardcast-free-object'));
    const original = live(row.scene);
    check(new Set(original.map(e => e.type)).size === 10 && row.graph.nodes.length === 0, 'ten editable object types form a saved scene-only board');
    await record('created');
    await mode('Hand-drawn'); await record('hand-drawn');
    await mode('Free edit');
    check(retained(await read(), original), 'view switches preserve every visual field, binding and frame membership');
    await page.getByRole('button', {name: 'New board', exact: true}).click();
    await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    check(retained(await read(), original), 'returning from another board preserves every object');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    check(retained(await read(), original), 'cold browser reopen retains every object and all bindings');
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('Add one topic while keeping my drawing.');
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    row = await saved(m => m.graph.nodes.length === 1 && m.lines.at(-1)?.ms !== undefined);
    check(retained(row, original), 'a model increment preserves all original free objects');
    await record('model-increment');
    await mode('Hand-drawn');
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: 'Current board', exact: true}).click();
    const pending = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: /^Excalidraw/}).click();
    const file = `${evidence}/free-objects.excalidraw`; await (await pending).saveAs(file);
    const drawing = JSON.parse(await fs.readFile(file, 'utf8'));
    check(retained(drawing, original), 'the actual editable export contains every original object');
    const picture = original.find(e => e.type === 'image');
    check(drawing.files[picture.fileId]?.dataURL.startsWith('data:image/svg+xml;base64,'), 'the exported image contains its bytes, not a private asset reference');
    await page.locator('input[type=file]').setInputFiles(file);
    const dialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
    await dialog.getByRole('button', {name: 'New meeting', exact: true}).click(); await dialog.waitFor({state: 'hidden'});
    row = await saved(m => m.id !== row.id && live(m.scene).some(e => e.id === picture.id));
    check(retained(row, original), 'reimporting the actual export restores all free objects');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await page.locator('image.free-scene').waitFor();
    check(retained(await read(), original), 'reimported data survives another cold browser reopen');
    await record('reimported');
    // These two supported import types have no creation tool in the bundled editor.
    const extra = ['magicframe', 'iframe'].map((type, i) => ({
      ...original[0], id: `imported-${type}`, type, x: 1400, y: 80 + i * 260, width: 240, height: 180,
      boundElements: null, frameId: null, groupIds: [], name: type === 'magicframe' ? 'Imported frame' : undefined,
      customData: type === 'iframe' ? {generationData: {status: 'done', html: '<!doctype html><p>Static imported content</p>'}} : {fixture: 'imported-frame'},
    }));
    drawing.elements.push(...extra);
    const allTypes = [...original, ...extra];
    await fs.writeFile(`${evidence}/all-types-input.excalidraw`, JSON.stringify(drawing, null, 2) + '\n');
    await page.locator('input[type=file]').setInputFiles(`${evidence}/all-types-input.excalidraw`);
    await dialog.getByRole('button', {name: 'New meeting', exact: true}).click(); await dialog.waitFor({state: 'hidden'});
    row = await saved(m => live(m.scene).some(e => e.id === 'imported-iframe'));
    check(retained(row, allTypes), 'all twelve supported object types import with their source fields');
    await mode('Free edit'); await mode('Hand-drawn');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await page.locator('image.free-scene').waitFor();
    check(retained(await read(), allTypes), 'the two import-only types also survive editor initialization and cold reopen');
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('Add another topic to the imported drawing.');
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    row = await saved(m => m.graph.nodes.length === 2 && m.lines.at(-1)?.ms !== undefined);
    check(retained(row, allTypes), 'all twelve types survive another model increment');
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: 'Current board', exact: true}).click();
    const allDownload = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: /^Excalidraw/}).click();
    const allFile = `${evidence}/all-types-export.excalidraw`; await (await allDownload).saveAs(allFile);
    check(retained(JSON.parse(await fs.readFile(allFile, 'utf8')), allTypes), 'the final editable export retains all twelve types');
    await record('all-types');
    check(report.pageErrors.length === 0, 'no uncaught browser errors');
  } catch (error) {
    report.error = String(error); await record('failure').catch(() => {});
    await fs.writeFile(`${evidence}/failure-ui.txt`, await page.locator('body').ariaSnapshot());
    throw error;
  } finally {await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n');}
};
