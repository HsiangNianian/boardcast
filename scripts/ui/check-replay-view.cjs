const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { pathToFileURL } = require('node:url');

module.exports = async function checkReplayView(page, baseUrl, evidence = '/tmp/boardcast-replay-view') {
  await fs.mkdir(evidence, { recursive: true });
  const report = { checks: [], errors: [], exports: {} };
  const check = (ok, name) => report.checks.push({ name, ok });
  page.on('pageerror', error => report.errors.push(error.message));
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({ lang: 'en', log: 'closed', 'mode-hint': 'dismissed' })) localStorage.setItem('boardcast.' + key, value);
    if (!localStorage.getItem('boardcast.mode')) localStorage.setItem('boardcast.mode', 'ink');
  });
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    const row = await new Promise((yes, no) => { const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
    db.close(); return row;
  });
  const download = async name => {
    await page.getByRole('button', { name: 'Export', exact: true }).click();
    await page.getByRole('menuitemradio', { name: 'Whole meeting', exact: true }).click();
    const pending = page.waitForEvent('download', { timeout: 60000 });
    await page.getByRole('menuitem', { name: /^Replay page/ }).click();
    const file = `${evidence}/${name}.html`;
    await (await pending).saveAs(file);
    const text = await fs.readFile(file, 'utf8');
    const data = JSON.parse(text.split('window.__boardcastData = ')[1].split(';\nwindow.__boardcastHanzi')[0]);
    report.exports[name] = data.pages.map(p => ({ count: p.steps?.length ?? 0, times: p.steps?.map(s => s.t) ?? [] }));
    await fs.writeFile(`${evidence}/${name}-data.json`, JSON.stringify(data, null, 2) + '\n');
    return data;
  };
  const mode = async name => {
    await page.getByRole('radio', { name, exact: true }).click();
    if (name === 'Free edit') await page.locator('.pencil-stage[data-ready=true]').waitFor();
    await page.waitForTimeout(750);
  };
  const show = async n => {
    await page.locator('.pages').getByRole('button', { name: `Board ${n}`, exact: true }).click();
    await page.waitForTimeout(750);
  };
  // Stored overlays use asset references; exports embed the same images.
  const steps = data => data.pages.map(p => p.steps?.map(s => ({ ...s, scene: s.scene && { ...s.scene, overlay: undefined } })));
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const free = scene => scene?.elements.filter(e => !e.isDeleted && !e.customData?.boardcast) ?? [];
  let offline;
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({ state: 'hidden' });
    report.scripts = await page.locator('script[src]').evaluateAll(s => s.map(e => e.src));
    const original = await page.evaluate(async () => {
      const start = Date.now() - 60000;
      const node = label => ({ id: 'n1', label, aliases: [], kind: 'node', marks: [], ink: 'ink' });
      const graph = label => ({ nodes: [node(label)], edges: [], groups: [], nextNode: 2, nextGroup: 1 });
      const pages = ['First', 'Second'].map((label, i) => ({ graph: graph(label + ' final'), steps: [
        { t: start + i * 10000, line: i * 2 + 1, d: { nodes: { set: [node(label + ' draft')] }, nextNode: 2 } },
        { t: start + i * 10000 + 5000, line: i * 2 + 2, d: { nodes: { set: [node(label + ' final')] } } },
      ] }));
      const row = { version: 2, storageRevision: 1, id: 'qa-replay-view', title: 'View without editing', titled: true, createdAt: start, updatedAt: start + 15000, graph: pages[0].graph, history: [], lines: ['First draft', 'First final', 'Second draft', 'Second final'].map((text, i) => ({ text, ms: 1, page: Math.floor(i / 2) })), steps: pages[0].steps, pages, page: 0 };
      const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => { r.result.createObjectStore('meetings', { keyPath: 'id' }); r.result.createObjectStore('assets'); }; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
      await new Promise((yes, no) => { const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error); });
      db.close(); localStorage.setItem('boardcast.meeting', row.id); return row;
    });
    await fs.writeFile(`${evidence}/fixture.json`, JSON.stringify(original, null, 2) + '\n');
    await page.reload(); await page.locator('.splash').waitFor({ state: 'hidden' });
    const before = await download('before');
    check(same(steps(before), steps(original)), 'hand-drawn export preserves historical steps');
    await mode('Free edit');
    await show(2); await show(1); await mode('Hand-drawn'); await mode('Free edit');
    await page.setViewportSize({ width: 1100, height: 740 });
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(750);
    await page.screenshot({ path: `${evidence}/viewed.png` });
    const viewed = await download('viewed');
    check(same(steps(viewed), steps(original)), 'first Free edit, page changes, mode changes and resize add no replay steps');
    check(same(viewed.pages.map(p => p.graph), original.pages.map(p => p.graph)) && same(viewed.lines, original.lines), 'passive viewing preserves graphs and transcript');
    await page.reload(); await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.locator('.pencil-stage[data-ready=true]').waitFor(); await page.waitForTimeout(750);
    const pencilReopen = await download('pencil-reopen');
    check(same(steps(pencilReopen), steps(original)), 'cold opening directly into Free edit adds no replay steps');
    await page.getByRole('button', { name: 'Replay', exact: true }).click();
    const range = page.locator('.replay-dock input[type=range]');
    await range.waitFor();
    check(await range.getAttribute('max') === '2', 'in-app replay retains the two historical steps');
    await page.locator('.replay-dock').getByRole('button', { name: 'Exit replay', exact: true }).click();
    await page.locator('.pencil-stage[data-ready=true]').waitFor();
    await page.waitForTimeout(750);
    const body = viewed.pages[0].scene.elements.find(e => e.id === 'n1');
    const canvas = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    const x = canvas.x + canvas.width / 2, y = canvas.y + canvas.height / 2 + 15;
    const actionStart = Date.now();
    await page.keyboard.press('v');
    await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x + 80, y + 40, { steps: 12 }); await page.mouse.up();
    await page.waitForTimeout(1100);
    const moved = await download('moved');
    const movedBody = moved.pages[0].scene.elements.find(e => e.id === 'n1');
    const last = moved.pages[0].steps.at(-1);
    check(Math.abs(movedBody.x - body.x) > 20 && last.scene?.elements.some(e => e.id === 'n1' && e.x === movedBody.x), 'one real drag saves its complete scene');
    check(moved.pages[0].steps.length === viewed.pages[0].steps.length + 1 && last.t >= actionStart && same(last.d, {}), 'one real drag records exactly one current scene-only step');
    await mode('Hand-drawn');
    await page.getByRole('button', { name: 'Undo', exact: true }).click(); await page.waitForTimeout(750);
    const undone = await download('undone');
    check(undone.pages[0].steps.length === moved.pages[0].steps.length + 1 && Math.abs(undone.pages[0].scene.elements.find(e => e.id === 'n1').x - body.x) < 1, 'undo records a step restoring the generated position');
    await page.getByRole('button', { name: 'Redo', exact: true }).click(); await page.waitForTimeout(750);
    const redone = await download('redone');
    check(redone.pages[0].steps.length === undone.pages[0].steps.length + 1 && redone.pages[0].scene.elements.find(e => e.id === 'n1').x === movedBody.x, 'redo records a step restoring the user position');
    await page.getByRole('button', { name: 'New board', exact: true }).click(); await mode('Free edit');
    await page.getByRole('radio', { name: 'Rectangle', exact: true }).locator('..').click();
    await page.mouse.move(420, 300); await page.mouse.down(); await page.mouse.move(650, 430, { steps: 12 }); await page.mouse.up();
    await page.waitForTimeout(1100);
    const drawn = await download('drawn');
    check(drawn.pages[2].steps.length === 1 && free(drawn.pages[2].steps[0].scene).length === 1, 'a first free object on an empty board records one step');
    await mode('Hand-drawn');
    await page.getByRole('button', { name: 'Undo', exact: true }).click(); await page.waitForTimeout(750);
    const removed = await download('removed');
    check(removed.pages[2].steps.length === 2 && free(removed.pages[2].steps[1].scene).length === 0, 'undo of the last free object records an empty scene');
    await page.getByRole('button', { name: 'Redo', exact: true }).click(); await page.waitForTimeout(750);
    const restored = await download('restored');
    check(restored.pages[2].steps.length === 3 && free(restored.pages[2].steps[2].scene).length === 1, 'redo restores a scene-only board in replay history');
    await page.reload(); await page.locator('.splash').waitFor({ state: 'hidden' });
    report.reopenScripts = await page.locator('script[src]').evaluateAll(s => s.map(e => e.src));
    await mode('Free edit'); await show(1); await show(2); await show(3); await mode('Hand-drawn');
    const reopened = await download('reopened');
    check(same(steps(reopened), steps(restored)), 'reopening and browsing customized, pristine and free-only pages leaves history unchanged');
    const saved = await read();
    check(same(steps(saved), steps(reopened)), 'saved history matches actual exported history');
    await fs.writeFile(`${evidence}/saved.json`, JSON.stringify(saved, null, 2) + '\n');
    await page.getByRole('button', { name: 'Transcript', exact: true }).click();
    await page.locator('.sidebar-tabs').getByRole('tab', { name: 'Contents', exact: true }).click();
    await page.getByRole('region', { name: 'Free objects', exact: true }).getByRole('button', { name: 'Delete', exact: true }).click();
    await page.waitForTimeout(750);
    const deleted = await download('deleted');
    check(deleted.pages[2].steps.length === 4 && free(deleted.pages[2].steps[3].scene).length === 0, 'explicitly deleting the last free object records its removal');
    const legacy = { type: 'excalidraw', version: 2, elements: viewed.pages[1].scene.elements.map(e => ({ ...e, customData: undefined })), files: {} };
    await fs.writeFile(`${evidence}/legacy.excalidraw`, JSON.stringify(legacy));
    await page.locator('input[type=file]').setInputFiles(`${evidence}/legacy.excalidraw`);
    const dialog = page.getByRole('dialog', { name: 'Import materials', exact: true });
    await dialog.getByRole('button', { name: 'Add to this meeting', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' }); await page.waitForTimeout(750);
    const imported = await download('imported');
    await mode('Free edit'); await show(1); await show(3); await mode('Hand-drawn');
    const importedViewed = await download('imported-viewed');
    check(same(steps(imported), steps(importedViewed)), 'browsing an external Excalidraw import preserves its import history');
    offline = await page.context().browser().newContext({ offline: true, viewport: { width: 1280, height: 800 } });
    const player = await offline.newPage();
    const requests = [];
    player.on('request', request => { if (/^https?:/.test(request.url())) requests.push(request.url()); });
    player.on('pageerror', error => report.errors.push(error.message));
    await player.goto(pathToFileURL(`${evidence}/viewed.html`).href);
    const slider = player.locator('input[type=range]'); await slider.waitFor();
    check(await slider.getAttribute('max') === '2', 'actual offline HTML retains the original first-page history');
    await player.locator('.player-pages').getByRole('button', { name: '2', exact: true }).click();
    check(await slider.getAttribute('max') === '2', 'actual offline HTML retains the original second-page history');
    await player.screenshot({ path: `${evidence}/offline.png` });
    check(requests.length === 0 && report.errors.length === 0, 'browser and offline player have no script errors or offline network requests');
    report.ok = report.checks.every(c => c.ok);
    assert(report.ok, JSON.stringify(report.checks.filter(c => !c.ok)));
  } catch (error) { report.error = String(error); throw error; }
  finally {
    await offline?.close();
    await fs.writeFile(`${evidence}/replay-view-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
