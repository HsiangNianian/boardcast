// Production App with controlled native status replies; actual Linux startup is checked separately.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async function checkModelLanguage(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/model-language/browser') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], messages: [], routes: [], imageRoutes: [], pageErrors: []};
  const check = (ok, label) => {assert(ok, label); report.checks.push(label);};
  const log = '/fixture/user notes/日志 (one); log: keep.log';
  const cases = [
    ['no-model', 'No model installed; download one in Settings → Manage models', 'No built-in board model is installed. Open Manage models to download one.', '尚未安装内置板书模型，请打开「管理模型」下载。'],
    ['engine', 'llama-server was not found in the app installation', 'The built-in board service is missing from the app installation. Reinstall Boardcast, then try again.', '应用安装中缺少内置板书服务，请重新安装 Boardcast 后重试。'],
    ['start', `llama-server failed to start; log: ${log}`, `The built-in board model could not start. Retry it in Manage models. Log: ${log}`, `内置板书模型未能启动，请在「管理模型」中重试。日志：${log}`],
    ['exit', `llama-server exited unexpectedly (signal: 11 (core dumped)); log: ${log}`, `The built-in board service stopped unexpectedly (signal: 11 (core dumped)). Retry it in Manage models. Log: ${log}`, `内置板书服务意外退出（signal: 11 (core dumped)），请在「管理模型」中重试。日志：${log}`],
    ['unknown', 'GPU fault EIO /fixture/日志.log', 'GPU fault EIO /fixture/日志.log', 'GPU fault EIO /fixture/日志.log'],
    ['empty', '', 'Built-in model unavailable', '内置模型不可用'],
  ];
  await page.addInitScript(({cases}) => {
    if (!/^https?:$/.test(location.protocol)) return;
    const query = new URL(location.href).searchParams;
    const [kind, message] = cases.find(c => c[0] === query.get('modelCase'));
    localStorage.clear(); localStorage.setItem('boardcast.lang', query.get('modelLang')); localStorage.setItem('boardcast.meeting', 'model-language');
    const graph = {nodes: [{id: 'n1', label: 'Original board 中文', kind: 'node', marks: [], aliases: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
    const now = Date.now() - 1000;
    const seed = {version: 2, storageRevision: 1, id: 'model-language', title: 'Model language 中文', titled: true, createdAt: now, updatedAt: now, graph, history: [], lines: [{text: 'ORIGINAL 中文', ms: 1, page: 0}], page: 0, pages: [{graph}]};
    const status = {state: kind === 'no-model' ? kind : 'error', message, gpus: [], vision: kind !== 'no-model'};
    const q = window.qaModelLanguage = {seed, original: JSON.stringify(seed), status, calls: [], callbacks: {}, events: {}};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event, id) => q.events[event] = (q.events[event] || []).filter(value => value !== id)};
    let callback = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: fn => {q.callbacks[++callback] = fn; return callback;}, unregisterCallback: id => delete q.callbacks[id], convertFileSrc: value => value,
      invoke: async (command, args = {}) => {
        q.calls.push({command, args});
        if (command === 'plugin:event|listen') {(q.events[args.event] ||= []).push(args.handler); return args.handler;}
        if (command === 'llm_status') return structuredClone(q.status);
        if (command === 'llm_ensure') return structuredClone(q.ensureStatus || q.status);
        if (command === 'voice_status') return {installed: true, listening: false};
        if (command === 'voice_models') return {selection: {provider: 'sensevoice', locale: 'auto'}, senseVoiceInstalled: true, listening: false, preparing: false, autoLocales: [], activeLocale: null, apple: null, appleError: null};
        if (command === 'voice_devices') return [];
        if (command === 'remote_get') return {baseUrl: 'https://fixture.invalid', model: 'Remote fixture', enabled: query.has('remote')};
        if (command === 'remote_test') return query.get('remote') === 'checking' ? new Promise(resolve => q.releaseRemote = resolve) : {ms: 1, reply: 'OK'};
        if (command === 'meeting_load') return args.storageVersion === 2 && args.id === seed.id ? JSON.stringify(q.seed) : null;
        if (command === 'meeting_save') {q.seed = JSON.parse(args.json); return;}
        if (command === 'meetings_list') return [];
        if (command === 'models_list') return {models: [{id: 'board', label: 'Board model', kind: 'llm', note: 'Fixture model', size: 2e9, installed: kind !== 'no-model', selected: kind !== 'no-model', partial: 0, recommended: true}], freeBytes: 50e9};
        if (command === 'plugin:app|version') return '0.21.0-fixture';
        if (command === 'session_wayland') return false;
        return null;
      },
    };
  }, {cases});
  page.on('pageerror', error => report.pageErrors.push(error.message));
  const copy = async (locator, expected, lang, kind, surface) => {
    const actual = (await locator.innerText()).trim();
    report.messages.push({lang, kind, surface, actual, expected, ok: actual === expected});
  };
  const settings = page.locator('.panel.settings');
  const models = page.locator('.model-panel');
  const status = page.locator('.chip.status span');
  const close = async (panel, zh) => {await panel.getByRole('button', {name: zh ? '关闭' : 'Close', exact: true}).click(); await panel.waitFor({state: 'hidden'});};
  try {
    for (const lang of ['zh', 'en']) {
      const zh = lang === 'zh';
      const settingsButton = () => page.getByRole('button', {name: zh ? '设置' : 'Settings', exact: true});
      const modelTab = () => settings.getByRole('tab', {name: zh ? '板书与模型' : 'Board & models', exact: true});
      for (const [kind, raw, en, cn] of cases) {
        const expected = zh ? cn : en;
        await page.goto(`${baseUrl}/?modelLang=${lang}&modelCase=${kind}`);
        await page.locator('.meeting-title').filter({hasText: 'Model language 中文'}).waitFor();
        await page.locator('.splash').waitFor({state: 'hidden'});
        await copy(status, expected, lang, kind, 'workbench');
        {
          await page.locator('.chip.status').click();
          await page.getByRole('dialog').waitFor();
          const direct = await models.isVisible();
          report.routes.push({lang, kind, direct});
          if (!direct) {await modelTab().click(); await settings.getByRole('button', {name: zh ? '管理模型' : 'Manage models', exact: true}).click();}
          await models.getByRole('button', {name: kind === 'no-model' ? (zh ? '下载' : 'Download') : (zh ? '重新启动' : 'Retry starting'), exact: true}).waitFor();
          check(!await page.evaluate(() => window.qaModelLanguage.calls.some(c => /^model_(download|select|delete)$/.test(c.command))), `${lang}/${kind}: opening model recovery does not download, select or delete`);
          if (kind === 'no-model' || kind === 'engine') await page.screenshot({path: path.join(evidence, `${lang}-${kind}-manager.png`)});
          await close(models, zh);
          if (await settings.isVisible()) await close(settings, zh);
        }
        await settingsButton().click(); await modelTab().click();
        await copy(settings.locator('section').filter({has: page.getByRole('heading', {name: zh ? '本地模型' : 'Local model', exact: true})}).locator('p.hint').first(), expected, lang, kind, 'settings');
        if (kind !== 'no-model') {
          await settings.getByRole('button', {name: zh ? '管理模型' : 'Manage models', exact: true}).click();
          const selected = models.locator('.model.current').filter({has: page.getByText('Board model', {exact: true})});
          await selected.waitFor();
          await copy(selected.getByRole('alert'), `${zh ? '所选模型未能启动。' : 'The selected model could not start.'} ${expected}`, lang, kind, 'selected-model');
          check(await models.getByRole('button', {name: zh ? '重新启动' : 'Retry starting', exact: true}).isEnabled(), `${lang}/${kind}: restart remains available`);
          if (kind === 'exit') await page.screenshot({path: path.join(evidence, `${lang}-exit-details.png`)});
          await close(models, zh);
        }
        if (kind === 'exit' || kind === 'no-model') {
          const before = await page.evaluate(() => JSON.stringify(window.qaModelLanguage.status));
          await settings.getByRole('tab', {name: zh ? '通用与外观' : 'General & appearance', exact: true}).click();
          await settings.getByRole('radio', {name: zh ? 'English' : '中文', exact: true}).click();
          await copy(status, zh ? en : cn, zh ? 'en' : 'zh', kind, 'live-language-switch');
          check(await page.evaluate(() => JSON.stringify(window.qaModelLanguage.status)) === before, `${lang}/${kind}: switching language preserves native diagnostic data`);
          await settings.getByRole('radio', {name: zh ? '中文' : 'English', exact: true}).click();
          await modelTab().click();
        }
        await close(settings, zh);
        if (kind === 'start') {
          // The last polled model has vision; ensure can fail or discover a subsequently removed model.
          const ensured = await page.evaluate(() => window.qaModelLanguage.calls.filter(c => c.command === 'llm_ensure').length);
          const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=', 'base64');
          for (const removed of [false, true]) {
            await page.evaluate(removed => window.qaModelLanguage.ensureStatus = removed ? {state: 'no-model', gpus: []} : null, removed);
            await page.locator('input[type="file"]').setInputFiles({name: `local-model-${removed}.png`, mimeType: 'image/png', buffer: png});
            const panel = page.getByRole('dialog', {name: zh ? '导入材料' : 'Import materials', exact: true});
            await panel.getByRole('alert').waitFor();
            const reason = removed ? cases[0][zh ? 3 : 2] : expected;
            await copy(panel.getByRole('alert'), zh ? `导入失败：${reason}` : `Import failed: ${reason}`, lang, removed ? 'removed-before-ensure' : kind, 'image-import');
            const error = await panel.getByRole('alert').innerText();
            await panel.getByRole('button', {name: zh ? '配置本地看图模型' : 'Configure local image model', exact: true}).click();
            await page.getByRole('dialog').waitFor();
            const direct = await models.isVisible();
            report.imageRoutes.push({lang, removed, direct});
            await close(direct ? models : settings, zh);
            await panel.getByRole('alert').waitFor();
            await panel.getByRole('button', {name: zh ? '重新导入' : 'Try importing again', exact: true}).click();
            await panel.getByRole('alert').waitFor();
            check(await panel.getByRole('alert').innerText() === error && (await panel.innerText()).includes(`local-model-${removed}.png`)
              && !await page.evaluate(() => window.qaModelLanguage.calls.some(c => /^model_(download|select|delete)$/.test(c.command))),
              `${lang}/${removed}: configuring image recovery preserves the file and permits retry without model mutations`);
            await panel.getByRole('button', {name: zh ? '取消导入' : 'Cancel import', exact: true}).click();
          }
          check(await page.evaluate(() => window.qaModelLanguage.calls.filter(c => c.command === 'llm_ensure').length) === ensured + 4, `${lang}: image failures and retained-file retries pass through native model startup`);
        }
        check(await page.evaluate(() => JSON.stringify(window.qaModelLanguage.seed) === window.qaModelLanguage.original), `${lang}/${kind}: status, recovery and language changes preserve the original meeting`);
      }
      for (const kind of ['no-model', 'engine']) for (const remote of ['checking', 'ready']) {
        await page.goto(`${baseUrl}/?modelLang=${lang}&modelCase=${kind}&remote=${remote}`);
        await page.locator('.splash').waitFor({state: 'hidden'});
        await status.filter({hasText: remote === 'ready' ? 'Remote fixture' : zh ? '正在检查远程模型…' : 'Checking remote model…'}).waitFor();
        await page.locator('.chip.status').click();
        await settings.waitFor();
        check(!await models.isVisible(), `${lang}/${kind}/${remote}: a remote status still opens settings when the local model is unavailable`);
        await close(settings, zh);
      }
    }
    check(report.pageErrors.length === 0, 'no production JavaScript errors');
    check(report.messages.every(m => m.ok), 'every known model status follows the selected language on all four surfaces; unknown diagnostics remain verbatim');
    check(report.routes.every(r => r.direct), 'missing and failed local model statuses open model management directly in both languages');
    check(report.imageRoutes.every(r => r.direct), 'desktop image recovery opens model management directly in both languages');
  } finally {await fs.writeFile(path.join(evidence, 'check.json'), JSON.stringify(report, null, 2));}
  console.log(`model-language: ${report.checks.length} checks; ${report.messages.length} text comparisons`);
};
