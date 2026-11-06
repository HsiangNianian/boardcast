const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkFreeBoardFollow(page, baseUrl, evidence = '/tmp/boardcast-free-board-follow') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], positions: {}, pageErrors: []};
  const check = (ok, name) => report.checks.push({name, ok: !!ok});
  let reply = '';
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.theme', 'light');
    localStorage.setItem('boardcast.mode', 'pencil'); localStorage.setItem('boardcast.log', 'closed');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19876');
  });
  await page.route('http://127.0.0.1:19876/**', route => route.fulfill({
    contentType: 'application/json', headers: {'access-control-allow-origin': '*'},
    body: JSON.stringify(route.request().method() === 'POST' ? {choices: [{message: {content: reply}}]} : {status: 'ok', data: [{id: 'fixture'}]}),
  }));
  const read = () => page.evaluate(async () => {
    const db = await new Promise(resolve => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => resolve(r.result);});
    const row = await new Promise(resolve => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => resolve(r.result);});
    db.close(); return row;
  });
  const marker = () => page.locator('.excalidraw canvas').evaluateAll(canvases => {
    for (const canvas of canvases) {
      const {width, height} = canvas, data = canvas.getContext('2d').getImageData(0, 0, width, height).data;
      let x0 = width, y0 = height, x1 = -1, y1 = -1;
      for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        const i = (y * width + x) * 4;
        if (data[i] > 220 && data[i + 1] < 60 && data[i + 2] > 220 && data[i + 3] > 180) {
          x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y);
        }
      }
      if (x1 >= 0) return {x0, y0, x1, y1};
    }
    return null;
  });
  const pan = async () => {
    const b = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    const before = await marker();
    await page.mouse.click(b.x + 25, b.y + 100); await page.keyboard.press('h');
    await page.mouse.move(b.x + 25, b.y + 100); await page.mouse.down();
    await page.mouse.move(b.x + b.width - 25, b.y + 130, {steps: 12}); await page.waitForTimeout(100);
    const during = await marker(); await page.mouse.up();
    await page.waitForTimeout(200);
    (report.gestures ??= []).push({before, during, after: await marker()});
  };
  const latest = () => page.getByRole('button', {name: 'Show full board', exact: true});
  const send = async (text, count, operation) => {
    reply = operation ?? `N 「${text}」`;
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text);
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    for (let i = 0; i < 100 && (await read())?.graph.nodes.length !== count; i++) await page.waitForTimeout(100);
    assert.equal((await read()).graph.nodes.length, count);
    await page.locator('.save-state.saved').waitFor();
    await page.waitForTimeout(500);
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const graph = {nodes: [{id: 'n1', label: 'Free edit anchor', aliases: [], kind: 'node', marks: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
      const shape = {id: 'reading-marker', type: 'rectangle', x: 450, y: 160, width: 120, height: 80, angle: 0, strokeColor: '#ff00ff', backgroundColor: '#ff00ff', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: false};
      const scene = {elements: [shape], generated: [], files: {}};
      const row = {version: 2, storageRevision: 1, id: 'qa-free-follow', title: 'Free edit reading', titled: true, createdAt: Date.now(), updatedAt: Date.now(), graph, scene, history: [], lines: []};
      const db = await new Promise(resolve => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => resolve(r.result);});
      await new Promise((resolve, reject) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor(); await page.waitForTimeout(500);
    assert.ok(await marker(), 'a real rendered magenta rectangle provides a camera marker');
    await pan(); assert.equal(await marker(), null, 'the actual hand-tool gesture moves the marker outside the canvas');
    check(await latest().count() === 1, 'free-edit pan offers the return action');
    await send('Add a decision while I read elsewhere.', 2);
    check(await marker() === null, 'model updates do not recenter a panned free-edit canvas');
    await page.screenshot({path: `${evidence}/free-pan.png`});
    if (await latest().count()) await latest().click();
    else {await page.locator('canvas.excalidraw__canvas.interactive').focus(); await page.keyboard.press('Shift+Digit1');}
    await page.waitForTimeout(500);
    check(!!await marker(), 'explicit fit brings the existing free shape back into view');

    await page.emulateMedia({reducedMotion: 'reduce'});
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor(); await page.waitForTimeout(500);
    const before = await marker(); assert.ok(before);
    await send('Add eight decisions without moving the camera.', 10,
      Array.from({length: 8}, (_, i) => `N 「Expanded free-edit decision ${i + 1} with details」`).join('\n'));
    const after = await marker(); report.positions.reduced = {before, after};
    check(JSON.stringify(before) === JSON.stringify(after), 'reduced motion keeps the free shape at the same screen pixels');
    check(await latest().count() === 1, 'reduced motion provides an explicit fit action in free edit');
    await page.setViewportSize({width: 640, height: 400}); await pan();
    if (await latest().count()) {
      const b = await latest().boundingBox();
      check(b.x >= 0 && b.y >= 0 && b.x + b.width <= 640 && b.y + b.height <= 400, 'free-edit fit remains visible in a compact window');
      await latest().focus(); await page.keyboard.press('Enter');
      check(await page.locator('#board-panel').evaluate(el => el === document.activeElement), 'free-edit keyboard fit restores board focus');
    } else check(false, 'free-edit fit remains visible in a compact window');
    await page.waitForTimeout(500); await page.screenshot({path: `${evidence}/free-compact.png`});
    const shape = (await read()).scene.elements.find(el => el.id === 'reading-marker');
    check(shape.x === 450 && shape.y === 160 && shape.width === 120 && shape.height === 80, 'camera operations do not move or resize the saved free object');
    await page.setViewportSize({width: 1280, height: 800});
    await page.emulateMedia({reducedMotion: 'no-preference'});
    await page.getByRole('button', {name: 'New meeting', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await pan();
    check(await latest().count() === 1, 'panning an empty free-edit board also offers a return action');
    const zoom = page.locator('.excalidraw button').filter({hasText: /^\d+%$/});
    const beforeZoom = await zoom.textContent();
    assert.notEqual(beforeZoom, '100%', 'this case starts with a non-default camera scale');
    await send('First decision after panning the empty board.', 1);
    report.positions.firstObject = {beforeZoom, afterZoom: await zoom.textContent()};
    check(beforeZoom === await zoom.textContent() && await latest().count() === 1, 'the first generated object keeps the chosen zoom after panning an empty board');
    check(report.pageErrors.length === 0, 'free-edit camera flow has no uncaught script errors');
    report.passed = report.checks.every(item => item.ok);
    assert.ok(report.passed, JSON.stringify(report.checks.filter(item => !item.ok)));
  } catch (error) {
    report.error = String(error); await page.screenshot({path: `${evidence}/failure.png`}); throw error;
  } finally {await fs.writeFile(`${evidence}/free-board-follow-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
