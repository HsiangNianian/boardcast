const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkPencilText(page, baseUrl, evidence = '/tmp/boardcast-pencil-text') {
  await fs.mkdir(evidence, {recursive:true});
  const report = {checks:[], pageErrors:[]};
  const check = (ok, name) => {report.checks.push({name, ok:!!ok}); assert.ok(ok, name)};
  page.on('pageerror', e => report.pageErrors.push(e.message));
  let pending, hold = false;
  const response = text => ({status:200, contentType:'application/json', headers:{'access-control-allow-origin':'*'}, body:JSON.stringify({choices:[{message:{content:text}}]})});
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang', 'en');
    localStorage.setItem('boardcast.mode', 'pencil');
    localStorage.setItem('boardcast.mode-hint', 'dismissed');
    localStorage.setItem('boardcast.log', 'closed');
    localStorage.setItem('boardcast.localLlm', 'http://127.0.0.1:19876');
  });
  await page.route('http://127.0.0.1:19876/**', route => {
    if (route.request().method() !== 'POST') return route.fulfill({status:200, contentType:'application/json', headers:{'access-control-allow-origin':'*'}, body:JSON.stringify({status:'ok', data:[{id:'fixture'}]})});
    if (hold) {pending = route; return}
    return route.fulfill(response('N 「Initial topic」'));
  });
  const read = () => page.evaluate(async () => {
    const id = localStorage.getItem('boardcast.meeting');
    if (!id) return null;
    const db = await new Promise((yes,no) => {const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
    try {return await new Promise((yes,no) => {const r=db.transaction('meetings').objectStore('meetings').get(id);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)})} finally {db.close()}
  });
  const saved = async predicate => {
    for (let i=0;i<100;i++) {const row=await read(); if(row && predicate(row)) {await page.locator('.save-state.saved').waitFor(); return read()} await page.waitForTimeout(100)}
    throw new Error('Expected text edit was not saved');
  };
  const send = async text => {await page.getByRole('textbox',{name:'What was said',exact:true}).fill(text);await page.getByRole('button',{name:'Put on the board',exact:true}).click()};
  const elements = row => row?.scene?.elements.filter(e => !e.isDeleted) ?? [];
  const textIn = (row, value) => elements(row).find(e => e.type === 'text' && e.originalText === value);
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state:'hidden'});
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await send('Establish the initial topic.');
    await saved(m => m.graph.nodes.length === 1 && m.lines.at(-1).ms !== undefined);
    hold = true;
    for (const kind of ['semantic','free']) {
      if(kind === 'free') await page.getByRole('button',{name:'New board',exact:true}).click();
      await page.getByRole('radio',{name:'Free edit',exact:true}).click();
      await page.locator('.pencil-stage[data-ready="true"]').waitFor();
      await page.locator('.save-state.saved').waitFor();
      const before = await read();
      await send('Add a concurrent topic during ' + kind + ' text editing.');
      for(let i=0;i<100&&!pending;i++) await page.waitForTimeout(20);
      assert.ok(pending, 'the actual App model request is held');
      const box = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
      if(kind === 'free') await page.getByRole('radio',{name:'Text',exact:true}).locator('..').click();
      await page.screenshot({path:`${evidence}/${kind}-before-edit.png`});
      if(kind === 'free') await page.mouse.click(box.x+box.width/2, box.y+box.height/2);
      else await page.mouse.dblclick(box.x+box.width/2, box.y+box.height/2);
      const editor = page.locator('textarea.excalidraw-wysiwyg');
      await editor.waitFor();
      const draft = kind === 'semantic' ? 'My edited topic' : '自由文字 draft';
      const final = draft + ' completed';
      await editor.fill(draft);
      await saved(m => textIn(m,draft) && m.lines.length === before.lines.length+1);
      const held = await read();
      check(held.lines.at(-1).ms === undefined && (held.steps?.length ?? 0) === (before.steps?.length ?? 0), `${kind}: draft and accepted transcript save without committing the active text session`);
      if(kind === 'semantic') {
        assert.deepEqual(held.graph.nodes[0].aliases,['Initial topic']);
        check(true,'semantic: saved draft retains only the name from before editing');
      }
      await pending.fulfill(response((kind === 'semantic' ? 'R n1 「Model overwrite」\n' : '') + 'N 「Concurrent topic」')); pending = null;
      await page.waitForTimeout(900);
      const during = await read();
      await fs.writeFile(`${evidence}/${kind}-during.json`,JSON.stringify({before,held,during},null,2)+'\n');
      await page.screenshot({path:`${evidence}/${kind}-during.png`});
      check(during.graph.nodes.length === before.graph.nodes.length && during.lines.at(-1).ms === undefined, `${kind}: model result waits while the text editor remains open`);
      check(await editor.inputValue() === draft && await editor.evaluate(e=>document.activeElement===e), `${kind}: model arrival preserves draft and focus`);
      await editor.press('End'); await page.keyboard.type(' completed',{delay:25});
      await editor.press('Control+z');
      const undoneText = await editor.inputValue();
      (report.textUndo ??= {})[kind] = undoneText;
      check(undoneText !== final && await editor.evaluate(e=>document.activeElement===e), `${kind}: system text Undo edits the textbox`);
      await editor.press('Control+Shift+z');
      check(await editor.inputValue() === final, `${kind}: system text Redo restores the textbox`);
      await saved(m => textIn(m,final));
      check((await read()).steps?.length === before.steps?.length, `${kind}: paused typing and local undo/redo add no board steps`);
      if(kind === 'free') await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
      else await editor.press('Escape');
      const after = await saved(m => m.graph.nodes.length === before.graph.nodes.length+1 && m.lines.at(-1).ms !== undefined && textIn(m,final));
      const steps = (after.steps ?? []).slice(before.steps?.length ?? 0);
      await fs.writeFile(`${evidence}/${kind}.json`,JSON.stringify({before,held,during,after,steps,final},null,2)+'\n');
      check(steps.length === 2 && steps.every(s=>textIn(s,final)), `${kind}: complete text session and model increment produce exactly two final steps`);
      if(kind === 'semantic') check(after.graph.nodes.find(n=>n.id==='n1')?.label === final && after.scene.modelCandidates?.n1?.label === 'Model overwrite', 'semantic: user text wins and the model suggestion remains recoverable');
      if(kind === 'semantic') {
        assert.deepEqual(after.graph.nodes.find(n=>n.id==='n1').aliases,['Initial topic','Model overwrite']);
        check(true,'semantic: typing and text undo leave no intermediate aliases after model arrival');
      }
      await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
      await page.getByRole('button',{name:'Undo',exact:true}).click();
      const undoModel = await saved(m => m.graph.nodes.length === before.graph.nodes.length);
      check(!!textIn(undoModel,final), `${kind}: first global Undo removes only the model increment`);
      await page.getByRole('button',{name:'Undo',exact:true}).click();
      const undoText = await saved(m => !textIn(m,final));
      check(JSON.stringify(undoText.graph) === JSON.stringify(before.graph), `${kind}: second global Undo restores the pre-edit board`);
      await page.getByRole('button',{name:'Redo',exact:true}).click();
      await page.getByRole('button',{name:'Redo',exact:true}).click();
      const redone = await saved(m => m.graph.nodes.length === after.graph.nodes.length && textIn(m,final));
      const content = e => Object.fromEntries(Object.entries(e).filter(([key])=>kind === 'free' || !['version','versionNonce','updated'].includes(key)));
      assert.deepEqual(redone.graph, after.graph);
      assert.deepEqual(content(textIn(redone,final)), content(textIn(after,final)));
      check(true, kind === 'semantic' ? 'semantic: two Redo actions restore graph and text except generated revision metadata' : 'free: two Redo actions restore every text field and the graph');
    }
    await page.getByRole('button',{name:'New board',exact:true}).click();
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.locator('.save-state.saved').waitFor();
    const emptyBefore = await read();
    await send('Add a topic while an empty text editor is open.');
    for(let i=0;i<100&&!pending;i++) await page.waitForTimeout(20);
    assert.ok(pending);
    await page.getByRole('radio',{name:'Text',exact:true}).locator('..').click();
    const box = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    await page.screenshot({path:`${evidence}/empty-before-edit.png`});
    await page.mouse.click(box.x+box.width/2,box.y+box.height/2);
    const emptyEditor = page.locator('textarea.excalidraw-wysiwyg');
    await emptyEditor.waitFor();
    await pending.fulfill(response('T 「After empty text」'));pending=null;
    await page.waitForTimeout(900);
    const emptyDuring = await read();
    check(emptyDuring.lines.at(-1).ms === undefined && emptyDuring.graph.nodes.length === 0, 'empty editor: result waits without a text change');
    await emptyEditor.press('Escape');
    const emptyAfter = await saved(m=>m.lines.at(-1).ms !== undefined && m.graph.nodes.length === 1);
    check((emptyAfter.steps?.length ?? 0)-(emptyBefore.steps?.length ?? 0) === 1 && !elements(emptyAfter).some(e=>!e.customData?.boardcast), 'empty editor: cancel releases the model and adds no empty text step');
    await fs.writeFile(`${evidence}/empty.json`,JSON.stringify({before:emptyBefore,during:emptyDuring,after:emptyAfter},null,2)+'\n');
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();
    await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    await page.getByRole('radio',{name:'Selection',exact:true}).locator('..').click();
    const standaloneBefore = await read();
    const canvas = await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    await page.mouse.dblclick(canvas.x+canvas.width/2,canvas.y+canvas.height/2);
    const standaloneEditor = page.locator('textarea.excalidraw-wysiwyg');
    await standaloneEditor.fill('你好');
    await standaloneEditor.press('Escape');
    await saved(m=>m.graph.nodes[0].label==='你好' && m.steps.length>standaloneBefore.steps.length);
    await page.waitForTimeout(1000);
    const standaloneAfter = await read();
    await fs.writeFile(`${evidence}/standalone.json`,JSON.stringify({before:standaloneBefore,after:standaloneAfter},null,2)+'\n');
    check(standaloneAfter.steps.length === standaloneBefore.steps.length+1,'standalone rename: generated underline reflow remains part of the text edit');
    const underline = row => elements(row).find(e=>e.id.endsWith(':underline'));
    assert.ok(underline(standaloneAfter.steps.at(-1)) && underline(standaloneAfter));
    assert.deepEqual(underline(standaloneAfter.steps.at(-1))?.points,underline(standaloneAfter)?.points);
    check(true,'standalone rename: replay stores the final generated underline');
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
    await page.getByRole('button',{name:'Undo',exact:true}).click();
    await saved(m=>JSON.stringify(m.graph)===JSON.stringify(standaloneBefore.graph));
    await page.getByRole('button',{name:'Redo',exact:true}).click();
    await saved(m=>JSON.stringify(m.graph)===JSON.stringify(standaloneAfter.graph));
    check(true,'standalone rename: one global Undo and Redo restore complete names');
    const beforeReload = await read();
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    const reopened = await read();
    assert.deepEqual(reopened.graph,beforeReload.graph);
    assert.deepEqual(reopened.pages.map(p=>p.scene),beforeReload.pages.map(p=>p.scene));
    check(true,'both text edits survive reload');
    check(!report.pageErrors.length,'no browser script errors');
  } catch(error) {
    report.error=String(error);
    await fs.writeFile(`${evidence}/failure-meeting.json`,JSON.stringify(await read().catch(()=>null),null,2)+'\n');
    await page.screenshot({path:`${evidence}/failure.png`});throw error;
  } finally {
    if(pending)await pending.abort().catch(()=>{});
    await fs.writeFile(`${evidence}/report.json`,JSON.stringify(report,null,2)+'\n');
  }
  return report;
};
