// Production App and real IndexedDB. Only a completed read's delivery and model replies are controlled.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async function checkStartup(page, baseUrl, evidence = '/tmp/boardcast-startup-check') {
  await fs.mkdir(evidence, {recursive:true});
  const report = {checks:[], timing:[], pageErrors:[]};
  const check = (ok, name) => {assert.ok(ok,name);report.checks.push(name);};
  page.on('pageerror', e => report.pageErrors.push(e.message));
  await page.addInitScript(() => {
    localStorage.setItem('boardcast.lang','en');
    window.qaIntro = {start:null,end:null};
    new MutationObserver(() => {
      const splash = document.querySelector('.splash');
      if (splash && window.qaIntro.start === null) window.qaIntro.start = performance.now();
      if (!splash && window.qaIntro.start !== null && window.qaIntro.end === null) window.qaIntro.end = performance.now();
    }).observe(document,{childList:true,subtree:true});
  });
  const heldFonts = [];
  try {
    await page.route(/\.woff2?(?:\?|$)/, route => new Promise(resolve => heldFonts.push(async () => {await route.continue().catch(()=>{});resolve();})));
    await page.goto(baseUrl,{waitUntil:'domcontentloaded'});
    await page.waitForFunction(() => window.qaIntro.end !== null);
    const slow = await page.evaluate(()=>({...window.qaIntro,fontStatus:document.fonts.status}));
    report.timing.push({case:'held fonts',...slow});
    check(heldFonts.length > 0 && slow.fontStatus === 'loading' && slow.end-slow.start <= 3300,'production intro closes within its three-second budget while UI fonts are still pending');
    check(await page.getByRole('button',{name:'Start a meeting',exact:true}).isEnabled(),'the workbench remains usable with fallback fonts');
    // The screenshot API normally waits for fonts; capture the actual pending-font frame directly.
    const cdp = await page.context().newCDPSession(page);
    const {data} = await cdp.send('Page.captureScreenshot');
    await fs.writeFile(path.join(evidence,'fonts-pending.png'),Buffer.from(data,'base64'));await cdp.detach();
    await Promise.all(heldFonts.splice(0).map(release=>release()));await page.unrouteAll();
    report.productionScripts = await page.locator('script[src]').evaluateAll(s=>s.map(e=>e.src));
    await page.reload();await page.locator('.splash').waitFor({state:'visible'});
    await page.keyboard.press('/');await page.locator('.splash').waitFor({state:'hidden'});
    check(await page.evaluate(()=>!document.activeElement?.matches('textarea')) && await page.getByRole('dialog').count()===0,'the intro consumes the skip key without activating the workbench shortcut');
    await page.keyboard.press('/');
    await page.waitForFunction(()=>document.activeElement?.matches('.write textarea'),null,{timeout:1000});
    check(await page.getByRole('textbox',{name:'What was said',exact:true}).evaluate(e=>document.activeElement===e),'keyboard input resumes after the intro');
    await page.reload();await page.getByRole('button',{name:'Skip intro',exact:true}).click();await page.locator('.splash').waitFor({state:'hidden'});
    check(await page.getByRole('dialog').count()===0,'clicking Skip leaves no dialog or unintended preparation action');
    await page.reload();await page.locator('.splash').waitFor({state:'visible'});await page.keyboard.press('Space');await page.locator('.splash').waitFor({state:'hidden'});
    check(await page.getByRole('dialog').count()===0 && await page.getByRole('button',{name:'Start a meeting',exact:true}).isVisible(),'Space skips the intro without starting a meeting');
    await page.reload();await page.locator('.splash').waitFor({state:'visible'});
    const settings=await page.getByRole('button',{name:'Settings',exact:true,includeHidden:true}).boundingBox();assert(settings);
    await page.mouse.click(settings.x+settings.width/2,settings.y+settings.height/2);await page.locator('.splash').waitFor({state:'hidden'});
    check(await page.getByRole('dialog').count()===0,'clicking over a background control dismisses only the intro');
    let failedFonts=0;
    await page.emulateMedia({reducedMotion:'reduce'});await page.route(/\.woff2?(?:\?|$)/,route=>{failedFonts++;return route.abort();});
    await page.reload({waitUntil:'domcontentloaded'});await page.waitForFunction(()=>window.qaIntro.end!==null);
    await page.waitForFunction(()=>document.fonts.status==='loaded');
    const reduced=await page.evaluate(()=>window.qaIntro);report.timing.push({case:'reduced motion and unavailable fonts',failedFonts,...reduced});
    check(failedFonts>0 && reduced.end-reduced.start<500,'reduced motion does not wait for animation or missing fonts');await page.unrouteAll();

    for (const scenario of ['typed-en','typed-zh','draw-delete','draft']) {
      const zh=scenario==='typed-zh';
      const context=await page.context().browser().newContext({viewport:{width:1280,height:800},reducedMotion:'reduce'});
      const p=await context.newPage();p.on('pageerror',e=>report.pageErrors.push(e.message));
      try {
        await p.addInitScript(zh=>{
          localStorage.setItem('boardcast.lang',zh?'zh':'en');
          localStorage.setItem('boardcast.mode','ink');localStorage.setItem('boardcast.mode-hint','dismissed');
          localStorage.setItem('boardcast.localLlm','http://127.0.0.1:19881');
          window.qaRestore={held:false,released:false,captured:false};
          if (!new URL(location.href).searchParams.has('holdStartup')) return;
          const get=IDBObjectStore.prototype.get;
          const complete=Object.getOwnPropertyDescriptor(IDBTransaction.prototype,'oncomplete');
          IDBObjectStore.prototype.get=function(key){
            if(this.name==='meetings' && key==='startup-original' && !window.qaRestore.captured){
              window.qaRestore.captured=true;
              Object.defineProperty(this.transaction,'oncomplete',{configurable:true,set(handler){
                complete.set.call(this,event=>{
                  window.qaRestore.held=true;
                  window.qaRestore.release=()=>{window.qaRestore.released=true;handler.call(this,event);};
                });
              }});
            }
            return get.call(this,key);
          };
        },zh);
        let reply=0;
        await p.route('http://127.0.0.1:19881/**',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(route.request().method()==='POST'?{choices:[{message:{content:`N 「Startup node ${++reply}」`}}]}:{data:[{id:'startup-fixture'}]})}));
        await p.goto(baseUrl);await p.locator('.splash').waitFor({state:'hidden'});
        const original=await p.evaluate(async()=>{
          const graph={nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1};
          const row={version:2,id:'startup-original',title:'Previous meeting',titled:true,createdAt:1,updatedAt:1,storageRevision:1,graph,history:[],lines:[{text:'Preserved original transcript',ms:0}]};
          const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
          await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(row);tx.oncomplete=yes;tx.onabort=()=>no(tx.error);});db.close();localStorage.setItem('boardcast.meeting',row.id);return row;
        });
        const rows=()=>p.evaluate(async()=>{
          const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
          const rows=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return rows;
        });
        await p.goto(baseUrl+'/?holdStartup=1');await p.waitForFunction(()=>window.qaRestore.held);await p.locator('.splash').waitFor({state:'hidden'});
        check((await rows()).length===1,`${scenario}: waiting to restore creates no empty meeting`);
        check(await p.locator('.save-state').innerText()===(zh?'正在打开上次的会议…':'Opening last meeting…'),`${scenario}: pending restore has a localized visible status`);
        for(const width of [1280,720,640]){
          await p.setViewportSize({width,height:480});
          check(await p.locator('.save-state').evaluate(e=>{const r=e.getBoundingClientRect();return r.left>=0 && r.right<=innerWidth && r.bottom<=innerHeight && document.documentElement.scrollWidth<=innerWidth;}),`${scenario}: restore feedback fits at ${width}px`);
        }
        await p.screenshot({path:path.join(evidence,scenario+'-loading.png')});await p.setViewportSize({width:1280,height:800});
        if(scenario==='draft'){
          const input=p.locator('.write textarea');await input.fill('Draft survives startup restore');
          await p.evaluate(()=>window.qaRestore.release());await p.locator('.meeting-title').filter({hasText:'Previous meeting'}).waitFor();
          check(await input.inputValue()==='Draft survives startup restore','draft: unsent input survives restoring the previous meeting');
          assert.deepEqual(await rows(),[original]);check(true,'draft: restoring with an unsent draft does not write either meeting');
          await input.press('Enter');await p.locator('.save-state.saved').waitFor();
          const saved=(await rows()).find(m=>m.id===original.id);
          check(saved.lines.length===2 && saved.lines[0].text===original.lines[0].text && saved.lines[1].text==='Draft survives startup restore','draft: explicit send appends to the restored meeting');
          continue;
        }
        if(scenario==='draw-delete'){
          await p.getByRole('radio',{name:'Free edit',exact:true}).click();await p.locator('.pencil-stage[data-ready=true]').waitFor();
          await p.getByRole('radio',{name:'Rectangle',exact:true}).locator('..').click();
          await p.mouse.move(420,300);await p.mouse.down();await p.mouse.move(640,440,{steps:12});await p.mouse.up();
          await p.locator('.save-state.saved').waitFor({timeout:4000});
          check((await rows()).some(m=>m.id!==original.id && m.scene?.elements.some(e=>e.type==='rectangle'&&!e.isDeleted)),`${scenario}: a real free edit saves while restore is pending`);
          await p.keyboard.press('Delete');await p.locator('.save-state.saved').waitFor();
        }else{
          const input=p.locator('.write textarea');
          for(const text of ['First new sentence','Second new sentence']){
            await input.fill(text);await input.press('Enter');
            check(await p.evaluate(()=>document.querySelector('.save-state.saved')!==null || !window.dispatchEvent(new Event('beforeunload',{cancelable:true}))),`${scenario}: ${text} is saved or protected against closing`);
            await p.locator('.save-state.saved').waitFor({timeout:4000});
          }
        }
        const before=await rows(),fresh=before.find(m=>m.id!==original.id);
        check(!!fresh && !await p.evaluate(()=>window.qaRestore.released),`${scenario}: new work is committed before the original read is delivered`);
        assert.deepEqual(before.find(m=>m.id===original.id),original);check(true,`${scenario}: original IndexedDB record stays unchanged`);
        if(scenario==='draw-delete')check(!fresh.scene?.elements.some(e=>!e.isDeleted) && fresh.lines.length===0,`${scenario}: deleting the last object returns the new meeting to empty`);
        else check(fresh.lines.length===2 && fresh.graph.nodes.length===2 && fresh.steps?.length===2,`${scenario}: both utterances and their replay steps persist separately`);
        await p.screenshot({path:path.join(evidence,scenario+'-pending.png')});
        await p.evaluate(()=>window.qaRestore.release());await p.waitForTimeout(750);
        check(await p.evaluate(()=>localStorage.getItem('boardcast.meeting'))===fresh.id,`${scenario}: late restore cannot replace the user's new meeting`);
        assert.deepEqual(await rows(),before);check(true,`${scenario}: delivering the old read changes neither saved record`);
        await p.goto(baseUrl);await p.locator('.splash').waitFor({state:'hidden'});await p.locator('.save-state.saved').waitFor();
        check(await p.evaluate(()=>localStorage.getItem('boardcast.meeting'))===fresh.id,`${scenario}: cold reopen selects the new saved meeting`);
        assert.deepEqual(await rows(),before);check(true,`${scenario}: cold reopen preserves the stored content and history`);
        await fs.writeFile(path.join(evidence,scenario+'-records.json'),JSON.stringify(before,null,2)+'\n');
      }catch(error){
        report.failedScenario=scenario;
        await p.screenshot({path:path.join(evidence,scenario+'-failure.png')}).catch(()=>{});
        throw error;
      }finally{await context.close();}
    }
    check(report.pageErrors.length===0,'startup flows have no browser script errors');report.passed=true;
  }catch(error){report.error=String(error);throw error;}
  finally{await Promise.all(heldFonts.map(release=>release()));await fs.writeFile(path.join(evidence,'startup-check.json'),JSON.stringify(report,null,2)+'\n');}
};
