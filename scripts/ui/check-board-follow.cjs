const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkBoardFollow(page, baseUrl, evidence = '/tmp/boardcast-board-follow') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], positions: {}, pageErrors: []};
  const check = (ok, name) => report.checks.push({name, ok: !!ok});
  let reply = 'N 「Added decision」';
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.mode', 'ink');
    localStorage.setItem('boardcast.log', 'closed');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19876');
  });
  await page.route('http://127.0.0.1:19876/**', route => route.fulfill({
    contentType: 'application/json', headers: {'access-control-allow-origin': '*'},
    body: JSON.stringify(route.request().method() === 'POST'
      ? {choices: [{message: {content: reply}}]} : {status: 'ok', data: [{id: 'fixture'}]}),
  }));
  const view = p => p.locator('.ink > svg').evaluate(svg => {
    const box = svg.viewBox.baseVal;
    return `${box.x} ${box.y} ${svg.clientWidth / box.width}`;
  });
  const pan = async p => {
    const b = await p.locator('.ink').boundingBox();
    await p.mouse.move(b.x + 25, b.y + 45); await p.mouse.down();
    await p.mouse.move(b.x + b.width - 25, b.y + 115, {steps: 12}); await p.mouse.up();
  };
  const settled = async p => {
    await p.waitForFunction(() => !document.querySelector('.ink [data-erasing]'));
    await p.waitForTimeout(150);
  };
  const read = () => page.evaluate(async () => {
    const db = await new Promise(resolve => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => resolve(r.result);});
    const row = await new Promise(resolve => {const r = db.transaction('meetings').objectStore('meetings').get('qa-board-follow'); r.onsuccess = () => resolve(r.result);});
    db.close(); return row;
  });
  const send = async (text, nodes, operation) => {
    reply = operation ?? `N 「${text}」`;
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text);
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await page.waitForFunction(count => document.querySelectorAll('.ink [data-key]').length >= count, nodes);
    for (let i = 0; i < 100; i++) {
      if ((await read()).graph.nodes.length === nodes) break;
      await page.waitForTimeout(100);
    }
    assert.equal((await read()).graph.nodes.length, nodes);
    await page.waitForFunction(id => [...document.querySelectorAll('.ink [data-key]')].some(el => el.dataset.key.startsWith(id + '|')), `n${nodes}`);
    await settled(page);
  };
  const latest = p => p.getByRole('button', {name: 'Show full board', exact: true});
  const reset = async p => {
    if (await latest(p).count()) await latest(p).click();
    else {
      const b = await p.locator('.ink').boundingBox();
      await p.mouse.dblclick(b.x + 30, b.y + 45);
    }
    await settled(p);
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const graph = {nodes: [{id: 'n1', label: 'Reading anchor', aliases: [], kind: 'node', marks: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
      const empty = {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1};
      const row = {version: 2, storageRevision: 1, id: 'qa-board-follow', title: 'Board reading', titled: true, createdAt: Date.now(), updatedAt: Date.now(), graph, history: [], lines: [], pages: [{graph}, {graph: empty}], page: 0};
      const db = await new Promise(resolve => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => resolve(r.result);});
      await new Promise((resolve, reject) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.ink [data-key]').first().waitFor();
    await pan(page);
    const panned = await view(page);
    check(await latest(page).count() === 1, 'manual pan offers a visible return action');
    await send('Add while I am reading.', 2);
    report.positions.pan = {before: panned, after: await view(page)};
    check(await view(page) === panned, 'new model content does not move a manually panned board');
    await page.screenshot({path: `${evidence}/manual-pan.png`});
    await reset(page);
    check(await view(page) !== panned, 'the explicit return action fits the current board');
    await page.screenshot({path: `${evidence}/fitted-board.png`});

    const b = await page.locator('.ink').boundingBox();
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    await page.mouse.wheel(0, -900); await page.waitForTimeout(100);
    const zoomed = await view(page);
    await send('Keep my chosen zoom.', 3);
    check(await view(page) === zoomed, 'new content preserves manual wheel zoom');
    await reset(page);

    reply = 'R n1 「An updated reading anchor」';
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('Update the first decision.');
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await page.locator('.ink [data-erasing]').first().waitFor({state: 'attached'});
    await pan(page);
    const duringErase = await view(page);
    await settled(page);
    check(await view(page) === duringErase, 'finishing an erase cannot restore a camera from before the gesture');
    await reset(page);

    await page.emulateMedia({reducedMotion: 'reduce'});
    await settled(page);
    const reduced = await view(page);
    await send('Add a larger group without moving my view.', 11,
      Array.from({length: 8}, (_, i) => `N 「Expanded decision ${i + 1} with several details」`).join('\n'));
    report.positions.reduced = {before: reduced, after: await view(page)};
    check(await view(page) === reduced, 'reduced motion prevents automatic camera changes as the board grows');
    check(await page.locator('.ink mask path[style*="bc-write"]').count() === 0, 'reduced motion renders final strokes without writing or erasing');
    check(await latest(page).count() === 1, 'reduced motion offers explicit access to new offscreen content');

    await page.setViewportSize({width: 640, height: 400});
    await pan(page);
    if (await latest(page).count()) {
      const button = await latest(page).boundingBox();
      check(button.x >= 0 && button.y >= 0 && button.x + button.width <= 640 && button.y + button.height <= 400, 'return action remains visible in a compact window');
      await latest(page).focus(); await page.keyboard.press('Enter');
      check(await page.locator('.ink').evaluate(el => el === document.activeElement), 'keyboard return keeps focus on the board');
    } else check(false, 'return action remains visible in a compact window');
    await page.screenshot({path: `${evidence}/compact.png`});
    await clickMeetingAction(page, 'Settings');
    await page.getByRole('dialog').getByRole('radio', {name: '中文', exact: true}).click();
    await page.getByRole('dialog').getByRole('button', {name: '关闭', exact: true}).click();
    await pan(page);
    const chinese = page.getByRole('button', {name: '显示完整白板', exact: true});
    check(await chinese.isVisible(), 'the shared return control updates to Chinese');
    await page.screenshot({path: `${evidence}/compact-chinese.png`});
    await chinese.focus(); await page.keyboard.press('Enter');
    await clickMeetingAction(page, '设置');
    await page.getByRole('dialog').getByRole('radio', {name: 'English', exact: true}).click();
    await page.getByRole('dialog').getByRole('button', {name: 'Close', exact: true}).click();
    await page.setViewportSize({width: 1280, height: 800});
    await page.emulateMedia({reducedMotion: 'no-preference'});
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
    await settled(page);
    check(await latest(page).count() === 0, 'changing pages starts with the new page fitted');

    await clickMeetingAction(page, 'Export');
    await page.getByRole('menuitemradio', {name: 'Whole meeting', exact: true}).click();
    const downloadPromise = page.waitForEvent('download', {timeout: 60000});
    await page.getByRole('menuitem', {name: /^Replay page/}).click();
    const file = path.join(evidence, 'board-reading.html');
    await (await downloadPromise).saveAs(file);
    const context = await page.context().browser().newContext({viewport: {width: 1280, height: 800}});
    try {
      await context.setOffline(true);
      const offline = await context.newPage();
      offline.on('pageerror', error => report.pageErrors.push(error.message));
      await offline.goto(pathToFileURL(path.resolve(file)).href);
      await offline.locator('.ink [data-key]').first().waitFor();
      await pan(offline);
      const before = await view(offline);
      check(await latest(offline).count() === 1, 'actual offline HTML includes the return action');
      const slider = offline.getByRole('slider').first();
      await slider.focus(); await offline.keyboard.press('Home'); await offline.keyboard.press('ArrowRight');
      await settled(offline);
      check(await view(offline) === before, 'offline replay steps preserve a manually chosen camera');
      await reset(offline);
      check(await view(offline) !== before, 'offline HTML return action fits the current replay step');
      await offline.waitForFunction(() => document.getAnimations().every(animation => animation.playState === 'finished' || animation.playState === 'idle'));
      await offline.screenshot({path: `${evidence}/offline.png`});
    } finally { await context.close(); }
    check(report.pageErrors.length === 0, 'the app and offline player have no uncaught script errors');
    report.passed = report.checks.every(item => item.ok);
    assert.ok(report.passed, JSON.stringify(report.checks.filter(item => !item.ok)));
  } catch (error) {
    report.error = String(error);
    await page.screenshot({path: `${evidence}/failure.png`});
    throw error;
  } finally {
    await fs.writeFile(`${evidence}/board-follow-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
