const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkUnderlayPages(page, baseUrl, evidence = '/tmp/boardcast-underlay-pages') {
  await fs.mkdir(evidence, { recursive: true });
  const report = { checks: [], errors: [] };
  page.on('pageerror', error => report.errors.push(error.message));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({ lang: 'en', mode: 'ink', log: 'closed', 'mode-hint': 'dismissed' })) localStorage.setItem('boardcast.' + key, value);
  });
  const fixture = await page.context().newPage();
  await fixture.setContent('<style>@page{size:600px 600px;margin:0}body{margin:0}.page{width:600px;height:600px;break-after:page;position:relative}.page:last-child{break-after:auto}h1{margin:0;position:absolute;left:60px;top:45px;font:24px sans-serif}.color{position:absolute;left:50px;top:140px;width:500px;height:380px}</style><div class="page"><h1>First green page</h1><div class="color" style="background:#16a34a"></div></div><div class="page"><h1>Second red page</h1><div class="color" style="background:#dc2626"></div></div>');
  const pdf = await fixture.pdf({ path: evidence + '/source.pdf', preferCSSPageSize: true, printBackground: true });
  await fixture.close();
  const colors = async bytes => page.evaluate(async url => {
    const image = new Image(); image.src = url; await image.decode();
    const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height;
    const context = canvas.getContext('2d'); context.drawImage(image, 0, 0);
    const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let green = 0, red = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 1] > 100 && data[i] < 60 && data[i + 2] < 120) green++;
      if (data[i] > 170 && data[i + 1] < 80 && data[i + 2] < 80) red++;
    }
    return { green, red };
  }, 'data:image/png;base64,' + bytes.toString('base64'));
  const check = (name, counts, wanted) => report.checks.push({ name, counts, ok: counts[wanted] > 1000 && counts[wanted === 'green' ? 'red' : 'green'] < 100 });
  const show = async (number, name) => {
    await page.getByRole('button', { name: 'Board ' + number, exact: true }).click();
    await page.locator(`.pages button[aria-current=page][aria-label="Board ${number}"]`).waitFor();
    await page.locator('.pencil-stage[data-ready=true]').waitFor();
    const wanted = number === 1 ? 'green' : 'red';
    let bytes, counts;
    for (let i = 0; i < 8; i++) {
      await page.waitForTimeout(250);
      bytes = await page.locator('.pencil-stage').screenshot(); counts = await colors(bytes);
      if (counts[wanted] > 1000 && counts[wanted === 'green' ? 'red' : 'green'] < 100) break;
    }
    await fs.writeFile(evidence + '/' + name + '.png', bytes); check(name, counts, wanted);
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.locator('input[type=file]').setInputFiles({ name: 'colors.pdf', mimeType: 'application/pdf', buffer: pdf });
    const dialog = page.getByRole('dialog', { name: 'Import materials', exact: true });
    await dialog.getByRole('button', { name: 'Add to this meeting', exact: true }).click({ timeout: 90000 });
    await dialog.waitFor({ state: 'hidden' }); await page.locator('.save-state.saved').waitFor();
    await page.getByRole('radio', { name: 'Free edit', exact: true }).click();
    for (const [number, name] of [[1, 'initial-green'], [2, 'second-red'], [1, 'return-green'], [2, 'return-red']]) await show(number, name);
    await page.getByRole('button', { name: 'Export', exact: true }).click();
    await page.getByRole('menuitemradio', { name: 'Current board', exact: true }).click();
    const pending = page.waitForEvent('download');
    await page.getByRole('menuitem', { name: /^PNG image/ }).click();
    await (await pending).saveAs(evidence + '/export-red.png');
    check('exported second page matches its source', await colors(await fs.readFile(evidence + '/export-red.png')), 'red');
    await page.reload(); await page.locator('.splash').waitFor({ state: 'hidden' });
    await page.getByRole('radio', { name: 'Free edit', exact: true }).click();
    await show(2, 'reloaded-red'); await show(1, 'reloaded-return-green');
    await page.waitForTimeout(750); await page.locator('.save-state.saved').waitFor();
    report.savedFiles = await page.evaluate(async () => {
      const db = await new Promise((yes, no) => { const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
      const meeting = await new Promise((yes, no) => { const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error); });
      db.close(); return meeting.pages.map(p => Object.keys(p.scene?.files ?? {}));
    });
    report.checks.push({ name: 'page navigation does not save another editor page background as a free image', ok: report.savedFiles.every(files => files.length === 0) });
    report.ok = report.checks.length === 8 && report.checks.every(c => c.ok) && !report.errors.length;
    assert(report.ok, JSON.stringify(report.checks.filter(c => !c.ok)));
  } catch (error) { report.error = String(error); throw error; }
  finally { await fs.writeFile(evidence + '/underlay-pages-check.json', JSON.stringify(report, null, 2) + '\n'); }
};
