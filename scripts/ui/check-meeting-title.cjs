const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkMeetingTitle(page, baseUrl, evidence) {
  const report = {checks: [], pageErrors: []};
  const check = (ok, name) => {assert(ok, name); report.checks.push(name); console.log(`PASS: ${name}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {if (!localStorage.getItem('boardcast.lang')) localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.log', 'open');});
  const rows = () => page.evaluate(async () => {
    if (!(await indexedDB.databases()).some(db => db.name === 'boardcast-v2')) return [];
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const all = await new Promise(yes => {const r = db.transaction('meetings').objectStore('meetings').getAll(); r.onsuccess = () => yes(r.result);}); db.close(); return all;
  });
  const title = page.locator('.meeting-title');
  const dialog = page.getByRole('dialog', {name: 'Rename meeting', exact: true});
  const input = dialog.getByRole('textbox', {name: 'Meeting title', exact: true});
  const open = async () => {await title.click(); await dialog.waitFor({timeout: 4000});};
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await open();
    check(await input.evaluate(el => el === document.activeElement), 'clicking the current title opens a focused title editor');
    await input.fill(''); await input.press('Enter');
    await dialog.getByRole('alert').getByText('Enter a meeting title.', {exact: true}).waitFor();
    check(await input.evaluate(el => el === document.activeElement) && (await rows()).length === 0, 'blank titles keep focus and do not create a meeting');
    await input.fill('Canceled name'); await input.press('Escape'); await dialog.waitFor({state: 'hidden'});
    check(await title.evaluate(el => el === document.activeElement) && (await rows()).length === 0, 'Escape cancels without saving and returns focus to the title');
    await open(); await input.fill('Composition draft');
    for (const key of ['Enter', 'Escape']) {
      await input.dispatchEvent('keydown', {key, isComposing: true});
      check(await dialog.isVisible() && await input.inputValue() === 'Composition draft' && (await rows()).length === 0, `IME ${key} cannot save or cancel a title`);
    }
    await dialog.getByRole('button', {name: 'Cancel', exact: true}).click();
    check(await title.evaluate(el => el === document.activeElement) && (await rows()).length === 0, 'pointer cancellation preserves the title and returns focus');
    await open(); await input.fill('  Named before speaking  '); await input.press('Enter');
    await dialog.waitFor({state: 'hidden'}); await page.locator('.save-state.saved').waitFor();
    const first = (await rows())[0];
    check(first?.title === 'Named before speaking' && first.titled && first.lines.length === 0, 'saving a title keeps a named empty meeting before its first sentence');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    check(await title.innerText() === first.title, 'a named empty meeting restores after reload');
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill('Keep this transcript while renaming.');
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await page.locator('.save-state.saved').waitFor();
    check((await rows())[0].title === first.title, 'new content does not replace a manually chosen title');
    await page.locator('.meeting-library').click();
    const library = page.getByRole('dialog', {name: 'Meetings', exact: true});
    await library.locator('.meeting-list > li').waitFor();
    check(await library.locator('.meeting-list > li').count() === 1, 'the independent library entry opens the same saved meeting');
    await page.keyboard.press('Escape');
    await page.evaluate(() => {
      window.qaTitlePut = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function(...args) {if (this.name === 'meetings') throw new DOMException('Title write denied', 'QuotaExceededError'); return window.qaTitlePut.apply(this, args);};
    });
    await open(); await input.fill('Retained title after failure'); await input.press('Enter');
    await dialog.getByRole('alert').filter({hasText: 'Title write denied'}).waitFor();
    check(await input.inputValue() === 'Retained title after failure' && await input.evaluate(el => el === document.activeElement) && (await rows())[0].title === first.title, 'failed saving retains the draft and focus while the stored title is unchanged');
    await page.screenshot({path: `${evidence}/save-failure.png`, animations: 'disabled'});
    await dialog.getByRole('button', {name: 'Save title', exact: true}).focus();
    await page.evaluate(() => {IDBObjectStore.prototype.put = window.qaTitlePut;});
    await page.keyboard.press('Enter'); await dialog.waitFor({state: 'hidden'}); await page.locator('.save-state.saved').waitFor();
    const recovered = (await rows())[0];
    check(recovered.title === 'Retained title after failure' && recovered.lines.some(l => l.text === 'Keep this transcript while renaming.'), 'retry saves the title together with the existing transcript');
    check(await title.evaluate(el => el === document.activeElement), 'successful saving restores focus to the header title');
    const long = '长会议名称 Long meeting title '.repeat(8).trim();
    await open(); await input.fill(long); await input.press('Enter'); await dialog.waitFor({state: 'hidden'});
    for (const [width, height] of [[1280, 800], [900, 600], [720, 480], [360, 800], [640, 400]]) {
      await page.setViewportSize({width, height}); await title.scrollIntoViewIfNeeded();
      const layout = await page.evaluate(() => {
        const library = document.querySelector('.meeting-library').getBoundingClientRect();
        const title = document.querySelector('.meeting-title').getBoundingClientRect();
        const span = document.querySelector('.meeting-title span');
        return {overflow: document.documentElement.scrollWidth > innerWidth, separated: library.right <= title.left, visible: title.width > 0 && title.right <= innerWidth && title.bottom <= innerHeight, textWidth: span.clientWidth, clipped: span.scrollWidth > span.clientWidth};
      });
      check(!layout.overflow && layout.separated && layout.visible && layout.textWidth >= 64 && layout.clipped && (await title.getAttribute('aria-label')).includes(long), `${width}x${height}: long titles keep readable space without hiding either action or their accessible name`);
      await page.screenshot({path: `${evidence}/header-${width}x${height}.png`, animations: 'disabled'});
      await title.focus(); await page.keyboard.press('Enter'); await dialog.waitFor();
      const controls = await dialog.locator('input, button').evaluateAll(elements => elements.map(el => {const r = el.getBoundingClientRect(); return r.width > 0 && r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight;}));
      check(controls.every(Boolean) && await input.inputValue() === long && await input.evaluate(el => el === document.activeElement), `${width}x${height}: keyboard editing shows the full title and reachable save/cancel controls`);
      await page.screenshot({path: `${evidence}/editor-${width}x${height}.png`, animations: 'disabled'});
      await input.press('Escape'); await dialog.waitFor({state: 'hidden'});
    }
    for (const lang of ['zh', 'en']) for (const theme of ['dark', 'light']) {
      await page.evaluate(({lang, theme}) => {localStorage.setItem('boardcast.lang', lang); localStorage.setItem('boardcast.theme', theme);}, {lang, theme});
      await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      await title.click();
      const localized = page.getByRole('dialog', {name: lang === 'zh' ? '修改会议名称' : 'Rename meeting', exact: true});
      const field = localized.getByRole('textbox', {name: lang === 'zh' ? '会议名称' : 'Meeting title', exact: true});
      check(await field.inputValue() === long && await field.evaluate(el => el === document.activeElement) && await localized.getByRole('button', {name: lang === 'zh' ? '保存名称' : 'Save title', exact: true}).isVisible(), `${lang}/${theme}: the compact title editor restores the full title with localized controls`);
      await page.screenshot({path: `${evidence}/editor-${lang}-${theme}.png`, animations: 'disabled'});
      await localized.getByRole('button', {name: lang === 'zh' ? '取消' : 'Cancel', exact: true}).click();
    }
    check(report.pageErrors.length === 0, 'title editing has no uncaught script errors');
  } finally {await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2) + '\n');}
};
