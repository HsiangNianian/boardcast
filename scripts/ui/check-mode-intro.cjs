const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkModeIntro(page, baseUrl, evidence = '/tmp/boardcast-mode-intro') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], layouts: [], pageErrors: []};
  const check = (condition, name) => {report.checks.push({name, ok: !!condition}); assert.ok(condition, name);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.mode', 'ink');});
  const hint = () => page.locator('#board-mode-hint');
  const mode = name => page.getByRole('radio', {name, exact: true});
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    check(await hint().count() === 0, 'starting the app does not show an unrelated mode explanation');
    await mode('Hand-drawn').click();
    check(await hint().count() === 0, 'selecting the current mode does not count as a switch');
    await page.locator('main').evaluate(element => {
      const dataTransfer = new DataTransfer();
      dataTransfer.items.add(new File(['one'], 'one.txt'));
      dataTransfer.items.add(new File(['two'], 'two.txt'));
      element.dispatchEvent(new DragEvent('drop', {bubbles: true, cancelable: true, dataTransfer}));
    });
    const error = await page.locator('.toast.err').textContent();
    await mode('Free edit').click();
    check(await hint().count() === 1, 'the first actual mode switch presents the capability explanation');
    check(await page.locator('.toast.err').textContent() === error, 'the explanation does not overwrite an existing import error');
    check((await hint().textContent()).includes('same meeting'), 'the explanation identifies two views of the same meeting');
    check(await page.getByRole('radiogroup', {name: 'Board mode'}).getAttribute('aria-describedby') === 'board-mode-hint', 'the mode controls expose their explanation as an accessible description');
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await mode('Free edit').focus(); await page.keyboard.press('Home');
    check(await mode('Hand-drawn').getAttribute('aria-checked') === 'true', 'Home selects hand-drawn mode');
    await page.getByRole('button', {name: 'Circle', exact: true}).click();
    const clear = await page.evaluate(() => {
      const note = document.querySelector('.mode-note').getBoundingClientRect(), lasso = document.querySelector('.lasso-hint').getBoundingClientRect(), ink = document.querySelector('.ink').getBoundingClientRect();
      return note.bottom <= lasso.y && lasso.bottom <= ink.y;
    });
    check(clear, 'the first-switch explanation and circle instructions do not cover each other or the canvas');
    await page.getByRole('button', {name: 'Circle', exact: true}).click();
    await mode('Hand-drawn').focus();
    await page.keyboard.press('ArrowRight');
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    check(await mode('Free edit').evaluate(el => el === document.activeElement), 'arrow-key selection keeps focus on the selected mode after the editor loads');
    await page.keyboard.press('Tab');
    check(await page.evaluate(() => document.activeElement.getAttribute('role') !== 'radio'), 'Tab leaves the mode group after its one selected tab stop');
    await mode('Free edit').focus(); await page.keyboard.press('Control+ArrowLeft');
    check(await mode('Free edit').getAttribute('aria-checked') === 'true', 'modified arrows do not switch views');
    await page.keyboard.press('ArrowDown');
    check(await mode('Hand-drawn').getAttribute('aria-checked') === 'true', 'vertical arrows also move between the two radio options');
    await page.keyboard.press('End');
    check(await mode('Free edit').getAttribute('aria-checked') === 'true', 'End selects free edit');

    for (const [width, height] of [[1280, 800], [720, 480], [640, 400]]) {
      await page.setViewportSize({width, height});
      if (width <= 900) await page.getByRole('tab', {name: 'Board', exact: true}).click();
      const boxes = await page.evaluate(() => {
        const box = selector => {const b = document.querySelector(selector).getBoundingClientRect(); return {x: b.x, y: b.y, width: b.width, height: b.height, bottom: b.bottom};};
        return {note: box('.mode-note'), button: box('.mode-note button'), canvas: box('.pencil-stage'), footer: box('.meeting-footer'), overflow: document.documentElement.scrollWidth > innerWidth};
      });
      report.layouts.push({width, height, ...boxes});
      check(!boxes.overflow && boxes.note.bottom <= boxes.canvas.y + 1 && boxes.canvas.height >= 100 && boxes.button.x >= 0 && boxes.button.x + boxes.button.width <= width && boxes.button.bottom <= height && boxes.footer.bottom <= height + 1, `explanation and controls fit without covering the ${width}×${height} canvas`);
      await page.screenshot({path: `${evidence}/mode-${width}.png`});
    }
    await page.getByRole('tab', {name: 'Meeting details', exact: true}).click();
    const switchedAt = Date.now();
    await mode('Hand-drawn').click();
    await page.waitForFunction(() => document.querySelector('#board-tab')?.getAttribute('aria-selected') === 'true' && !document.querySelector('#board-panel')?.inert);
    await hint().waitFor({state: 'visible', timeout: 1000});
    report.compactSwitch = {selected: await page.getByRole('tab', {name: 'Board', exact: true}).getAttribute('aria-selected'), visible: await hint().isVisible(), elapsedMs: Date.now() - switchedAt};
    check(report.compactSwitch.selected === 'true' && report.compactSwitch.visible, 'switching board mode from compact meeting details reveals the board and explanation');
    await page.locator('.hero').waitFor({state: 'visible', timeout: 1000});
    report.compactWelcome = await page.evaluate(() => {
      const hero = document.querySelector('.hero'), box = hero.getBoundingClientRect(), style = getComputedStyle(hero);
      return {top: box.top, height: box.height, bottom: box.bottom, noteBottom: document.querySelector('.mode-note').getBoundingClientRect().bottom, opacity: style.opacity, visibility: style.visibility, maxHeight: style.maxHeight, gridRows: getComputedStyle(hero.parentElement).gridTemplateRows};
    });
    check(report.compactWelcome.top >= report.compactWelcome.noteBottom && report.compactWelcome.height >= 100 && report.compactWelcome.bottom <= 400 && report.compactWelcome.opacity === '1' && report.compactWelcome.visibility === 'visible', 'the empty-board welcome stays visible below the explanation');
    await page.getByRole('button', {name: 'Start a meeting', exact: true}).click();
    const preparation = page.getByRole('dialog', {name: 'Prepare to start', exact: true});
    await preparation.waitFor();
    check(await preparation.isVisible(), 'the compact welcome still opens meeting preparation');
    await page.keyboard.press('Escape'); await preparation.waitFor({state: 'hidden'});
    await page.screenshot({path: `${evidence}/mode-compact-handdrawn.png`});
    await mode('Free edit').click();
    await clickMeetingAction(page, 'Settings');
    await page.getByRole('dialog').getByRole('radio', {name: '中文', exact: true}).click();
    await page.getByRole('dialog').getByRole('button', {name: '关闭', exact: true}).click();
    check((await hint().textContent()).includes('同一会议') && await page.getByRole('button', {name: '知道了', exact: true}).isVisible(), 'the visible explanation and dismissal update to Chinese');
    await page.screenshot({path: `${evidence}/mode-chinese.png`});
    const light = await page.locator('.mode-note').evaluate(el => getComputedStyle(el).backgroundColor);
    await page.emulateMedia({colorScheme: 'dark'});
    await page.waitForFunction(before => getComputedStyle(document.querySelector('.mode-note')).backgroundColor !== before, light);
    await page.locator('.excalidraw.theme--dark').waitFor();
    check(await page.getByRole('button', {name: '知道了', exact: true}).isVisible(), 'the explanation follows the system dark theme and keeps its dismissal visible');
    await page.screenshot({path: `${evidence}/mode-chinese-dark.png`});
    await page.getByRole('button', {name: '知道了', exact: true}).focus(); await page.keyboard.press('Enter');
    check(await hint().count() === 0 && await mode('自由编辑').evaluate(el => el === document.activeElement), 'keyboard dismissal returns focus to the selected view');
    check(await page.evaluate(() => localStorage.getItem('boardcast.mode-hint')) === 'dismissed', 'dismissal is stored as a device preference');
    await mode('手绘').click(); await mode('自由编辑').click();
    check(await hint().count() === 0, 'later switches do not repeat a dismissed explanation');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await mode('Free edit').click(); await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    check(await hint().count() === 0, 'dismissal survives reopening the app');

    const context = await page.context().browser().newContext({viewport: {width: 900, height: 600}, reducedMotion: 'reduce'});
    try {
      await context.addInitScript(() => {
        localStorage.setItem('boardcast.lang', 'en');
        const set = Storage.prototype.setItem;
        Storage.prototype.setItem = function(key, value) {if (key === 'boardcast.mode-hint') throw new DOMException('Blocked', 'QuotaExceededError'); return set.call(this, key, value);};
      });
      const other = await context.newPage(); other.on('pageerror', error => report.pageErrors.push(error.message));
      await other.goto(baseUrl); await other.locator('.splash').waitFor({state: 'hidden'});
      await other.getByRole('radio', {name: 'Free edit', exact: true}).click();
      await other.getByRole('button', {name: 'Got it', exact: true}).click();
      await other.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      check(await other.locator('#board-mode-hint').count() === 0 && await other.evaluate(() => localStorage.getItem('boardcast.mode-hint')) === null, 'blocked preference storage still remembers dismissal for the current session');
    } finally {await context.close();}
    check(report.pageErrors.length === 0, 'mode guidance and keyboard navigation have no uncaught script errors');
    report.passed = true;
  } catch (error) {
    report.error = String(error); await page.screenshot({path: `${evidence}/failure.png`}); throw error;
  } finally {await fs.writeFile(`${evidence}/mode-intro-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
