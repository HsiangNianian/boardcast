const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');

// Production App and real meeting storage; clipboard replies are controlled.
module.exports=async function checkSummaryCopy(page,baseUrl,evidence){
 const report={baseUrl,checks:[],failures:[],pageErrors:[]};
 const check=(ok,label)=>{report[ok?'checks':'failures'].push(label);console.log(`${ok?'PASS':'FAIL'}: ${label}`);};
 const summary=()=>page.getByRole('region',{name:'Meeting summary',exact:true});
 const action=name=>summary().getByRole('button',{name,exact:true});
 const fallback=()=>summary().getByRole('textbox',{name:'Text to copy manually',exact:true});
 const settle=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 const open=async title=>{
  await page.locator('.meeting-library').click();
  const library=page.getByRole('dialog',{name:'Meetings',exact:true});
  await library.locator('.meeting-list > li').filter({has:page.locator('.title',{hasText:title})}).locator('button.open').click();
  await library.waitFor({state:'hidden'});await page.locator('.meeting-title').filter({hasText:title}).waitFor();
  await page.getByRole('tab',{name:'Summary',exact:true}).click();await summary().waitFor();
 };
 const stored=()=>page.evaluate(async()=>{
  const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
  const rows=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return JSON.stringify(rows);
 });
 page.on('pageerror',error=>report.pageErrors.push(error.message));
 await page.addInitScript(()=>{
  localStorage.setItem('boardcast.lang','en');
  const q=window.qaSummaryCopy={mode:'legacy',pending:[],writes:[],legacy:[]};
  Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:text=>{q.writes.push(text);return q.mode==='held'?new Promise((resolve,reject)=>q.pending.push({resolve,reject})):Promise.reject(Error('Injected clipboard denial'));}}});
  document.execCommand=command=>{if(command==='copy')q.legacy.push(document.activeElement?.value);return q.mode==='legacy';};
 });
 try{
  await fs.mkdir(evidence,{recursive:true});await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
  report.scripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
  await page.evaluate(async()=>{
   const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
   const graph={nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1};
   const rows=['A','B'].map(key=>({version:2,storageRevision:1,id:`qa-copy-${key}`,title:`Copy meeting ${key}`,titled:true,createdAt:1,updatedAt:2,graph,history:[],lines:[{id:`line-${key}`,text:`Meeting ${key} source`,speaker:'A',ms:1}],summary:`### Summary ${key}\n- CONTENT_ONLY_${key}`}));
   await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');rows.forEach(row=>tx.objectStore('meetings').put(row));tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();localStorage.setItem('boardcast.meeting',rows[0].id);
  });
  await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.getByRole('tab',{name:'Summary',exact:true}).click();
  const before=await stored();
  await action('Copy source').click();await settle();
  check(await page.evaluate(()=>window.qaSummaryCopy.legacy.at(-1)==='### Summary A\n- CONTENT_ONLY_A'),'legacy clipboard fallback receives complete Markdown');
  check(await action('Copy source').evaluate(el=>document.activeElement===el),'legacy clipboard success preserves the copy button focus');
  await page.evaluate(()=>{window.qaSummaryCopy.mode='fail';});await action('Text only').click();await fallback().waitFor();
  check(await fallback().evaluate(el=>el.value.includes('CONTENT_ONLY_A')&&!el.value.includes('###')&&document.activeElement===el&&el.selectionStart===0&&el.selectionEnd===el.value.length),'failed copy selects the requested plain text for manual copying');
  await action('Close copy text').click();await settle();
  check(await action('Text only').evaluate(el=>document.activeElement===el),'closing manual copy returns focus to its exact copy action');
  await action('Copy source').click();await fallback().waitFor();await action('Edit summary').click();await open('Copy meeting B');
  check(await fallback().count()===0,'manual copy content from the previous meeting does not appear in another meeting');
  check(await summary().getByRole('textbox',{name:'Edit summary',exact:true}).count()===0,'summary editing state resets when opening another meeting');
  await page.screenshot({path:path.join(evidence,'after-switch.png')});
  for(const success of [false,true]){
   await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open('Copy meeting A');
   await page.evaluate(()=>{window.qaSummaryCopy.mode='held';});await action('Copy source').click();await open('Copy meeting B');
   const input=page.getByRole('textbox',{name:'What was said',exact:true});await input.fill('UNSENT_B_DRAFT');
   const legacy=await page.evaluate(()=>window.qaSummaryCopy.legacy.length);
   await page.evaluate(success=>{const p=window.qaSummaryCopy.pending.shift();if(success)p.resolve();else p.reject(Error('Late denial'));},success);await settle();
   check(await fallback().count()===0&&await input.evaluate(el=>document.activeElement===el&&el.value==='UNSENT_B_DRAFT'),`late clipboard ${success?'success':'failure'} leaves the new meeting and its input focus alone`);
   check(await page.evaluate(()=>window.qaSummaryCopy.legacy.length)===legacy,`late clipboard ${success?'success':'failure'} cannot run legacy copy in another meeting`);
   check(!(await page.locator('.toast').allTextContents()).join('').match(/copied|copy failed|clipboard/i),`late clipboard ${success?'success':'failure'} does not show a stale copy notification`);
  }
  await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open('Copy meeting A');await page.evaluate(()=>{window.qaSummaryCopy.mode='held';});
  await action('Copy source').click();await action('Text only').click();
  await page.evaluate(()=>window.qaSummaryCopy.pending[1].reject(Error('Second copy denied')));await fallback().waitFor();
  await page.evaluate(()=>window.qaSummaryCopy.pending[0].reject(Error('First copy denied')));await settle();
  check(!(await fallback().inputValue()).includes('###')&&await page.evaluate(()=>window.qaSummaryCopy.legacy.length)===1,'an older copy cannot replace the latest requested format or repeat legacy copy');
  await action('Close copy text').click();await page.evaluate(()=>{window.qaSummaryCopy.pending=[];});await action('Copy source').click();
  const input=page.getByRole('textbox',{name:'What was said',exact:true});await input.fill('NEW_FOCUS_IN_SAME_MEETING');
  await page.evaluate(()=>window.qaSummaryCopy.pending[0].reject(Error('Delayed denial in same meeting')));await fallback().waitFor();await settle();
  check(await input.evaluate(el=>document.activeElement===el&&el.value==='NEW_FOCUS_IN_SAME_MEETING'),'delayed copy failure preserves newer input focus within the same meeting');
  check((await fallback().inputValue()).includes('### Summary A'),'delayed manual fallback still contains the requested Markdown');
  check((await summary().locator('.summary-copy-fallback').innerText()).includes('Select the text below'),'manual-copy guidance remains accurate when newer focus prevents automatic selection');
  check(await stored()===before,'copy, cancellation and meeting changes preserve both saved meetings');
  await action('Delete summary').click();await action('Undo deletion').waitFor();await action('Close copy text').click();await settle();
  check(await summary().evaluate(el=>document.activeElement===el)&&await action('Undo deletion').isEnabled(),'closing manual copy after deletion focuses the remaining summary region');
  await action('Undo deletion').click();await settle();await page.locator('.save-state.saved').waitFor();
  const restored=JSON.parse(await stored()),original=JSON.parse(before);
  check(restored.find(m=>m.id==='qa-copy-A').summary===original.find(m=>m.id==='qa-copy-A').summary&&JSON.stringify(restored.find(m=>m.id==='qa-copy-B'))===JSON.stringify(original.find(m=>m.id==='qa-copy-B')),'undo restores the summary after copying while the other meeting stays unchanged');
  await action('Edit summary').click();await summary().getByRole('textbox',{name:'Edit summary',exact:true}).fill(Array.from({length:80},(_,i)=>`- Long summary line ${i}: 中文和日本語の記録`).join('\n'));await action('Done editing').click();
  await page.setViewportSize({width:640,height:400});await page.evaluate(()=>{window.qaSummaryCopy.mode='fail';});await action('Copy source').click();await fallback().waitFor();
  await fallback().press('Tab');check(await action('Close copy text').evaluate(el=>document.activeElement===el),'keyboard reaches manual-copy Close for a long summary in a compact window');
  await page.keyboard.press('Enter');await settle();
  report.compactFocus=await action('Copy source').evaluate(el=>{const r=el.getBoundingClientRect(),host=el.closest('.summary').getBoundingClientRect(),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return {focused:document.activeElement===el,button:r.toJSON(),host:host.toJSON(),hit:hit?.outerHTML.slice(0,300),visible:r.y>=host.y&&r.bottom<=host.bottom&&r.x>=0&&r.right<=innerWidth&&r.y>=0&&r.bottom<=innerHeight&&!!hit&&el.contains(hit)};});
  check(report.compactFocus.focused&&report.compactFocus.visible,'closing a long manual-copy panel restores visible, unobscured keyboard focus');
  await page.screenshot({path:path.join(evidence,'compact-keyboard-return.png')});
  check(report.pageErrors.length===0,'no uncaught browser errors');
 }finally{await fs.writeFile(path.join(evidence,'report.json'),JSON.stringify(report,null,2));}
 assert.equal(report.failures.length,0,report.failures.join('\n'));
};
