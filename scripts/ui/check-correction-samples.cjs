const {click: clickMeetingAction} = require('./meeting-action.cjs');
// Real App/IndexedDB exports, then controlled desktop IPC for cancel/error/busy states.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkCorrectionSamples(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/correction-samples') {
  await fs.mkdir(evidence, {recursive:true});
  const report = {baseUrl, checks:[], pageErrors:[]};
  const check = (ok, label) => {assert.ok(ok, label);report.checks.push(label);};
  const engine = name => import(pathToFileURL(path.resolve(__dirname, '../../src/engine', name)));
  const [{Corrections}, {emptyGraph}, {parseOps, applyOps}, {rename}] = await Promise.all(['corrections.ts','graph.ts','ops.ts','edit.ts'].map(engine));
  const seen=emptyGraph(), ops=parseOps('N 「Wrong sample label」'), drawn=applyOps(seen,ops), tracker=new Corrections();
  tracker.round({at:1,seen,context:[{speaker:'A',text:'SYNTHETIC_CONTEXT'}],said:{speaker:'B',text:'SYNTHETIC_CORRECTION'},ops,drawn});
  tracker.edit(rename(drawn,{kind:'node',id:'n1'},'Correct sample label'),'Renamed synthetic label');tracker.close();
  const make = (id, count=1) => ({version:2,storageRevision:1,id,title:id,titled:true,createdAt:1,updatedAt:2,graph:seen,history:[],lines:[],corrections:Array.from({length:count},()=>structuredClone(tracker.list[0]))});
  const current=make('qa-samples-current');current.migration={sourceHash:'prior-version-source'};
  const rows=[current,...Array.from({length:25},(_,i)=>make(`qa-samples-${i}`,4))];
  const legacy={...make(current.id),version:1,title:'Preserved older copy'};
  const expectedIds=[`${current.id}-1`,`legacy:${current.id}-1`,...rows.slice(1).flatMap(row=>row.corrections.map((_,i)=>`${row.id}-${i+1}`))].sort();
  report.fixture={currentMeetings:rows.length,olderCopies:1,exportableSamples:expectedIds.length};
  const settings=page.getByRole('dialog',{name:'Settings',exact:true});
  const open = async () => {await clickMeetingAction(page, 'Settings');await settings.getByRole('tab',{name:'Maintenance & advanced',exact:true}).click();};
  const read = () => page.evaluate(async()=>{
    const records=[];
    for(const name of ['boardcast','boardcast-v2']) {
      const db=await new Promise((yes,no)=>{const r=indexedDB.open(name);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
      records.push(await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);}));db.close();
    }
    return records;
  });
  const installFailure = () => page.evaluate(()=>{
    const get=IDBObjectStore.prototype.get,getAll=IDBObjectStore.prototype.getAll;
    window.qaSamples={failure:null,reads:0};
    IDBObjectStore.prototype.get=function(key){if(this.name==='meetings'&&key==='qa-samples-7'&&window.qaSamples.failure==='read')throw new DOMException('Injected sample meeting read failure','NotReadableError');return get.call(this,key);};
    IDBObjectStore.prototype.getAll=function(...args){if(this.name==='meetings'){window.qaSamples.reads++;if(window.qaSamples.failure==='list')throw new DOMException('Injected sample list failure','NotReadableError');}return getAll.apply(this,args);};
  });
  let nativeContext;
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.addInitScript(()=>{localStorage.setItem('boardcast.lang',new URL(location.href).searchParams.get('samplesLang')||'en');localStorage.setItem('boardcast.theme','light');localStorage.setItem('boardcast.localLlm','http://127.0.0.1:19889');});
  await page.route('http://127.0.0.1:19889/**',r=>r.fulfill({json:{status:'ok',data:[]}}));
  let downloads=0;page.on('download',()=>downloads++);
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    await page.evaluate(async ({rows,legacy})=>{
      for(const [name,values] of [['boardcast',[legacy]],['boardcast-v2',rows]]) {
        const db=await new Promise((yes,no)=>{const r=indexedDB.open(name);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
        await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');for(const value of values)tx.objectStore('meetings').put(value);tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();
      }
      localStorage.setItem('boardcast.meeting',rows[0].id);
    },{rows,legacy});
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    report.productionScripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    const before=await read();
    await installFailure();await page.evaluate(()=>{window.qaSamples.failure='list';});
    await clickMeetingAction(page, 'Settings');
    check(await page.evaluate(()=>window.qaSamples.reads)===0,'opening general settings does not read every meeting');
    await settings.getByRole('tab',{name:'Maintenance & advanced',exact:true}).click();
    await settings.getByRole('alert').filter({hasText:'Injected sample list failure'}).waitFor();
    check(await settings.getByRole('button',{name:'Export eval cases',exact:true}).isDisabled()&&await settings.getByText(/^None yet/).count()===0,'list failure is an error with exports disabled, not an empty collection');
    await page.screenshot({path:`${evidence}/sample-read-error.png`});
    await page.evaluate(()=>{window.qaSamples.failure=null;});
    await settings.getByRole('button',{name:'Retry reading samples',exact:true}).focus();await page.keyboard.press('Enter');
    await settings.getByText('102 collected.',{exact:true}).waitFor();
    check(true,'keyboard retry counts 102 samples including the preserved older-version copy');
    check(await page.evaluate(()=>document.activeElement?.closest('dialog')?.getAttribute('aria-label')==='Settings'),'keyboard retry keeps focus inside settings when its button disappears');
    await page.keyboard.press('Tab');
    check(await settings.getByRole('button',{name:'Export eval cases',exact:true}).evaluate(el=>el===document.activeElement),'Tab after reading retry reaches the first export action');
    await page.evaluate(()=>{window.qaSamples.failure='read';});
    await settings.getByRole('button',{name:'Export eval cases',exact:true}).click();
    await settings.getByRole('alert').filter({hasText:'Samples were not exported.'}).waitFor();
    check(downloads===0&&await settings.getByText('Injected sample meeting read failure',{exact:false}).count()===1,'one unreadable meeting prevents a misleading partial download');
    await page.screenshot({path:`${evidence}/sample-export-error.png`});
    await page.evaluate(()=>{window.qaSamples.failure=null;});
    const download = async (button, name) => {
      const ready=page.waitForEvent('download');await settings.getByRole('button',{name:button,exact:true}).click();
      const file=await ready;await file.saveAs(`${evidence}/${name}`);return fs.readFile(`${evidence}/${name}`,'utf8');
    };
    const cases=JSON.parse(await download('Export eval cases','samples.cases.json'));
    check(JSON.stringify(cases.map(x=>x.id).sort())===JSON.stringify(expectedIds),'eval export keeps every current, archived and older-copy sample exactly once');
    check(cases.every(x=>x.gold.nodes.some(names=>names.includes('Correct sample label'))&&x.gold.forbidden_nodes.includes('Wrong sample label')&&x.turns[0].text==='SYNTHETIC_CORRECTION'&&x.history[0].text==='SYNTHETIC_CONTEXT'),'eval cases preserve the corrected target, rejected name, utterance and context');
    const pairs=(await download('Export training data','samples.dpo.jsonl')).trim().split('\n').map(JSON.parse);
    check(JSON.stringify(pairs.map(x=>x.id).sort())===JSON.stringify(expectedIds),'JSONL export contains the same 102 unique sample identities');
    check(pairs.every(x=>x.prompt.some(m=>m.content.includes('SYNTHETIC_CORRECTION'))&&x.chosen[0].content==='N 「Correct sample label」'&&x.rejected[0].content==='N 「Wrong sample label」'),'preference rows retain prompt, chosen correction and rejected output');
    await settings.getByRole('status').filter({hasText:'Sent to your browser for download:'}).waitFor();
    check(true,'web handoff reports a browser download instead of a native saved path');
    check(JSON.stringify(await read())===JSON.stringify(before),'counting and exporting leave all original IndexedDB records unchanged');
    await page.screenshot({path:`${evidence}/sample-download-success.png`});
    await settings.getByRole('button',{name:'Close',exact:true}).click();
    await page.evaluate(async()=>{
      for(const name of ['boardcast','boardcast-v2']) {
        const db=await new Promise(yes=>{const r=indexedDB.open(name);r.onsuccess=()=>yes(r.result);});
        await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').clear();tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();
      }
      localStorage.removeItem('boardcast.meeting');
    });
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open();
    await settings.getByText(/^None yet/).waitFor();
    check(await settings.getByRole('button',{name:'Export eval cases',exact:true}).isDisabled()&&await settings.getByRole('button',{name:'Export training data',exact:true}).isDisabled(),'a genuinely empty collection disables both exports');
    await page.setViewportSize({width:640,height:400});await page.goto(baseUrl+'?samplesLang=zh');await page.locator('.splash').waitFor({state:'hidden'});
    await installFailure();await page.evaluate(()=>{window.qaSamples.failure='list';});
    await clickMeetingAction(page, '设置');
    const chinese=page.getByRole('dialog',{name:'设置',exact:true});
    await chinese.getByRole('combobox',{name:'设置分类',exact:true}).selectOption('maintenance');
    await chinese.getByRole('alert').filter({hasText:'部分会议未能读取'}).waitFor();
    await chinese.evaluate(el=>Promise.all(el.getAnimations({subtree:true}).filter(a=>Number.isFinite(a.effect.getComputedTiming().endTime)).map(a=>a.finished.catch(()=>{}))));
    const retry=chinese.getByRole('button',{name:'重新读取样本',exact:true});await retry.focus();
    check(await retry.evaluate(el=>{const r=el.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth;})&&await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'Chinese error and keyboard retry remain reachable at 640 by 400');
    await page.screenshot({path:`${evidence}/sample-chinese-compact.png`});
    await page.evaluate(()=>{window.qaSamples.failure=null;});await page.keyboard.press('Enter');await chinese.getByText(/^还没有：/).waitFor();
    check(true,'Chinese retry recovers without reopening settings');

    nativeContext=await page.context().browser().newContext({viewport:{width:1280,height:800}});
    const native=await nativeContext.newPage();native.on('pageerror',e=>report.pageErrors.push(e.message));
    const nativeRows=[make('qa-native-samples-current'),make('qa-native-samples-archive')];
    await native.addInitScript(rows=>{
      localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.meeting',rows[0].id);
      const q=window.qaSamplesNative={rows,calls:[],callbacks:{},events:{},holdSave:false,saveFail:null,saveResult:null};let callback=0;
      window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>{q.events[event]=(q.events[event]||[]).filter(x=>x!==id);}};
      window.__TAURI_INTERNALS__={transformCallback:fn=>{q.callbacks[++callback]=fn;return callback;},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:v=>v,
        invoke:async(command,args={})=>{
          q.calls.push({command,args});
          if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler;}
          if(command==='llm_status'||command==='llm_ensure')return {state:'no-model',gpus:[]};
          if(command==='voice_status')return {installed:false,listening:false};
          if(command==='voice_models')return {selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:false,listening:false,preparing:false,autoLocales:[],activeLocale:null,apple:null};
          if(command==='voice_devices')return [];
          if(command==='remote_get')return {baseUrl:'',model:'',enabled:false};
          if(command==='meeting_load')return args.storageVersion===2 ? JSON.stringify(q.rows.find(r=>r.id===args.id))||null : null;
          if(command==='meetings_list')return args.storageVersion===2?q.rows.map(r=>({id:r.id,title:r.title,createdAt:1,updatedAt:2,lines:0,pages:1,unfinished:0})):[];
          if(command==='models_list')return {models:[],freeBytes:1000000};
          if(command==='plugin:app|version')return '0.21.0-fixture';
          if(command==='session_wayland')return false;
          if(command==='export_save'){const error=q.saveFail,result=q.saveResult;if(q.holdSave)await new Promise(resolve=>q.releaseSave=resolve);if(error)throw Error(error);return result;}
          return null;
        }};
    },nativeRows);
    await native.goto(baseUrl);await native.locator('.splash').waitFor({state:'hidden'});
    await clickMeetingAction(native, 'Settings');
    const panel=native.getByRole('dialog',{name:'Settings',exact:true});await panel.getByRole('tab',{name:'Maintenance & advanced',exact:true}).click();await panel.getByText('2 collected.',{exact:true}).waitFor();
    check(await native.evaluate(()=>window.qaSamplesNative.calls.some(x=>x.command==='meetings_list'&&x.args.strict===true)),'sample aggregation requests strict native listing');
    await native.evaluate(()=>{window.qaSamplesNative.holdSave=true;});
    await panel.getByRole('button',{name:'Export eval cases',exact:true}).evaluate(el=>{el.click();el.click();});
    await native.waitForFunction(()=>!!window.qaSamplesNative.releaseSave);
    await native.keyboard.press('Escape');
    check(await panel.count()===1&&await panel.getByRole('button',{name:'Close',exact:true}).isDisabled()&&await native.evaluate(()=>window.qaSamplesNative.calls.filter(x=>x.command==='export_save').length)===1,'a pending native save prevents duplicate submission and premature dialog dismissal');
    await panel.getByRole('status').filter({hasText:'Preparing samples'}).waitFor();
    await native.screenshot({path:`${evidence}/sample-native-protocol-pending.png`});
    await native.evaluate(()=>{window.qaSamplesNative.holdSave=false;window.qaSamplesNative.releaseSave();});
    await native.waitForFunction(()=>!Array.from(document.querySelectorAll('button')).find(x=>x.textContent==='Export eval cases')?.disabled);
    check(await panel.getByText(/^Exported:/).count()===0&&await panel.getByRole('alert').count()===0,'native cancellation restores actions without success or error');
    await native.evaluate(()=>{window.qaSamplesNative.saveFail='Injected native ENOSPC';});
    await panel.getByRole('button',{name:'Export training data',exact:true}).click();await panel.getByRole('alert').filter({hasText:'Injected native ENOSPC'}).waitFor();
    check(await panel.getByRole('button',{name:'Export training data',exact:true}).isEnabled(),'native save failure preserves samples and permits direct retry');
    await native.evaluate(()=>{window.qaSamplesNative.saveFail=null;window.qaSamplesNative.saveResult='/fixture/corrections.dpo.jsonl';});
    await panel.getByRole('button',{name:'Export training data',exact:true}).click();await panel.getByRole('status').filter({hasText:'Exported: /fixture/corrections.dpo.jsonl'}).waitFor();
    const calls=await native.evaluate(()=>window.qaSamplesNative.calls.filter(x=>x.command==='export_save'));
    check(calls.length===3&&calls[1].args.dataBase64===calls[2].args.dataBase64,'retry submits the same complete JSONL after a failed native save');
    report.nativeProtocolSaves=calls.map(x=>({name:x.args.name,bytes:Buffer.from(x.args.dataBase64,'base64').length}));
    check(report.pageErrors.length===0,'all sample workflows have no browser script errors');report.passed=true;return report;
  } catch(error) {report.error=String(error);await page.screenshot({path:`${evidence}/sample-failure.png`});throw error;}
  finally {await nativeContext?.close();await fs.writeFile(`${evidence}/samples-check.json`,JSON.stringify(report,null,2)+'\n');}
};
