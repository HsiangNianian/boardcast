// Production App with controlled model/storage replies; no real audio or user files.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkComposer(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const report = {checks: [], pageErrors: []};
  const check = (ok, label) => {assert.ok(ok, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    const graph = {nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1};
    const q = window.qaComposer = {meetings: {}, calls: [], callbacks: {}, events: {}, saveHold: false, saveFail: false, saves: [], modelMode: 'ready', requests: [], heldModels: []};
    q.meetings['composer-fixture'] = {version: 2, storageRevision: 1, id: 'composer-fixture', title: 'Composer fixture', titled: true, createdAt: 1, updatedAt: 2, graph, history: [], lines: []};
    localStorage.setItem('boardcast.meeting', 'composer-fixture');
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => {q.events[event] = (q.events[event] || []).filter(value => value !== id);}};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status' || command === 'llm_ensure') return {state: 'ready', url: 'http://127.0.0.1:19883', model: 'Fixture', device: 'Fixture', gpus: []};
        if (command === 'remote_prepare') return crypto.randomUUID();
        if (command === 'llm_chat') { q.requests.push(args.body); if (q.modelMode === 'hold') await new Promise(resolve => q.heldModels.push(resolve)); if (q.modelMode === 'fail') throw new Error('HTTP 503: Injected model failure'); return {choices: [{message: {content: '-'}}]}; }
        if (command === 'voice_status') return {installed: true, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null};
        if (command === 'voice_devices') return ['Fixture microphone'];
        if (command === 'remote_get') return {baseUrl: '', model: '', enabled: false};
        if (command === 'meeting_load') return args.storageVersion === 2 && q.meetings[args.id] ? JSON.stringify(q.meetings[args.id]) : null;
        if (command === 'meeting_save') {
          if (q.saveHold) await new Promise(resolve => q.saves.push(resolve));
          if (q.saveFail) throw new Error('Injected disk full');
          if ((q.meetings[args.id]?.storageRevision ?? 0) !== args.expectedRevision) throw new Error('Unexpected stale revision');
          q.meetings[args.id] = JSON.parse(args.json); return;
        }
        if (command === 'meetings_list') return [];
        if (command === 'plugin:app|version') return '0.21.0-fixture';
        if (command === 'models_list') return {models: [], freeBytes: 1000000000};
        if (command === 'session_wayland') return false;
        return null;
      },
    };
  });
  const area = () => page.getByRole('textbox', {name: 'What was said', exact: true});
  const send = () => page.getByRole('button', {name: 'Put on the board', exact: true});
  const fold = () => page.locator('.dock .fold');
  const speaker = () => page.locator('.dock .who');
  const rows = () => page.evaluate(() => window.qaComposer.meetings[localStorage.getItem('boardcast.meeting')]?.lines ?? []);
  const focused = locator => locator.evaluate(element => document.activeElement === element);
  const waitSaved = text => page.waitForFunction(text => window.qaComposer.meetings[localStorage.getItem('boardcast.meeting')]?.lines.some(line => line.text === text && line.ms !== undefined), text);
  const submit = async (input, text, who) => {
    const before = (await rows()).length;
    await area().fill(input); await area().press('Enter'); await waitSaved(text);
    const lines = await rows();
    check(lines.length === before + 1 && lines.at(-1).text === text && lines.at(-1).speaker === who, `speaker input: ${JSON.stringify(input)}`);
  };
  try {
    await fs.mkdir(evidence, {recursive: true});
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
    await area().fill('Unsent folding draft'); await fold().click();
    check(await page.locator('.dock.closed').isVisible() && await focused(fold()), 'collapse keeps keyboard focus on the toggle');
    check(await page.evaluate(() => localStorage.getItem('boardcast.composer')) === 'closed', 'collapsed preference is persisted');
    await page.keyboard.press('/'); await area().waitFor();
    await page.waitForFunction(() => document.activeElement?.tagName === 'TEXTAREA');
    check(await area().inputValue() === 'Unsent folding draft' && await focused(area()), 'slash expands and focuses the retained draft');
    await area().press('End'); await page.keyboard.press('/');
    check(await area().inputValue() === 'Unsent folding draft/', 'slash inside the input remains ordinary text');
    await fold().click(); await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    check(await page.locator('.dock.closed').isVisible(), 'reloading retains the collapsed preference');
    await fold().click(); await page.waitForFunction(() => document.activeElement?.tagName === 'TEXTAREA');
    check(await focused(area()), 'the expand button restores input focus');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    check(await area().isVisible() && await page.evaluate(() => localStorage.getItem('boardcast.composer')) === 'open', 'reloading retains the expanded preference');
    await area().fill('Keep this dialog draft');
    await page.locator('.dock .speaker-actions').getByRole('button', {name: 'Speakers', exact: true}).click();
    const names = page.getByRole('dialog', {name: 'Speakers', exact: true});
    await names.waitFor(); await names.getByRole('button', {name: 'Cancel', exact: true}).focus(); await page.keyboard.press('/');
    check(await page.evaluate(() => document.activeElement?.closest('dialog')?.getAttribute('aria-label')) === 'Speakers' && await page.locator('.dock textarea').inputValue() === 'Keep this dialog draft', 'slash cannot steal focus or alter the draft under a dialog');
    await page.keyboard.press('Escape'); await names.waitFor({state: 'hidden'});
    check(await focused(page.locator('.dock .speaker-actions').getByRole('button', {name: 'Speakers', exact: true})), 'closing speaker management returns to its trigger');

    await speaker().click();
    check((await speaker().getAttribute('aria-label')).includes('Speaker B,'), 'speaker badge cycles to the next existing speaker');
    await submit('@Alice Hello from Alice', 'Hello from Alice', 'Alice');
    await submit('＠小林 中文发言', '中文发言', '小林');
    await submit('@あいうえおかきくけこさし 日本語の発言', '日本語の発言', 'あいうえおかきくけこさし');
    await submit('@abcdefghijklm Keep the long prefix', '@abcdefghijklm Keep the long prefix', 'あいうえおかきくけこさし');
    await submit('@Jane Doe speaks', 'Doe speaks', 'Jane');
    await submit('A\r\nCopied first line\r\nCopied second line', 'Copied first line\nCopied second line', 'A');
    const beforeName = (await rows()).length;
    await area().fill('＠下一位'); await area().press('Enter');
    check(await area().inputValue() === '' && (await rows()).length === beforeName && (await speaker().getAttribute('aria-label')).includes('Speaker 下一位,'), 'a name-only full-width prefix changes speaker without adding an empty line');
    await submit('Next statement', 'Next statement', '下一位');
    await area().fill('   \n  ');
    check(await send().isDisabled(), 'whitespace-only input cannot be sent');
    await area().press('Enter');
    check(await area().inputValue() === '   \n  ', 'empty Enter does not consume a draft');

    const beforeMultiline = (await rows()).length;
    await area().fill('First line'); await area().press('Shift+Enter'); await page.keyboard.insertText('Second line');
    check(await area().inputValue() === 'First line\nSecond line' && (await rows()).length === beforeMultiline, 'Shift+Enter inserts a newline without sending');
    await area().press('Enter'); await waitSaved('First line\nSecond line');
    check((await rows()).at(-1).text === 'First line\nSecond line', 'Enter keeps multiline text in one transcript item');

    const cdp = await page.context().newCDPSession(page);
    for (const text of ['正在选择中文候选', '日本語の変換候補']) {
      const before = (await rows()).length;
      await area().fill('');
      await cdp.send('Input.imeSetComposition', {text, selectionStart: text.length, selectionEnd: text.length});
      await area().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', code: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true})));
      check(await area().inputValue() === text && (await rows()).length === before, `composition Enter preserves the ${text.startsWith('日') ? 'Japanese' : 'Chinese'} candidate without sending`);
      await cdp.send('Input.insertText', {text}); await area().press('Enter'); await waitSaved(text);
      check((await rows()).length === before + 1, 'committed composition sends once on the next Enter');
    }
    await cdp.detach();

    const longText = 'Long clipboard line with 中文と日本語.\n'.repeat(1500).trim();
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.evaluate(text => navigator.clipboard.writeText(text), longText);
    await area().fill(''); await area().press('Control+V');
    const size = await area().evaluate(element => ({height: element.getBoundingClientRect().height, scroll: element.scrollHeight}));
    check(await area().inputValue() === longText && size.height <= 120 && size.scroll > size.height, 'a long multilingual clipboard paste is intact in a bounded scrolling input');
    report.longPasteLayouts = [];
    for (const [width, height] of [[720, 480], [640, 400]]) {
      await page.setViewportSize({width, height}); await send().focus();
      const layout = await send().evaluate(element => ({button: element.getBoundingClientRect().toJSON(), height: innerHeight, overflow: document.documentElement.scrollWidth > innerWidth, input: document.querySelector('.dock textarea').getBoundingClientRect().height}));
      check(!layout.overflow && layout.button.y >= 0 && layout.button.bottom <= height && layout.input <= 120, `long input and keyboard send remain reachable at ${width} by ${height}`);
      report.longPasteLayouts.push({width, height, ...layout});
    }
    await page.screenshot({path: `${evidence}/composer-long-paste.png`, animations: 'disabled'});
    await page.setViewportSize({width: 1280, height: 800});
    await area().press('Enter'); await waitSaved(longText);
    check((await rows()).at(-1).text === longText, 'the full long paste is saved without truncation');
    report.longPasteCharacters = longText.length;

    await page.evaluate(() => {window.qaComposer.modelMode = 'hold';});
    await area().fill('Transcript before model completion'); await area().press('Enter');
    await page.waitForFunction(() => window.qaComposer.heldModels.length > 0 && window.qaComposer.meetings[localStorage.getItem('boardcast.meeting')].lines.some(line => line.text === 'Transcript before model completion'));
    check((await rows()).at(-1).ms === undefined && await area().inputValue() === '', 'accepted text is saved while board generation is still pending');
    await page.evaluate(() => {const q = window.qaComposer; q.modelMode = 'ready'; q.heldModels.splice(0).forEach(resolve => resolve());});
    await waitSaved('Transcript before model completion');
    await page.evaluate(() => {window.qaComposer.modelMode = 'fail';});
    await area().fill('Retained after model failure'); await area().press('Enter'); await waitSaved('Retained after model failure');
    const failed = (await rows()).at(-1);
    check(failed.failed && await area().inputValue() === '', 'a model error retains the accepted transcript for retry');
    await page.evaluate(() => {window.qaComposer.modelMode = 'ready';});
    await page.locator('.line').filter({hasText: 'Retained after model failure'}).getByRole('button', {name: 'Retry this line', exact: true}).click();
    await page.waitForFunction(id => window.qaComposer.meetings[localStorage.getItem('boardcast.meeting')].lines.some(line => line.id === id && line.ms !== undefined && !line.failed), failed.id);
    check((await rows()).filter(line => line.id === failed.id).length === 1, 'retry updates the original line without another composer submission');

    await page.evaluate(() => {window.qaComposer.saveHold = true;});
    await area().fill('Last accepted line before switching'); await area().press('Enter');
    await page.waitForFunction(() => window.qaComposer.saves.length > 0);
    await area().fill('Draft that must survive switching');
    await page.getByRole('button', {name: 'New meeting', exact: true}).click(); await page.locator('.transition-state').waitFor();
    await area().press('Enter');
    check(await area().inputValue() === 'Draft that must survive switching', 'Enter during the save barrier preserves a draft that the meeting cannot yet accept');
    check(await send().isDisabled() && await area().evaluate(element => element.readOnly), 'the save barrier exposes read-only input and a disabled send action');
    await page.screenshot({path: `${evidence}/composer-saving.png`, animations: 'disabled'});
    await page.evaluate(() => {const q = window.qaComposer; q.saveHold = false; q.saves.splice(0).forEach(resolve => resolve());});
    await page.waitForFunction(() => localStorage.getItem('boardcast.meeting') !== 'composer-fixture'); await page.locator('.transition-state').waitFor({state: 'hidden'});
    check(await area().inputValue() === 'Draft that must survive switching' && await send().isEnabled(), 'the retained draft becomes sendable when the new meeting is ready');
    await area().press('Enter'); await waitSaved('Draft that must survive switching');
    check(await page.evaluate(() => !window.qaComposer.meetings['composer-fixture'].lines.some(line => line.text === 'Draft that must survive switching')), 'the unsent draft is accepted only into the new meeting after the barrier');

    const failedMeeting = await page.evaluate(() => localStorage.getItem('boardcast.meeting'));
    await page.evaluate(() => {window.qaComposer.saveHold = true;});
    await area().fill('Accepted line before a failed save'); await area().press('Enter');
    await page.waitForFunction(() => window.qaComposer.saves.length > 0);
    await area().fill('Unsent draft after a failed save');
    await page.getByRole('button', {name: 'New meeting', exact: true}).click(); await page.locator('.transition-state').waitFor();
    await area().press('Enter');
    await page.evaluate(() => {const q = window.qaComposer; q.saveFail = true; q.saveHold = false; q.saves.splice(0).forEach(resolve => resolve());});
    await page.locator('.save-error').waitFor(); await page.locator('.transition-state').waitFor({state: 'hidden'});
    check(await area().inputValue() === 'Unsent draft after a failed save' && await send().isEnabled() && await area().isEditable() && await page.evaluate(() => localStorage.getItem('boardcast.meeting')) === failedMeeting, 'a failed save restores editing in the original meeting without consuming its draft');
    await page.evaluate(() => {window.qaComposer.saveFail = false;});
    await area().press('Enter'); await waitSaved('Unsent draft after a failed save');
    check((await rows()).some(line => line.text === 'Accepted line before a failed save') && (await rows()).some(line => line.text === 'Unsent draft after a failed save'), 'recovered storage saves both the already-accepted line and the preserved draft');
    check(report.pageErrors.length === 0, 'composer flows have no browser script errors');
    report.passed = true;
  } catch (error) {
    report.error = error.stack || String(error);
    report.failureState = await page.evaluate(() => ({draft: document.querySelector('.dock textarea')?.value.slice(0, 150), focused: document.activeElement?.outerHTML.slice(0, 300), id: localStorage.getItem('boardcast.meeting'), savedLines: window.qaComposer.meetings[localStorage.getItem('boardcast.meeting')]?.lines.length})).catch(() => null);
    throw error;
  }
  finally {await fs.writeFile(`${evidence}/composer-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
