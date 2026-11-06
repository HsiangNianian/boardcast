const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {execFileSync} = require('node:child_process');

module.exports = async function checkTopicLinks(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/topic-links') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {baseUrl, checks: [], pageErrors: [], files: {}};
  page.on('pageerror', error => report.pageErrors.push(error.message));
  const check = (condition, label) => { assert.ok(condition, label); report.checks.push(label); };
  const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
  const download = async (format, key) => {
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: format === 'Whole meeting PDF' ? 'Whole meeting' : 'Current board', exact: true}).click();
    const pending = page.waitForEvent('download');
    await page.getByRole('menuitem', {name: new RegExp(`^${format}`)}).click();
    const item = await pending, file = path.join(evidence, `${key}-${item.suggestedFilename()}`);
    await item.saveAs(file); report.files[key] = file; return file;
  };
  await page.addInitScript(() => { localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.theme', 'light'); localStorage.setItem('boardcast.mode', 'ink'); });
  await page.emulateMedia({reducedMotion: 'reduce'});
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(scripts => scripts.map(script => script.src));
    const fixture = await page.evaluate(async () => {
      const graph = {nodes: [{id: 'n1', label: 'Meeting goals and decisions', kind: 'topic', aliases: [], marks: [], ink: 'ink'}, {id: 'n2', label: 'Collect actionable follow-up items', kind: 'item', parent: 'n1', aliases: [], marks: [], ink: 'ink'}], edges: [{from: 'n1', to: 'n2', label: 'Next steps', ink: 'ink'}], groups: [], nextNode: 3, nextGroup: 1};
      const row = {version: 2, storageRevision: 1, id: 'qa-topic-link', title: 'Topic link routing', titled: true, createdAt: 1, updatedAt: 2, graph, history: [], lines: [{id: 'l1', text: 'Collect actionable follow-up items', speaker: 'A', ms: 1}], steps: [{t: 1, line: 1, d: {nodes: {set: graph.nodes}, edges: {set: graph.edges}, nextNode: 3}}]};
      const db = await new Promise((resolve, reject) => { const request = indexedDB.open('boardcast-v2', 1); request.onupgradeneeded = () => { request.result.createObjectStore('meetings', {keyPath: 'id'}); request.result.createObjectStore('assets'); }; request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
      await new Promise((resolve, reject) => { const transaction = db.transaction('meetings', 'readwrite'); transaction.objectStore('meetings').put(row); transaction.oncomplete = resolve; transaction.onerror = () => reject(transaction.error); });
      db.close(); localStorage.setItem('boardcast.meeting', row.id); return row;
    });
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.locator('.ink [data-key="n1~n2l|Next steps"] path').first().waitFor();
    report.inkBoxes = await page.locator('.ink [data-key]').evaluateAll(elements => Object.fromEntries(elements.filter(element => ['n1|Meeting goals and decisions', 'n2|Collect actionable follow-up items', 'n1~n2l|Next steps'].includes(element.dataset.key)).map(element => { const box = element.getBBox(); return [element.dataset.key, {x: box.x, y: box.y, width: box.width, height: box.height}]; })));
    const inkLabel = report.inkBoxes['n1~n2l|Next steps'];
    check(!overlaps(inkLabel, report.inkBoxes['n1|Meeting goals and decisions']) && !overlaps(inkLabel, report.inkBoxes['n2|Collect actionable follow-up items']), 'actual hand-drawn edge-label strokes do not overlap topic or item text');
    await page.screenshot({path: `${evidence}/hand-drawn.png`});
    const svgFile = await download('SVG vector', 'hand-drawn-svg');
    await download('PNG image', 'hand-drawn-png');
    const pdf = await download('Whole meeting PDF', 'meeting-pdf');
    execFileSync('pdftoppm', ['-f', '1', '-singlefile', '-png', '-scale-to', '1600', pdf, path.join(evidence, 'pdf-board')]);
    const svgPage = await page.context().newPage();
    try {
      await svgPage.goto(pathToFileURL(path.resolve(svgFile)).href);
      report.svgBounds = await svgPage.evaluate(() => {
        const svg = document.documentElement, view = svg.viewBox.baseVal;
        const boxes = [...svg.querySelectorAll('path')].map(element => element.getBBox());
        return {view: {x: view.x, y: view.y, width: view.width, height: view.height}, paths: {x: Math.min(...boxes.map(box => box.x)), y: Math.min(...boxes.map(box => box.y)), x1: Math.max(...boxes.map(box => box.x + box.width)), y1: Math.max(...boxes.map(box => box.y + box.height))}};
      });
      const {view, paths} = report.svgBounds;
      check(paths.x >= view.x && paths.y >= view.y && paths.x1 <= view.x + view.width && paths.y1 <= view.y + view.height, 'SVG export bounds contain all arrow and label strokes');
      await svgPage.screenshot({path: `${evidence}/exported-svg.png`});
    } finally { await svgPage.close(); }
    await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.waitForFunction(() => document.querySelector('.save-state.saved'));
    const drawing = JSON.parse(await fs.readFile(await download('Excalidraw', 'editable'), 'utf8'));
    const edge = drawing.elements.find(element => element.id === 'n1>n2');
    const text = drawing.elements.find(element => element.id === 'n1>n2:label');
    const title = drawing.elements.find(element => element.id === 'n1:t');
    const item = drawing.elements.find(element => element.id === 'n2:t');
    report.excalidraw = {edge, text, title, item};
    report.fontMeasurements = await page.evaluate(elements => {
      const context = document.createElement('canvas').getContext('2d');
      return elements.map(element => { context.font = `${element.fontSize}px Excalifont, Xiaolai`; return {id: element.id, fontFamily: element.fontFamily, width: element.width, measured: context.measureText(element.text).width}; });
    }, [title, item, text]);
    check(report.fontMeasurements.every(element => element.fontFamily === 5 && element.width + 1 >= element.measured), 'first free-edit scene measures generated text after its actual font has loaded');
    check(edge.points.length === 4 && edge.startBinding?.elementId === 'n1' && edge.endBinding?.elementId === 'n2', 'editable export preserves the routed edge ID and both semantic bindings');
    check(text?.text === 'Next steps' && text.containerId === 'n1>n2' && !overlaps(text, title) && !overlaps(text, item), 'editable bound label remains outside both visible text elements');
    check(drawing.boardcast.graph.edges.length === 1 && drawing.boardcast.graph.edges[0].label === 'Next steps' && drawing.boardcast.graph.nodes[1].parent === 'n1', 'routing preserves explicit edge and parent semantics');
    await page.screenshot({path: `${evidence}/free-edit.png`});
    await download('PNG image', 'free-edit-png');
    const fallbackContext = await page.context().browser().newContext({viewport: {width: 1280, height: 800}});
    try {
      await fallbackContext.addInitScript(() => {
        localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.theme', 'light');
        const load = FontFaceSet.prototype.load;
        FontFaceSet.prototype.load = function(font, text) { if (font.includes('Excalifont')) { window.__blockedFontLoads = (window.__blockedFontLoads || 0) + 1; return new Promise(() => {}); } return load.call(this, font, text); };
      });
      const fallbackPage = await fallbackContext.newPage();
      await fallbackPage.goto(baseUrl); await fallbackPage.locator('.splash').waitFor({state: 'hidden'});
      await fallbackPage.evaluate(async row => {
        const db = await new Promise((resolve, reject) => { const request = indexedDB.open('boardcast-v2', 1); request.onupgradeneeded = () => { request.result.createObjectStore('meetings', {keyPath: 'id'}); request.result.createObjectStore('assets'); }; request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
        await new Promise((resolve, reject) => { const transaction = db.transaction('meetings', 'readwrite'); transaction.objectStore('meetings').put(row); transaction.oncomplete = resolve; transaction.onerror = () => reject(transaction.error); });
        db.close(); localStorage.setItem('boardcast.meeting', row.id);
      }, fixture);
      await fallbackPage.reload(); await fallbackPage.locator('.splash').waitFor({state: 'hidden'});
      const started = Date.now();
      await fallbackPage.getByRole('radio', {name: 'Free edit', exact: true}).click();
      await fallbackPage.locator('.pencil-stage').waitFor();
      check(await fallbackPage.locator('.pencil-editor').evaluate(element => element.inert), 'the editor cannot accept a first gesture before its initial scene is ready');
      await fallbackPage.locator('.pencil-stage[data-ready="true"]').waitFor({timeout: 15000});
      await fallbackPage.waitForFunction(async () => {
        const db = await new Promise(resolve => { const request = indexedDB.open('boardcast-v2', 1); request.onsuccess = () => resolve(request.result); });
        const row = await new Promise(resolve => { const request = db.transaction('meetings').objectStore('meetings').get('qa-topic-link'); request.onsuccess = () => resolve(request.result); }); db.close();
        return row?.scene?.elements.some(element => element.id === 'n1:t' && element.fontFamily === 2);
      }, null, {timeout: 15000});
      report.fontTimeoutMs = Date.now() - started;
      report.blockedFontLoads = await fallbackPage.evaluate(() => window.__blockedFontLoads || 0);
      check(report.blockedFontLoads > 0 && report.fontTimeoutMs < 12000, 'a font request that never resolves falls back to local text instead of hanging');
      await fallbackPage.screenshot({path: `${evidence}/font-timeout-fallback.png`});
      await fallbackPage.getByRole('radio', {name: 'Rectangle', exact: true}).locator('..').click();
      await fallbackPage.mouse.move(400, 520); await fallbackPage.mouse.down(); await fallbackPage.mouse.move(490, 580, {steps: 8}); await fallbackPage.mouse.up();
      await fallbackPage.waitForFunction(async () => {
        const db = await new Promise(resolve => { const request = indexedDB.open('boardcast-v2', 1); request.onsuccess = () => resolve(request.result); });
        const row = await new Promise(resolve => { const request = db.transaction('meetings').objectStore('meetings').get('qa-topic-link'); request.onsuccess = () => resolve(request.result); }); db.close();
        return row?.scene?.elements.some(element => element.type === 'rectangle' && !element.isDeleted && !element.customData?.boardcast);
      });
      check(true, 'the first free shape after initialization is persisted instead of overwritten');
    } finally { await fallbackContext.close(); }
    check(report.pageErrors.length === 0, 'topic routing browser flow has no script errors');
    report.passed = true;
    await fs.rm(`${evidence}/failure.png`, {force: true});
    return report;
  } catch (error) {
    report.error = String(error); await page.screenshot({path: `${evidence}/failure.png`}); throw error;
  } finally { await fs.writeFile(`${evidence}/topic-link-check.json`, JSON.stringify(report, null, 2) + '\n'); }
};
