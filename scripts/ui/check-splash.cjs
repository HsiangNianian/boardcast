const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkSplash(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => {assert.ok(ok, label); report.checks.push(label);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    window.qaIntroTiming = {start:null,end:null};
    new MutationObserver(() => {
      const timing=window.qaIntroTiming;
      if (document.querySelector('.splash') && timing.start===null) timing.start=performance.now();
      if (!document.querySelector('.splash') && timing.start!==null && timing.end===null) timing.end=performance.now();
    }).observe(document,{childList:true,subtree:true});
  });
  try {
    await page.goto(`${baseUrl}/scripts/ui/fixtures/splash.html`);
    await page.getByRole('button', {name: 'Skip intro', exact: true}).waitFor();
    await page.locator('#background').evaluate(button => button.focus());
    check(await page.evaluate(() => document.activeElement?.textContent === 'Skip intro'), 'the welcome dialog focuses Skip and keeps background controls inert');
    await page.keyboard.press('/');
    await page.locator('.splash').waitFor({state: 'hidden'});
    check(await page.evaluate(() => window.qa.done === 1 && window.qa.keys.length === 0 && window.qa.clicks === 0), 'the skip key is consumed instead of reaching application shortcuts');
    await page.keyboard.press('x');
    check(await page.evaluate(() => window.qa.keys.includes('x')), 'ordinary keyboard input resumes after the welcome dialog closes');
    await page.reload();
    await page.getByRole('button', {name: 'Skip intro', exact: true}).click();
    await page.locator('.splash').waitFor({state: 'hidden'});
    check(await page.evaluate(() => window.qa.done === 1), 'clicking Skip completes once despite parent rerenders every 25 ms');
    await page.route('**/src/ui/Wordmark.tsx*', route => route.fulfill({contentType: 'text/javascript', body: 'export default function Wordmark(){return null}'}));
    await page.reload();
    const started = Date.now();
    await page.locator('.splash').waitFor({state: 'hidden'});
    report.maxWaitMs = Date.now() - started;
    report.maxVisibleMs = await page.evaluate(() => window.qaIntroTiming.end-window.qaIntroTiming.start);
    check(report.maxVisibleMs < 3100 && await page.evaluate(() => window.qa.done === 1), 'a wordmark that never reports readiness cannot keep the overlay past its three-second budget plus scheduling tolerance');
    await page.emulateMedia({reducedMotion: 'reduce'});
    await page.reload();
    const reducedStarted = Date.now();
    await page.locator('.splash').waitFor({state: 'hidden'});
    report.reducedWaitMs = Date.now() - reducedStarted;
    check(report.reducedWaitMs < 700, 'reduced motion skips the intro without waiting for glyphs');
    check(report.pageErrors.length === 0, 'welcome interactions have no browser script errors'); report.passed = true;
  } catch (error) {report.error = String(error); throw error;}
  finally {await fs.mkdir(evidence, {recursive: true}); await fs.writeFile(`${evidence}/splash-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
