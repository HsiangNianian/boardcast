const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {click: meetingAction} = require('./meeting-action.cjs');

module.exports = async function checkPageSources(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], errors: []};
  const check = (ok, label) => {assert(ok, label); report.checks.push(label);};
  page.on('pageerror', error => report.errors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({lang: 'en', log: 'closed', 'mode-hint': 'dismissed'})) localStorage.setItem('boardcast.' + key, value);
  });
  const fixture = await page.context().newPage();
  await fixture.setContent(`<style>@page{size:600px 700px;margin:0}body{margin:0;font:18px/28px sans-serif}.sheet{box-sizing:border-box;padding:50px;width:600px;height:700px;break-after:page}.sheet:last-child{break-after:auto}h1{font-size:28px}</style>
    <div class="sheet"><h1>1 Deployment decisions</h1><p>Alpha will review the plan. Bravo will prepare the release. Preserve both source pages for the meeting.</p></div>
    <div class="sheet"><h1>References</h1><p>[1] Smith, A. Deployment notes, 2024.</p><p>[2] Jones, B. Release checklist, 2025.</p></div>
    <div class="sheet"><h1>Appendix A: Release checklist</h1><p>The final checks happen on the third page of the original file.</p></div>`);
  const pdf = await fixture.pdf({path: evidence + '/source.pdf', preferCSSPageSize: true});
  await fixture.close();
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const row = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return row;
  });
  const saved = async predicate => {
    for (let n = 0; n < 120; n++) {const row = await read(); if (row && predicate(row)) return row; await page.waitForTimeout(75);}
    throw Error('Expected imported pages were not saved');
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('input[type=file]').setInputFiles({name: 'source-pages.pdf', mimeType: 'application/pdf', buffer: pdf});
    const dialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
    await dialog.getByRole('button', {name: 'Add to this meeting', exact: true}).click({timeout: 60000});
    await dialog.waitFor({state: 'hidden'});
    await page.locator('.save-state.saved').waitFor();
    const initial = await saved(m => m.pages?.length === 2);
    check(initial.pages.map(p => p.graph.underlay.no).join() === '1,3', 'real PDF import skips references while retaining the original page numbers');
    report.initial = initial;
    const second = page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true});
    check(await second.getAttribute('aria-description') === 'Source page 3', 'the second board exposes the distinct source page to assistive technology');
    check(await second.locator('.page-source').innerText() === 'p. 3', 'desktop thumbnails visibly distinguish source page 3 from board 2');
    await second.click();
    check((await page.locator('.board-location').innerText()).includes('Source page 3'), 'the current board location shows the original source page');
    await page.screenshot({path: evidence + '/desktop.png'});
    await page.getByRole('button', {name: 'Transcript', exact: true}).click();
    await page.getByRole('tab', {name: 'Contents', exact: true}).click();
    check(await page.locator('.content-page option:checked').innerText() === 'Board 2 · Source page 3', 'the content list uses the same source page');
    await page.setViewportSize({width: 720, height: 480});
    await page.locator('.page-picker').waitFor({state: 'visible'});
    check(await page.locator('.page-picker option:checked').innerText() === 'Board 2 / 2 · Source page 3', 'compact navigation keeps the board count and source page together');
    check(await page.locator('.page-picker').evaluate(el => el.getBoundingClientRect().height >= 34), 'the compact selector retains its 34px native text height');
    await page.locator('.page-picker').focus(); await page.keyboard.press('Home');
    check(await page.locator('.page-picker').inputValue() === '0' && await page.locator('.page-picker').evaluate(el => el === document.activeElement), 'keyboard selection retains focus while changing imported pages');
    await page.locator('.page-picker').selectOption('1');
    await page.screenshot({path: evidence + '/compact.png'});
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'compact imported-page labels do not create horizontal overflow');
    await page.setViewportSize({width: 1280, height: 800});
    await meetingAction(page, 'Export');
    await page.getByRole('menuitemradio', {name: 'Whole meeting', exact: true}).click();
    const pending = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: /^Replay page/}).click();
    const html = path.join(evidence, 'source-pages.html'); await (await pending).saveAs(html);
    const offline = await page.context().browser().newContext({offline: true, viewport: {width: 720, height: 480}});
    try {
      const replay = await offline.newPage(); const errors = []; replay.on('pageerror', e => errors.push(e.message));
      await replay.goto(pathToFileURL(html).href); await replay.locator('.player').waitFor();
      await replay.locator('.player-pages').getByRole('button', {name: '2', exact: true}).click();
      check(await replay.locator('.player-source').innerText() === 'Source page 3', 'the actual offline HTML shows the original page after navigation');
      check(await replay.locator('.player-pages').getByRole('button', {name: '2', exact: true}).getAttribute('aria-description') === 'Source page 3', 'offline page controls expose source descriptions');
      check(await replay.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'offline source-page labels fit the compact player');
      await replay.screenshot({path: evidence + '/offline.png'});
      check(!errors.length, 'offline replay has no uncaught errors');
    } finally {await offline.close();}
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    check((await read()).pages.map(p => p.graph.underlay.no).join() === '1,3', 'cold reload preserves original page metadata without a migration');
    await page.locator('.pages .new').click();
    check((await page.locator('.board-location').innerText()).trim() === 'Board 3 / 3', 'a new blank page does not inherit the previous source number');
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    await meetingAction(page, 'Settings');
    await page.getByRole('dialog').getByRole('radio', {name: '中文', exact: true}).click();
    await page.keyboard.press('Escape');
    check((await page.locator('.board-location').innerText()).includes('原文件第 3 页'), 'source-page feedback follows the interface language immediately');
    await page.setViewportSize({width: 720, height: 480});
    check((await page.locator('.page-picker option:checked').innerText()).includes('原文件第 3 页'), 'Chinese compact navigation retains the source page');
    await page.screenshot({path: evidence + '/compact-zh.png'});
    check(!report.errors.length, 'the complete import and navigation flow has no uncaught errors');
    report.passed = true;
  } catch (error) {
    report.error = String(error); await page.screenshot({path: evidence + '/failure.png'}); throw error;
  } finally {await fs.writeFile(evidence + '/report.json', JSON.stringify(report, null, 2) + '\n');}
};
