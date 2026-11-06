const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkTranscriptState(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], layouts: [], pageErrors: [], requests: 0};
  const check = (value, label) => {assert(value, label); report.checks.push(label); console.log(`PASS: ${label}`);};
  const pending = [];
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', new URL(location.href).searchParams.get('lang') === 'zh' ? 'zh' : 'en');
    localStorage.setItem('boardcast.log', 'open');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19879');
  });
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.route('http://127.0.0.1:19879/**', async route => {
    if (route.request().method() !== 'POST') return route.fulfill({status: 200, json: {status: 'ok'}});
    report.requests++;
    const text = await new Promise(resolve => pending.push(resolve));
    await route.fulfill({status: 200, json: {choices: [{message: {content: text}}]}}).catch(() => {});
  });
  const line = text => page.locator('#sidebar-transcript-panel article.line').filter({has: page.locator('.body > p', {hasText: text})});
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const row = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get('qa-transcript-state'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return row;
  });
  const reply = async text => {
    for (let n = 0; !pending.length && n < 100; n++) await page.waitForTimeout(20);
    assert(pending.length, 'a real App model request must be waiting'); pending.shift()(text);
  };
  const send = async text => {
    await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text);
    await page.getByRole('button', {name: 'Put on the board', exact: true}).click();
    await line(text).waitFor();
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    await page.evaluate(async () => {
      const graph = {nodes: [{id: 'n1', kind: 'node', label: 'Existing decision', aliases: [], marks: [], ink: 'ink'}], edges: [], groups: [], nextNode: 2, nextGroup: 1};
      const lines = [
        {id: 'drawn', text: 'Completed statement with a long source.', speaker: '主持会议的Alexandra', ms: 1234, ops: 'N 「Existing decision」', via: 'Remote extraordinarily-long-model-name-at-a-retained-provider.example', heard: {waitMs: 200, asrMs: 80, voiceprintMs: 0, soft: false}, queueMs: 0},
        {id: 'unchanged', text: 'No change was required.', speaker: 'A', ms: 0, ops: '-', via: 'Local'},
        {id: 'grouped', text: 'Processed in the same batch.', speaker: 'A', ms: 12, ops: '', via: 'Local'},
        {id: 'historical', text: 'Historical result without operations.', speaker: 'B', ms: 18},
        {id: 'fallback', text: 'Used the local fallback.', speaker: 'B', ms: 50, ops: 'N 「Existing decision」', via: 'Local', fellBack: true},
        {id: 'failed', text: 'Retain this failed statement.', speaker: 'B', ms: 0, failed: true, boardFailure: 'request', via: 'Old source', fellBack: true},
        {id: 'cancelled', text: 'A canceled statement.', speaker: 'B', ms: 0, failed: true, boardFailure: 'cancelled'},
        {id: 'unavailable', text: 'Model was unavailable.', speaker: 'B', ms: 0, failed: true, boardFailure: 'unavailable'},
        {id: 'manual', text: 'A manual board edit.', manual: true, ms: 0},
      ];
      const row = {version: 2, storageRevision: 1, id: 'qa-transcript-state', title: 'Transcript states', titled: true, createdAt: 1, updatedAt: 2, graph, lines, history: [], pages: [{graph}, {graph}], page: 0};
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id);
    });
    const original = await read();
    await fs.writeFile(`${evidence}/original-meeting.json`, JSON.stringify(original, null, 2));
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    await line('Completed statement').waitFor();
    const summary = line('Completed statement').locator('summary');
    await summary.scrollIntoViewIfNeeded();
    check((await summary.innerText()).startsWith('Board updated') && !/\bms\b/.test(await summary.innerText()), 'completed lines show the board outcome instead of milliseconds');
    check((await summary.innerText()).includes('provider.example'), 'saved model provenance remains visible when remote configuration is disabled');
    check((await line('No change was required').locator('summary').innerText()).startsWith('No board changes'), 'no-change is distinct from a board update');
    check((await line('Processed in the same batch').locator('summary').innerText()) === 'Processed · Local' && (await line('Historical result').locator('summary').innerText()) === 'Processed', 'batched and historical results do not invent a board update');
    check((await line('Used the local fallback').locator('summary').innerText()).includes('local fallback'), 'fallback remains visible beside the stored local source');
    check(await line('Retain this failed').getByRole('button', {name: 'Retry this line', exact: true}).isVisible(), 'failed speech retains its recovery action');
    check(!(await line('Retain this failed').locator('summary').innerText()).includes('local fallback'), 'a failed attempt does not display stale fallback success');
    check(await line('A manual board edit').locator('details').count() === 0, 'manual edits do not acquire a model-processing status');
    await summary.focus(); await page.keyboard.press('Enter');
    await line('Completed statement').locator('details[open]').waitFor();
    check((await line('Completed statement').locator('.timing').innerText()) === 'pause 200 · recognition 80 · voice 0 · queue 0 · model 1234 ms', 'keyboard details show every known timing, including zero-valued measurements');
    check(await line('Completed statement').locator('pre').innerText() === 'N 「Existing decision」', 'the recorded model operations remain available');
    await page.screenshot({path: `${evidence}/expanded-details.png`});
    await page.keyboard.press('Enter');
    check(await summary.evaluate(element => document.activeElement === element) && await line('Completed statement').locator('details[open]').count() === 0, 'collapsing details keeps focus on their visible control');
    await line('Historical result').locator('summary').click();
    check(await line('Historical result').locator('pre').innerText() === 'This record has no board-operation details.', 'missing historical operation data is not reported as a no-change result');
    await line('A canceled statement').locator('summary').click();
    check(await line('A canceled statement').locator('pre').innerText() === 'Speech recorded; board generation was canceled.', 'canceled work is explained as canceled rather than a model error');
    await line('Model was unavailable').locator('summary').click();
    check(await line('Model was unavailable').locator('pre').innerText() === 'Speech recorded; the board model is not configured.', 'unavailable models retain their specific explanation in details');

    for (const lang of ['en', 'zh']) {
      await page.goto(`${baseUrl}/?lang=${lang}`); await page.locator('.splash').waitFor({state: 'hidden'});
      await line('Completed statement').waitFor();
      check((await line('Completed statement').locator('summary').innerText()).startsWith(lang === 'zh' ? '已更新白板' : 'Board updated'), `${lang}: outcome labels follow the interface language`);
      for (const [width, height, theme] of [[1280, 800, 'light'], [900, 600, 'dark'], [720, 480, 'light'], [360, 240, 'dark']]) {
        await page.setViewportSize({width, height});
        await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
        const target = line('Completed statement'); await target.locator('summary').scrollIntoViewIfNeeded();
        const geometry = await target.evaluate(element => {
          const speaker = element.querySelector('.name'), summary = element.querySelector('summary'), paragraph = element.querySelector('.body > p');
          let top = 0, bottom = innerHeight;
          for (let parent = summary.parentElement; parent; parent = parent.parentElement) if (/auto|scroll|hidden|clip/.test(getComputedStyle(parent).overflowY)) {
            const rect = parent.getBoundingClientRect(); top = Math.max(top, rect.top); bottom = Math.min(bottom, rect.bottom);
          }
          const rect = summary.getBoundingClientRect(), capture = document.querySelector('.dock .mic')?.getBoundingClientRect();
          return {speakerFits: speaker.scrollWidth <= speaker.clientWidth + 1, summaryFits: summary.scrollWidth <= summary.clientWidth + 1, noOverlap: rect.top >= paragraph.getBoundingClientRect().bottom - 1, pageFits: document.documentElement.scrollWidth <= innerWidth + 1, summaryFont: parseFloat(getComputedStyle(summary).fontSize), summaryVisible: Math.min(rect.bottom, bottom) - Math.max(rect.top, top) >= Math.min(24, rect.height) - 1, captureVisible: !capture || capture.top >= -1 && capture.bottom <= innerHeight + 1};
        });
        report.layouts.push({lang, width, height, theme, geometry});
        check(Object.entries(geometry).every(([key, value]) => key === 'summaryFont' ? value >= 12 : value), `${lang} ${width}x${height}: readable speaker, wrapped source and reachable details`);
        await page.screenshot({path: `${evidence}/${lang}-${width}.png`});
      }
    }
    await page.setViewportSize({width: 1280, height: 800});
    await page.goto(`${baseUrl}/?lang=en`); await page.locator('.splash').waitFor({state: 'hidden'});
    await line('Completed statement').waitFor();
    check(JSON.stringify(await read()) === JSON.stringify(original), 'reading details, language, theme and size changes do not rewrite the saved meeting');

    await send('Active request remains in flight.');
    await send('Queued statement one.');
    await send('Queued statement two.');
    check((await line('Active request').locator('summary').innerText()) === 'Drawing…', 'the active request is identified as drawing');
    check((await line('Queued statement one').locator('summary').innerText()) === 'Queued' && (await line('Queued statement two').locator('summary').innerText()) === 'Queued', 'later utterances are queued while the first request is still running');
    await line('Queued statement one').locator('summary').click();
    check(await line('Queued statement one').locator('pre').innerText() === 'Queued', 'pending details never claim that processing completed without changes');
    await reply('N 「First generated item」');
    await line('Active request').locator('summary').getByText(/^Board updated/).waitFor();
    await line('Queued statement one').locator('summary').getByText('Drawing…', {exact: true}).waitFor();
    check((await line('Queued statement two').locator('summary').innerText()) === 'Drawing…', 'the next batched request changes both queued rows to drawing');
    await reply('-');
    await line('Queued statement two').locator('summary').getByText(/^No board changes/).waitFor();
    check((await line('Queued statement one').locator('summary').innerText()).startsWith('Processed'), 'a grouped predecessor reports completion without inventing its own operations');
    await page.locator('.save-state.saved').waitFor();
    const completed = await read();
    check(completed.lines.length === original.lines.length + 3 && completed.lines.filter(value => value.ms === undefined).length === 0, 'processing states settle without duplicating transcript records');
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    await line('Retain this failed').getByRole('button', {name: 'Retry this line', exact: true}).click();
    await line('Retain this failed').locator('summary').getByText('Drawing…', {exact: true}).waitFor();
    check(true, 'retrying an earlier board uses that board’s active request for its status');
    await reply('N 「Recovered on earlier board」');
    await line('Retain this failed').locator('summary').getByText(/^Board updated/).waitFor();
    await page.locator('.save-state.saved').waitFor();
    const retried = await read();
    check(retried.lines.length === completed.lines.length && retried.lines.find(value => value.id === 'failed').fellBack === false, 'retry updates the original row once and clears the previous fallback flag');
    check(retried.page === 1 && retried.pages[0].graph.nodes.some(n => n.label === 'Recovered on earlier board') && !retried.graph.nodes.some(n => n.label === 'Recovered on earlier board'), 'a legacy line without a page belongs to the first board when retried from another board');
    check(report.pageErrors.length === 0, 'no uncaught browser errors');
    await page.screenshot({path: `${evidence}/completed-queue.png`});
  } finally {
    for (const resolve of pending.splice(0)) resolve('-');
    await fs.writeFile(`${evidence}/report.json`, JSON.stringify(report, null, 2));
  }
};
