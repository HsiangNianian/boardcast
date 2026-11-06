const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkCircleFeedback(page, baseUrl, evidence = '/tmp/boardcast-circle-feedback') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], requests: [], gestures: [], pageErrors: []};
  const check = (ok, name) => report.checks.push({name, ok: !!ok});
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({lang: 'en', mode: 'ink', log: 'closed', 'mode-hint': 'dismissed', localLlm: 'http://127.0.0.1:19878'})) localStorage.setItem('boardcast.' + key, value);
    window.qaCircleRequests = [];
    const original = window.fetch.bind(window);
    window.fetch = (input, options) => {
      if (!String(input).startsWith('http://127.0.0.1:19878')) return original(input, options);
      if (options?.method !== 'POST') return Promise.resolve(new Response('{"status":"ok"}'));
      return new Promise(resolve => window.qaCircleRequests.push({resolve, body: JSON.parse(options.body)}));
    };
  });
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    const row = await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get('qa-circle-feedback'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    db.close(); return row;
  });
  const saved = async predicate => {
    for (let i = 0; i < 100; i++) {const row = await read(); if (row && predicate(row)) return row; await page.waitForTimeout(50);}
    throw Error('Expected circle/transcript state was not saved');
  };
  const hint = () => page.locator('.lasso-hint');
  const loops = () => page.locator('.ink-lasso [data-loop]:not([data-loop="live"])').count();
  const enter = async () => {if (await page.getByRole('button', {name: 'Circle', exact: true}).getAttribute('aria-pressed') !== 'true') await page.getByRole('button', {name: 'Circle', exact: true}).click();};
  const cancel = async () => {if (await hint().count()) await hint().getByRole('button', {name: 'Cancel', exact: true}).click();};
  const rect = async (x, y, width, height) => {
    await page.mouse.move(x, y); await page.mouse.down();
    for (const [px, py] of [[x + width, y], [x + width, y + height], [x, y + height], [x, y]]) await page.mouse.move(px, py, {steps: 10});
    await page.mouse.up();
  };
  const around = async ids => {
    await enter();
    await page.locator('.ink').click({trial: true});
    await page.waitForFunction(() => {
      const svg = document.querySelector('.ink > svg'), box = svg.getBoundingClientRect(), view = svg.viewBox.baseVal;
      return Math.abs(box.width / box.height - view.width / view.height) < 0.002;
    });
    const box = await page.locator('.ink [data-key]').evaluateAll((elements, ids) => {
      const boxes = elements.filter(e => ids.some(id => e.dataset.key.startsWith(id + '|'))).map(e => e.getBoundingClientRect()).filter(b => b.width && b.height);
      if (!boxes.length) throw Error('No rendered target boxes');
      const x = Math.min(...boxes.map(b => b.x)) - 12, y = Math.min(...boxes.map(b => b.y)) - 12;
      return {x, y, width: Math.max(...boxes.map(b => b.right)) + 12 - x, height: Math.max(...boxes.map(b => b.bottom)) + 12 - y};
    }, ids);
    const board = await page.locator('.ink').boundingBox();
    assert(box.x >= board.x && box.y >= board.y && box.x + box.width <= board.x + board.width && box.y + box.height <= board.y + board.height, 'loop fits the actual canvas');
    report.gestures.push({ids, box, board, viewBox: await page.locator('.ink > svg').getAttribute('viewBox')});
    await rect(box.x, box.y, box.width, box.height);
    report.gestures.at(-1).hint = await hint().innerText();
  };
  const send = async text => {await page.getByRole('textbox', {name: 'What was said', exact: true}).fill(text); await page.getByRole('button', {name: 'Put on the board', exact: true}).click();};
  const request = async index => {
    await page.waitForFunction(i => window.qaCircleRequests.length > i, index);
    const body = await page.evaluate(i => window.qaCircleRequests[i].body, index);
    const text = body.messages.at(-1).content;
    const current = text.slice(0, text.lastIndexOf('\n新的一句：')).split('\n').at(-1);
    const ids = current.startsWith('当前手指着：') ? [...current.matchAll(/\b(n\d+)（/g)].map(m => m[1]) : [];
    report.requests.push({index, ids, text}); return ids;
  };
  const respond = (index, content = '-', fail = false) => page.evaluate(({index, content, fail}) => {
    window.qaCircleRequests[index].resolve(new Response(fail ? 'Injected model failure' : JSON.stringify({choices: [{message: {content}}]}), {status: fail ? 503 : 200, headers: {'content-type': 'application/json'}}));
  }, {index, content, fail});
  const all = Array.from({length: 10}, (_, i) => 'n' + (i + 1));
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    const fixture = await page.evaluate(async () => {
      const node = (id, label, extra = {}) => ({id, label, kind: 'node', aliases: [], marks: [], ink: 'ink', ...extra});
      const graph = {nodes: Array.from({length: 10}, (_, i) => node('n' + (i + 1), 'Target ' + (i + 1))), edges: [], groups: [], nextNode: 11, nextGroup: 1};
      const layout = {regionAt: [['__diagram', {x: 0, y: 0}]], nodeAt: graph.nodes.map((n, i) => [n.id, {id: n.id, x: i % 5 * 200, y: Math.floor(i / 5) * 160, w: 140, h: 60}]), routeAt: [], boxAt: []};
      const topic = {nodes: [node('n11', 'Architecture and ' + 'long project scope '.repeat(8), {kind: 'topic'}), ...Array.from({length: 9}, (_, i) => node('n' + (i + 12), 'Requirement ' + (i + 1), {kind: 'item', parent: 'n11'}))], edges: [], groups: [], nextNode: 21, nextGroup: 1};
      const row = {version: 2, storageRevision: 1, id: 'qa-circle-feedback', title: 'Circle feedback', titled: true, createdAt: Date.now(), updatedAt: Date.now(), graph, layout, history: [], lines: [], pages: [{graph, layout}, {graph: topic}], page: 0};
      const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2', 1); r.onupgradeneeded = () => {r.result.createObjectStore('meetings', {keyPath: 'id'}); r.result.createObjectStore('assets');}; r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
      await new Promise((yes, no) => {const tx = db.transaction('meetings', 'readwrite'); tx.objectStore('meetings').put(row); tx.oncomplete = yes; tx.onerror = () => no(tx.error);});
      db.close(); localStorage.setItem('boardcast.meeting', row.id); return row;
    });
    await fs.writeFile(`${evidence}/fixture.json`, JSON.stringify(fixture, null, 2));
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); await page.locator('.ink [data-key^="n10|"]').first().waitFor();
    report.productionScripts = await page.locator('script[src]').evaluateAll(xs => xs.map(x => x.src));
    await enter();
    let board = await page.locator('.ink').boundingBox();
    await page.mouse.click(board.x + 20, board.y + board.height - 50);
    check(await loops() === 0, 'a click does not become a circle target');
    await rect(board.x + 20, board.y + board.height - 70, 35, 35);
    await page.getByText('That loop has nothing from the board in it; try again', {exact: true}).waitFor();
    check(await loops() === 0, 'an actual empty loop explains the miss and retains no target');
    await around(['n1']);
    check(await loops() === 1 && /1 target\b/.test(await hint().innerText()), 'a real loop reports one target for the next sentence');
    await around(['n1', 'n2']);
    check(await loops() === 1 && (await hint().innerText()).includes('Target 2'), 'overlapping loops merge and include both actual node hits');
    await hint().getByRole('button', {name: 'Undo', exact: true}).click();
    check(await loops() === 1 && !(await hint().innerText()).includes('Target 2'), 'undoing a merged loop restores the prior target set');
    await around(['n4']);
    check(await loops() === 2 && (await hint().innerText()).includes('Target 4'), 'separate loops retain separate targets');
    await page.keyboard.press('Escape');
    check(await loops() === 0 && await hint().count() === 0, 'Escape cancels circles without editing board content');

    await around(all.slice(0, 5)); await around(all.slice(5));
    check(await loops() === 2 && /8 targets\b/.test(await hint().innerText()) && /most recent 8 targets/.test(await hint().innerText()), 'the limit also applies to targets accumulated across separate loops');
    await hint().getByRole('button', {name: 'Undo', exact: true}).click();
    check(await loops() === 1 && /5 targets\b/.test(await hint().innerText()) && !(await hint().innerText()).includes('most recent'), 'undoing the last separate loop updates the count and removes the limit notice');
    await cancel();
    await around(all);
    check(/8 targets\b/.test(await hint().innerText()), 'a large loop reports the eight targets that will actually be used');
    check(/most recent 8 targets/.test(await hint().innerText()), 'exceeding eight targets has an explicit retained-target explanation');
    await page.screenshot({path: `${evidence}/limit-desktop.png`});
    await send('LIMIT_REFERENCE');
    assert.deepEqual(await request(0), ['n2', 'n7', 'n3', 'n8', 'n4', 'n9', 'n5', 'n10']);
    check(await loops() === 0 && await hint().count() === 0, 'sending consumes circles and sends exactly the existing latest-eight selection');
    await respond(0); await saved(m => m.lines.find(l => l.text === 'LIMIT_REFERENCE')?.ms !== undefined);
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    await page.locator('.ink [data-key^="n20|"]').first().waitFor();
    await around(Array.from({length: 10}, (_, i) => 'n' + (i + 11)));
    check(/1 target\b/.test(await hint().innerText()) && /selected parents/.test(await hint().innerText()), 'a large topic loop explains that its child items are represented by the selected parent');
    check((await hint().innerText()).includes('Architecture and…') && !(await hint().innerText()).includes('long project scope'), 'long target names use a bounded preview while the original name stays intact');
    await send('GROUPED_REFERENCE'); assert.deepEqual(await request(1), ['n11']);
    await respond(1); await saved(m => m.lines.find(l => l.text === 'GROUPED_REFERENCE')?.ms !== undefined);
    check(true, 'the actual request preserves the existing parent-topic reduction');
    await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
    await page.locator('.ink [data-key^="n1|"]').first().waitFor();

    await send('QUEUE_BLOCKER'); assert.deepEqual(await request(2), []);
    await around(['n1']); await send('QUEUE_ALPHA');
    await around(['n2']); await send('QUEUE_BETA');
    await around(['n3']);
    check(await page.evaluate(() => window.qaCircleRequests.length) === 3, 'new pointed sentences wait behind the held model request');
    check(await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).isEnabled(), 'pending generation leaves other boards reachable');
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    check(await loops() === 0 && await hint().count() === 0, 'navigation clears the unsent circle while queued sentences retain their targets');
    await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
    await around(['n3']);
    await respond(2); assert.deepEqual(await request(3), ['n1']);
    await respond(3, '-', true); assert.deepEqual(await request(4), ['n2']);
    await respond(4, 'M n2 星标');
    const queued = await saved(m => m.lines.some(l => l.text === 'QUEUE_ALPHA' && l.failed) && m.graph.nodes.find(n => n.id === 'n2').marks.includes('星标'));
    check(queued.lines.find(l => l.text === 'QUEUE_ALPHA').pointing.join() === 'n1' && queued.lines.find(l => l.text === 'QUEUE_BETA').pointing.join() === 'n2', 'stored queued lines retain their own targets after success and failure');
    check(await loops() === 1 && (await hint().innerText()).includes('Target 3'), 'older replies leave a new unsent circle intact');
    await cancel(); await send('TARGET_REMOVAL'); assert.deepEqual(await request(5), []);
    await around(['n3']); await respond(5, 'D n3');
    await saved(m => !m.graph.nodes.some(n => n.id === 'n3'));
    check((await hint().innerText()).includes('That loop has nothing from the board in it; try again') && !(await hint().innerText()).includes('1 target'), 'a reply that deletes the unsent target replaces its count with an empty-target explanation');
    await send('AFTER_REMOVAL'); assert.deepEqual(await request(6), []); await respond(6);
    const removed = await saved(m => m.lines.find(l => l.text === 'AFTER_REMOVAL')?.ms !== undefined);
    check(!removed.lines.find(l => l.text === 'AFTER_REMOVAL').pointing && await loops() === 0, 'the next sentence excludes the deleted target in both its actual request and saved transcript');
    await around(['n4']);
    await page.locator('.pages').getByRole('button', {name: 'Board 2', exact: true}).click();
    check(await loops() === 0 && await hint().count() === 0, 'changing the board clears the unsent circle');
    await page.getByRole('button', {name: 'Transcript', exact: true}).click();
    await page.getByRole('button', {name: 'Retry this line', exact: true}).click();
    assert.deepEqual(await request(7), ['n1']); await respond(7, 'M n1 下划线');
    const recovered = await saved(m => m.page === 1 && m.pages[0].graph.nodes.find(n => n.id === 'n1').marks.includes('下划线') && !m.lines.find(l => l.text === 'QUEUE_ALPHA').failed);
    check(recovered.lines.length === 7 && recovered.lines.find(l => l.text === 'QUEUE_ALPHA').id === queued.lines.find(l => l.text === 'QUEUE_ALPHA').id && recovered.graph.nodes.every(n => !n.marks.length), 'retry on another page uses the original target without duplicating a line or changing the viewed page');
    await page.getByRole('button', {name: 'Transcript', exact: true}).click();
    await page.locator('.pages').getByRole('button', {name: 'Board 1', exact: true}).click();
    await page.locator('.ink [data-key^="n1|"]').first().waitFor();
    await around(['n4']);
    await page.getByRole('radio', {name: 'Free edit', exact: true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
    check(await loops() === 0 && await hint().count() === 0, 'switching board view discards unsent circle intent');
    await around(all);
    await page.setViewportSize({width: 640, height: 400});
    await page.locator('.meeting-more').waitFor({state: 'visible'});
    const box = await hint().evaluate(el => ({rect: el.getBoundingClientRect().toJSON(), canvas: document.querySelector('.ink').getBoundingClientRect().toJSON(), textOverflow: [...el.querySelectorAll('span')].some(s => s.scrollWidth > s.clientWidth + 1 || s.scrollHeight > s.clientHeight + 1), overflow: document.documentElement.scrollWidth > innerWidth}));
    report.compact = box;
    check(!box.overflow && !box.textOverflow && box.rect.x >= 0 && box.rect.right <= 640 && box.rect.bottom <= 400, 'the complete count and limit remain readable at 640×400');
    check(box.rect.bottom <= box.canvas.top && box.canvas.height >= 100 && await page.getByRole('button', {name: 'Show full board', exact: true}).isVisible(), 'the compact hint has its own row and leaves at least 100 pixels for the canvas');
    await page.getByRole('button', {name: 'Show full board', exact: true}).click();
    check(await loops() === 1 && /8 targets\b/.test(await hint().innerText()), 'fitting the compact board retains the drawn loop and its targets');
    await page.screenshot({path: `${evidence}/limit-compact-en.png`});
    await clickMeetingAction(page, 'Settings');
    await page.getByRole('dialog').getByRole('radio', {name: '中文', exact: true}).click();
    await page.getByRole('dialog').getByRole('button', {name: '关闭', exact: true}).click();
    check(/8 个目标/.test(await hint().innerText()) && /最近/.test(await hint().innerText()), 'the active count and overflow explanation switch to Chinese');
    await page.waitForFunction(() => document.querySelector('.lasso-hint').getBoundingClientRect().bottom <= document.querySelector('.ink').getBoundingClientRect().top);
    report.compactChinese = await hint().evaluate(el => ({rect: el.getBoundingClientRect().toJSON(), canvas: document.querySelector('.ink').getBoundingClientRect().toJSON(), textOverflow: [...el.querySelectorAll('span')].some(s => s.scrollWidth > s.clientWidth + 1 || s.scrollHeight > s.clientHeight + 1)}));
    check(!report.compactChinese.textOverflow && report.compactChinese.canvas.height >= 100, 'Chinese guidance remains fully readable in its own row above the canvas');
    await page.screenshot({path: `${evidence}/limit-compact-zh.png`});
    await hint().getByRole('button', {name: '取消', exact: true}).focus(); await page.keyboard.press('Enter');
    check(await hint().count() === 0 && await loops() === 0, 'keyboard cancellation remains reachable in the compact Chinese layout');
    check(await page.getByRole('button', {name: '圈选', exact: true}).evaluate(el => {const box = el.getBoundingClientRect(); return document.activeElement === el && box.top >= 0 && box.bottom <= innerHeight;}), 'keyboard cancellation returns focus to the visible Circle tool');
    await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'});
    const final = await read();
    check(final.lines.length === 7 && final.lines.every(l => !l.failed) && final.pages[1].graph.nodes[0].label === fixture.pages[1].graph.nodes[0].label && await hint().count() === 0, 'reopening retains completed target attribution and full names without restoring stale circles');
    check(report.pageErrors.length === 0, 'circle interactions have no uncaught script errors');
    report.passed = report.checks.every(c => c.ok);
    assert(report.passed, report.checks.filter(c => !c.ok).map(c => c.name).join('\n'));
  } catch (error) {report.error = String(error); await page.screenshot({path: `${evidence}/failure.png`}); throw error;}
  finally {await fs.writeFile(`${evidence}/circle-feedback-check.json`, JSON.stringify(report, null, 2) + '\n');}
};
