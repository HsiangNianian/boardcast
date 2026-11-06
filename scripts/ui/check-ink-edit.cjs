const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async (page, baseUrl, evidence) => {
  const report = {checks: [], pageErrors: [], observations: {}};
  const check = (ok, name) => {report.checks.push({name, ok: !!ok}); console.log(ok ? 'PASS:' : 'FAIL:', name);};
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const frame = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.mode', 'ink');
    localStorage.setItem('boardcast.log', 'open'); localStorage.setItem('boardcast.mode-hint', 'dismissed');
  });
  await page.emulateMedia({reducedMotion: 'reduce'});
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2'); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});
    try {return await new Promise((yes, no) => {const r = db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting')); r.onsuccess = () => yes(r.result); r.onerror = () => no(r.error);});}
    finally {db.close();}
  });
  const saved = async () => {await frame(); await page.locator('.save-state.saved').waitFor(); return read();};
  const toolbar = page.locator('.ink-toolbar');
  const rename = toolbar.getByRole('button', {name: 'Rename', exact: true});
  const input = toolbar.getByRole('textbox', {name: 'Rename', exact: true});
  const begin = async () => {await rename.focus(); await page.keyboard.press('Enter'); await input.waitFor();};
  const focused = () => rename.evaluate(el => el === document.activeElement);
  const point = async label => {
    await page.waitForFunction(label => [...document.querySelectorAll('.ink [data-key]')].some(el => el.dataset.key === label && el.querySelector('path')), label);
    const r = await page.locator('.ink [data-key]').filter({has: page.locator('path')}).evaluateAll((els, label) => {
      const el = els.find(e => e.dataset.key === label); if (!el) return null;
      const r = el.getBoundingClientRect(); return {x: r.x + r.width / 2, y: r.y + r.height / 2};
    }, label);
    assert.ok(r, 'visible label: ' + label); return r;
  };
  const click = async label => {
    const p = await point(label), canvas = await page.locator('.ink').boundingBox();
    assert.ok(p.x >= canvas.x && p.y >= canvas.y && p.x <= canvas.x + canvas.width && p.y <= canvas.y + canvas.height, 'locate the label before clicking: ' + label);
    await page.mouse.click(p.x, p.y); await toolbar.waitFor();
  };
  const locate = async label => {
    await page.locator('.sidebar-tabs').getByRole('tab', {name: 'Contents', exact: true}).click();
    const row = page.locator('.content-list li').filter({has: page.locator('strong').filter({hasText: new RegExp('^' + label + '$')})});
    await row.getByRole('button', {name: 'Locate', exact: true}).click(); await toolbar.waitFor(); await frame();
  };
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    const n = (id, kind, label, extra = {}) => ({id, kind, label, aliases: [], marks: [], ink: 'ink', ...extra});
    const graph = {nodes: [n('n1', 'topic', 'Plan'), n('n2', 'item', 'First task', {parent: 'n1'}), n('n3', 'item', 'Nested task', {parent: 'n2'}), n('n4', 'node', 'Client'), n('n5', 'node', 'Service'), n('n6', 'region', 'Source excerpt', {text: 'Immutable original source.', at: {x: 20, y: 20, w: 520, h: 90}})], edges: [{from: 'n4', to: 'n5', label: 'requests', ink: 'ink'}], groups: [], nextNode: 7, nextGroup: 1, underlay: {w: 560, h: 130, no: 1}};
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="560" height="130"><rect width="560" height="130" fill="white"/><text x="24" y="55" font-size="20">Immutable original source.</text></svg>';
    const fixture = {type: 'excalidraw', elements: [], files: {}, boardcast: {version: 2, graph, generated: [], underlay: 'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64')}};
    await page.locator('input[type=file]').setInputFiles({name: 'ink-edit.excalidraw', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(fixture))});
    const dialog = page.getByRole('dialog', {name: 'Import materials', exact: true});
    await dialog.getByRole('button', {name: 'New meeting', exact: true}).click(); await dialog.waitFor({state: 'hidden'});
    await page.locator('.ink [data-key="n4|Client"] path').first().waitFor();
    let before = await saved();
    await click('n4|Client'); await begin(); await page.keyboard.press('Enter');
    let after = await saved();
    check(same(after.graph, before.graph) && after.lines.length === before.lines.length && after.steps.length === before.steps.length, 'unchanged node rename adds no transcript, replay or undo step');
    await frame(); check(await focused(), 'node rename completion returns keyboard focus to Rename');
    await begin(); await input.fill('Draft client');
    await input.dispatchEvent('keydown', {key: 'Escape', code: 'Escape', isComposing: true}); await frame();
    check(await input.count() === 1 && await input.inputValue().catch(() => '') === 'Draft client', 'IME Escape cancels a candidate without closing the rename draft');
    await page.screenshot({path: path.join(evidence, 'ime-escape.png')});
    if (!await input.count()) {await begin(); await input.fill('Draft client');}
    const accepted = await input.evaluate(el => el.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', code: 'Enter', isComposing: true, bubbles: true, cancelable: true})));
    check(!accepted && await input.count() === 1, 'IME Enter cannot implicitly submit the rename form');
    await page.keyboard.press('Escape');
    await frame(); check(await focused(), 'Escape discards the draft and restores Rename focus');
    await begin(); await input.fill('   '); await page.keyboard.press('Enter'); await frame();
    check(await input.count() === 1 && await toolbar.getByRole('button', {name: 'OK', exact: true}).isDisabled().catch(() => false), 'empty node name stays editable and cannot be submitted');
    if (await input.count()) await page.keyboard.press('Escape');
    before = await saved(); await begin(); await input.fill('Client ready'); await page.keyboard.press('Enter');
    await page.locator('.ink [data-key="n4|Client ready"] path').first().waitFor(); after = await saved();
    const client = after.graph.nodes.find(n => n.id === 'n4');
    check(client.label === 'Client ready' && same(client.aliases, ['Client']) && after.lines.length === before.lines.length + 1 && after.steps.length === before.steps.length + 1, 'node rename commits one step and retains only the real old name');
    check(await focused(), 'changed node rename restores keyboard focus');

    await page.getByRole('radio', {name: 'Free edit', exact: true}).click(); await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.getByRole('radio', {name: 'Hand-drawn', exact: true}).click();
    await locate('Client ready → Service'); before = await saved(); await begin(); await page.keyboard.press('Enter'); after = await saved();
    check(same(after.graph, before.graph) && after.lines.length === before.lines.length && after.steps.length === before.steps.length, 'unchanged edge rename adds no transcript, replay or undo step');
    await begin(); await input.fill(''); await page.keyboard.press('Enter'); after = await saved();
    check(after.graph.edges.length === 1 && !after.graph.edges[0].label && after.graph.nodes.length === 6, 'empty edge label removes only the label');
    check(!after.scene.elements.some(e => e.id === 'n4>n5:label' && !e.isDeleted), 'clearing an edge label leaves no detached or unnamed text object');
    check(await focused(), 'clearing an edge label keeps Rename keyboard reachable');
    before = after; await begin(); await page.keyboard.press('Enter'); after = await saved();
    check(after.lines.length === before.lines.length && after.steps.length === before.steps.length, 'clearing an already empty edge is a no-op');
    await begin(); await input.fill('sends requests'); await page.keyboard.press('Enter'); after = await saved();
    check(after.graph.edges[0].label === 'sends requests', 'edge label can be restored');
    check(after.scene.elements.some(e => e.id === 'n4>n5:label' && !e.isDeleted && e.originalText === 'sends requests' && e.customData?.boardcast?.kind === 'edge'), 'restored edge text retains semantic ownership in the editable scene');

    await locate('Plan'); before = await saved();
    const selected = await page.locator('.ink-selection rect').boundingBox();
    const p = {x: selected.x + selected.width / 2, y: selected.y + selected.height / 2};
    const scale = await page.locator('.ink > svg').evaluate(el => el.getScreenCTM().a);
    await page.mouse.move(p.x, p.y); await page.mouse.down(); await page.mouse.move(p.x + 65, p.y + 35, {steps: 12}); await page.mouse.up();
    after = await saved();
    const a = new Map(before.layout.regionAt).get('n1'), b = new Map(after.layout.regionAt).get('n1');
    report.observations.move = {before: a, after: b, scale};
    check(Math.abs((b.x - a.x) * scale - 65) < 2 && Math.abs((b.y - a.y) * scale - 35) < 2 && same(after.graph, before.graph), 'dragging a topic moves its region by the full gesture and preserves its subtree');
    check(after.steps.length === before.steps.length + 1, 'the whole drag records one replay step');
    await page.getByRole('button', {name: 'Undo', exact: true}).click(); let restored = await saved();
    check(same(restored.graph, before.graph) && same(restored.layout, before.layout), 'one undo restores the complete topic position');
    await page.getByRole('button', {name: 'Redo', exact: true}).click(); restored = await saved();
    check(same(restored.graph, after.graph) && same(restored.layout, after.layout), 'one redo restores the complete move');

    await locate('Source excerpt'); before = await saved();
    check(await rename.count() === 0 && await toolbar.getByRole('button', {name: 'Delete', exact: true}).count() === 0, 'fixed source text exposes marks without rename or delete');
    await page.locator('.ink').focus(); await page.keyboard.press('Delete'); after = await saved();
    check(same(after.graph, before.graph) && after.lines.length === before.lines.length, 'Delete cannot remove fixed source text');
    await locate('Client ready → Service'); before = await saved();
    await toolbar.getByRole('button', {name: 'Delete', exact: true}).focus(); await page.keyboard.press('Enter'); after = await saved();
    check(after.graph.edges.length === 0 && same(after.graph.nodes, before.graph.nodes), 'keyboard edge deletion preserves both nodes');
    await page.getByRole('button', {name: 'Undo', exact: true}).click(); after = await saved();
    check(same(after.graph, before.graph), 'undo restores the labeled edge');
    await locate('Plan'); before = await saved(); await page.locator('.ink').focus(); await page.keyboard.press('Delete'); after = await saved();
    check(same(after.graph.nodes.map(n => n.id), ['n4', 'n5', 'n6']) && same(after.graph.edges, before.graph.edges), 'topic deletion removes its descendants while preserving the other graph');
    await page.getByRole('button', {name: 'Undo', exact: true}).click(); after = await saved();
    check(same(after.graph, before.graph), 'one undo restores the complete topic subtree');
    await locate('Client ready');
    for (const [width, height] of [[1280, 800], [900, 600], [720, 480]]) {
      await page.setViewportSize({width, height}); await frame();
      const bounds = await toolbar.evaluate(el => {const r = el.getBoundingClientRect(), c = el.closest('.ink').getBoundingClientRect(); return {x: r.x, y: r.y, right: r.right, bottom: r.bottom, canvas: {x: c.x, y: c.y, right: c.right, bottom: c.bottom}};});
      const c = bounds.canvas;
      check(bounds.x >= c.x && bounds.y >= c.y && bounds.right <= c.right && bounds.bottom <= c.bottom, `${width}: real selected-object controls stay inside the canvas`);
      await page.screenshot({path: path.join(evidence, `toolbar-${width}.png`)});
    }
    await page.setViewportSize({width: 1280, height: 800});
    before = await saved(); await page.reload(); await page.locator('.splash').waitFor({state: 'hidden'}); after = await saved();
    check(same(after.graph, before.graph) && same(after.layout, before.layout) && same(after.steps, before.steps), 'cold reload retains the final graph, positions and edit timeline');
    check(!report.pageErrors.length, 'no uncaught application errors');
    assert.ok(report.checks.every(c => c.ok), report.checks.filter(c => !c.ok).map(c => c.name).join('\n'));
  } catch (error) {report.error = String(error); await page.screenshot({path: path.join(evidence, 'failure.png')}); throw error;}
  finally {await fs.writeFile(path.join(evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');}
};
