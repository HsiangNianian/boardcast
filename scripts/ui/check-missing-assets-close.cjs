// Actual App + native exit protocol with a missing underlay. No real app exit or disk mutation.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkMissingAssetsClose(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence,{recursive:true});
  await page.addInitScript(() => {
    localStorage.clear();localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.meeting','missing-assets-fixture');
    const empty = {nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1};
    const graph = {...empty,underlay:{w:500,h:300}};
    const seed={version:2,storageRevision:1,id:'missing-assets-fixture',title:'Missing original image',titled:true,createdAt:1,updatedAt:1,graph,history:[],lines:[{text:'An archived transcript without model context',ms:1}],page:0,pages:[{graph,underlay:'asset:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png'},{graph:empty}]};
    const q=window.qaMissing={seed,original:JSON.stringify(seed),calls:[],acks:[],callbacks:{},events:{},next:0};
    q.emit=(name,payload)=>(q.events[name]||[]).forEach(id=>q.callbacks[id]?.({event:name,id,payload}));
    q.exit=()=>{const request={id:++q.next,reason:'quit'};q.emit('shell-exit-request',request);return request.id};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>{q.events[event]=(q.events[event]||[]).filter(n=>n!==id)}};
    let callback=0;
    window.__TAURI_INTERNALS__={transformCallback:fn=>{q.callbacks[++callback]=fn;return callback},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:value=>value,
      invoke:async(command,args={})=>{
        q.calls.push(command);
        if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler}
        if(command==='llm_status'||command==='llm_ensure')return{state:'ready',url:'http://127.0.0.1:19880',model:'Fixture',device:'Fixture',gpus:[]};
        if (command === 'remote_prepare') return crypto.randomUUID();
        if (command === 'llm_chat') return {choices: [{message: {content: '-'}}]};
        if(command==='voice_status')return{installed:true,listening:false};
        if(command==='voice_models')return{selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:true,listening:false,preparing:false,autoLocales:[],activeLocale:null,apple:null};
        if(command==='voice_devices')return[];
        if(command==='remote_get')return{baseUrl:'',model:'',enabled:false};
        if(command==='meeting_load')return args.storageVersion===2&&args.id===seed.id?JSON.stringify(seed):null;
        if(command==='meeting_asset_load')return null;
        if(command==='meeting_save')throw new Error('Original with missing assets must never be overwritten');
        if(command==='export_save'){q.exported=args;return '/fixture/available-text.md'}
        if(command==='meetings_list')return args.storageVersion===2?[{id:seed.id,title:seed.title,createdAt:1,updatedAt:1,lines:seed.lines.length}]:[];
        if(command==='shell_pending_exit')return null;
        if(command==='shell_resolve_exit'){q.acks.push(args);return true}
        if(command==='plugin:app|version')return'0.21.0-fixture';
        if(command==='models_list')return{models:[],freeBytes:1000000};
        if(command==='session_wayland')return false;
        return null;
      }};
  });
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  const report={pageErrors:errors};
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    await page.waitForFunction(()=>window.qaMissing.events['shell-exit-request']?.length);
    const warning=page.locator('.asset-warning');
    await warning.getByText('1 image was unavailable when opening; originals are kept',{exact:true}).waitFor();
    await warning.locator('summary').click();
    assert.match(await warning.innerText(),/File missing/);
    assert.match(await warning.innerText(),/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\.png/);
    await page.locator('.pages').getByRole('button',{name:'Board 2',exact:true}).click();
    await page.locator('.save-state.saved').waitFor();
    const readOnly=await page.evaluate(()=>window.qaMissing.exit());
    await page.waitForFunction(id=>window.qaMissing.acks.some(a=>a.id===id),readOnly);
    assert.equal(await page.evaluate(id=>window.qaMissing.acks.find(a=>a.id===id).allow,readOnly),true);
    assert.equal(await page.evaluate(()=>window.qaMissing.calls.filter(c=>c==='meeting_save').length),0);
    await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Keep this new edit despite the missing original image.');
    await page.getByRole('button',{name:'Put on the board',exact:true}).click();
    await page.locator('.save-error').waitFor();
    await warning.getByRole('button',{name:'Export available text',exact:true}).click();
    await page.waitForFunction(()=>!!window.qaMissing.exported);
    const rescued=await page.evaluate(()=>window.qaMissing.exported);
    assert.match(Buffer.from(rescued.dataBase64,'base64').toString('utf8'),/Keep this new edit despite the missing original image/);
    const edited=await page.evaluate(()=>window.qaMissing.exit());
    await page.waitForFunction(id=>window.qaMissing.acks.some(a=>a.id===id),edited);
    assert.equal(await page.evaluate(id=>window.qaMissing.acks.find(a=>a.id===id).allow,edited),false);
    assert.equal(await page.evaluate(()=>JSON.stringify(window.qaMissing.seed)===window.qaMissing.original),true);
    assert.ok((await page.locator('body').innerText()).includes('Keep this new edit despite the missing original image.'));
    assert.deepEqual(errors,[]);
    Object.assign(report,{readOnlyCloseAllowed:true,pageBrowsingDoesNotRewrite:true,missingOriginalPreserved:true,editedCloseBlocked:true,unsavedEditRetained:true,resourceWarning:true,availableTextRescue:true,passed:true});
    await page.screenshot({path:`${evidence}/missing-assets-edit-retained.png`});
    return report;
  }catch(error){report.error=String(error);await page.screenshot({path:`${evidence}/missing-assets-failure.png`});throw error;}
  finally{await fs.writeFile(`${evidence}/missing-assets-close-check.json`,JSON.stringify(report,null,2)+'\n');}
};
