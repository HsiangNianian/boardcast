// Run in headed Chromium attached with connectOverCDP({noDefaults:true}); no host audio.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async function checkHoldFocus(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/hold-focus/browser') {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],focusEvents:[],pageErrors:[]};
  const check=(ok,label)=>report.checks.push({label,ok});
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.addInitScript(()=>{
    if(!/^https?:$/.test(location.protocol))return;
    localStorage.clear();localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.meeting','hold-focus');
    const graph={nodes:[{id:'n1',label:'Original board',kind:'node',marks:[],aliases:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1};
    const seed={version:2,storageRevision:1,id:'hold-focus',title:'Hold focus',titled:true,createdAt:1,updatedAt:1,graph,history:[],lines:[{text:'ORIGINAL',ms:1,page:0}],page:0,pages:[{graph}]};
    const q=window.qaHold={calls:[],callbacks:{},events:{},seed,original:JSON.stringify(seed),listening:false,togglePending:false,holdStart:false,startError:null,blur:0,visibility:[]};
    addEventListener('blur',()=>q.blur++);document.addEventListener('visibilitychange',()=>q.visibility.push(document.visibilityState));
    q.emit=(event,payload)=>{if(event==='mic-toggle')q.togglePending=!q.togglePending;(q.events[event]||[]).forEach(id=>q.callbacks[id]?.({event,payload}));};
    q.listen=listening=>{q.listening=listening;q.emit('voice-status',{installed:true,listening,device:'Fixture microphone'});};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>q.events[event]=(q.events[event]||[]).filter(value=>value!==id)};
    let next=0;
    window.__TAURI_INTERNALS__={transformCallback:fn=>{q.callbacks[++next]=fn;return next;},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:value=>value,invoke:async(command,args={})=>{
      q.calls.push({command,args});
      if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler;}
      if(command==='shell_take_toggle'){const pending=q.togglePending;q.togglePending=false;return pending;}
      if(command==='voice_status')return{installed:true,listening:q.listening};
      if(command==='voice_start'){if(q.holdStart)await new Promise(resolve=>q.finishStart=resolve);if(q.startError)throw q.startError;q.listen(true);return;}
      if(command==='voice_stop'){q.listen(false);return;}
      if(command==='voice_models')return{selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:true,listening:q.listening,preparing:false,autoLocales:[],activeLocale:null,apple:null,appleError:null};
      if(command==='voice_devices')return['Fixture microphone'];
      if(command==='llm_status'||command==='llm_ensure')return{state:'idle',model:'Fixture',gpus:[]};
      if(command==='remote_get')return{baseUrl:'',model:'',enabled:false};
      if(command==='models_list')return{models:[],freeBytes:50e9};
      if(command==='meeting_load')return args.storageVersion===2&&args.id===seed.id?JSON.stringify(q.seed):null;
      if(command==='meeting_save'){q.seed=JSON.parse(args.json);return;}
      if(command==='meetings_list')return[];
      if(command==='plugin:app|version')return'0.21.0-fixture';
      if(command==='session_wayland')return false;
      return null;
    }};
  });
  const away=await page.context().newPage();await away.goto('about:blank');
  const count=command=>page.evaluate(command=>window.qaHold.calls.filter(c=>c.command===command).length,command);
  const canvasFocus=()=>page.evaluate(()=>document.activeElement?.blur());
  const load=async()=>{
    await page.bringToFront();await page.goto(baseUrl);await page.locator('.meeting-title').filter({hasText:'Hold focus'}).waitFor();await page.locator('.splash').waitFor({state:'hidden'});
    await page.waitForFunction(()=>window.qaHold.events['voice-status']?.length&&window.qaHold.events['mic-toggle']?.length);await canvasFocus();
  };
  const waitStopped=()=>page.waitForFunction(()=>!window.qaHold.listening,null,{polling:50,timeout:1600}).then(()=>true,()=>false);
  const blur=async label=>{
    const before=await page.evaluate(()=>window.qaHold.blur);await away.bringToFront();
    await page.waitForFunction(before=>window.qaHold.blur>before,before,{polling:50,timeout:3000});
    report.focusEvents.push({label,...await page.evaluate(()=>({blur:window.qaHold.blur,hasFocus:document.hasFocus(),visibility:[...window.qaHold.visibility]}))});
  };
  const unhold=async()=>{await page.bringToFront();await page.keyboard.up('Space');};
  try {
    await load();await page.keyboard.down('Space');await page.getByRole('button',{name:'Stop listening',exact:true}).waitFor();
    await page.keyboard.down('Space');check(await count('voice_start')===1,'holding/repeating bare Space opens one capture');
    await page.keyboard.up('Space');check(await waitStopped(),'normal release stops held capture');
    await page.getByRole('button',{name:'Start listening',exact:true}).waitFor();

    for(const modifier of ['Control','Alt','Meta','Shift']){
      await load();await page.keyboard.press(`${modifier}+Space`);await page.waitForTimeout(400);
      check(await count('voice_start')===0,`${modifier}+Space does not invoke hold-to-talk`);
    }

    await load();await page.keyboard.down('Space');await page.getByRole('button',{name:'Stop listening',exact:true}).waitFor();
    await blur('held capture');check(await waitStopped(),'moving focus away stops held capture without a Space keyup');
    await unhold();await page.waitForTimeout(400);
    check(await count('voice_stop')===1,'focus loss and later keyup release held capture once');
    check(await page.evaluate(()=>JSON.stringify(window.qaHold.seed)===window.qaHold.original),'focus cancellation preserves the original meeting');

    await load();await page.evaluate(()=>window.qaHold.holdStart=true);await page.keyboard.down('Space');await page.waitForFunction(()=>typeof window.qaHold.finishStart==='function');
    await blur('pending capture');await page.evaluate(()=>window.qaHold.finishStart());
    await page.waitForFunction(()=>window.qaHold.calls.some(c=>c.command==='voice_stop')||window.qaHold.listening,null,{polling:50});
    check(await waitStopped(),'a start resolving after focus loss is stopped');await unhold();

    await load();await page.keyboard.down('Space');await page.getByRole('button',{name:'Stop listening',exact:true}).waitFor();await page.keyboard.up('Space');
    await blur('release grace period');check(await waitStopped(),'focus loss during the release grace period stops capture');await page.waitForTimeout(450);
    check(await count('voice_stop')===1,'the old release timer does not stop again');
    await page.bringToFront();await canvasFocus();await page.keyboard.down('Space');await page.getByRole('button',{name:'Stop listening',exact:true}).waitFor();
    check(await count('voice_start')===2,'returning after focus cancellation permits a fresh explicit hold');await page.keyboard.up('Space');await waitStopped();

    await load();await page.getByRole('button',{name:'Start listening',exact:true}).click();await page.getByRole('button',{name:'Stop listening',exact:true}).waitFor();
    await blur('continuous capture');await page.waitForTimeout(450);
    check(await page.evaluate(()=>window.qaHold.listening)&&await count('voice_stop')===0,'focus loss preserves capture started by the ordinary button');
    await page.bringToFront();await page.getByRole('button',{name:'Stop listening',exact:true}).click();await waitStopped();

    await load();await page.keyboard.down('Control');await page.keyboard.down('Shift');await page.keyboard.down('Space');
    await page.evaluate(()=>window.qaHold.emit('mic-toggle'));
    await page.keyboard.up('Space');await page.keyboard.up('Shift');await page.keyboard.up('Control');await page.waitForTimeout(450);
    check(await page.evaluate(()=>window.qaHold.listening)&&await count('voice_start')===1&&await count('voice_stop')===0,'native toggle combined with Ctrl+Shift+Space stays a continuous capture');

    await load();await page.getByRole('textbox',{name:'What was said',exact:true}).focus();await page.keyboard.press('Space');
    check(await count('voice_start')===0,'Space in the transcript input stays text input');
    await page.getByRole('button',{name:'Settings',exact:true}).click();await page.keyboard.press('Space');
    check(await count('voice_start')===0,'Space on a dialog button does not start capture');
    await page.keyboard.press('Escape');await page.getByRole('dialog').waitFor({state:'hidden'});await canvasFocus();
    await page.evaluate(()=>document.body.dispatchEvent(new KeyboardEvent('keydown',{key:' ',code:'Space',bubbles:true,isComposing:true})));
    check(await count('voice_start')===0,'IME composition Space does not start capture');
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready="true"]').waitFor();await canvasFocus();await page.keyboard.press('Space');
    check(await count('voice_start')===0,'Space in Free edit is left to the canvas');

    await load();await page.keyboard.press('?');const help=page.getByRole('dialog',{name:'Keyboard shortcuts',exact:true});await help.waitFor();
    check((await help.innerText()).includes('leaving the window'),'keyboard help explains focus-loss stopping');
    await page.screenshot({path:path.join(evidence,'hold-help-en.png')});
    check(!report.pageErrors.length,'no production JavaScript errors');
  } finally {await unhold().catch(()=>{});await away.close();await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2));}
  assert(report.checks.every(c=>c.ok),JSON.stringify(report.checks.filter(c=>!c.ok)));
  console.log(`hold-focus: ${report.checks.length} checks`);
};
