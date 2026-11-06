const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkContentNavigation(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], failures: [], pageErrors: []};
  const check = async (value, label) => {
    report.checks.push({label, passed: !!value});
    if (!value) {
      report.failures.push(label);
      await page.screenshot({path: `${evidence}/failure-${report.failures.length}.png`});
    }
  };
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', new URL(location.href).searchParams.get('lang') || 'en');
    localStorage.setItem('boardcast.log', 'open');
    localStorage.setItem('boardcast.mode', 'ink');
  });
  await page.emulateMedia({reducedMotion: 'reduce'});
  const panel = page.locator('#sidebar-contents-panel');
  const picker = panel.getByRole('combobox');
  const openContents = () => page.locator('.sidebar-tabs [aria-controls="sidebar-contents-panel"]').click();
  const row = label => panel.locator('li').filter({has: page.locator('strong', {hasText: label})}).first();
  const name = panel.getByRole('textbox', {name: 'Name', exact: true});
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const data = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get('qa-content-navigation'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return data;
  });
  const change = async (key, value, label) => {
    await picker.focus(); await page.keyboard.press(key);
    await page.waitForFunction(value => document.querySelector('#sidebar-contents-panel select')?.value === value, value);
    await page.waitForTimeout(50);
    await check(await picker.evaluate(el => el === document.activeElement), label);
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const graph = label => ({nodes: label ? [{id: 'n1', kind: 'node', label, aliases: [], marks: [], ink: 'ink'}] : [], edges: [], groups: [], nextNode: 2, nextGroup: 1});
      const shape = {id: 'same-free-id', type: 'rectangle', x: 200, y: 200, width: 100, height: 80, angle: 0, strokeColor: '#a83c2c', backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: false};
      const scene = {elements: [shape], generated: [], files: {}};
      const pages = [{graph: graph('First board decision'), scene}, {graph: graph('Second board decision'), scene: structuredClone(scene)}, {graph: graph('')}];
      const meeting = {version: 2, storageRevision: 1, id: 'qa-content-navigation', title: 'Keyboard contents', titled: true, createdAt: 1, updatedAt: 2, graph: pages[0].graph, scene, lines: [], history: [], pages, page: 0};
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(meeting); tx.oncomplete = yes; tx.onerror = () => no(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', meeting.id);
    });
    await fs.writeFile(`${evidence}/original-meeting.json`, JSON.stringify(await read(), null, 2));
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await openContents();
    await change('ArrowDown', '1', 'keyboard page change keeps focus on the page selector');
    await row('Second board decision').waitFor();
    await change('End', '2', 'keyboard navigation reaches the last empty board without losing focus');
    await panel.getByText('This page has no board content yet.', {exact: true}).waitFor();
    await change('Home', '0', 'Home returns to the first board and keeps its visible control focused');
    await picker.evaluate(el => {
      el.value = '1'; el.dispatchEvent(new Event('change', {bubbles: true}));
      document.querySelector('.sidebar-tabs [aria-controls="sidebar-summary-panel"]').focus();
    });
    await page.waitForFunction(() => document.querySelector('#sidebar-contents-panel select')?.value === '1');
    await page.waitForTimeout(50);
    await check(await page.locator('.sidebar-tabs [aria-controls="sidebar-summary-panel"]').evaluate(el => el === document.activeElement), 'deferred page focus restoration does not steal focus already moved elsewhere');
    await change('Home', '0', 'page selection resumes normally after focus moves elsewhere');

    await row('First board decision').getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('Uncommitted Chinese input');
    const enterPrevented = await name.evaluate(el => !el.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', isComposing: true, bubbles: true, cancelable: true})));
    await check(enterPrevented && await name.count() === 1, 'IME Enter is reserved for composition and cannot submit the rename');
    await name.dispatchEvent('keydown', {key: 'Escape', isComposing: true, bubbles: true, cancelable: true});
    await check(await name.count() === 1, 'IME Escape leaves the rename form and draft intact');
    if (!await name.count()) await row('First board decision').getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('Discarded draft'); await page.keyboard.press('Escape');
    await page.waitForFunction(() => document.activeElement?.matches('[data-content-edit]'), undefined, {timeout: 1000});
    await check(await name.count() === 0 && await row('First board decision').getByRole('button', {name: 'Rename', exact: true}).evaluate(el => el === document.activeElement), 'ordinary Escape cancels only the rename and returns focus to its action');

    await row('First board decision').getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('Never apply to another board');
    await page.locator('nav.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    await row('Second board decision').waitFor();
    await check(await name.count() === 0 && await page.locator('nav.pages').getByRole('button', {name: 'Board 2', exact: true}).evaluate(el => el === document.activeElement), 'external page navigation discards the old draft without stealing focus');
    await row('Second board decision').getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('Another discarded draft');
    await change('Home', '0', 'changing pages during rename retains the selector focus');
    await check(await name.count() === 0 && await row('First board decision').count() === 1, 'identical object IDs on separate pages do not share rename drafts');

    await row('First board decision').getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('中文決定 Japanese 日本語'); await page.keyboard.press('Enter');
    await row('中文決定 Japanese 日本語').waitFor();
    await check(await row('中文決定 Japanese 日本語').getByRole('button', {name: 'Rename', exact: true}).evaluate(el => el === document.activeElement), 'completed text input commits once and returns focus to the renamed object');
    const free = panel.getByRole('region', {name: 'Free objects'});
    await free.getByRole('button', {name: 'Rename', exact: true}).click();
    await name.fill('Named free shape'); await page.keyboard.press('Enter');
    await free.getByText('Named free shape', {exact: true}).waitFor();
    await page.locator('.save-state.saved').waitFor();
    const saved = await read();
    await check(saved.pages[0].graph.nodes[0].label === '中文決定 Japanese 日本語' && saved.pages[1].graph.nodes[0].label === 'Second board decision', 'renaming is saved to the original board only');
    await check(saved.pages[0].scene.elements.find(e => e.id === 'same-free-id').customData?.boardcastName === 'Named free shape'
      && !saved.pages[1].scene.elements.find(e => e.id === 'same-free-id').customData?.boardcastName, 'free-object naming remains scoped to its own board');

    for (const lang of ['en', 'zh']) {
      await page.goto(`${baseUrl}/?lang=${lang}`); await page.locator('.splash').waitFor({state: 'hidden'}); await openContents();
      for (const [width, height] of [[1280, 800], [900, 600], [720, 480], [360, 240]]) {
        await page.setViewportSize({width, height});
        await change('End', '2', `${lang} ${width}x${height}: keyboard page selection stays reachable`);
        await change('Home', '0', `${lang} ${width}x${height}: return to the first board keeps focus`);
        await check(await picker.evaluate(el => {
          const b = el.getBoundingClientRect(); let top = 0, bottom = innerHeight;
          for (let parent = el.parentElement; parent; parent = parent.parentElement) {
            if (/auto|scroll|hidden|clip/.test(getComputedStyle(parent).overflowY)) {
              const box = parent.getBoundingClientRect(); top = Math.max(top, box.top); bottom = Math.min(bottom, box.bottom);
            }
          }
          return b.top >= top - 1 && b.bottom <= bottom + 1 && b.left >= 0 && b.right <= innerWidth;
        }), `${lang} ${width}x${height}: focused page selector is inside its visible scroll area`);
        await check(await panel.locator('.content-kind').evaluateAll(elements => elements.every(el => parseFloat(getComputedStyle(el).fontSize) >= 12)), `${lang} ${width}x${height}: object types meet the 12px text minimum`);
        await page.screenshot({path: `${evidence}/${lang}-${width}.png`});
      }
    }
    await check(report.pageErrors.length === 0, 'no uncaught browser errors');
  } finally {
    await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2));
  }
  assert.deepEqual(report.failures, []);
};
