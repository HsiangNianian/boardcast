const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const {fixtures} = require('./check-drawing-import.cjs');

module.exports = async function checkReplayFork(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: [], requests: []};
  const check = (ok, name) => {report.checks.push({name, ok: !!ok}); assert.ok(ok, name); console.log('PASS:', name);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    if (window.top !== window) return;
    for (const [key, value] of Object.entries({lang: 'en', log: 'open', 'mode-hint': 'dismissed', localLlm: 'http://127.0.0.1:19876'})) localStorage.setItem('boardcast.' + key, value);
  });
  const replies = ['N 「Middle topic」', 'N 「Future topic」', 'N 「Other board」', 'R n1 「Branch topic」\nN 「Branch addition」'];
  await page.route('http://127.0.0.1:19876/**', route => {
    const post = route.request().method() === 'POST';
    if (post) report.requests.push(route.request().postDataJSON());
    return route.fulfill({status: 200, contentType: 'application/json', headers: {'access-control-allow-origin': '*'}, body: JSON.stringify(post ? {choices: [{message: {content: replies[report.requests.length - 1] ?? 'N 「Unexpected extra request」'}}]} : {status: 'ok'})});
  });
  const read = () => page.evaluate(async () => {
    if (!(await indexedDB.databases()).some(d => d.name === 'boardcast-v2')) return null;
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    try {return await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});}
    finally {db.close();}
  });
  const saved = async predicate => {
    for (let i = 0; i < 120; i++) {const m = await read(); if (m && predicate(m)) {await page.locator('.save-state.saved').waitFor(); return read();} await page.waitForTimeout(100);}
    throw Error('Expected replay branch state was not saved');
  };
  const snap = async name => {const m = await read(); await fs.writeFile(`${evidence}/${name}.json`, JSON.stringify(m, null, 2) + '\n'); await page.screenshot({path: `${evidence}/${name}.png`}); return m;};
  const live = scene => (scene?.elements ?? []).filter(e => !e.isDeleted && !e.customData?.boardcast);
  // Editor reconstruction changes revision metadata, not the saved drawing's visual or semantic fields.
  const content = value => JSON.parse(JSON.stringify(value, (_, v) => v?.type && v?.id ? Object.fromEntries(Object.entries(v).filter(([k]) => !['version', 'versionNonce', 'updated', 'index'].includes(k))) : v));
  const send = async text => {await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text); await page.getByRole('button', {name: 'Put on the board', exact: true}).click();};
  const mode = async name => {await page.getByRole('radio', {name, exact: true}).click(); if (name === 'Free edit') await page.locator('.pencil-stage[data-ready="true"]').waitFor();};
  const show = async index => {await page.locator('.pages').getByRole('button', {name: `Board ${index + 1}`, exact: true}).click(); await page.locator(`.pages button[aria-current="page"][aria-label="Board ${index + 1}"]`).waitFor(); await page.locator('.save-state.saved').waitFor();};
  const range = page.locator('.replay-dock input[type=range]');
  const replayAt = async at => {
    await page.getByRole('button', {name: 'Replay', exact: true}).click(); await range.waitFor();
    const pause = page.locator('.replay-dock').getByRole('button', {name: 'Pause', exact: true}); if (await pause.count()) await pause.click();
    await range.focus(); await range.press('Home'); for (let i = 0; i < at; i++) await range.press('ArrowRight');
    assert.equal(await range.inputValue(), String(at));
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    const fixture = structuredClone(fixtures.metadata);
    fixture.elements.find(e => e.id === 'free-box').customData.boardcastName = 'Original free shape';
    await page.locator('input[type=file]').setInputFiles({name: 'branch-source.excalidraw', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(fixture))});
    const dialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
    await dialog.getByRole('button', {name: 'New meeting', exact: true}).click(); await dialog.waitFor({state: 'hidden'});
    await saved(m => m.scene?.overlay && !m.scene.pendingImport);
    await send('Add the second topic.');
    await saved(m => m.graph.nodes.length === 2 && m.lines.at(-1)?.ms !== undefined && m.steps?.at(-1)?.scene?.generated.some(e => e.id === 'n2'));
    const midpoint = await snap('midpoint'); const at = midpoint.steps.length;
    await send('Add a later topic which must stay out of the earlier branch.');
    await saved(m => m.graph.nodes.length === 3 && m.lines.at(-1)?.ms !== undefined);
    await mode('Free edit');
    await page.getByRole('radio', {name: 'Rectangle', exact: true}).locator('..').click();
    const box = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    await page.mouse.move(box.x + box.width * .65, box.y + box.height * .65); await page.mouse.down();
    await page.mouse.move(box.x + box.width * .8, box.y + box.height * .8, {steps: 12}); await page.mouse.up();
    const later = await saved(m => live(m.scene).length === live(midpoint.scene).length + 1);
    const futureId = live(later.scene).find(e => !live(midpoint.scene).some(old => old.id === e.id)).id;
    await mode('Hand-drawn');
    await page.getByRole('button', {name: 'New board', exact: true}).click();
    await send('Create a separate board which must remain unchanged.'); await saved(m => m.page === 1 && m.graph.nodes.length === 1 && m.lines.at(-1)?.ms !== undefined);
    await show(0); const before = await snap('source-before');
    const assets = await page.evaluate(async () => {
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      try {const s = db.transaction('assets').objectStore('assets'); const get = r => new Promise((yes, no) => {r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);}); const [keys, values] = await Promise.all([get(s.getAllKeys()), get(s.getAll())]); return Object.fromEntries(keys.map((k, i) => [k, values[i]]));}
      finally {db.close();}
    });
    await fs.writeFile(`${evidence}/source-assets.json`, JSON.stringify(assets, null, 2) + '\n');
    await replayAt(0);
    const fork = page.getByRole('button', {name: 'Fork board', exact: true});
    check(await fork.isDisabled() && /Play to a point/.test(await fork.getAttribute('title')), 'step zero disables branching with an explanation');
    assert.deepEqual(await read(), before); check(true, 'reading step zero never changes the saved meeting');
    await range.focus(); for (let i = 0; i < at; i++) await range.press('ArrowRight');
    await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
    const contents = page.locator('#sidebar-contents-panel');
    await contents.getByText('Middle topic', {exact: true}).waitFor();
    check(await contents.getByText('Future topic', {exact: true}).count() === 0, 'the selected middle step excludes the later generated topic');
    await fork.focus(); await fork.press('Enter'); await range.waitFor({state: 'hidden'});
    let row = await saved(m => m.pages?.length === 3 && m.page === 2 && m.lines.length === before.lines.length + 1);
    check(row.id === before.id && row.lines.at(-1).manual && row.lines.at(-1).page === 2 && /Board 3 forked from board 1/.test(row.lines.at(-1).text), 'keyboard branching appends one board and one correctly scoped note');
    assert.deepEqual(content(row.pages.slice(0, 2)), content(before.pages)); check(true, 'both original boards retain graph, layout, scene, assets and replay steps');
    assert.deepEqual(row.graph, midpoint.graph); check(true, 'the new board starts with the graph at the selected step');
    assert.deepEqual(content(live(row.scene)), content(live(midpoint.scene)));
    check(!live(row.scene).some(e => e.id === futureId), 'the earlier free objects and image survive without the later drawing');
    assert.deepEqual(content(row.steps.slice(0, at)), content(midpoint.steps)); check(true, 'the branch retains the replay prefix and its original timing');
    await snap('forked-middle');
    await send('Continue only this historical branch.');
    row = await saved(m => m.graph.nodes.some(n => n.label === 'Branch addition') && m.lines.at(-1)?.ms !== undefined);
    assert.deepEqual(content(row.pages.slice(0, 2)), content(before.pages)); check(true, 'subsequent model changes cannot rewrite either original board');
    const request = JSON.stringify(report.requests.at(-1));
    check(request.includes('Middle topic') && !request.includes('Future topic') && !request.includes('Other board'), 'the branch model receives its selected history, not future or other-board content');
    const item = contents.getByRole('region', {name: 'Free objects', exact: true}).locator('li').filter({has: page.getByText('Original free shape', {exact: true})});
    await item.getByRole('button', {name: 'Rename', exact: true}).click(); await contents.getByRole('textbox', {name: 'Name', exact: true}).fill('Branch-only free shape'); await page.keyboard.press('Enter');
    row = await saved(m => live(m.scene).some(e => e.customData?.boardcastName === 'Branch-only free shape'));
    assert.deepEqual(content(row.pages.slice(0, 2)), content(before.pages)); check(true, 'editing a branch free object leaves the source object and history intact');
    const final = await snap('branch-edited');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await page.locator('image.free-scene').waitFor();
    row = await read();
    assert.deepEqual(row, final); check(true, 'cold browser reopen restores the entire saved branch without rewriting it');
    await replayAt(final.steps.length);
    await fork.click(); await range.waitFor({state: 'hidden'});
    row = await saved(m => m.pages?.length === 4 && m.page === 3);
    assert.deepEqual(row.graph, final.graph); assert.deepEqual(content(live(row.scene)), content(live(final.scene)));
    check(true, 'branching at the final step preserves subsequent model and free-object edits');
    assert.deepEqual(content(row.pages.slice(0, 2)), content(before.pages)); check(true, 'a second branch still leaves the two original boards unchanged');
    await show(0); await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
    await contents.getByText('Future topic', {exact: true}).waitFor(); await contents.getByText('Original free shape', {exact: true}).waitFor();
    check(await contents.getByText('Branch topic', {exact: true}).count() === 0, 'returning to the source shows its own later topic and original free-object name');
    await snap('original-after');
    check(report.requests.length === 4 && !report.pageErrors.length, 'no duplicate model submissions or uncaught browser errors');
  } catch (error) {report.error = String(error); await snap('failure').catch(() => {}); throw error;}
  finally {await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n');}
};
