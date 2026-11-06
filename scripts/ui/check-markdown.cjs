// Download from the production App, then render with the already installed Markdown/Mermaid packages.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {createRequire} = require('node:module');
const {pathToFileURL} = require('node:url');

module.exports = async function checkMarkdown(page, baseUrl, evidence) {
  const report = {checks: [], pageErrors: [], rendered: {}, files: {}};
  const check = (ok, name) => {report.checks.push({name, ok: !!ok}); console.log(`${ok ? 'PASS' : 'FAIL'}: ${name}`);};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({lang: 'en', theme: 'light', mode: 'ink', 'mode-hint': 'dismissed'})) {
      if (!localStorage.getItem('boardcast.' + key)) localStorage.setItem('boardcast.' + key, value);
    }
  });
  const from = name => createRequire(require.resolve(name));
  const parse = from('react-markdown').resolve('remark-parse');
  const mdast = createRequire(parse).resolve('mdast-util-from-markdown');
  const {micromark} = await import(pathToFileURL(createRequire(mdast).resolve('micromark')).href);
  const {gfm, gfmHtml} = await import(pathToFileURL(from('remark-gfm').resolve('micromark-extension-gfm')).href);
  const convert = from('@excalidraw/excalidraw').resolve('@excalidraw/mermaid-to-excalidraw');
  const mermaidDist = path.dirname(createRequire(convert).resolve('mermaid'));
  const viewer = await page.context().newPage();
  await viewer.route('http://markdown-review.invalid/**', async route => {
    const pathname = new URL(route.request().url()).pathname;
    if (pathname === '/') return route.fulfill({contentType: 'text/html', body: '<!doctype html><meta charset="utf-8"><main></main>'});
    if (!pathname.startsWith('/vendor/')) return route.abort();
    const file = path.resolve(mermaidDist, pathname.slice('/vendor/'.length));
    assert(file.startsWith(mermaidDist + path.sep));
    await route.fulfill({contentType: 'application/javascript', body: await fs.readFile(file)});
  });
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const row = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get('qa-markdown'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return row;
  });
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    const fixture = await page.evaluate(async () => {
      const empty = () => ({nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1});
      const node = (id, kind, label, extra = {}) => ({id, kind, label, aliases: [], marks: [], ink: 'ink', ...extra});
      const title = '# 周会 *中文* 日本語 &copy; #';
      const topic = '*topic* _literal_ [文档](file.md)';
      const formula = '`x` + ``y`` + α²';
      const source = 'Source <tag> &copy;\n中文\n日本語';
      const diagram = '中文 "入口" #quot; <tag> &copy;\n```\n日本語';
      const edge = '| edge "quoted" & #quot;';
      const group = 'Group <b> & #quot;';
      const said = '~~原话~~ `code` ![image](file.png)\n## literal heading\n\n- literal list';
      const summary = '### 结论\n\n**保留格式**\n\n```text\nfirst\n\n\nlast\n```';
      const a = {...empty(), nextNode: 8, nodes: [
        node('n1', 'topic', topic), node('n2', 'item', '- [x] literal', {parent: 'n1', marks: ['圈出']}),
        node('n3', 'note', '<img src=x> &copy;', {parent: 'n2'}), node('n4', 'formula', formula, {parent: 'n1'}),
        node('n5', 'question', '*loose* question'), node('n6', 'region', 'Source', {text: source, marks: ['高亮'], spans: [{mark: '圈出', text: '*span*', boxes: []}], at: {x: 30, y: 30, w: 400, h: 80}}),
        node('n7', 'topic', 'DRAFT_MUST_BE_OMITTED', {ink: 'draft'}),
      ], edges: [{from: 'n1', to: 'n6', label: '*cross*', ink: 'ink'}]};
      const b = {...empty(), nextNode: 4, nextGroup: 2, nodes: [node('n1', 'node', diagram, {group: 'g1', marks: ['星标']}), node('n2', 'node', 'Exit'), node('n3', 'note', '*attached*', {parent: 'n1'})], edges: [{from: 'n1', to: 'n2', label: edge, ink: 'ink'}], groups: [{id: 'g1', label: group}]};
      const rectangle = {id: 'free', type: 'rectangle', x: 10, y: 10, width: 80, height: 60, angle: 0, strokeColor: '#111111', backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: false};
      const boards = [a, b, empty()];
      const pages = boards.map((graph, i) => ({graph, steps: graph.nodes.length ? [{t: 1, line: i, d: {nodes: {set: graph.nodes}, edges: {set: graph.edges}, groups: {set: graph.groups}, nextNode: graph.nextNode, nextGroup: graph.nextGroup}}] : [], ...(i === 0 ? {scene: {elements: [rectangle], generated: [], files: {}}} : {})}));
      const row = {version: 2, storageRevision: 1, id: 'qa-markdown', title, titled: true, createdAt: 1, updatedAt: 2, graph: boards[2], history: [], lines: [{text: said, speaker: 'A_*B', ms: 1}, {text: '第二板 日本語 English', page: 1, ms: 1}, {text: 'Returned first board', page: 0, ms: 1}, {text: 'Manual edit history', manual: true, page: 1, ms: 0}], summary, pages, page: 2, steps: []};
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
      return {title, topic, formula, source, diagram, edge, group, said, summary};
    });
    const before = await read(); await fs.writeFile(path.join(evidence, 'meeting-before.json'), JSON.stringify(before, null, 2));
    for (const lang of ['en', 'zh']) {
      await page.evaluate(value => localStorage.setItem('boardcast.lang', value), lang);
      await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      report.productionScripts = await page.locator('script[src]').evaluateAll(els => els.map(e => e.src));
      await page.getByRole('button', {name: lang === 'en' ? 'Export' : '导出', exact: true}).click();
      await page.getByRole('menuitemradio', {name: lang === 'en' ? 'Whole meeting' : '整场会议', exact: true}).click();
      const pending = page.waitForEvent('download');
      await page.getByRole('menuitem', {name: /^Markdown/}).click();
      const item = await pending, filename = `${lang}.md`;
      await item.saveAs(path.join(evidence, filename));
      const markdown = await fs.readFile(path.join(evidence, filename), 'utf8'); report.files[lang] = filename;
      check(markdown.includes(fixture.summary), `${lang}: authored summary formatting and blank lines are unchanged`);
      check(!markdown.includes('DRAFT') && markdown.includes('Manual edit history'), `${lang}: draft nodes are omitted and existing editing history is retained`);
      check(markdown.includes(lang === 'en' ? 'omits free drawings' : '不包含自由图形'), `${lang}: visual omissions are explicitly described`);
      const html = micromark(markdown, {allowDangerousHtml: true, extensions: [gfm()], htmlExtensions: [gfmHtml()]});
      await fs.writeFile(path.join(evidence, `${lang}.html`), `<!doctype html><meta charset="utf-8">${html}`);
      await viewer.goto('http://markdown-review.invalid/');
      await viewer.locator('main').evaluate((element, content) => {element.innerHTML = content;}, html);
      await viewer.addStyleTag({content: 'body{font:16px/1.6 sans-serif;margin:30px;max-width:1100px}pre{white-space:pre-wrap;background:#f4f4f4;padding:12px}svg{max-width:100%}'});
      const observed = await viewer.locator('main').evaluate(main => {
        const text = el => {const copy = el.cloneNode(true); copy.querySelectorAll('br').forEach(br => br.replaceWith('\n')); return copy.textContent;};
        return {headings: [...main.querySelectorAll('h1,h2,h3')].map(text), items: [...main.querySelectorAll('li')].map(text), code: [...main.querySelectorAll('code')].map(text), images: main.querySelectorAll('img').length, inputs: main.querySelectorAll('input').length, links: main.querySelectorAll('a').length, marks: [...main.querySelectorAll('mark')].map(text), text: text(main)};
      });
      report.rendered[lang] = observed;
      check(observed.headings[0] === fixture.title && observed.headings.includes(fixture.topic), `${lang}: title and topic render as literal text`);
      check(observed.items.includes('A_*B' + (lang === 'en' ? ': ' : '：') + fixture.said), `${lang}: multiline transcript remains one item with its literal speaker`);
      check(observed.images === 0 && observed.inputs === 0 && observed.links === 0 && observed.text.includes('<img src=x> &copy;'), `${lang}: plain text does not create images, checkboxes, links or HTML`);
      check(observed.code.includes(fixture.formula), `${lang}: formula backticks and Unicode survive rendering`);
      check(observed.marks.includes(fixture.source) && observed.text.includes('*span*') && observed.text.includes('*cross*'), `${lang}: source annotations, spans and link labels remain literal`);
      const boardName = n => lang === 'en' ? `Board ${n}` : `第 ${n} 张板`;
      check(JSON.stringify(observed.headings.filter(h => [1, 2, 3].map(boardName).includes(h))) === JSON.stringify([1, 2, 3, 1, 2, 1, 2].map(boardName)), `${lang}: board and transcript sections preserve blank boards and original order`);
      const diagrams = await viewer.locator('code.language-mermaid').allTextContents();
      check(diagrams.length === 1 && diagrams[0].includes('n1 -->'), `${lang}: Mermaid remains a single complete fenced block`);
      const rendered = await viewer.evaluate(async () => {
        const {default: mermaid} = await import('/vendor/mermaid.esm.min.mjs');
        mermaid.initialize({startOnLoad: false, securityLevel: 'strict'});
        const code = document.querySelector('code.language-mermaid');
        try {
          const {svg} = await mermaid.render('review-diagram', code.textContent);
          const result = document.createElement('div'); result.innerHTML = svg; code.parentElement.replaceWith(result);
          const labels = [...result.querySelectorAll('.nodeLabel, .edgeLabel, .cluster-label')].map(el => {
            const copy = el.cloneNode(true); copy.querySelectorAll('br').forEach(br => br.replaceWith('\n')); return copy.textContent.trim();
          });
          return {labels};
        } catch (error) {return {error: String(error)};}
      });
      report.rendered[lang].mermaid = rendered;
      check(!rendered.error && [fixture.diagram, fixture.edge, fixture.group].every(label => rendered.labels.includes(label)), `${lang}: the installed Mermaid renderer preserves node, edge and group text`);
      await viewer.screenshot({path: path.join(evidence, `${lang}-rendered.png`), fullPage: true});
    }
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    const after = await read(); await fs.writeFile(path.join(evidence, 'meeting-after.json'), JSON.stringify(after, null, 2));
    check(JSON.stringify(before) === JSON.stringify(after), 'export and reload leave the saved meeting unchanged');
    check(report.pageErrors.length === 0, 'the production App raises no uncaught errors');
  } finally {
    await viewer.close();
    await fs.writeFile(path.join(evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  assert(report.checks.length === 24 && report.checks.every(item => item.ok), JSON.stringify(report.checks.filter(item => !item.ok)));
};
