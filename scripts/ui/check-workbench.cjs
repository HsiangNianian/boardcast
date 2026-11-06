const meetingAction = require('./meeting-action.cjs');
// Run with an isolated Playwright page against the production preview.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkWorkbench(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, {recursive:true});
  await page.addInitScript(() => localStorage.setItem('boardcast.lang', 'en'));
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(baseUrl);
  await page.locator('.splash').waitFor({state:'hidden'});
  const layouts=[];
  for (const [width,height] of [[1280,800],[900,600],[720,480],[640,400]]) {
    await page.setViewportSize({width,height});
    const layout = await page.evaluate(() => ({width:innerWidth,height:innerHeight,overflow:document.documentElement.scrollWidth>innerWidth,board:document.querySelector('main').getBoundingClientRect().height,footer:document.querySelector('.meeting-footer').getBoundingClientRect().bottom}));
    assert.equal(layout.overflow,false); assert.ok(layout.board>=100,JSON.stringify(layout)); assert.ok(layout.footer<=height);
    layouts.push(layout);
    if (width<=900) {
      const boardTab=page.getByRole('tab',{name:'Board',exact:true});
      await boardTab.click();
      assert.equal(await page.locator('main').getAttribute('aria-hidden'),'false');
      await page.keyboard.press('ArrowRight');
      assert.equal(await page.getByRole('tab',{name:'Meeting details',exact:true}).getAttribute('aria-selected'),'true');
      assert.equal(await page.locator('main').getAttribute('aria-hidden'),'true');
      await page.keyboard.press('Home');
      assert.equal(await boardTab.getAttribute('aria-selected'),'true');
    }
    await page.screenshot({path:`${evidence}/workbench-${width}.png`});
  }
  await page.setViewportSize({width:1280,height:800});
  await page.getByRole('button',{name:'Start a meeting',exact:true}).click();
  await page.getByRole('dialog').waitFor();
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('dialog').count(),0);
  await (await meetingAction(page, 'Import')).click();
  await page.keyboard.press('End');
  assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('role')),'menuitem');
  await page.keyboard.press('Home');
  assert.equal(await page.evaluate(()=>document.activeElement===document.querySelector('[role="menu"] [role="menuitem"]')),true);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('menu').count(),0);
  assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('aria-label')),'Import');

  await page.getByRole('radio',{name:'Free edit',exact:true}).click();
  await page.locator('.excalidraw').waitFor();
  assert.equal(await page.getByRole('button',{name:'Replay',exact:true}).isEnabled(),false,'opening an empty editor is not a replay step');
  await page.getByRole('radio',{name:'Rectangle',exact:true}).locator('..').click();
  await page.mouse.move(520,300); await page.mouse.down(); await page.mouse.move(740,450,{steps:12}); await page.mouse.up();
  await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
  await page.locator('image.free-scene').waitFor();
  await page.locator('.save-state.saved').waitFor();
  const read = () => page.evaluate(async () => {
    const db=await new Promise((resolve,reject)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});
    const rows=await new Promise((resolve,reject)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});
    db.close();return rows;
  });
  const first=(await read())[0];
  const shape=first.scene.elements.find(e=>e.type==='rectangle'&&!e.isDeleted);
  assert.ok(shape); assert.equal(first.version,2);
  await page.screenshot({path:`${evidence}/workbench-free-pad.png`});
  await page.getByRole('radio',{name:'Free edit',exact:true}).click();
  await page.locator('.excalidraw').waitFor();
  await (await meetingAction(page, 'Export')).click();
  await page.getByRole('menuitemradio',{name:'Current board',exact:true}).click();
  const downloadPromise=page.waitForEvent('download');
  await page.getByRole('menuitem',{name:/^Excalidraw/}).click();
  const download=await downloadPromise;
  const exported=JSON.parse(await fs.readFile(await download.path(),'utf8'));
  assert.ok(exported.elements.some(e=>e.id===shape.id&&!e.isDeleted));
  await page.locator('.save-state.saved').waitFor();
  await page.reload();
  await page.locator('.splash').waitFor({state:'hidden'});
  await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
  await page.locator('image.free-scene').waitFor();
  assert.ok((await read())[0].scene.elements.some(e=>e.id===shape.id&&!e.isDeleted));

  // A failed transaction must keep the live meeting and block a destructive switch.
  await page.evaluate(() => {
    window.qaOriginalPut=IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put=function(...args) { if(this.name==='meetings') throw new DOMException('The quota has been exceeded.','QuotaExceededError'); return window.qaOriginalPut.apply(this,args); };
  });
  await page.getByRole('textbox',{name:'What was said',exact:true}).fill('A retained transcript after a failed save.');
  await page.getByRole('button',{name:'Put on the board',exact:true}).click();
  await page.locator('.save-error').waitFor();
  assert.match(await page.locator('.save-error').innerText(), /Browser storage quota is exhausted/);
  assert.match(await page.locator('.save-error').innerText(), /Do not clear this site/);
  await (await meetingAction(page, 'New meeting')).click();
  await page.locator('.transition-state').waitFor({state:'hidden'});
  assert.equal(await page.locator('image.free-scene').count(),1);
  assert.equal(await page.locator('.save-error').count(),1);
  await page.screenshot({path:`${evidence}/workbench-save-failure.png`});
  await page.evaluate(() => {IDBObjectStore.prototype.put=window.qaOriginalPut;delete window.qaOriginalPut;});
  await page.locator('.save-error').getByRole('button',{name:'Retry save',exact:true}).click();
  await page.locator('.save-state.saved').waitFor();
  assert.ok((await read())[0].lines.some(l=>l.text==='A retained transcript after a failed save.'));
  await (await meetingAction(page, 'New meeting')).click();
  await page.getByRole('heading',{name:'Just talk. The whiteboard draws itself.'}).waitFor();
  await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Transcript-only meeting content.');
  await page.getByRole('button',{name:'Put on the board',exact:true}).click();
  await page.locator('.save-state.saved').waitFor();
  await (await meetingAction(page, 'Export')).click();
  await page.getByRole('menuitemradio',{name:'Current board',exact:true}).click();
  assert.equal(await page.getByRole('menuitem',{name:/^PNG/}).isEnabled(),false);
  await page.getByRole('menuitemradio',{name:'Whole meeting',exact:true}).click();
  assert.equal(await page.getByRole('menuitem',{name:/^Markdown/}).isEnabled(),true);
  assert.equal(await page.getByRole('menuitem',{name:/^Whole meeting PDF/}).isEnabled(),true);
  const mdPromise=page.waitForEvent('download');
  await page.getByRole('menuitem',{name:/^Markdown/}).click();
  const md=await mdPromise;
  assert.match(await fs.readFile(await md.path(),'utf8'),/Transcript-only meeting content/);
  for (let i=0;i<12;i++) {
    await page.getByRole('textbox',{name:'What was said',exact:true}).fill(`Reading position ${i}. `+'Keep earlier transcript lines available while new lines arrive. '.repeat(4));
    await page.getByRole('button',{name:'Put on the board',exact:true}).click();
  }
  if (!await page.locator('.log').isVisible()) await page.getByRole('button',{name:'Transcript',exact:true}).click();
  await page.locator('.lines').hover();
  await page.mouse.wheel(0,-5000);
  await page.getByRole('button',{name:'Jump to latest',exact:true}).waitFor();
  await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Newest line while reading earlier notes.');
  await page.getByRole('button',{name:'Put on the board',exact:true}).click();
  assert.ok(await page.locator('.lines').evaluate(el=>el.scrollTop<2));
  await page.getByRole('button',{name:'1 new transcript line · Jump to latest',exact:true}).waitFor();
  await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Another new line while reading earlier notes.');
  await page.getByRole('button',{name:'Put on the board',exact:true}).click();
  await page.getByRole('button',{name:'2 new transcript lines · Jump to latest',exact:true}).waitFor();
  assert.ok(await page.locator('.lines').evaluate(el=>el.scrollTop<2));
  await page.getByRole('button',{name:/Jump to latest$/}).click();
  await page.waitForFunction(()=>{const el=document.querySelector('.lines');return el.scrollHeight-el.clientHeight-el.scrollTop<32});
  assert.equal(await page.locator('.follow-transcript').count(),0);
  assert.deepEqual(errors,[]);
  const report={layouts,freeSceneRoundTrip:true,saveFailureRecovery:true,transcriptOnlyMarkdown:true,menuKeyboard:true,preparationEntry:true,transcriptReadingPosition:true,unreadTranscriptCount:true,pageErrors:errors};
  await fs.writeFile(`${evidence}/workbench-check.json`,JSON.stringify(report,null,2)+'\n');
  return report;
};
