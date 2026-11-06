const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkErrorLocalization(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/error-localization/browser') {
  await fs.mkdir(evidence, {recursive:true});
  const report = {checks:[], messages:[], pageErrors:[]};
  const check = (ok, name) => {assert(ok,name);report.checks.push(name);};
  const read = (p,id) => p.evaluate(async id => {
    const db = await new Promise((yes,no) => {const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row = await new Promise((yes,no) => {const r=db.transaction('meetings').objectStore('meetings').get(id);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    db.close();return row;
  },id);
  try {
    for (const lang of ['zh','en']) {
      const zh = lang === 'zh';
      const ctx = await page.context().browser().newContext({locale:zh?'zh-CN':'en-US',viewport:{width:1280,height:800},reducedMotion:'reduce'});
      ctx.on('page',p=>p.on('pageerror',e=>report.pageErrors.push(e.message)));
      await ctx.addInitScript(lang=>{
        if (!/^https?:$/.test(location.protocol)) return;
        localStorage.setItem('boardcast.lang',lang);
        localStorage.setItem('boardcast.log','open');
        localStorage.setItem('boardcast.localLlm','http://127.0.0.1:19879');
      },lang);
      await ctx.route('http://127.0.0.1:19879/**',r=>r.fulfill({status:200,contentType:'application/json',body:JSON.stringify(r.request().method()==='POST'?{choices:[{message:{content:'N 「Retained board item」'}}]}:{status:'ok',data:[{id:'error-fixture'}]})}));
      const p = await ctx.newPage();
      try {
        await p.goto(baseUrl);await p.locator('.splash').waitFor({state:'hidden'});
        await p.evaluate(async()=>{
          const graph={nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1};
          const createdAt=Date.now()-1000;
          const base={version:2,storageRevision:1,id:'error-language-good',title:'Preserved meeting 中文',titled:true,createdAt,updatedAt:createdAt,graph,history:[],lines:[{text:'Original transcript 中文',ms:1}],pages:[{graph}]};
          const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
          await new Promise((yes,no)=>{
            const tx=db.transaction('meetings','readwrite'),store=tx.objectStore('meetings');
            store.put(base);store.put({...base,id:'error-language-future',version:99});store.put({...base,id:'error-language-scene',scene:{elements:null,generated:[],files:{}}});
            tx.oncomplete=yes;tx.onabort=()=>no(tx.error);
          });db.close();
        });
        report.productionScripts = await p.locator('script[src]').evaluateAll(s=>s.map(s=>s.src));
        for(const [id,reason] of [
          ['error-language-future',zh?'这场会议由新版 Boardcast 保存。请更新 Boardcast 后再打开，原件未改动。':'This meeting was saved by a newer Boardcast version. Update Boardcast to open it; its original is unchanged.'],
          ['error-language-scene',zh?'保存的自由编辑内容格式不正确。':'Invalid saved board scene'],
        ]) {
          const before = await read(p,id);
          await p.evaluate(id=>localStorage.setItem('boardcast.meeting',id),id);await p.reload();
          await p.locator('.toast.err').waitFor();
          const text = await p.locator('.toast.err > span').innerText();
          report.messages.push({lang,case:id,text});
          check(text === `${zh?'打开会议失败：':"Couldn't open the meeting: "}${reason}`,`${lang}: startup reports ${id} reason`);
          assert.deepEqual(await read(p,id),before);check(true,`${lang}: rejected original ${id} is unchanged`);
          await p.locator('.splash').waitFor({state:'hidden'});await p.screenshot({path:path.join(evidence,`${lang}-${id}.png`)});
        }
        await p.evaluate(()=>localStorage.setItem('boardcast.meeting','error-language-good'));await p.reload();
        await p.locator('.meeting-title').filter({hasText:'Preserved meeting 中文'}).waitFor();await p.locator('.splash').waitFor({state:'hidden'});
        const original = await read(p,'error-language-good');
        const dialog = p.getByRole('dialog',{name:zh?'导入材料':'Import materials',exact:true});
        const bad = JSON.stringify({elements:[],files:{},boardcast:{version:2,graph:{nodes:[]},generated:[]}});
        await p.locator('input[type=file]').setInputFiles({name:'broken.excalidraw',mimeType:'application/json',buffer:Buffer.from(bad)});
        await dialog.getByRole('alert').waitFor();
        const importText = await dialog.getByRole('alert').innerText();report.messages.push({lang,case:'import',text:importText});
        check(importText === (zh?'导入失败：绘图中的 Boardcast 结构数据无效。':'Import failed: Invalid Boardcast graph'),`${lang}: invalid graph reason is visible`);
        assert.deepEqual(await read(p,'error-language-good'),original);check(true,`${lang}: failed import does not change the meeting`);
        await dialog.getByRole('button',{name:zh?'重新导入':'Try importing again',exact:true}).click();await dialog.getByRole('alert').waitFor();
        assert.deepEqual(await read(p,'error-language-good'),original);check(true,`${lang}: retry rejects the same invalid file without writes`);
        await p.screenshot({path:path.join(evidence,`${lang}-import.png`)});
        await dialog.getByRole('button',{name:zh?'取消导入':'Cancel import',exact:true}).click();await dialog.waitFor({state:'hidden'});
        const other = await ctx.newPage();await other.goto(baseUrl);
        await other.locator('.meeting-title').filter({hasText:'Preserved meeting 中文'}).waitFor();await other.locator('.splash').waitFor({state:'hidden'});
        const late = 'Keep my late unsaved transcript 中文';
        await p.locator('textarea[aria-label]').fill(late);
        await other.locator('textarea[aria-label]').fill('Saved in the other window 中文');await other.getByRole('button',{name:zh?'写上白板':'Put on the board',exact:true}).click();
        await other.locator('.lines .pending').waitFor({state:'hidden'});await other.locator('.save-state.saved').waitFor();
        await other.waitForFunction(async()=>{
          const db=await new Promise(yes=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);});
          const row=await new Promise(yes=>{const r=db.transaction('meetings').objectStore('meetings').get('error-language-good');r.onsuccess=()=>yes(r.result);});db.close();
          return row?.lines.some(l=>l.text==='Saved in the other window 中文');
        });
        const savedOther = await read(p,'error-language-good');
        await p.getByRole('button',{name:zh?'写上白板':'Put on the board',exact:true}).click();await p.locator('.lines .pending').waitFor({state:'hidden'});await p.locator('.save-error').waitFor();
        const saveText = await p.locator('.save-error').innerText();report.messages.push({lang,case:'conflict',text:saveText});
        check(saveText.includes(zh?'这场会议已在其他窗口更改，无法覆盖保存。当前修改仍在这里，可先导出副本，再重新打开会议。':'This meeting changed in another window. Reopen it before saving; your current edits are still here.'),`${lang}: cross-window conflict reason is visible`);
        await p.locator('.save-error').getByRole('button',{name:zh?'重试保存':'Retry save',exact:true}).click();await p.locator('.save-error').waitFor();
        await p.getByRole('button',{name:zh?'新会议':'New meeting',exact:true}).click();await p.locator('.transition-state').waitFor({state:'hidden'});
        check((await p.locator('.meeting-title').innerText()).includes('Preserved meeting 中文'),`${lang}: failed flush blocks switching`);
        check(await p.locator('.lines').getByText(late,{exact:true}).count()===1,`${lang}: late edits remain visible`);
        assert.deepEqual(await read(p,'error-language-good'),savedOther);check(true,`${lang}: conflict and retries preserve the other window's stored revision`);
        await p.setViewportSize({width:640,height:400});
        const rescue = p.locator('.save-error').getByRole('button',{name:zh?'导出副本':'Export a copy',exact:true});
        await p.waitForFunction(()=>{
          const button=document.querySelector('.save-error > button:last-child');
          if(!button)return false;
          button.scrollIntoView({block:'center'});
          const b=button.getBoundingClientRect();
          return b.x>=0&&b.y>=0&&b.right<=innerWidth&&b.bottom<=innerHeight;
        });
        const bounds = await rescue.boundingBox();
        check(bounds && bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= 640 && bounds.y + bounds.height <= 400,`${lang}: recovery button fits the 640x400 viewport after scrolling`);
        await p.screenshot({path:path.join(evidence,`${lang}-conflict-small.png`)});await p.setViewportSize({width:1280,height:800});
        const pending = p.waitForEvent('download',{timeout:60000});await p.locator('.save-error').getByRole('button',{name:zh?'导出副本':'Export a copy',exact:true}).click();
        const download=await pending,file=path.resolve(evidence,`${lang}-recovery.html`);await download.saveAs(file);
        const offline=await page.context().browser().newContext({locale:zh?'en-US':'zh-CN',reducedMotion:'reduce'});
        try {
          const requests=[];await offline.route(/^https?:\/\//,r=>{requests.push(r.request().url());return r.abort();});await offline.setOffline(true);
          const player=await offline.newPage();player.on('pageerror',e=>report.pageErrors.push(e.message));await player.goto(pathToFileURL(file).href);
          await player.locator('.player .ink > svg').waitFor();await player.waitForFunction(text=>document.querySelector('.player .lines')?.textContent.includes(text),late);
          check(await player.getByRole('button',{name:zh?'播放':'Play',exact:true}).isVisible(),`${lang}: offline recovery retains exported language`);
          const data=await player.evaluate(()=>window.__boardcastData);
          check(data.lines.some(l=>l.text===late)&&data.lines.some(l=>l.text==='Original transcript 中文'),`${lang}: offline copy retains original and unsaved transcript`);
          check(!requests.length,`${lang}: recovery opens without network`);
          await player.screenshot({path:path.join(evidence,`${lang}-offline-recovery.png`)});
        } finally {await offline.close();}
        assert.deepEqual(await read(p,'error-language-good'),savedOther);check(true,`${lang}: rescue export does not overwrite storage`);
      } catch(error) {report.failedLang=lang;report.aria=await p.locator('body').ariaSnapshot();await p.screenshot({path:path.join(evidence,`${lang}-failure.png`)}).catch(()=>{});throw error;}
      finally {await ctx.close();}
    }
    check(!report.pageErrors.length,'no browser script errors');report.passed=true;return report;
  } catch(error) {report.error=String(error);throw error;}
  finally {await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2)+'\n');}
};
