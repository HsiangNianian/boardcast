const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkContentReplay(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {baseUrl, checks: [], pageErrors: [], fontErrors: []};
  const check = (condition, label) => { assert.ok(condition, label); report.checks.push(label); };
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('response', response => { if (/\.woff2?(?:\?|$)/.test(response.url()) && response.status() >= 400) report.fontErrors.push(`${response.status()} ${response.url()}`); });
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.theme', 'light');
    localStorage.setItem('boardcast.mode', 'ink');
  });
  const panel = page.locator('#sidebar-contents-panel');
  const tabs = page.locator('.sidebar-tabs');
  const range = page.locator('.replay-dock input[type="range"]');
  const saved = () => page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => { const request = indexedDB.open('boardcast-v2', 1); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
    const row = await new Promise((resolve, reject) => { const request = db.transaction('meetings').objectStore('meetings').get('qa-content-replay'); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
    db.close(); return row;
  });
  const pause = async () => { const button = page.locator('.replay-dock').getByRole('button', {name: 'Pause', exact: true}); if (await button.count()) await button.click(); };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(scripts => scripts.map(script => script.src));
    await page.evaluate(async () => {
      const node = label => ({id: 'n1', label, aliases: [], kind: 'node', marks: [], ink: 'ink'});
      const graph = {nodes: [node('Later semantic label')], edges: [], groups: [], nextNode: 2, nextGroup: 1};
      const emptyScene = {elements: [], generated: [], files: {}};
      const scene = {elements: [{id: 'future-free', type: 'rectangle', x: 40, y: 30, width: 100, height: 80, angle: 0, strokeColor: '#111111', backgroundColor: '#ff00ff', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 1, isDeleted: false, boundElements: null, link: null, locked: false, customData: {boardcastName: 'FUTURE_FREE_OBJECT'}}], generated: [], files: {}};
      const row = {version: 2, storageRevision: 1, id: 'qa-content-replay', title: 'Content replay boundaries', titled: true, createdAt: 1, updatedAt: 2, graph, history: [], lines: [{id: 'l1', text: 'Earlier statement', speaker: 'A', ms: 1}, {id: 'l2', text: 'Later statement', speaker: 'B', ms: 1}], scene,
        steps: [{t: 1, line: 1, d: {nodes: {set: [node('Earlier semantic label')]}, nextNode: 2}, scene: emptyScene}, {t: 6000, line: 2, d: {nodes: {set: graph.nodes}}, scene}]};
      const db = await new Promise((resolve, reject) => { const request = indexedDB.open('boardcast-v2', 1); request.onupgradeneeded = () => { request.result.createObjectStore('meetings', {keyPath: 'id'}); request.result.createObjectStore('assets'); }; request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
      await new Promise((resolve, reject) => { const transaction = db.transaction('meetings', 'readwrite'); transaction.objectStore('meetings').put(row); transaction.oncomplete = resolve; transaction.onerror = () => reject(transaction.error); });
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await tabs.getByRole('tab', {name: 'Contents', exact: true}).click();
    const semantic = panel.getByRole('region', {name: 'Board content', exact: true});
    await semantic.getByRole('button', {name: 'Rename', exact: true}).click();
    await panel.getByRole('textbox', {name: 'Name', exact: true}).fill('UNSUBMITTED_RENAME');
    for (const modifier of ['Control', 'Meta']) {
      await panel.getByRole('textbox', {name: 'Name', exact: true}).press(`${modifier}+Shift+R`);
      check(await range.count() === 0 && await panel.getByRole('textbox', {name: 'Name', exact: true}).inputValue() === 'UNSUBMITTED_RENAME', `${modifier}+Shift+R leaves text editing alone`);
    }
    await page.getByRole('button', {name: 'Replay', exact: true}).click(); await pause();
    await range.focus(); await range.press('End');
    await semantic.getByText('Later semantic label', {exact: true}).waitFor();
    check(await panel.locator('.content-rename input:not([readonly]):enabled').count() === 0, 'entering replay cannot retain an editable rename form');
    check(await panel.getByRole('button', {name: 'Rename', exact: true}).evaluateAll(buttons => buttons.length > 0 && buttons.every(button => button.disabled)), 'all semantic and free rename buttons are disabled in replay');
    check(await panel.getByRole('button', {name: 'Delete', exact: true}).evaluateAll(buttons => buttons.length > 0 && buttons.every(button => button.disabled)), 'all semantic and free delete buttons are disabled in replay');
    await range.focus(); await range.press('Home');
    check(await panel.getByText('FUTURE_FREE_OBJECT', {exact: true}).count() === 0 && await panel.getByRole('region', {name: 'Board content', exact: true}).count() === 0, 'step zero does not expose final semantic or free objects');
    await range.press('ArrowRight');
    await semantic.getByText('Earlier semantic label', {exact: true}).waitFor();
    check(await panel.getByText('FUTURE_FREE_OBJECT', {exact: true}).count() === 0, 'an earlier step uses its own scene snapshot');
    const at = await range.inputValue();
    await tabs.getByRole('tab', {name: 'Transcript', exact: true}).click();
    await page.keyboard.press('ArrowRight');
    check(await tabs.getByRole('tab', {name: 'Summary', exact: true}).getAttribute('aria-selected') === 'true' && await range.inputValue() === at, 'sidebar tab arrows do not seek replay');
    await page.keyboard.press('End');
    check(await tabs.getByRole('tab', {name: 'Contents', exact: true}).getAttribute('aria-selected') === 'true' && await range.inputValue() === at, 'sidebar tab End changes only the active tab');
    const locate = semantic.getByRole('button', {name: 'Locate', exact: true});
    await locate.focus(); await locate.press(' ');
    await page.waitForFunction(() => document.activeElement?.id === 'board-panel');
    report.locateKey = {focus: await page.evaluate(() => ({id: document.activeElement?.id, text: document.activeElement?.textContent})), playing: await page.locator('.replay-dock').getByRole('button', {name: 'Pause', exact: true}).count() > 0};
    check(report.locateKey.focus.id === 'board-panel' && !report.locateKey.playing, 'Space activates Locate without starting replay');
    await range.focus(); await range.press('End');
    await panel.getByText('FUTURE_FREE_OBJECT', {exact: true}).waitFor();
    check(true, 'the final step restores its free-object entry');
    await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    check(await page.getByRole('radio', {name: 'Rectangle', exact: true}).count() === 0, 'free-edit replay hides drawing tools using the editor read-only mode');
    await page.locator('main').focus(); await page.keyboard.press('r');
    await page.mouse.move(450, 310); await page.mouse.down(); await page.mouse.move(550, 380, {steps: 5}); await page.mouse.up();
    await page.locator('.replay-dock').getByRole('button', {name: 'Exit replay', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.getByRole('radio', {name: 'Rectangle', exact: true}).waitFor();
    await page.waitForTimeout(750);
    const row = await saved();
    check(row.scene.elements.filter(element => !element.isDeleted).length === 1 && row.scene.elements[0].id === 'future-free', 'free-edit replay gestures do not change saved objects and editing returns after exit');
    check(row.graph.nodes[0].label === 'Later semantic label' && row.lines.length === 2 && !row.lines.some(line => line.manual), 'reading and locating historical content preserve live graph and transcript history');
    await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
    await page.waitForTimeout(750);
    const beforeKeys = await saved();
    const board = page.locator('#board-panel');
    let navigations = 0;
    page.on('framenavigated', frame => { if (frame === page.mainFrame()) navigations++; });
    for (const modifier of ['Control', 'Meta']) {
      for (const focus of ['board', 'slider']) {
        await board.focus(); await page.keyboard.press(`${modifier}+Shift+R`);
        await range.waitFor(); await pause();
        check(true, `${modifier}+Shift+R enters replay before the ${focus} exit check`);
        await (focus === 'slider' ? range : board).focus();
        await page.keyboard.press(`${modifier}+Shift+R`);
        await range.waitFor({state: 'hidden'});
        check(navigations === 0, `${modifier}+Shift+R exits from the ${focus} without reloading`);
      }
    }
    await board.focus();
    await board.dispatchEvent('keydown', {key: 'R', ctrlKey: true, shiftKey: true, isComposing: true});
    check(await range.count() === 0, 'an IME composition key cannot enter replay');
    await page.keyboard.press('Control+Shift+R'); await range.waitFor(); await pause();
    await range.focus();
    await range.dispatchEvent('keydown', {key: 'Escape', isComposing: true});
    check(await range.count() === 1, 'an IME composition Escape cannot exit replay');
    await range.press('Home'); await range.press('ArrowRight');
    check(await range.inputValue() === '1', 'a focused replay slider advances exactly one step');
    await range.press('Escape'); await range.waitFor({state: 'hidden'});
    check(navigations === 0, 'Escape exits replay from its slider');
    const afterKeys = await saved();
    check(['graph', 'lines', 'steps', 'scene'].every(key => JSON.stringify(afterKeys[key]) === JSON.stringify(beforeKeys[key])), 'keyboard replay leaves the stored meeting content intact');
    await board.focus(); await page.keyboard.press('?');
    const help = page.getByRole('dialog', {name: 'Keyboard shortcuts', exact: true});
    await help.waitFor(); await page.keyboard.press('Control+Shift+R');
    check(await help.isVisible() && await range.count() === 0, 'a help dialog isolates application shortcuts');
    await page.keyboard.press('Escape');
    const platform = await page.evaluate(() => navigator.platform);
    await page.evaluate(() => Object.defineProperty(navigator, 'platform', {value: 'MacIntel', configurable: true}));
    await board.focus(); await page.keyboard.press('?'); await help.waitFor();
    check(await help.locator('kbd').allTextContents().then(keys => keys.includes('⇧⌘R') && keys.includes('⌘Z / ⇧⌘Z') && keys.every(key => !key.includes('Ctrl+'))), 'Mac help displays Command and the native Shift-Command-Z redo shortcut');
    await page.screenshot({path: `${evidence}/replay-shortcuts-mac-help.png`, animations: 'disabled'});
    await page.keyboard.press('Escape');
    await page.evaluate(value => Object.defineProperty(navigator, 'platform', {value, configurable: true}), platform);
    check(report.pageErrors.length === 0, 'content replay flow has no browser script errors');
    check(report.fontErrors.length === 0, 'both board modes load their fonts without HTTP errors');
    report.passed = true;
    await fs.rm(`${evidence}/content-replay-failure.png`, {force: true});
    return report;
  } catch (error) {
    report.error = String(error);
    await page.screenshot({path: `${evidence}/content-replay-failure.png`});
    throw error;
  } finally {
    await fs.writeFile(`${evidence}/content-replay-check.json`, JSON.stringify(report, null, 2) + '\n');
  }
};
