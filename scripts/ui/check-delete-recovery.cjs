const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const {pathToFileURL} = require('node:url');
const path = require('node:path');
const {createHash} = require('node:crypto');

// Production App in two real browser tabs; model replies alone are synthetic.
module.exports = async (page, baseUrl) => {
  const evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/delete-races';
  await fs.mkdir(evidence, {recursive: true});
  const context = page.context(), errors = [];
  const other = await context.newPage();
  for (const target of [page, other]) target.on('pageerror', error => errors.push(error.message));
  await context.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.log', 'open');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19879');
  });
  await context.route('http://127.0.0.1:19879/**', route => route.fulfill({status: 200, contentType: 'application/json', body: JSON.stringify(route.request().method() === 'POST' ? {choices: [{message: {content: 'N 「Retained board item」'}}]} : {status: 'ok', data: [{id: 'deletion-fixture'}]})}));
  try {
    await page.goto(baseUrl);
    await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const graph = {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1, underlay: {w: 100, h: 60}};
      const image = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><rect width="100" height="60" fill="#b0d0ff"/></svg>');
      const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(image)))].map(b => b.toString(16).padStart(2, '0')).join('').slice(0, 32);
      const createdAt = Date.now() - 60_000;
      const meeting = {version: 2, id: 'delete-ui-race', title: 'Shared old meeting', titled: true, createdAt, updatedAt: createdAt, graph, history: [], lines: [{text: 'Original transcript', ms: 1}], pages: [{graph, underlay: `asset:${hash}.svg`}]};
      const db = await new Promise((resolve, reject) => {
        const req = indexedDB.open('boardcast-v2', 1);
        req.onupgradeneeded = () => {req.result.createObjectStore('meetings', {keyPath: 'id'}); req.result.createObjectStore('assets');};
        req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error);
      });
      await new Promise((resolve, reject) => {
        const tx = db.transaction(['meetings', 'assets'], 'readwrite');
        tx.objectStore('meetings').put(meeting);
        tx.objectStore('assets').put(image, `${meeting.id}/${hash}.svg`);
        tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);
      });
      db.close();
      localStorage.setItem('boardcast.meeting', meeting.id);
    });
    await page.reload();
    await page.locator('.meeting-title').filter({hasText: 'Shared old meeting'}).waitFor();
    await other.goto(baseUrl);
    await other.locator('.meeting-title').filter({hasText: 'Shared old meeting'}).waitFor();
    await other.locator('.splash').waitFor({state: 'hidden'});
    const draft = 'Keep my late unsaved transcript';
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(draft);
    await other.locator('.meeting-library').click();
    const library = other.getByRole('dialog', {name: 'Meetings', exact: true});
    const row = library.locator('.meeting-list > li').filter({has: other.locator('.title', {hasText: 'Shared old meeting'})});
    await row.getByRole('button', {name: 'Delete', exact: true}).click();
    await row.getByText('Delete this meeting?', {exact: false}).waitFor();
    await row.getByRole('button', {name: 'Delete', exact: true}).click();
    await row.waitFor({state: 'hidden'});
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await page.locator('.save-error').waitFor();
    assert.match(await page.locator('.save-error').innerText(), /deleted|another window/);
    assert.equal(await page.locator('.lines').getByText(draft, {exact: true}).count(), 1);
    await page.locator('.save-error').getByRole('button', {name: 'Retry save', exact: true}).click();
    await page.locator('.save-error').waitFor();
    await page.getByRole('button', {name: 'New meeting', exact: true}).click();
    await page.locator('.transition-state').waitFor({state: 'hidden'});
    assert.match(await page.locator('.meeting-title').innerText(), /Shared old meeting$/);
    assert.equal(await page.locator('.lines').getByText(draft, {exact: true}).count(), 1);
    await page.screenshot({path: `${evidence}/retained-edit.png`});
    const copying = page.waitForEvent('download');
    await page.locator('.save-error').getByRole('button', {name: 'Export a copy', exact: true}).click();
    const html = await copying, htmlPath = await html.path(), bytes = await fs.readFile(htmlPath);
    const copyPath = path.resolve(evidence, 'recovery.html');
    await fs.writeFile(copyPath, bytes);
    const offline = await context.browser().newContext({reducedMotion: 'reduce'});
    let recovered;
    try {
      await offline.setOffline(true);
      const player = await offline.newPage();
      player.on('pageerror', error => errors.push(error.message));
      await player.goto(pathToFileURL(copyPath).href);
      await player.locator('.player .ink > svg').waitFor();
      await player.waitForFunction(draft => document.querySelector('.player .lines')?.textContent.includes(draft), draft);
      await player.waitForFunction(() => document.querySelectorAll('.player .ink path').length > 30);
      recovered = await player.evaluate(() => ({lines: window.__boardcastData.lines, pages: window.__boardcastData.pages}));
      assert.ok(recovered.lines.some(line => line.text === 'Original transcript'));
      assert.ok(recovered.pages[0].graph.nodes.some(node => node.label === 'Retained board item'));
      assert.match(Buffer.from(recovered.pages[0].underlay.split(',')[1], 'base64').toString(), /#b0d0ff/);
      await player.screenshot({path: `${evidence}/offline-recovery.png`});
    } finally { await offline.close(); }
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: 'Whole meeting', exact: true}).click();
    const downloading = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: /^Markdown/}).click();
    const download = await downloading;
    const text = await fs.readFile(await download.path(), 'utf8');
    assert.ok(text.includes(draft) && text.includes('Original transcript'));
    await fs.writeFile(`${evidence}/retained-transcript.md`, text);
    const stored = await page.evaluate(async () => {
      const db = await new Promise(resolve => { const req = indexedDB.open('boardcast-v2'); req.onsuccess = () => resolve(req.result); });
      const tx = db.transaction(['meetings', 'assets']);
      const record = tx.objectStore('meetings').get('delete-ui-race');
      const assets = tx.objectStore('assets').getAllKeys(IDBKeyRange.bound('delete-ui-race/', 'delete-ui-race/\uffff'));
      const marker = tx.objectStore('assets').get('deleted:delete-ui-race');
      await new Promise((resolve, reject) => {tx.oncomplete = resolve; tx.onabort = () => reject(tx.error);});
      db.close(); return {record: record.result ?? null, assets: assets.result, marker: marker.result};
    });
    assert.deepEqual(stored, {record: null, assets: [], marker: true});
    assert.deepEqual(errors, []);
    await fs.writeFile(`${evidence}/ui-check.json`, JSON.stringify({passed: true, baseUrl, productionScripts: await page.locator('script[src]').evaluateAll(s => s.map(s => s.src)),
      deletedInOtherTab: true, lateSaveRejected: true, retryDidNotResurrect: true, blockedSwitchRetainsDraft: true, markdownRecovery: true,
      offlineRecovery: {bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), data: recovered}, stored, pageErrors: errors}, null, 2) + '\n');
  } finally {
    await other.close();
  }
};
