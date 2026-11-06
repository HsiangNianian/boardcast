const meetingAction = require('./meeting-action.cjs');
const {click: clickMeetingAction} = meetingAction;
// Actual production App with controlled native save/open replies; no OS dialog or launcher is used.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkExportHandoff(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => { assert.ok(ok, label); report.checks.push(label); };
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.clear(); localStorage.setItem('boardcast.lang', new URL(location.href).searchParams.get('handoffLang') || 'en');
    localStorage.setItem('boardcast.meeting', 'export-handoff');
    const graph = {nodes: [{id: 'n1', label: 'Export content', kind: 'node', marks: [], aliases: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
    const seed = {version: 2, storageRevision: 1, id: 'export-handoff', title: 'Export handoff', titled: true, createdAt: 1, updatedAt: 1, graph, history: [], lines: [{text: 'PRESERVED_EXPORT_TRANSCRIPT', speaker: 'A', ms: 1, page: 0}], page: 0, pages: [{graph}]};
    const q = window.qaExport = {seed, calls: [], events: {}, callbacks: {}, saveResult: '/fixture/meeting notes.md', saveHold: false, saveFail: null, meetingFail: null, openHold: false, openFail: null, revealFail: null};
    if (new URL(location.href).searchParams.has('badTime')) seed.updatedAt = 1e30;
    q.emit = (event, payload) => (q.events[event] || []).forEach(id => q.callbacks[id]?.({event, id, payload}));
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => {q.events[event] = (q.events[event] || []).filter(value => value !== id);}};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'ready', url: 'http://127.0.0.1:19881', model: 'Fixture', device: 'Fixture', gpus: []};
        if (command === 'remote_prepare') return crypto.randomUUID();
        if (command === 'llm_chat') return {choices: [{message: {content: '-'}}]};
        if (command === 'voice_status') return {installed: true, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null};
        if (command === 'voice_devices') return [];
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') return args.storageVersion === 2 && args.id === seed.id ? JSON.stringify(q.seed) : null;
        if (command === 'meeting_save') {if (q.meetingFail) throw new Error(q.meetingFail); q.seed = JSON.parse(args.json); return;}
        if (command === 'meetings_list') return [];
        if (command === 'models_list') return {models: [], freeBytes: 1000000};
        if (command === 'plugin:app|version') return '0.21.0-fixture';
        if (command === 'session_wayland') return false;
        if (command === 'export_save') {
          const failure = q.saveFail, result = q.saveResult;
          if (q.saveHold) await new Promise(resolve => {q.releaseSave = resolve;});
          if (failure) throw new Error(failure);
          return result;
        }
        if (command === 'export_open') {
          const failure = q.openFail;
          if (q.openHold) await new Promise(resolve => {q.releaseOpen = resolve;});
          if (failure) throw new Error(failure);
        }
        if (command === 'export_reveal' && q.revealFail) throw new Error(q.revealFail);
        return null;
      },
    };
  });
  const exported = () => meetingAction(page, 'Export');
  const toast = page.locator('.toast');
  const count = command => page.evaluate(command => window.qaExport.calls.filter(call => call.command === command).length, command);
  const exportFile = async (format = 'Markdown', lang = 'en') => {
    await clickMeetingAction(page, lang === 'en' ? 'Export' : '导出');
    await page.getByRole('menuitemradio', {name: format === 'PNG' ? (lang === 'en' ? 'Current board' : '当前白板') : (lang === 'en' ? 'Whole meeting' : '整场会议'), exact: true}).click();
    await page.getByRole('menuitem', {name: new RegExp(`^${format}`)}).click();
  };
  try {
    await fs.mkdir(evidence, {recursive: true});
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
    check(await page.locator('.save-state.saved time').getAttribute('datetime') === new Date(1).toISOString(), 'save status displays the confirmed stored revision time');
    await page.setViewportSize({width: 640, height: 400});
    await page.locator('.meeting-more').waitFor();
    const headerFits = () => page.evaluate(() => {
      const label = document.querySelector('.library-label').getBoundingClientRect(), saved = document.querySelector('.save-state').getBoundingClientRect();
      return label.right <= saved.left && saved.right <= innerWidth;
    });
    check(await headerFits(), 'the English meeting-library label does not overlap save status in a short window');
    await (await exported()).focus(); await page.keyboard.press('Enter');
    check(await page.evaluate(() => document.activeElement?.textContent === 'Whole meeting'), 'export focuses the scope chooser');
    await page.keyboard.press('ArrowDown'); await page.keyboard.press('Enter');
    check(await page.getByRole('menuitemradio', {name: 'Current board', exact: true}).getAttribute('aria-checked') === 'true' && await page.getByRole('menuitem').count() === 4, 'keyboard selects the four current-board formats');
    await page.keyboard.press('End');
    check(await page.evaluate(() => document.activeElement?.textContent.startsWith('Excalidraw')), 'menu arrows include both scopes and formats');
    const box = await page.getByRole('menu').boundingBox();
    check(box.x >= 0 && box.y >= 0 && box.x + box.width <= 640 && box.y + box.height <= 401, 'the changed scope stays inside a short window');
    await page.screenshot({path: `${evidence}/export-current-board.png`, animations: 'disabled'});
    await page.keyboard.press('Home'); await page.keyboard.press('Enter');
    check(await page.getByRole('menuitem').count() === 3, 'whole meeting offers Markdown, PDF and offline replay');
    await page.keyboard.press('Escape');
    check(await page.locator('.meeting-more').evaluate(button => document.activeElement === button), 'closing the compact export menu restores More focus');

    await page.evaluate(() => {window.qaExport.saveResult = null;});
    await exportFile(); await page.waitForFunction(() => window.qaExport.calls.some(call => call.command === 'export_save'));
    await toast.waitFor({state: 'hidden'});
    await page.evaluate(() => new Promise(requestAnimationFrame));
    report.cancelFocus = await page.evaluate(() => ({tag: document.activeElement.tagName, more: document.activeElement === document.querySelector('.meeting-more')}));
    await page.waitForFunction(() => document.activeElement === document.querySelector('.meeting-more'));
    check(true, 'canceling the system save restores the persistent compact trigger');
    check(await (await exported()).isEnabled() && await page.getByRole('button', {name: 'Open file', exact: true}).count() === 0, 'canceling the system save has no success or file actions');

    await page.setViewportSize({width: 1280, height: 800});
    await exportFile(); await toast.waitFor({state: 'hidden'});
    await page.waitForFunction(() => document.activeElement === document.querySelector('.meeting-actions [aria-label="Export"]'));
    check(true, 'canceling a desktop export restores its re-enabled button');
    await page.setViewportSize({width: 640, height: 400});
    await page.locator('.meeting-more').waitFor();
    for (const focus of ['input', 'dialog', 'resize']) {
      await page.evaluate(() => {window.qaExport.saveHold = true; window.qaExport.releaseSave = null;});
      await exportFile(); await page.waitForFunction(() => typeof window.qaExport.releaseSave === 'function');
      if (focus === 'input') await page.locator('.write textarea').fill('Unsent export focus draft');
      if (focus === 'dialog') {await clickMeetingAction(page, 'Settings'); await page.getByRole('dialog').waitFor();}
      if (focus === 'resize') {await page.setViewportSize({width: 1280, height: 800}); await page.locator('.meeting-actions [aria-label="Export"]').waitFor();}
      await page.evaluate(() => {window.qaExport.saveHold = false; window.qaExport.releaseSave();});
      await toast.waitFor({state: 'hidden'}); await page.evaluate(() => new Promise(requestAnimationFrame));
      if (focus === 'input') {
        check(await page.locator('.write textarea').evaluate(el => document.activeElement === el && el.value === 'Unsent export focus draft'), 'finishing a background export preserves the current input focus and draft');
        await page.locator('.write textarea').fill('');
      } else if (focus === 'dialog') {
        check(await page.getByRole('dialog').evaluate(el => el.contains(document.activeElement)), 'finishing a background export keeps focus in the current dialog');
        await page.keyboard.press('Escape'); await page.getByRole('dialog').waitFor({state: 'detached'});
      } else {
        report.resizedFocus = await page.evaluate(() => ({active: document.activeElement.outerHTML.slice(0, 500), returned: document.activeElement === document.querySelector('.meeting-actions [aria-label="Export"]')}));
        await page.waitForFunction(() => document.activeElement === document.querySelector('.meeting-actions [aria-label="Export"]'));
        check(true, 'an export completed after resizing uses the currently mounted trigger');
        await page.setViewportSize({width: 640, height: 400}); await page.locator('.meeting-more').waitFor();
      }
    }

    await page.evaluate(() => {window.qaExport.saveResult = '/fixture/meeting notes.md'; window.qaExport.saveHold = true; window.qaExport.releaseSave = null;});
    await exportFile(); await page.waitForFunction(() => typeof window.qaExport.releaseSave === 'function');
    check((await toast.innerText()).includes('Choose where to save') && await (await exported()).isDisabled(), 'waiting for the system save is explicit and disables repeat exports');
    await page.keyboard.press('Escape');
    const saves = await count('export_save');
    await page.evaluate(() => {window.qaExport.emit('shell-menu-action', 'export'); window.qaExport.emit('shell-menu-action', 'export');});
    check(await page.getByRole('menu').count() === 0 && await count('export_save') === saves, 'native menu requests cannot duplicate a pending export');
    await page.evaluate(() => {window.qaExport.saveHold = false; window.qaExport.releaseSave();});
    await toast.getByRole('button', {name: 'Open file', exact: true}).waitFor();
    await page.waitForTimeout(4200);
    check((await toast.innerText()).includes('/fixture/meeting notes.md'), 'the saved path and actions remain after the ordinary toast timeout');
    const bytes = await page.evaluate(() => window.qaExport.calls.find(call => call.command === 'export_save').args.dataBase64);
    const [{default: Markdown}, {createElement}, {renderToStaticMarkup}] = await Promise.all([import('react-markdown'), import('react'), import('react-dom/server')]);
    const renderedMarkdown = renderToStaticMarkup(createElement(Markdown, {children: Buffer.from(bytes, 'base64').toString('utf8')}));
    check(renderedMarkdown.includes('PRESERVED_EXPORT_TRANSCRIPT'), 'the save receives the generated meeting content');

    await page.evaluate(() => {window.qaExport.openHold = true; window.qaExport.openFail = 'Delayed old launch failure';});
    await toast.getByRole('button', {name: 'Open file', exact: true}).click();
    await page.waitForFunction(() => typeof window.qaExport.releaseOpen === 'function');
    check(await toast.getByRole('button', {name: 'Show in folder', exact: true}).isDisabled() && await count('export_open') === 1, 'a pending file action cannot be duplicated');
    await exportFile(); await toast.getByText('Saved: /fixture/meeting notes.md', {exact: true}).waitFor();
    await page.evaluate(() => {window.qaExport.openHold = false; window.qaExport.openFail = null; window.qaExport.releaseOpen();});
    await page.waitForFunction(() => !document.querySelector('.toast button')?.disabled);
    check((await toast.innerText()).startsWith('Saved:'), 'an old file action cannot overwrite a newer export even at the same path');

    await page.evaluate(() => {window.qaExport.openFail = 'No application is associated with Markdown';});
    await toast.getByRole('button', {name: 'Open file', exact: true}).click();
    await page.locator('.toast.err').waitFor();
    check((await toast.innerText()).includes('No application is associated') && (await toast.innerText()).includes('/fixture/meeting notes.md'), 'open failure keeps the path and offers retry or folder recovery');
    await page.evaluate(() => {window.qaExport.revealFail = 'File manager unavailable';});
    await toast.getByRole('button', {name: 'Show in folder', exact: true}).click();
    await toast.getByText(/File manager unavailable/).waitFor();
    await page.evaluate(() => {window.qaExport.revealFail = null;});
    await toast.getByRole('button', {name: 'Show in folder', exact: true}).click();
    await toast.getByText('Asked the system to show the folder for: /fixture/meeting notes.md', {exact: true}).waitFor();
    check(await count('export_reveal') === 2, 'folder recovery retries without generating or saving another export');
    check(await page.evaluate(() => window.qaExport.calls.filter(call => ['export_open', 'export_reveal'].includes(call.command)).every(call => call.args.path === '/fixture/meeting notes.md')), 'file actions pass only the native save result');
    await page.screenshot({path: `${evidence}/export-saved-actions.png`, animations: 'disabled'});
    await toast.getByRole('button', {name: 'Close', exact: true}).click();

    await page.evaluate(() => {window.qaExport.saveFail = 'Permission denied (EACCES)';});
    await exportFile(); await toast.getByText(/Permission denied/).waitFor();
    check(await (await exported()).isEnabled() && await toast.getByRole('button', {name: 'Open file', exact: true}).count() === 0, 'save failure preserves the error and releases the export action without claiming a file');
    check((await toast.innerText()).includes('export file or folder is not writable'), 'permission failure explains file or folder access and a writable export location');
    await page.evaluate(() => {window.qaExport.saveFail = null;});
    await toast.getByRole('button', {name: 'Export again', exact: true}).click(); await toast.getByRole('button', {name: 'Open file', exact: true}).waitFor();
    check((await toast.innerText()).startsWith('Saved:'), 'failed export can be retried successfully');

    await page.route('**/player/player.js', route => route.fulfill({status: 503, body: 'Unavailable fixture'}));
    const beforeBuildFailure = await count('export_save');
    await exportFile('Replay page'); await page.locator('.toast.err').waitFor();
    await page.waitForFunction(() => document.activeElement === document.querySelector('.meeting-more'));
    check(true, 'a generation error also restores focus after the export trigger is enabled');
    check(await count('export_save') === beforeBuildFailure && await (await exported()).isEnabled(), 'generation failure never opens a save dialog and allows retry');
    check(!(await toast.innerText()).includes('SVG vector'), 'a replay generation failure does not offer the PNG fallback');
    await page.unroute('**/player/player.js');

    await page.goto(`${baseUrl}/?handoffLang=zh`); await page.locator('.splash').waitFor({state: 'hidden'});
    await clickMeetingAction(page, '导出');
    await page.getByRole('menuitemradio', {name: '整场会议', exact: true}).click();
    await page.getByRole('menuitem', {name: /^Markdown/}).click();
    await toast.getByRole('button', {name: '显示所在文件夹', exact: true}).waitFor();
    check(await headerFits(), 'the Chinese meeting-library label does not overlap save status in a short window');
    check((await toast.innerText()).includes('已保存：/fixture/meeting notes.md') && await toast.getByRole('button', {name: '打开文件', exact: true}).isEnabled(), 'Chinese saved-file actions are localized');
    await page.screenshot({path: `${evidence}/export-saved-actions-zh.png`, animations: 'disabled'});

    for (const lang of ['en', 'zh']) {
      await page.goto(`${baseUrl}/?handoffLang=${lang}`); await page.locator('.splash').waitFor({state: 'hidden'});
      await page.setViewportSize({width: 720, height: 480});
      const en = lang === 'en';
      const cases = [
        ['full', 'No space left on device (os error 28)', en ? 'Free space and retry' : '释放空间后重试', en ? 'choose another drive' : '选择其他磁盘'],
        ['permission', 'Permission denied (os error 13)', en ? 'Check its permissions' : '检查写入权限', en ? 'another writable folder' : '其他可写文件夹'],
        ['quota', 'The quota has been exceeded.', en ? 'Do not clear this site' : '不要清除此网站', en ? 'quota is exhausted' : '存储额度不足'],
        ['unknown', 'Unexpected storage failure', null, null],
      ];
      for (const [kind, error, hint, exportHint] of cases) {
        await page.setViewportSize(kind === 'full' ? {width: 640, height: 400} : {width: 720, height: 480});
        const before = await page.evaluate(() => JSON.stringify(window.qaExport.seed));
        const line = `RETAINED_${lang}_${kind}`;
        await page.evaluate(error => {window.qaExport.meetingFail = error;}, error);
        await page.getByRole('textbox', {name: en ? 'What was said' : '说的话', exact: true}).fill(line);
        await page.getByRole('button', {name: en ? 'Put on the board' : '写上白板', exact: true}).click();
        const banner = page.locator('.save-error');
        await banner.getByText(error, {exact: true}).waitFor();
        check((hint ? (await banner.innerText()).includes(hint) : await banner.locator('p').count() === 1) && (await banner.innerText()).includes(error), `${lang}/${kind}: save guidance preserves the original error without inventing a cause`);
        await clickMeetingAction(page, en ? 'New meeting' : '新会议');
        await page.locator('.transition-state').waitFor({state: 'hidden'});
        check(await page.evaluate(() => JSON.stringify(window.qaExport.seed)) === before && (await page.locator('.lines').innerText()).includes(line) && await banner.isVisible(), `${lang}/${kind}: failed switching retains the old record and the new transcript`);
        await page.screenshot({path: `${evidence}/save-guidance-${lang}-${kind}.png`, animations: 'disabled'});
        await page.evaluate(() => {window.qaExport.meetingFail = null;});
        await banner.getByRole('button', {name: en ? 'Retry save' : '重试保存', exact: true}).focus();
        if (kind === 'full') {
          check(await page.evaluate(() => {
            const button = document.activeElement.getBoundingClientRect(), header = document.querySelector('.topbar').getBoundingClientRect();
            return button.top >= header.top && button.bottom <= header.bottom && button.left >= 0 && button.right <= innerWidth && document.documentElement.scrollWidth <= innerWidth;
          }), `${lang}: keyboard focus reveals Retry save inside the scrollable 640×400 header`);
        }
        await page.keyboard.press('Enter');
        await page.locator('.save-state.saved').waitFor();
        check(await page.evaluate(line => window.qaExport.seed.lines.some(item => item.text === line), line) && await banner.count() === 0, `${lang}/${kind}: keyboard retry saves the retained transcript and clears the error`);
        if (exportHint) {
          await page.evaluate(error => {window.qaExport.saveFail = error;}, error);
          await exportFile(kind === 'permission' ? 'PNG' : 'Markdown', lang); await toast.getByText(error, {exact: false}).waitFor();
          check((await toast.innerText()).includes(exportHint) && (await toast.innerText()).includes(error) && await toast.getByRole('button', {name: en ? 'Open file' : '打开文件', exact: true}).count() === 0, `${lang}/${kind}: export offers cause-specific recovery without claiming a saved file`);
          if (kind === 'permission') check(!(await toast.innerText()).includes(en ? 'SVG vector' : 'SVG 矢量图'), `${lang}: a PNG save error keeps storage recovery instead of suggesting another format`);
          await page.screenshot({path: `${evidence}/export-guidance-${lang}-${kind}.png`, animations: 'disabled'});
          await page.evaluate(() => {window.qaExport.saveFail = null;});
          await toast.getByRole('button', {name: en ? 'Export again' : '重新导出', exact: true}).click();
          await toast.getByRole('button', {name: en ? 'Open file' : '打开文件', exact: true}).waitFor();
          check((await toast.innerText()).includes('/fixture/meeting notes.md'), `${lang}/${kind}: export retries through the existing save flow`);
        }
      }
    }
    await page.goto(`${baseUrl}/?badTime=1`); await page.locator('.splash').waitFor({state: 'hidden'});
    check(await page.locator('.save-state.saved').isVisible() && await page.locator('.save-state time').count() === 0, 'an invalid historical timestamp cannot crash the workbench');
    check(report.pageErrors.length === 0, 'export handoff has no browser script errors'); report.passed = true;
  } catch (error) {report.error = String(error); throw error;}
  finally {await fs.writeFile(`${evidence}/export-handoff-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
