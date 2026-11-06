const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {execFileSync} = require('node:child_process');

module.exports = async (page, baseUrl, evidence) => {
  const report = {checks: [], pageErrors: [], graphs: {}, exports: {}};
  const check = (ok, name) => {assert.ok(ok, name); report.checks.push(name); console.log('PASS:', name);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.mode', 'ink');
    localStorage.setItem('boardcast.log', 'open'); localStorage.setItem('boardcast.mode-hint', 'dismissed');
  });
  await page.route('https://cdn.jsdelivr.net/**', route => route.fulfill({status: 404, body: 'Offline glyph check'}));
  await page.emulateMedia({reducedMotion: 'reduce'});
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    try {return await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});}
    finally {db.close();}
  });
  const download = async (lang, format, scope = 'Current board') => {
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: scope, exact: true}).click();
    const pending = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: new RegExp('^' + format)}).click();
    const file = await pending, target = path.join(evidence, lang + '-' + file.suggestedFilename());
    await file.saveAs(target); report.exports[lang + ':' + format] = path.basename(target); return target;
  };
  const labels = {
    en: ['Decisions and next steps for the client and service integration', 'Keep the original proposal', 'Prepare the release', 'Who will review this?', 'Confirm the owner', 'Client', 'Service', 'Deployment group', 'Source paragraph', 'Exact source text remains available.', 'requests'],
    zh: ['客户端与服务端联调会议的决定和后续行动', '保留原始方案', '准备发布', '由谁复核？', '确认负责人', '客户端', '服务端', '部署分组', '原文段落', '原始资料的完整文字仍然可以查看。', '请求'],
    ja: ['クライアントとサービスの連携に関する決定と次の作業', '元の提案を保持する', 'リリースを準備する', '誰が確認しますか？', '担当者を確認する', 'クライアント', 'サービス', '展開グループ', '参考資料', '元の資料の文章をそのまま確認できます。', 'リクエスト'],
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    for (const [lang, words] of Object.entries(labels)) {
      const n = (id, kind, label, extra = {}) => ({id, kind, label, aliases: [], marks: [], ink: 'ink', by: 'A', ...extra});
      const nodes = [
        n('n1', 'topic', words[0], {icon: '云'}),
        n('n2', 'item', words[1], {parent: 'n1', marks: ['星标']}),
        n('n3', 'item', words[2], {parent: 'n2', marks: ['下划线']}),
        n('n4', 'formula', 'α', {parent: 'n1'}),
        n('n5', 'question', words[3], {parent: 'n1'}),
        n('n6', 'note', words[4], {parent: 'n2'}),
        n('n7', 'node', words[5], {group: 'g1', icon: '手机', marks: ['圈出']}),
        n('n8', 'node', words[6], {group: 'g1', icon: '服务器', marks: ['划掉']}),
        n('n9', 'region', words[8], {text: words[9], at: {x: 20, y: 20, w: 550, h: 90}, marks: ['高亮']}),
        n('n10', 'formula', 'x² + y₂ ≤ r²'),
        n('n11', 'formula', '∫ f(x) dx ≈ Σ xᵢ / n'),
        n('n12', 'note', words[4] + ' — 2', {parent: 'n7'}),
      ];
      if (lang === 'ja') for (let i = 0; i < 18; i++) nodes.push(n('n' + (13 + i), 'item', '追加の作業 ' + (i + 1), {parent: 'n1'}));
      const graph = {nodes, edges: [{from: 'n7', to: 'n8', label: words[10], ink: 'ink', by: 'A'}], groups: [{id: 'g1', label: words[7]}], nextNode: nodes.length + 1, nextGroup: 2, underlay: {w: 600, h: 150, no: 1}};
      report.graphs[lang] = graph;
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="600" height="150"><rect width="600" height="150" fill="white"/><text x="24" y="55" font-size="20">${words[9]}</text></svg>`;
      const fixture = {type: 'excalidraw', elements: [], files: {}, boardcast: {version: 2, graph, generated: [], underlay: 'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64')}};
      await page.locator('input[type=file]').setInputFiles({name: lang + '-graph-kinds.excalidraw', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(fixture))});
      const dialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
      await dialog.getByRole('button', {name: 'New meeting', exact: true}).click(); await dialog.waitFor({state: 'hidden'});
      await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      await page.locator('.ink [data-key="n11|∫ f(x) dx ≈ Σ xᵢ / n"]').waitFor();
      check(await page.locator('.ink [data-key="n4|α"] path').count() > 0, lang + ': the Greek-only formula has visible strokes');
      const keys = await page.locator('.ink [data-key]').evaluateAll(elements => elements.filter(e => e.querySelector('path')).map(e => e.dataset.key));
      check(nodes.filter(n => n.kind !== 'region').every(n => keys.some(k => k.startsWith(n.id + '|') && k.includes(n.label))), lang + ': all generated node kinds and long labels have visible ink');
      check(keys.some(k => k.startsWith('g1t|')) && keys.some(k => /^n7[>~]n8/.test(k)) && keys.some(k => k.startsWith('n9|h')), lang + ': group title, directed edge and fixed-source mark are drawn');
      await page.screenshot({path: path.join(evidence, lang + '-ink.png')});
      await page.locator('.save-state.saved').waitFor();
      assert.deepEqual((await read()).graph, graph);
      const inkSvg = await download(lang + '-ink', 'SVG vector');
      const exported = await page.context().newPage();
      try {
        await exported.goto(pathToFileURL(inkSvg).href);
        const fits = await exported.evaluate(() => {const v = document.documentElement.viewBox.baseVal; return [...document.querySelectorAll('path')].every(e => {const b = e.getBBox(); return b.x >= v.x && b.y >= v.y && b.x + b.width <= v.x + v.width && b.y + b.height <= v.y + v.height;});});
        check(fits, lang + ': hand-drawn SVG contains every stroke within its export bounds');
      } finally {await exported.close();}
      const md = await fs.readFile(await download(lang, 'Markdown', 'Whole meeting'), 'utf8');
      check(nodes.every(n => md.includes(n.kind === 'region' ? n.text : n.label)) && md.includes(words[7]), lang + ': Markdown retains every semantic kind, formula and source text');
      await page.getByRole('radio', {name: 'Free edit', exact: true}).click(); await page.locator('.pencil-stage[data-ready="true"]').waitFor();
      const drawing = JSON.parse(await fs.readFile(await download(lang, 'Excalidraw'), 'utf8'));
      assert.deepEqual(drawing.boardcast.graph, graph);
      check(nodes.filter(n => n.kind !== 'region').every(n => drawing.elements.some(e => e.type === 'text' && (e.originalText ?? e.text).includes(n.label))), lang + ': editable export retains each kind and full text');
      check(drawing.elements.some(e => e.id === 'g1') && drawing.elements.some(e => e.id === 'n7>n8' && e.startBinding?.elementId === 'n7' && e.endBinding?.elementId === 'n8'), lang + ': editable groups and both arrow bindings survive');
      await page.screenshot({path: path.join(evidence, lang + '-free-edit.png')});
      await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
      if (lang === 'ja') {
        await download(lang, 'PNG image'); await download(lang, 'Animated SVG');
        const pdf = await download(lang, 'Whole meeting PDF', 'Whole meeting');
        execFileSync('pdftoppm', ['-f', '1', '-singlefile', '-png', '-scale-to', '1600', pdf, path.join(evidence, 'ja-pdf')]);
        check((await fs.stat(path.join(evidence, 'ja-pdf.png'))).size > 1000, 'the actual PDF renders the multilingual board');
        const html = await download(lang, 'Replay page', 'Whole meeting');
        const offline = await page.context().browser().newContext({viewport: {width: 1280, height: 800}, reducedMotion: 'reduce'});
        const remote = report.offlineRequests = [];
        try {
          await offline.route(/^https?:/, route => {remote.push(route.request().url()); return route.abort();});
          const replay = await offline.newPage(); await replay.goto(pathToFileURL(html).href);
          await replay.locator('.ink [data-key="n4|α"] path').first().waitFor();
          await replay.locator('.ink [data-key="__loose|补充"] path').first().waitFor();
          assert.deepEqual(await replay.evaluate(() => window.__boardcastData.pages[0].graph), graph);
          check(!remote.length, 'offline replay renders Greek, Japanese and all graph data without HTTP');
          const notices = await replay.evaluate(() => [...document.head.childNodes].filter(n => n.nodeType === Node.COMMENT_NODE).map(n => n.textContent).join('\n'));
          check(notices.includes('SIL OPEN FONT LICENSE') && notices.includes('Microsoft Corporation') && notices.includes('THE FONT SOFTWARE IS PROVIDED'), 'offline replay includes both bundled handwriting licenses');
          await replay.screenshot({path: path.join(evidence, 'offline-replay.png')});
        } finally {await offline.close();}
      }
      const saved = await read(); await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      assert.deepEqual((await read()).graph, graph);
      check((await read()).id === saved.id, lang + ': switching views, exporting and cold reload preserve the meeting graph');
    }
    check(!report.pageErrors.length, 'no uncaught application errors');
  } catch (error) {report.error = String(error); await page.screenshot({path: path.join(evidence, 'failure.png')}); throw error;}
  finally {await fs.writeFile(path.join(evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');}
};
