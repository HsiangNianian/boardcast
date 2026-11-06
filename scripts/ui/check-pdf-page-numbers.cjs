const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {execFileSync} = require('node:child_process');

module.exports = async function checkPdfPageNumbers(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], failures: [], cases: [], errors: []};
  const check = (ok, label) => (ok ? report.checks : report.failures).push(label);
  page.on('pageerror', error => report.errors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
  report.scripts = await page.locator('script[src]').evaluateAll(nodes => nodes.map(node => node.src));
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const row = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return row;
  });
  try {
    for (const sample of [
      {name: 'gaps-en', lang: 'en', kinds: ['', 'image', '', 'scene', ''], expected: ['Board 2', 'Board 4']},
      {name: 'only-third-en', lang: 'en', kinds: ['', '', 'scene', ''], expected: ['Board 3']},
      {name: 'single-en', lang: 'en', kinds: ['image'], expected: ['Board']},
      {name: 'gaps-zh', lang: 'zh', kinds: ['', 'image', '', 'scene', ''], expected: ['第 2 张板', '第 4 张板']},
      {name: 'transcript-only', lang: 'en', kinds: ['', ''], expected: []},
    ]) {
      const row = await page.evaluate(async sample => {
        const empty = () => ({nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1});
        const png = document.createElement('canvas'); png.width = 640; png.height = 360;
        const ctx = png.getContext('2d'); ctx.fillStyle = '#29b6ce'; ctx.fillRect(0, 0, 640, 360);
        ctx.fillStyle = '#111'; ctx.font = '32px sans-serif'; ctx.fillText('IMPORTED MATERIAL', 40, 70);
        const pages = sample.kinds.map(kind => {
          const p = {graph: empty(), steps: []};
          if (kind === 'image') {p.graph.underlay = {w: 640, h: 360, no: 7}; p.underlay = png.toDataURL();}
          if (kind === 'scene') p.scene = {elements: [{id: 'free-rectangle', type: 'rectangle', x: 30, y: 40, width: 320, height: 180, angle: 0, strokeColor: '#6743a5', backgroundColor: '#6743a5', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: false}], generated: [], files: {}};
          return p;
        });
        const lines = sample.kinds.flatMap((kind, i) => kind ? [{text: `NOTES_FOR_BOARD_${i + 1}`, speaker: 'Alice', ms: 1, page: i}] : []);
        if (!lines.length) lines.push({text: 'TRANSCRIPT_WITHOUT_DRAWING', speaker: 'Alice', ms: 1, page: 1});
        const row = {version: 2, storageRevision: 1, id: 'qa-pdf-' + sample.name, title: 'PDF numbering ' + sample.name, titled: true, createdAt: 1, updatedAt: 2, graph: pages.at(-1).graph, history: [], lines, pages, page: pages.length - 1, steps: []};
        const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
        await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error);}); db.close();
        for (const [key, value] of Object.entries({lang: sample.lang, theme: 'light', log: 'closed', 'mode-hint': 'dismissed', meeting: row.id})) localStorage.setItem('boardcast.' + key, value);
        return row;
      }, sample);
      await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
      await page.locator('.meeting-title').filter({hasText: row.title}).waitFor();
      await fs.writeFile(path.join(evidence, sample.name + '-before.json'), JSON.stringify(row, null, 2));
      await page.screenshot({path: path.join(evidence, sample.name + '-app.png')});
      await page.evaluate(() => {
        window.__pdfHeaders = [];
        const fill = CanvasRenderingContext2D.prototype.fillText;
        CanvasRenderingContext2D.prototype.fillText = function (...args) {
          if (this.canvas.width === 2400 && args[1] > 1200 && args[2] === 50) window.__pdfHeaders.push(args[0]);
          return fill.apply(this, args);
        };
      });
      await page.getByRole('button', {name: sample.lang === 'en' ? 'Export' : '导出', exact: true}).click();
      await page.getByRole('menuitemradio', {name: sample.lang === 'en' ? 'Whole meeting' : '整场会议', exact: true}).click();
      const pdfAction = page.getByRole('menuitem', {name: sample.lang === 'en' ? /^Whole meeting PDF/ : /^整场会 PDF/});
      check(await pdfAction.isEnabled(), sample.name + ': PDF remains available with an empty current board');
      const downloading = page.waitForEvent('download', {timeout: 60000}); await pdfAction.click();
      const file = path.join(evidence, sample.name + '.pdf'); await (await downloading).saveAs(file);
      const headers = await page.evaluate(() => window.__pdfHeaders);
      const info = execFileSync('pdfinfo', [file], {encoding: 'utf8'});
      const count = Number(/Pages:\s+(\d+)/.exec(info)?.[1]);
      const entry = {...sample, headers, count, ocr: [], file}; report.cases.push(entry);
      check(JSON.stringify(headers) === JSON.stringify(sample.expected), sample.name + ': drawn headers retain original board numbers');
      check(count === sample.expected.length + 1, sample.name + ': PDF omits blank boards and retains the transcript');
      execFileSync('pdftoppm', ['-png', '-scale-to', '1600', file, path.join(evidence, sample.name + '-page')]);
      if (sample.expected.length) {
        execFileSync('pdftoppm', ['-f', '1', '-l', String(sample.expected.length), '-png', '-scale-to', '2400', '-x', '0', '-y', '0', '-W', '2400', '-H', '105', file, path.join(evidence, sample.name + '-header')]);
        if (sample.lang === 'en') for (let i = 0; i < sample.expected.length; i++) {
          const ocr = execFileSync('tesseract', [path.join(evidence, `${sample.name}-header-${i + 1}.png`), 'stdout', '-l', 'eng', '--psm', '6'], {encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
          entry.ocr.push(ocr);
          check(new RegExp('\\b' + sample.expected[i] + '\\b').test(ocr), `${sample.name}: actual PDF page ${i + 1} visibly labels ${sample.expected[i]}`);
        }
      }
      const after = await read(); await fs.writeFile(path.join(evidence, sample.name + '-after.json'), JSON.stringify(after, null, 2));
      check(JSON.stringify(after) === JSON.stringify(row), sample.name + ': export leaves stored meeting unchanged');
    }
    check(!report.errors.length, 'all PDF exports finish without uncaught browser errors');
    report.passed = !report.failures.length;
    assert.equal(report.failures.length, 0, report.failures.join('\n'));
  } catch (error) {
    report.error = String(error); await page.screenshot({path: path.join(evidence, 'failure.png')}); throw error;
  } finally {await fs.writeFile(path.join(evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');}
};
