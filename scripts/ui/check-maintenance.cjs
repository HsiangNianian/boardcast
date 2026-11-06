// Runs actual panels and IPC wrappers with a controlled native bridge; never downloads or installs.
const assert = require('node:assert/strict');
module.exports = async function checkMaintenance(page, baseUrl) {
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    window.qa = {calls: [], saved: [], exports: [], downloadError: {stage: 'download', message: 'Network unavailable'}, prepareError: null, installError: 'Installer unavailable', exportResult: null, exportError: null};
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (command, args) => {
        window.qa.calls.push(command);
        if (command === 'update_check') {
          if (window.qa.checkError) throw window.qa.checkError;
          return window.qa.checkInfo ?? null;
        }
        if (command === 'update_download') {
          if (window.qa.downloadHold) await new Promise(resolve => {window.qa.finishDownload = resolve;});
          if (window.qa.downloadError) throw window.qa.downloadError;
        }
        if (command === 'update_install' && window.qa.installError) throw {stage: 'install', message: window.qa.installError};
        if (command === 'export_save') {
          window.qa.exports.push(args);
          if (window.qa.exportError) throw new Error(window.qa.exportError);
          return window.qa.exportResult;
        }
        if (command === 'diagnostics_info') return {os: 'fixture', arch: 'fixture', log: 'llama_context: n_ctx = 4096\nprompt: PRIVATE-EN\nPRIVATE-CONTINUATION\nprompt: PRIVATE-ZH 私人内容\n{"content":"PRIVATE-JSON"}\nAuthorization: Bearer private-token\nslot print_timing: eval time = 12.00 ms / 4 tokens'};
        if (command === 'llm_status') return {state: 'error', message: 'PRIVATE-STATUS Permission denied (os error 13)', model: '/home/alice/model.gguf'};
        if (command === 'voice_models') return {appleError: 'TimeoutError PRIVATE-APPLE'};
        if (command === 'plugin:app|version') return '1.0.0';
        if (command === 'plugin:event|listen') return 1;
        return null;
      },
    };
  });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${baseUrl}/scripts/ui/fixtures/maintenance.html`);
  for (const lang of ['en', 'zh']) {
    const messages = await page.evaluate(async lang => {
      window.qa.setLang(lang);
      const messages = [];
      for (const error of ['UPDATE_CHECK_TIMEOUT', new Error('HTTP 502 fixture failure')]) {
        window.qa.checkError = error;
        try { await window.qa.checkUpdate(); } catch (error) { messages.push(error.message); }
      }
      window.qa.checkError = null;
      return {messages, latest: await window.qa.checkUpdate()};
    }, lang);
    assert.equal(messages.latest, null);
    assert.match(messages.messages[0], lang === 'zh' ? /检查更新超时.*网络或代理.*重试.*当前版本/ : /timed out.*connection or proxy.*try again.*current version/);
    assert.match(messages.messages[1], lang === 'zh' ? /无法检查更新.*重试.*HTTP 502 fixture failure/ : /Could not check for updates.*try again.*HTTP 502 fixture failure/);
  }
  await page.evaluate(() => window.qa.setLang('en'));
  await page.getByRole('button', {name: 'Open update', exact: true}).click();
  const update = page.getByRole('dialog', {name: 'New version available', exact: true});
  const calls = () => page.evaluate(() => window.qa.calls.filter(command => ['update_download', 'prepare', 'update_install', 'llm_ensure'].includes(command)));
  await update.getByRole('button', {name: 'Download update', exact: true}).click();
  await update.getByRole('alert').filter({hasText: 'could not be downloaded'}).waitFor();
  assert.deepEqual(await calls(), ['update_download']);
  await page.evaluate(() => { window.qa.downloadError = {stage: 'signature', message: 'Invalid signature'}; });
  await update.getByRole('button', {name: 'Retry download', exact: true}).click();
  await update.getByRole('alert').filter({hasText: 'signature could not be verified'}).waitFor();
  assert.deepEqual(await calls(), ['update_download', 'update_download']);
  await page.evaluate(() => { window.qa.downloadError = null; window.qa.prepareError = 'Disk full'; window.qa.downloadHold = true; });
  await update.getByRole('button', {name: 'Retry download', exact: true}).click();
  await page.waitForFunction(() => !!window.qa.finishDownload);
  assert(await update.getByRole('button', {name: 'Later', exact: true}).isEnabled());
  await update.getByRole('button', {name: 'Later', exact: true}).click();
  await update.waitFor({state: 'hidden'});
  await page.getByRole('button', {name: 'Open update', exact: true}).click();
  assert(await update.getByRole('button', {name: 'Updating…', exact: true}).isDisabled());
  assert.deepEqual(await calls(), ['update_download', 'update_download', 'update_download']);
  await page.keyboard.press('Escape');
  await update.waitFor({state: 'hidden'});
  await page.evaluate(() => {window.qa.downloadHold = false; window.qa.finishDownload();});
  await page.getByRole('button', {name: 'Open update', exact: true}).click();
  await update.getByRole('status').filter({hasText: 'downloaded and verified'}).waitFor();
  assert.deepEqual(await calls(), ['update_download', 'update_download', 'update_download'], 'background download must not stop/save/install automatically');
  await update.getByRole('button', {name: 'Save and restart', exact: true}).click();
  await update.getByRole('alert').filter({hasText: 'Disk full'}).waitFor();
  assert.deepEqual(await calls(), ['update_download', 'update_download', 'update_download', 'prepare']);
  await page.evaluate(() => { window.qa.prepareError = null; });
  await update.getByRole('button', {name: 'Retry save and install', exact: true}).click();
  await update.getByRole('alert').filter({hasText: 'Installation failed'}).waitFor();
  assert.deepEqual(await calls(), ['update_download', 'update_download', 'update_download', 'prepare', 'prepare', 'update_install', 'llm_ensure']);
  assert.equal(await update.getByText('Recording is stopped.', {exact: false}).count(), 1);
  await update.getByRole('button', {name: 'Retry save and install', exact: true}).click();
  await update.getByRole('alert').filter({hasText: 'Installation failed'}).waitFor();
  assert.equal((await calls()).filter(command => command === 'update_download').length, 3, 'verified download must be reused');
  assert.equal((await calls()).filter(command => command === 'prepare').length, 3, 'each install attempt must save the latest revision');
  await update.getByRole('button', {name: 'Later', exact: true}).click();

  await page.getByRole('button', {name: 'Open diagnostics', exact: true}).click();
  const diagnostics = page.getByRole('dialog', {name: 'Export diagnostics', exact: true});
  await diagnostics.locator('details').filter({hasText: 'llama-server.json'}).waitFor();
  const preview = await diagnostics.locator('pre').allTextContents();
  assert.equal(preview.length, 4);
  assert.ok(!preview.join('').includes('PRIVATE-') && !preview.join('').includes('private-token') && !preview.join('').includes('alice'));
  const info = JSON.parse(preview[0]);
  assert.deepEqual(info.voice.error.httpStatus, [401]);
  assert.deepEqual(info.model.message.osError, [13]);
  assert.equal(info.speechModels.appleError.type, 'TimeoutError');
  assert.equal(info.voice.device, 'C:\\Users\\[user]\\microphone');
  assert.deepEqual(JSON.parse(preview[2])[0].httpStatus, [503]);
  assert.deepEqual(JSON.parse(preview[3]).contextTokens, [4096]);
  for (const lang of ['en', 'zh']) {
    await page.evaluate(lang => window.qa.setLang(lang), lang);
    await page.setViewportSize({width: 640, height: 400});
    const compact = page.getByRole('dialog', {name: lang === 'en' ? 'Export diagnostics' : '导出诊断信息', exact: true});
    const button = compact.getByRole('button', {name: lang === 'en' ? 'Export zip' : '导出 zip', exact: true});
    await button.scrollIntoViewIfNeeded();
    const bounds = await button.boundingBox();
    assert.ok(bounds && bounds.y >= 0 && bounds.y + bounds.height <= 400, 'compact diagnostic export stays reachable');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  }
  await page.evaluate(() => window.qa.setLang('en'));
  await page.setViewportSize({width: 1280, height: 800});
  await diagnostics.getByRole('button', {name: 'Export zip', exact: true}).click();
  await diagnostics.getByRole('button', {name: 'Export zip', exact: true}).waitFor();
  assert.deepEqual(await page.evaluate(() => window.qa.saved), [], 'cancelled system save must not report success');
  assert.equal(await diagnostics.count(), 1, 'cancelled save keeps the preview open');
  await page.evaluate(() => { window.qa.exportError = 'Disk full'; });
  await diagnostics.getByRole('button', {name: 'Export zip', exact: true}).click();
  await diagnostics.getByRole('alert').filter({hasText: 'Disk full'}).waitFor();
  assert.equal(await page.evaluate(() => window.qa.saved.at(-1).ok), false);
  await page.evaluate(() => { window.qa.exportError = null; window.qa.exportResult = '/tmp/diagnostics.zip'; });
  await diagnostics.getByRole('button', {name: 'Export zip', exact: true}).click();
  await diagnostics.waitFor({state: 'detached'});
  assert.equal(await page.evaluate(() => window.qa.saved.at(-1).ok), true);
  const exports = await page.evaluate(() => window.qa.exports);
  assert.equal(exports.length, 3, 'cancel, failed save and retry each make one export request');
  assert.ok(exports.every(item => item.dataBase64 === exports[0].dataBase64), 'retries keep the reviewed snapshot');
  const zip = Buffer.from(exports[0].dataBase64, 'base64');
  assert.equal(zip.readUInt32LE(0), 0x04034b50);
  assert.ok(!zip.includes(Buffer.from('PRIVATE-')) && !zip.includes(Buffer.from('private-token')));
  for (const text of preview) assert.ok(zip.includes(Buffer.from(text)), 'ZIP contains the exact previewed content');
  assert.deepEqual(errors, []);
  console.log('PASS: download/signature/save/install failures, retained download retry, save before every install, diagnostic cancel/failure/retry, content-free summaries and exact preview-to-ZIP bytes.');
};
