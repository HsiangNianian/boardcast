const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkDemos(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], demos: [], pageErrors: []};
  const check = (condition, label) => {assert.ok(condition, label); report.checks.push(label);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19880'); localStorage.setItem('boardcast.log', 'open');});
  await page.route('http://127.0.0.1:19880/**', route => route.fulfill({status: 503, body: 'No local model for recorded samples'}));
  const index = JSON.parse(await fs.readFile('assets/demos/index.json', 'utf8'));
  const rows = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const all = await new Promise(yes => {const r = db.transaction('meetings').objectStore('meetings').getAll(); r.onsuccess = () => yes(r.result);}); db.close(); return all;
  });
  await fs.mkdir(evidence, {recursive: true});
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    for (const lang of ['en', 'zh']) {
      await page.evaluate(lang => localStorage.setItem('boardcast.lang', lang), lang); await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      const play = page.getByRole('button', {name: lang === 'en' ? 'Play sample meeting' : '播放示例会议', exact: true});
      for (const demo of index.filter(demo => demo.lang === lang)) {
        await page.locator('.demo-pick select').selectOption(demo.id);
        await play.click(); await page.locator('.replay-dock').waitFor();
        const range = page.locator('.replay-dock input[type=range]');
        const data = JSON.parse(await fs.readFile(`assets/demos/${demo.id}.json`, 'utf8'));
        check(Number(await range.getAttribute('max')) === data.steps.length, `${demo.id}: the recorded sample starts its complete replay`);
        await range.focus(); await range.press('End');
        await page.waitForFunction(total => Number(document.querySelector('.replay-dock input[type=range]')?.value) === total, data.steps.length);
        await page.locator('.replay-dock').getByRole('button', {name: lang === 'en' ? 'Exit replay' : '退出回放', exact: true}).click();
        await page.locator('.meeting-library').click();
        const library = page.getByRole('dialog', {name: lang === 'en' ? 'Meetings' : '会议', exact: true});
        await library.locator('.meeting-list').waitFor({state: 'attached'});
        await library.getByRole('status').waitFor({state: 'hidden'});
        check(await library.locator('.meeting-list > li').count() === 0 && (await rows()).length === 0, `${demo.id}: an untouched sample is separate from real meeting history`);
        await page.keyboard.press('Escape');
        await page.locator('.meeting-actions').getByRole('button', {name: lang === 'en' ? 'New meeting' : '新会议', exact: true}).click();
        await play.waitFor();
        report.demos.push({id: demo.id, lang, steps: data.steps.length, passed: true});
      }
    }
    await page.evaluate(() => localStorage.setItem('boardcast.lang', 'en')); await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    const demo = index.find(demo => demo.lang === 'en');
    await page.locator('.demo-pick select').selectOption(demo.id);
    await page.route(`**/demos/${demo.id}.json`, route => route.fulfill({contentType: 'application/json', body: '{'}));
    const before = await page.evaluate(() => localStorage.getItem('boardcast.meeting'));
    await page.getByRole('button', {name: 'Play sample meeting', exact: true}).click(); await page.locator('.toast.err').waitFor();
    check(before === await page.evaluate(() => localStorage.getItem('boardcast.meeting')) && await page.locator('.replay-dock').count() === 0, 'a broken sample preserves the current meeting and permits retry');
    await page.unroute(`**/demos/${demo.id}.json`);
    await page.getByRole('button', {name: 'Play sample meeting', exact: true}).click(); await page.locator('.replay-dock').waitFor();
    await page.locator('.replay-dock').getByRole('button', {name: 'Exit replay', exact: true}).click();
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('My own addition to this sample');
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click(); await page.locator('.save-state.saved').waitFor();
    const saved = await rows();
    check(saved.length === 1 && saved[0].lines.some(line => line.text === 'My own addition to this sample'), 'adding personal content turns a sample into one saved meeting');
    await page.locator('.meeting-library').click();
    await page.getByRole('dialog', {name: 'Meetings', exact: true}).locator('.meeting-list > li').waitFor();
    check(report.pageErrors.length === 0, 'all recorded sample flows have no browser script errors'); report.passed = true;
  } catch (error) {report.error = String(error); await page.screenshot({path: `${evidence}/demos-failure.png`}); throw error;}
  finally {await fs.writeFile(`${evidence}/demos-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
