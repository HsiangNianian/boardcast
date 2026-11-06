const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkTranscriptReading(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/transcript-reading') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {baseUrl, checks: [], failures: [], views: [], pageErrors: [], offline: {requests: [], errors: []}};
  const check = (ok, label) => (ok ? report.checks : report.failures).push(label);
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.theme', 'light');
    localStorage.setItem('boardcast.mode', 'ink');
    localStorage.setItem('boardcast.log', 'open');
  });
  await page.emulateMedia({reducedMotion: 'reduce'});
  const lines = page.locator('#sidebar-transcript-panel .lines');
  const latest = page.locator('.follow-transcript');
  const position = () => lines.evaluate(list => {
    let scroller = list;
    while (scroller.parentElement && !(/auto|scroll/.test(getComputedStyle(scroller).overflowY) && scroller.scrollHeight > scroller.clientHeight)) scroller = scroller.parentElement;
    const box = scroller.getBoundingClientRect();
    return {top: scroller.scrollTop, remaining: scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop, height: scroller.clientHeight, scroller: scroller.className, x: box.x, y: box.y, width: box.width};
  });
  const send = async text => {
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text);
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await lines.getByText(text, {exact: true}).waitFor({state: 'attached'});
    await page.waitForTimeout(150);
  };
  let offlineContext;
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    for (const [width, height] of [[1280, 800], [720, 600], [640, 400]]) {
      await page.setViewportSize({width, height});
      const id = `qa-reading-${width}-${height}`;
      await page.evaluate(async id => {
        const label = p => `长会节点 BOARD_${p + 1}`;
        const lines = Array.from({length: 2000}, (_, i) => ({id: `line-${i}`, text: `LONG_LINE_${String(i + 1).padStart(4, '0')} ${i % 50 === 0 ? label(Math.floor(i / 50)) : 'Discuss the next decision.'} 保留完整原话。日本語の記録。 Keep this earlier passage readable while more statements arrive.`, speaker: 'Alice', page: Math.floor(i / 50), ms: 1}));
        const pages = Array.from({length: 40}, (_, p) => {
          const node = {id: 'n1', label: label(p), aliases: [], kind: 'node', marks: [], ink: 'ink', by: 'Alice'};
          const graph = {nodes: [node], edges: [], groups: [], nextNode: 2, nextGroup: 1};
          return {graph, steps: [{t: p * 90000, line: p * 50 + 1, d: {nodes: {set: [node]}, nextNode: 2}}, {t: p * 90000 + 88200, line: (p + 1) * 50, d: {}}]};
        });
        const row = {version: 2, storageRevision: 1, id, title: 'Long meeting 阅读与回放', titled: true, createdAt: 1, updatedAt: 2, graph: pages[0].graph, steps: pages[0].steps, history: [], pages, page: 0, lines, summary: 'A long meeting with forty boards and two thousand transcript lines.'};
        const db = await new Promise((resolve, reject) => { const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
        await new Promise((resolve, reject) => { const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = resolve; tx.onerror = () => reject(tx.error); });
        db.close(); localStorage.setItem('boardcast.meeting', id);
      }, id);
      await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      await page.waitForFunction(() => document.querySelectorAll('#sidebar-transcript-panel .line').length === 2000);
      await page.waitForTimeout(250);
      const view = {width, height, initial: await position()}; report.views.push(view);
      check(view.initial.height > 0 && view.initial.remaining < 3, `${width}x${height}: opening a long transcript follows its latest line`);
      await lines.focus(); await lines.press('End'); await page.waitForTimeout(250);
      const end = await position();
      await page.mouse.move(end.x + end.width / 2, end.y + end.height - 20);
      await page.mouse.wheel(0, -600); await page.waitForTimeout(250);
      check((await position()).remaining > 100 && await latest.count() === 1, `${width}x${height}: scrolling upward pauses following`);
      await lines.focus(); await lines.press('Home'); await page.waitForTimeout(250);
      const reading = await position();
      view.reading = reading;
      await send(`NEW_READING_${width}_1`);
      check(await latest.textContent() === '1 new transcript line · Jump to latest' && Math.abs((await position()).top - reading.top) < 3, `${width}x${height}: a new failed-generation line preserves the reading position and increments unread`);
      await send(`NEW_READING_${width}_2`);
      check(await latest.textContent() === '2 new transcript lines · Jump to latest', `${width}x${height}: unread lines accumulate once`);
      await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Summary', exact: true}).click();
      await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Transcript', exact: true}).click();
      view.afterTabs = await position();
      check(Math.abs(view.afterTabs.top - reading.top) < 3 && await latest.count() === 1, `${width}x${height}: sidebar tabs preserve paused reading`);
      if (width <= 900) {
        await page.getByRole('tab', {name: 'Board', exact: true}).click();
        await page.getByRole('tab', {name: 'Meeting details', exact: true}).click();
        check(Math.abs((await position()).top - reading.top) < 3 && await latest.count() === 1, `${width}x${height}: compact board/detail switching preserves paused reading`);
      }
      await latest.click(); await page.waitForTimeout(250);
      view.afterLatest = await position();
      check(view.afterLatest.remaining < 3 && await latest.count() === 0, `${width}x${height}: Jump to latest reaches the actual scroll container's end`);
      await send(`NEW_READING_${width}_3`);
      check((await position()).remaining < 3 && await latest.count() === 0, `${width}x${height}: following continues after another line arrives`);
      await page.locator('.save-state.saved').waitFor();
      await page.screenshot({path: path.join(evidence, `reading-${width}.png`), animations: 'disabled'});
      const saved = await page.evaluate(async id => {
        const db = await new Promise((resolve, reject) => {const r = indexedDB.open('boardcast-v2', 1); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);});
        const row = await new Promise((resolve, reject) => {const r = db.transaction('meetings').objectStore('meetings').get(id); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);}); db.close(); return row;
      }, id);
      check(saved.lines.length === 2003 && saved.lines.slice(0, 2000).every((line, i) => line.id === `line-${i}`) && saved.pages.length === 40, `${width}x${height}: long-meeting records and all boards remain stored`);
      view.saved = {lines: saved.lines.length, pages: saved.pages.length};
      await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
      const contents = page.locator('#sidebar-contents-panel');
      await contents.locator('select').selectOption('19');
      await contents.getByRole('button', {name: 'Find in transcript', exact: true}).click();
      await lines.locator('.line.jump').waitFor({state: 'attached'}); await page.waitForTimeout(150);
      const jump = await lines.locator('.line').nth(950).evaluate(line => ({text: line.textContent, top: line.getBoundingClientRect().top, bottom: line.getBoundingClientRect().bottom}));
      const scroll = await position(); view.sourceJump = {jump, scroll};
      check(jump.text.includes('LONG_LINE_0951') && jump.top >= scroll.y && jump.bottom <= scroll.y + scroll.height, `${width}x${height}: locating a board's source reveals the historical line`);
      await page.waitForFunction(() => !document.querySelector('.line.jump'));
      check(Math.abs((await position()).top - scroll.top) < 3 && await latest.count() === 1, `${width}x${height}: source reading stays paused after its highlight clears`);
      await page.locator('.save-state.saved').waitFor();
      if (width === 1280) {
        report.resizes = [];
        await lines.evaluate(element => element.focus({preventScroll: true}));
        for (const [nextWidth, nextHeight] of [[900, 600], [640, 400], [720, 600], [1280, 800]]) {
          await page.setViewportSize({width: nextWidth, height: nextHeight}); await page.waitForTimeout(200);
          const viewport = await position();
          const anchor = await lines.locator('.line').nth(950).evaluate(line => line.getBoundingClientRect().toJSON());
          report.resizes.push({following: false, width: nextWidth, height: nextHeight, viewport, anchor});
          check(anchor.bottom > viewport.y && anchor.top < viewport.y + viewport.height && await latest.count() === 1, `resize to ${nextWidth}x${nextHeight} preserves the historical reading anchor`);
          check(await lines.evaluate(element => document.activeElement === element), `resize to ${nextWidth}x${nextHeight} preserves transcript keyboard focus`);
          if (nextWidth === 640) await page.screenshot({path: path.join(evidence, 'resized-reading.png'), animations: 'disabled'});
        }
        await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Summary', exact: true}).click();
        await page.setViewportSize({width: 640, height: 400}); await page.waitForTimeout(200);
        await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Transcript', exact: true}).click();
        let viewport = await position();
        let anchor = await lines.locator('.line').nth(950).boundingBox();
        check(anchor.y + anchor.height > viewport.y && anchor.y < viewport.y + viewport.height && await latest.count() === 1, 'resizing while Summary is open preserves the hidden transcript anchor');
        await page.getByRole('tab', {name: 'Board', exact: true}).click();
        await page.setViewportSize({width: 900, height: 600}); await page.waitForTimeout(200);
        await page.getByRole('tab', {name: 'Meeting details', exact: true}).click();
        viewport = await position(); anchor = await lines.locator('.line').nth(950).boundingBox();
        check(anchor.y + anchor.height > viewport.y && anchor.y < viewport.y + viewport.height && await latest.count() === 1, 'resizing while the board is open preserves the hidden transcript anchor');
        await page.setViewportSize({width: 1280, height: 800}); await page.waitForTimeout(200);
        await latest.click();
        for (const [nextWidth, nextHeight] of [[640, 400], [640, 600], [720, 400], [1280, 800]]) {
          await page.setViewportSize({width: nextWidth, height: nextHeight}); await page.waitForTimeout(200);
          const viewport = await position(); report.resizes.push({following: true, width: nextWidth, height: nextHeight, viewport});
          check(viewport.remaining < 3 && await latest.count() === 0, `resize to ${nextWidth}x${nextHeight} keeps following the latest transcript`);
        }
      }
    }
    await clickMeetingAction(page, 'Export');
    await page.getByRole('menuitemradio', {name: 'Whole meeting', exact: true}).click();
    const pending = page.waitForEvent('download', {timeout: 60000});
    await page.getByRole('menuitem', {name: /^Replay page/}).click();
    const download = await pending, file = path.join(evidence, 'long-meeting.html'); await download.saveAs(file);
    offlineContext = await page.context().browser().newContext({viewport: {width: 640, height: 400}, reducedMotion: 'reduce'});
    await offlineContext.route(/^https?:\/\//, route => {report.offline.requests.push(route.request().url()); return route.abort();});
    await offlineContext.setOffline(true);
    const offline = await offlineContext.newPage(); offline.on('pageerror', error => report.offline.errors.push(error.message));
    await offline.goto(pathToFileURL(path.resolve(file)).href); await offline.locator('.player-pages button').nth(39).waitFor({state: 'attached'});
    report.offline.data = await offline.evaluate(() => ({lines: window.__boardcastData.lines.length, pages: window.__boardcastData.pages.length}));
    check(report.offline.data.lines === 2003 && report.offline.data.pages === 40, 'offline HTML contains all 40 boards and 2003 transcript lines');
    const boards = offline.locator('.player-pages button'); await boards.first().focus();
    for (let i = 1; i < 40; i++) await offline.keyboard.press('Tab');
    const visible = await boards.last().evaluate(button => ({focused: button === document.activeElement, button: button.getBoundingClientRect().toJSON(), strip: button.parentElement.getBoundingClientRect().toJSON()}));
    report.offline.navigation = visible;
    check(visible.focused && visible.button.left >= visible.strip.left - 1 && visible.button.right <= visible.strip.right + 1, 'Tab reaches and reveals the fortieth board within the narrow page strip');
    await offline.keyboard.press(' '); check(await boards.last().getAttribute('aria-current') === 'page', 'Space selects the fortieth exported board');
    const range = offline.locator('.replay-dock input'); await range.press('Home'); await range.press('ArrowRight');
    check(await range.inputValue() === '1' && await offline.locator('.line').count() === 1951, 'late-board replay shows the correct historical transcript prefix');
    await range.press('Escape');
    check(await range.inputValue() === '2' && await offline.locator('.line').count() === 2003, 'exiting late-board replay restores the complete long transcript');
    await offline.waitForTimeout(100);
    const tail = await offline.locator('.lines .line').last().evaluate(line => ({line: line.getBoundingClientRect().toJSON(), viewport: line.closest('.log').getBoundingClientRect().toJSON()}));
    report.offline.finalLine = tail;
    check(tail.line.bottom > tail.viewport.top && tail.line.bottom <= tail.viewport.bottom + 1, 'ending replay brings the final transcript line into view');
    check(await offline.evaluate(() => document.documentElement.scrollWidth <= innerWidth && scrollY === 0), 'narrow long-meeting replay does not scroll the outer document');
    await offline.screenshot({path: path.join(evidence, 'offline-long-meeting.png'), animations: 'disabled'});
    check(report.offline.requests.length === 0 && report.offline.errors.length === 0, 'long-meeting HTML runs offline without requests or script errors');
    check(report.pageErrors.length === 0, 'long-transcript application flow has no script errors');
    report.passed = report.failures.length === 0; assert.deepEqual(report.failures, []);
    return report;
  } catch (error) {
    report.error = String(error); await page.screenshot({path: path.join(evidence, 'failure.png'), animations: 'disabled'}); throw error;
  } finally {
    if (offlineContext) await offlineContext.close();
    await fs.writeFile(path.join(evidence, 'transcript-reading-check.json'), JSON.stringify(report, null, 2) + '\n');
  }
};
