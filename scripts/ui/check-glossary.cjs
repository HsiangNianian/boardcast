const {click: clickMeetingAction} = require('./meeting-action.cjs');
// Production App, actual localStorage/IndexedDB and typed input; model replies are synthetic.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

module.exports = async function checkGlossary(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/glossary-current') {
  await fs.mkdir(evidence, {recursive:true});
  const report={baseUrl,checks:[],pageErrors:[],modelGlossaries:[]};
  const check=(ok,label)=>{assert.ok(ok,label);report.checks.push(label);};
  page.on('pageerror',error=>report.pageErrors.push(error.message));
  await page.addInitScript(()=>{
    localStorage.setItem('boardcast.lang',new URL(location.href).searchParams.get('glossaryLang')||'en');
    localStorage.setItem('boardcast.log','open');localStorage.setItem('boardcast.localLlm','http://127.0.0.1:19894');
  });
  await page.route('http://127.0.0.1:19894/**',async route=>{
    if(route.request().method()==='POST') {
      const body=route.request().postDataJSON();
      report.modelGlossaries.push(body.messages.at(-1).content.split('\n')[0]);
      await route.fulfill({json:{choices:[{message:{content:'N 「Glossary board」'}}]}});
    } else await route.fulfill({json:{status:'ok',data:[{id:'glossary-fixture'}]}});
  });
  const read=()=>page.evaluate(async()=>{
    if(!(await indexedDB.databases()).some(d=>d.name==='boardcast-v2'))return [];
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const rows=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return rows;
  });
  const stored=async(predicate)=>{
    for(let n=0;n<100;n++){const rows=await read(),row=rows.find(predicate);if(row)return row;await page.waitForTimeout(100);}
    throw Error('Expected meeting state was not persisted within 10 seconds');
  };
  const settings=page.getByRole('dialog',{name:'Settings',exact:true});
  const globalField=settings.getByRole('textbox',{name:'All meetings',exact:true});
  const localField=settings.getByRole('textbox',{name:'This meeting only',exact:true});
  const open=async()=>{await clickMeetingAction(page, 'Settings');await settings.getByRole('tab',{name:'Terminology & materials',exact:true}).click();};
  const close=()=>settings.getByRole('button',{name:'Close',exact:true}).click();
  const library=page.getByRole('dialog',{name:'Meetings',exact:true});
  const openLibrary=()=>page.locator('.meeting-library').click();
  const sourceRow=()=>library.locator('.meeting-list > li').filter({has:page.locator('.title',{hasText:/^Glossary source$/})});
  const say=async(text)=>{await page.getByRole('textbox',{name:'What was said',exact:true}).fill(text);await page.getByRole('button',{name:'Put on the board',exact:true}).click();};
  const currentId=()=>page.evaluate(()=>localStorage.getItem('boardcast.meeting'));
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    report.productionScripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    await open();await localField.fill('BeforeCall = before call');await close();
    const first=await stored(m=>m.glossary==='BeforeCall = before call');
    check(first.lines.length===0&&first.graph.nodes.length===0,'meeting terminology alone is saved before the first transcript or board');
    check(await page.getByRole('button',{name:'Export',exact:true}).isDisabled(),'terminology alone does not enable an empty meeting export');
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open();
    check(await localField.inputValue()==='BeforeCall = before call','cold reload restores terminology entered before a meeting starts');
    await localField.fill('');await close();await stored(m=>m.id===first.id&&!m.glossary);
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open();
    check(await localField.inputValue()==='','clearing the only meeting content stays cleared after reload');
    await page.evaluate(()=>{window.qaGlossaryPut=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(...args){if(this.name==='meetings')throw new DOMException('Injected glossary save failure','QuotaExceededError');return window.qaGlossaryPut.apply(this,args);};});
    const draft='BeforeCall = before call\nDraft = draft alias';await localField.fill(draft);await close();await page.locator('.save-error').waitFor();
    await openLibrary();await library.locator('.meeting-list > li.current').getByText('Unsaved changes',{exact:true}).waitFor();
    check(true,'a terminology-only failed save remains visible as the current unsaved meeting');
    await library.getByRole('button',{name:'Close',exact:true}).click();await clickMeetingAction(page, 'New meeting');await page.locator('.transition-state').waitFor({state:'hidden'});
    check(await currentId()===first.id,'save failure prevents switching away from terminology-only edits');
    await open();check(await localField.inputValue()===draft,'failed save retains the complete editable terminology draft');await close();
    await page.evaluate(()=>{IDBObjectStore.prototype.put=window.qaGlossaryPut;delete window.qaGlossaryPut;});
    await page.locator('.save-error').getByRole('button',{name:'Retry save',exact:true}).click();await stored(m=>m.id===first.id&&m.glossary===draft);
    check(true,'Retry save persists the original terminology draft after storage recovers');

    const globalBad='Core = raw\nraw = probe\nC# = c sharp\nBroken = cash = ignored\n= orphan\nEverywhere = universal\nEverywhere = universal, overall\nFirst = shared\nSecond = shared';
    const localBad='LocalName = scoped\nLocalName = scoped, local alias\n= local orphan';
    const globalGood=globalBad.split('\n').filter((_,i)=>i!==3&&i!==4).join('\n'),localGood=localBad.split('\n').slice(0,2).join('\n');
    await open();await globalField.fill(globalBad);await localField.fill(localBad);
    check(await globalField.getAttribute('aria-invalid')==='true'&&await localField.getAttribute('aria-invalid')==='true','each glossary marks its own malformed lines without erasing the text');
    for(const [field,expected] of [[globalField,'Lines 4, 5 are not applied.'],[localField,'Lines 3 are not applied.']]) {
      check(await field.evaluate((el,expected)=>document.getElementById(el.getAttribute('aria-describedby'))?.textContent.startsWith(expected),expected),'the field describes its own exact invalid line numbers');
    }
    await page.screenshot({path:`${evidence}/glossary-invalid-lines.png`});await close();await stored(m=>m.id===first.id&&m.glossary===localBad);
    check(await page.evaluate(()=>localStorage.getItem('boardcast.glossary'))===globalBad,'global terminology preserves malformed draft text in local preferences');
    const input='probe / raw / c sharp / cash / scoped / universal / shared';
    const corrected='raw / raw / C# / cash / LocalName / Everywhere / shared';
    await say(input);const source=await stored(m=>m.id===first.id&&m.lines.some(l=>l.text===corrected&&typeof l.ms==='number'));
    check(source.lines[0].rawText===input&&source.history[0].rawText===input,'single-pass correction preserves the original input in transcript and model history');
    check(await page.getByText(corrected,{exact:true}).getAttribute('title')===`Original transcription: ${input}`,'the visible corrected transcript exposes the original transcription');
    check(report.modelGlossaries.includes('术语（起名照这个写法）：Core、raw、C#、Everywhere、First、Second、LocalName'),'model input merges duplicate names and excludes malformed glossary rows');
    await page.screenshot({path:`${evidence}/glossary-corrected-transcript.png`});
    await open();check(await globalField.inputValue()===globalBad&&await localField.inputValue()===localBad,'closing settings and processing input preserve both complete drafts');
    await globalField.fill(globalGood);await localField.fill(localGood);
    check(await settings.locator('[aria-invalid="true"]').count()===0&&await settings.getByRole('status').count()===0,'repairing the rows clears their validation messages');await close();await stored(m=>m.id===first.id&&m.glossary===localGood);
    check((await read()).find(m=>m.id===first.id).lines[0].text===corrected,'editing terminology does not rewrite previously stored transcript text');
    await openLibrary();await library.locator('.meeting-list > li.current').getByRole('button',{name:'Rename',exact:true}).click();await library.getByRole('textbox',{name:'Meeting title',exact:true}).fill('Glossary source');await library.getByRole('button',{name:'Save title',exact:true}).click();await library.getByRole('textbox',{name:'Meeting title',exact:true}).waitFor({state:'hidden'});await library.getByRole('button',{name:'Close',exact:true}).click();
    const original=await stored(m=>m.id===first.id&&m.title==='Glossary source');await fs.writeFile(`${evidence}/source-before-continue.json`,JSON.stringify(original,null,2)+'\n');
    await clickMeetingAction(page, 'New meeting');await page.waitForFunction(id=>localStorage.getItem('boardcast.meeting')!==id,first.id);
    await open();check(await localField.inputValue()===''&&await globalField.inputValue()===globalGood,'a new meeting clears only its own terminology while global terminology remains');await close();
    await say('scoped / universal');const otherId=await currentId();await stored(m=>m.id===otherId&&m.lines.some(l=>l.text==='scoped / Everywhere'&&typeof l.ms==='number'));
    check(true,'a different meeting applies global aliases without leaking the source meeting aliases');
    await openLibrary();await sourceRow().getByRole('button',{name:'Continue on it',exact:true}).click();await library.waitFor({state:'hidden'});const continuedId=await currentId();
    const continued=await stored(m=>m.id===continuedId&&m.glossary===localGood);
    check(continuedId!==first.id&&continuedId!==otherId&&continued.pages[0].base.nodes.length>0,'Continue on an archived meeting creates a new meeting with its board and terminology');
    await open();check(await localField.inputValue()===localGood,'continued meeting settings show the inherited local terminology');await localField.fill('ForkName = scoped');await close();await say('scoped');await stored(m=>m.id===continuedId&&m.glossary==='ForkName = scoped'&&m.lines.some(l=>l.text==='ForkName'&&l.rawText==='scoped'&&typeof l.ms==='number'));
    const after=(await read()).find(m=>m.id===first.id);await fs.writeFile(`${evidence}/source-after-fork.json`,JSON.stringify(after,null,2)+'\n');
    check(JSON.stringify(after)===JSON.stringify(original),'editing the continued meeting leaves every source meeting field unchanged');
    await openLibrary();await sourceRow().locator('button.open').click();await library.waitFor({state:'hidden'});await open();
    check(await localField.inputValue()===localGood&&await globalField.inputValue()===globalGood,'reopening the source restores its own terminology without inheriting edits from the fork');
    await localField.fill('CurrentDraft = current alias');await close();await openLibrary();await sourceRow().getByRole('button',{name:'Continue on it',exact:true}).click();await library.waitFor({state:'hidden'});const currentForkId=await currentId();
    await stored(m=>m.id===currentForkId&&m.glossary==='CurrentDraft = current alias');
    check(currentForkId!==first.id,'Continue on the current meeting carries its latest terminology through the save barrier');
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await open();
    check(await localField.inputValue()==='CurrentDraft = current alias'&&await globalField.inputValue()===globalGood,'cold reopening the continued meeting retains inherited terminology and global preferences');await close();
    await fs.writeFile(`${evidence}/saved-meetings.json`,JSON.stringify(await read(),null,2)+'\n');

    await page.setViewportSize({width:640,height:400});await page.goto(baseUrl+'?glossaryLang=zh');await page.locator('.splash').waitFor({state:'hidden'});
    await clickMeetingAction(page, '设置');const zh=page.getByRole('dialog',{name:'设置',exact:true});await zh.getByRole('combobox',{name:'设置分类',exact:true}).selectOption('glossary');
    const zhGlobal=zh.getByRole('textbox',{name:'所有会议',exact:true}),zhLocal=zh.getByRole('textbox',{name:'只用于这场会',exact:true});
    await zhGlobal.fill('服务 = 服物\n坏格式 = A = B');await zhLocal.fill('本会 = 本会别名\n= 缺名称');await zhGlobal.focus();await page.keyboard.press('Tab');
    check(await zhLocal.evaluate(el=>document.activeElement===el),'keyboard navigation reaches the local glossary in a 640 by 400 Chinese window');
    check(await zhLocal.evaluate(el=>document.getElementById(el.getAttribute('aria-describedby'))?.textContent.startsWith('第 2 行未生效。')),'compact Chinese validation is linked to the field and names the bad line');
    await zh.evaluate(el=>Promise.all(el.getAnimations({subtree:true}).filter(a=>Number.isFinite(a.effect.getComputedTiming().endTime)).map(a=>a.finished.catch(()=>{}))));
    check(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)&&await zhLocal.evaluate(el=>{const r=el.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;}),'the focused terminology editor stays reachable without horizontal page overflow');
    await page.screenshot({path:`${evidence}/glossary-chinese-compact.png`});
    check(report.pageErrors.length===0,'all glossary workflows have no browser script errors');report.passed=true;return report;
  }catch(error){report.error=String(error);await page.screenshot({path:`${evidence}/glossary-failure.png`});throw error;}
  finally{await fs.writeFile(`${evidence}/glossary-check.json`,JSON.stringify(report,null,2)+'\n');}
};
