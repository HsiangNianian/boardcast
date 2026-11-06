const meetingAction = require('./meeting-action.cjs');
// Production UI with the visual-matrix native fixture; no audio or user data.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const baseUrl = process.argv[2] || 'http://127.0.0.1:1430';
const evidence = process.argv[3] || '/tmp/boardcast-touch-targets';

(async () => {
  await fs.mkdir(evidence, {recursive:true});
  const source = await fs.readFile(path.join(__dirname, 'run-visual-matrix.cjs'), 'utf8');
  const fixture = source.match(/function nativeFixture\([\s\S]*?\n}\n/)?.[0];
  assert(fixture, 'reuse the visual-matrix native fixture');
  const browser = await chromium.launch({headless:true, executablePath:process.env.CHROMIUM_PATH || '/usr/bin/chromium'});
  const rows = [];
  try {
    for (const lang of ['en','zh']) for (const theme of ['light','dark']) for (const [width,height] of [[1280,800],[900,600],[720,480],[480,720]]) {
      const name = `${lang}-${theme}-${width}x${height}`;
      if (process.env.TOUCH_CASE && !new RegExp(process.env.TOUCH_CASE).test(name)) continue;
      const row = {name, targets:[], pageErrors:[]}; rows.push(row);
      const context = await browser.newContext({viewport:{width,height}, hasTouch:true, reducedMotion:'reduce'});
      try {
        await context.addInitScript({content:`(${fixture})(${JSON.stringify({lang,theme})})`});
        const page = await context.newPage(); page.setDefaultTimeout(10000);
        page.on('pageerror', error => row.pageErrors.push(error.message));
        await page.goto(baseUrl); await page.locator('.splash').waitFor({state:'hidden'});
        await page.evaluate(() => document.fonts.ready);
        assert(await page.evaluate(() => matchMedia('(pointer: coarse)').matches));
        async function target(locator, state) {
          await locator.scrollIntoViewIfNeeded();
          // Scrolling old transcript lines reveals Jump to latest and changes the viewport.
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          await locator.scrollIntoViewIfNeeded();
          const measured = await locator.evaluate(el => {
            const r = el.getBoundingClientRect(), hit = document.elementFromPoint(r.x+r.width/2, r.y+r.height/2);
            return {label:el.getAttribute('aria-label') || el.textContent.trim().slice(0,80), x:r.x, y:r.y, width:r.width, height:r.height,
              hit:hit?.outerHTML.slice(0,300),
              inside:r.x>=-1 && r.y>=-1 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1, reachable:el.contains(hit)};
          });
          row.targets.push({state, ...measured});
          if (!measured.inside || !measured.reachable) await page.screenshot({path:path.join(evidence, `${name}-unreachable.png`)});
          assert(measured.width >= 43.99 && measured.height >= 43.99, `${state}: ${measured.label} is ${measured.width} x ${measured.height}, expected at least 44 x 44`);
          assert(measured.inside && measured.reachable, `${state}: ${measured.label} is clipped or covered`);
        }
        async function scan(root, state) {
          // Only the active dialog is scanned when modal; its background is inert.
          const controls = root.locator('button:not(:disabled), select:not(:disabled), textarea:not(:disabled), input:not(:disabled):not([type=checkbox], [type=radio], [type=range], [type=hidden]), label:has(> input:is([type=checkbox], [type=radio]))').filter({visible:true});
          for (const control of await controls.elementHandles()) {
            if (await control.evaluate(el => !el.isConnected || !!el.closest('.excalidraw'))) continue;
            await target(control, state);
          }
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${state}: horizontal shell overflow`);
          await page.screenshot({path:path.join(evidence, `${name}-${state}.png`)});
        }
        await target(page.locator('.meeting-footer .mic'), 'start');
        await page.evaluate(() => window.qaMatrixEmit('voice-status', {installed:true,listening:true,device:'System default microphone'}));
        await page.waitForFunction(() => document.querySelector('.meeting-footer .mic')?.getAttribute('aria-pressed') === 'true');
        await target(page.locator('.meeting-footer .mic'), 'stop');
        await scan(page.locator('.app'), 'workbench');
        if (await page.locator('.meeting-more').isVisible()) {
          await page.locator('.meeting-more').tap();
          await scan(page.locator('.meeting-menu'), 'meeting-actions');
          await page.keyboard.press('Escape');
        }
        await page.locator('.dock .speaker-actions > .small').tap();
        await scan(page.locator('dialog[open]'), 'speakers');
        await page.keyboard.press('Escape');
        if (width <= 900) await page.getByRole('tab', {name:lang==='zh'?'会议详情':'Meeting details', exact:true}).tap();
        for (const tab of (lang==='zh' ? ['原话','摘要','内容列表'] : ['Transcript','Summary','Contents'])) {
          await page.locator('.sidebar-tabs').getByRole('tab', {name:tab, exact:true}).tap();
          await scan(page.locator('.log'), tab === (lang==='zh'?'原话':'Transcript') ? 'transcript' : tab === (lang==='zh'?'摘要':'Summary') ? 'summary' : 'contents');
        }
        for (const kind of ['Import','Export']) {
          await (await meetingAction(page, lang==='zh' ? (kind==='Import'?'导入':'导出') : kind)).tap();
          await scan(page.locator('.action-menu'), kind.toLowerCase());
          await page.keyboard.press('Escape');
        }
        await (await meetingAction(page, lang==='zh'?'设置':'Settings')).tap();
        const dialog = page.locator('dialog[open]');
        for (const [i, section] of ['general','audio','models','privacy','glossary','maintenance'].entries()) {
          const mobile = dialog.locator('.settings-mobile-nav select');
          if (await mobile.isVisible()) await mobile.selectOption(section);
          else await dialog.locator('.settings-nav [role=tab]').nth(i).tap();
          await scan(dialog, section);
        }
        await page.keyboard.press('Escape');
        await (await meetingAction(page, lang==='zh'?'结束并复盘':'End and review')).tap();
        await scan(page.locator('dialog[open]'), 'review');
        assert.deepEqual(row.pageErrors, []);
        row.ok = true;
      } catch (error) {row.error = String(error); console.error(name, row.error);}
      finally {await context.close();}
      console.log(name, row.ok ? `${row.targets.length} targets passed` : 'FAILED');
    }
  } finally {await browser.close(); await fs.writeFile(path.join(evidence, 'report.json'), JSON.stringify(rows, null, 2)+'\n');}
  assert(rows.length && rows.every(row => row.ok), 'all selected touch cases pass');
})().catch(error => {console.error(error); process.exitCode = 1;});
