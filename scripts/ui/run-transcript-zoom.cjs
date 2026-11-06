// Reuse the long-meeting flow, then zoom a real Chromium tab in a disposable profile.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const checkReading = require('./check-transcript-reading.cjs');
const baseUrl = process.argv[2] || 'http://127.0.0.1:1438';
const evidence = process.argv[3] || '/tmp/boardcast-transcript-zoom';

(async () => {
  await fs.mkdir(evidence, {recursive: true});
  const temporary = await fs.mkdtemp('/tmp/boardcast-reading-zoom-');
  const extension = path.join(temporary, 'extension');
  await fs.mkdir(extension);
  await fs.writeFile(path.join(extension, 'manifest.json'), JSON.stringify({manifest_version: 3, name: 'Boardcast reading zoom QA', version: '1.0', permissions: ['tabs'], background: {service_worker: 'zoom.js'}}));
  await fs.writeFile(path.join(extension, 'zoom.js'), 'chrome.runtime.onInstalled.addListener(() => {});');
  const report = {baseUrl, checks: [], failures: [], views: [], pageErrors: []};
  const check = (ok, label) => (ok ? report.checks : report.failures).push(label);
  let context, page;
  try {
    context = await chromium.launchPersistentContext(path.join(temporary, 'profile'), {executablePath: process.env.CHROMIUM_PATH || '/usr/bin/chromium', headless: true, viewport: {width: 1280, height: 800}, reducedMotion: 'reduce', args: ['--no-sandbox', `--disable-extensions-except=${extension}`, `--load-extension=${extension}`]});
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
    page = context.pages()[0]; page.setDefaultTimeout(15000);
    page.on('pageerror', error => report.pageErrors.push(error.message));
    try { await checkReading(page, baseUrl, path.join(evidence, 'reading')); }
    catch (error) {
      if (error.code !== 'ERR_ASSERTION') throw error;
      report.failures.push('long-meeting reading flow failed; see reading/transcript-reading-check.json');
    }
    const lines = page.locator('#sidebar-transcript-panel .lines');
    const latest = page.locator('.follow-transcript');
    await page.setViewportSize({width: 1280, height: 800}); await page.waitForTimeout(200);
    await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
    await page.locator('#sidebar-contents-panel select').selectOption('19');
    await page.getByRole('button', {name: 'Find in transcript', exact: true}).click();
    await lines.locator('.line.jump').waitFor({state: 'attached'});
    await page.waitForFunction(() => !document.querySelector('.line.jump'));
    await lines.evaluate(element => element.focus({preventScroll: true}));
    const box = await lines.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, -200); await page.waitForTimeout(250);
    const index = await lines.evaluate(list => {
      const middle = list.getBoundingClientRect().top + list.clientHeight / 2;
      return [...list.querySelectorAll('.line')].findIndex(line => {
        const box = line.getBoundingClientRect(); return box.top <= middle && box.bottom >= middle;
      });
    });
    assert(index >= 0, 'a historical transcript line is under the reading cursor');
    report.anchorIndex = index;
    for (const following of [false, true]) {
      if (following) { await latest.click(); await page.waitForTimeout(200); }
      for (const zoom of [1.25, 1.5, 2, 1.5, 1]) {
        const actual = await worker.evaluate(async ({baseUrl, zoom}) => {
          const tabs = await chrome.tabs.query({url: `${baseUrl}/*`});
          await chrome.tabs.setZoom(tabs[0].id, zoom); return chrome.tabs.getZoom(tabs[0].id);
        }, {baseUrl, zoom});
        await page.waitForFunction(width => innerWidth === width, Math.round(1280 / zoom));
        await page.waitForTimeout(250);
        const view = await lines.evaluate((list, index) => {
          const scroller = getComputedStyle(list).overflowY === 'visible' ? list.closest('.log') : list;
          const entries = list.querySelectorAll('.line');
          return {width: innerWidth, height: innerHeight, dpr: devicePixelRatio, scale: visualViewport.scale, remaining: scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop, viewport: scroller.getBoundingClientRect().toJSON(), anchor: entries[index].getBoundingClientRect().toJSON(), tail: entries[entries.length - 1].getBoundingClientRect().toJSON(), focused: document.activeElement === list};
        }, index);
        report.views.push({following, zoom, actual, ...view});
        check(Math.abs(actual - zoom) < 0.01 && Math.abs(view.dpr - zoom) < 0.01 && view.scale === 1, `${following ? 'following' : 'reading'} at ${zoom * 100}% uses browser zoom`);
        if (following) check(view.remaining < 32 && view.tail.bottom > view.viewport.top && view.tail.bottom <= view.viewport.bottom + 1 && await latest.count() === 0, `${zoom * 100}% zoom follows the visible latest transcript`);
        else {
          check(view.anchor.bottom > view.viewport.top && view.anchor.top < view.viewport.bottom && await latest.count() === 1, `${zoom * 100}% zoom preserves the user-scrolled historical line`);
          check(view.focused, `${zoom * 100}% zoom preserves transcript keyboard focus`);
        }
        if (zoom === 2) await page.screenshot({path: path.join(evidence, `${following ? 'following' : 'reading'}-200-percent.png`), animations: 'disabled'});
      }
    }
    check(report.pageErrors.length === 0, 'continuous browser zoom has no script errors');
    report.passed = report.failures.length === 0;
    assert.deepEqual(report.failures, []);
  } catch (error) {
    report.error = String(error);
    if (page) await page.screenshot({path: path.join(evidence, 'failure.png')}).catch(() => {});
    throw error;
  } finally {
    if (context) await context.close();
    await fs.rm(temporary, {recursive: true, force: true});
    report.closed = true;
    await fs.writeFile(path.join(evidence, 'transcript-zoom-check.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({passed: report.passed, checks: report.checks.length, failures: report.failures, closed: report.closed}));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
