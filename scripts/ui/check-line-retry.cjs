const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkLineRetry(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence, {recursive:true});
  await page.addInitScript(() => { localStorage.setItem('boardcast.lang', 'en'); localStorage.removeItem('boardcast.localLlm'); });
  const errors = [], requests = []; let mode = 'fail', finish;
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (request.method() === 'POST' && request.url().includes('chat/completions')) requests.push(request.url()); });
  await page.route('http://127.0.0.1:19879/**', async route => {
    if (route.request().method() !== 'POST') return route.fulfill({status:200, json:{status:'ok'}});
    const current = mode;
    if (current === 'hold') await new Promise(resolve => {finish = resolve;});
    return route.fulfill(current === 'success'
      ? {status:200, json:{choices:[{message:{content:'N 「Recovered on original board」'}}]}}
      : {status:503, body:'Injected unavailable model'});
  });
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes,no) => {const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
    const row = await new Promise((yes,no) => {const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
    db.close();return row;
  });
  const saved = async predicate => {for(let i=0;i<80;i++){const m=await read();if(m&&predicate(m))return m;await page.waitForTimeout(100)}throw new Error('Expected retry snapshot not saved')};
  const retry = () => page.getByRole('button', {name:'Retry this line', exact:true});
  const report = {pageErrors:errors};
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state:'hidden'});
    // Two existing empty boards let us verify navigation while the model is unavailable.
    await page.evaluate(async () => {
      const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets')};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
      const graph={nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1};
      const m={version:2,storageRevision:1,id:'retry-fixture',title:'Retry fixture',titled:true,createdAt:1,updatedAt:1,graph,history:[],lines:[],pages:[{graph},{graph}],page:0};
      await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(m);tx.oncomplete=yes;tx.onerror=()=>no(tx.error)});
      db.close();localStorage.setItem('boardcast.meeting',m.id);
    });
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    await page.getByRole('textbox', {name:'What was said', exact:true}).fill('Keep this original statement for recovery.');
    await page.getByRole('button', {name:'Put on the board', exact:true}).click();
    await retry().waitFor();
    assert.equal(requests.length,0,'an unconfigured model must not POST an empty base URL');
    assert.equal(await page.locator('.line.pending').count(),0);
    await page.locator('.save-state.saved').waitFor();
    let original = await saved(m => m.lines[0]?.boardFailure === 'unavailable');
    const id = original.lines[0].id; assert.ok(id);
    await page.getByRole('button',{name:'Configure board model',exact:true}).click();
    await page.getByRole('tab',{name:'Board & models',exact:true}).click();
    await page.getByPlaceholder('http://127.0.0.1:8080').fill('http://127.0.0.1:19879');
    await page.getByRole('dialog').getByRole('button',{name:'Close',exact:true}).click();
    await retry().click();
    await saved(m => m.lines[0]?.boardFailure === 'request');
    assert.equal(requests.length,1);
    mode='hold';
    await retry().evaluate(button => {button.click();button.click();});
    await page.locator('.line.pending').waitFor();
    for(let i=0;i<50&&!finish;i++) await page.waitForTimeout(50);
    assert.ok(finish); assert.equal(requests.length,2,'rapid clicks enqueue only once');
    finish(); await retry().waitFor();
    await saved(m => m.lines[0]?.failed);
    await page.locator('.pages').getByRole('button',{name:'Board 2',exact:true}).click();
    mode='success'; await retry().click();
    const recovered = await saved(m => m.page===1 && !m.lines[0]?.failed && m.pages?.[0]?.graph.nodes.some(n=>n.label==='Recovered on original board'));
    assert.equal(requests.length,3);
    assert.equal(recovered.lines.length,1); assert.equal(recovered.lines[0].id,id); assert.equal(recovered.lines[0].page,0);
    assert.equal(recovered.graph.nodes.length,0,'the currently displayed second board is unchanged');
    assert.equal(recovered.pages[0].graph.nodes.length,1);
    assert.equal(await retry().count(),0,'a successful line has no retry action');
    await page.getByRole('button',{name:'New meeting',exact:true}).click();
    await page.getByRole('heading',{name:'Just talk. The whiteboard draws itself.',exact:true}).waitFor();
    assert.deepEqual(errors,[]);
    Object.assign(report,{unconfiguredRecorded:true,noEmptyRequest:true,repeatedFailureRetryable:true,rapidClicksDeduplicated:true,originalPageRecovered:true,oneTranscriptOneObject:true,successCannotRetry:true,passed:true});
    await fs.rm(`${evidence}/line-retry-failure.png`,{force:true});
    await page.screenshot({path:`${evidence}/line-retry-complete.png`});
    return report;
  } catch(error) {report.error=String(error);await page.screenshot({path:`${evidence}/line-retry-failure.png`});throw error;}
  finally {await fs.writeFile(`${evidence}/line-retry-check.json`,JSON.stringify(report,null,2)+'\n');}
};
