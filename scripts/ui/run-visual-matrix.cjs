const meetingAction = require('./meeting-action.cjs');
// Actual browser zoom through an isolated temporary extension/profile. No project dependency.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const baseUrl = process.argv[2] || 'http://127.0.0.1:1424';
const chosen = process.env.MATRIX_CASE ? new RegExp(process.env.MATRIX_CASE) : null;
const evidence = process.argv[3] || (chosen ? '/tmp/boardcast-visual-matrix-filtered' : 'docs/ui-ux-audit-2026-11-03/evidence/implementation/visual-matrix');
const labels = {
  en: {settings:'Settings', close:'Close', import:'Import', board:'Board', details:'Meeting details', transcript:'Transcript', summary:'Summary', contents:'Contents', stop:'Stop listening', review:'End and review', cancel:'Cancel',speakers:'Speakers'},
  zh: {settings:'设置', close:'关闭', import:'导入', board:'白板', details:'会议详情', transcript:'原话', summary:'摘要', contents:'内容列表', stop:'停止收音', review:'结束并复盘', cancel:'取消',speakers:'说话人'},
};
const rows = [];

function nativeFixture({lang, theme}) {
  localStorage.clear();
  Object.entries({'boardcast.lang':lang,'boardcast.theme':theme,'boardcast.log':'open','boardcast.mode':'ink','boardcast.meeting':'visual-fixture'}).forEach(([k,v])=>localStorage.setItem(k,v));
  const graph = {nodes:[
    {id:'n1',label:lang==='zh'?'会议目标与决定':'Meeting goals and decisions',kind:'topic',aliases:[],marks:[],ink:'ink',by:'Alice'},
    {id:'n2',label:lang==='zh'?'整理可执行的后续事项':'Collect actionable follow-up items',kind:'item',parent:'n1',aliases:[],marks:[],ink:'ink',by:'Bob'},
  ],edges:[{from:'n1',to:'n2',label:lang==='zh'?'下一步':'Next steps',ink:'ink'}],groups:[],nextNode:3,nextGroup:1};
  const seed={version:2,storageRevision:1,id:'visual-fixture',title:lang==='zh'?'产品评审：收音、记录与后续事项的完整工作流程':'Product review: capture, notes, and follow-up across the complete workflow',titled:true,createdAt:1,updatedAt:2,graph,history:[],lines:Array.from({length:16},(_,i)=>({id:`line-${i}`,speaker:i%2?'Bob':'Alice',text:lang==='zh'?`第 ${i+1} 条原话：会议需要保留决定的上下文，让下一步工作可以继续。`:`Transcript ${i+1}: Keep the context behind this decision so the team can continue the next task.`,ms:0,page:0})),summary:lang==='zh'?'## 会议决定\n\n- 保留原话与决定的关系。\n- 明确每一项后续任务。':'## Meeting decisions\n\n- Preserve the context of each decision.\n- Identify each follow-up task.'};
  const callbacks={},events={}; let next=0;
  window.qaMatrixEmit=(event,payload)=>(events[event]||[]).forEach(id=>callbacks[id]?.({event,id,payload}));
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>{events[event]=(events[event]||[]).filter(x=>x!==id)}};
  window.__TAURI_INTERNALS__={transformCallback:f=>{callbacks[++next]=f;return next},unregisterCallback:id=>delete callbacks[id],convertFileSrc:x=>x,invoke:async(command,args={})=>{
    if(command==='plugin:event|listen'){(events[args.event]||=[]).push(args.handler);return args.handler}
    if(command==='llm_status'||command==='llm_ensure')return {state:'ready',url:'http://127.0.0.1:19889',model:'Local board model',device:'Fixture',gpus:[]};
    if (command === 'remote_prepare') return crypto.randomUUID();
    if (command === 'llm_chat') return {choices: [{message: {content: '-'}}]};
    if(command==='voice_status')return {installed:true,listening:true,device:lang==='zh'?'系统默认麦克风':'System default microphone'};
    if(command==='voice_models')return {selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:true,listening:true,preparing:false,autoLocales:[],activeLocale:null,apple:null,appleError:null};
    if(command==='voice_devices')return ['System default microphone'];
    if(command==='remote_get')return {baseUrl:'',model:'',enabled:false};
    if(command==='meeting_load')return args.storageVersion===2&&args.id===seed.id?JSON.stringify(seed):null;
    if(command==='meeting_save'){Object.assign(seed,JSON.parse(args.json));return}
    if(command==='meetings_list')return [];
    if(command==='models_list')return {models:[],freeBytes:10000000000};
    if(command==='plugin:app|version')return '0.21.0-fixture';
    if(command==='session_wayland')return false;
    return null;
  }};
}

async function inside(locator, width, height, scroll = false) {
  if(scroll)await locator.scrollIntoViewIfNeeded();
  const box=await locator.boundingBox();
  assert(box, 'required control is visible');
  assert(box.x>=-1&&box.y>=-1&&box.x+box.width<=width+1&&box.y+box.height<=height+1,`control is clipped: ${JSON.stringify(box)} in ${width}x${height}`);
}

(async()=>{
  await fs.mkdir(evidence,{recursive:true});
  const temporary=await fs.mkdtemp('/tmp/boardcast-visual-');
  const extension=path.join(temporary,'zoom-extension');
  await fs.mkdir(extension);
  await fs.writeFile(path.join(extension,'manifest.json'),JSON.stringify({manifest_version:3,name:'Boardcast isolated zoom QA',version:'1.0',permissions:['tabs'],background:{service_worker:'zoom.js'}}));
  await fs.writeFile(path.join(extension,'zoom.js'),'chrome.runtime.onInstalled.addListener(() => {});');
  try {
    for(const lang of ['en','zh'])for(const theme of ['light','dark']){
      const context=await chromium.launchPersistentContext(path.join(temporary,`${lang}-${theme}`),{executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',headless:true,viewport:{width:1280,height:800},reducedMotion:'reduce',args:['--no-sandbox',`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
      try {
        const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker');
        await context.addInitScript(nativeFixture,{lang,theme});
        const page=context.pages()[0];
        const errors=[];page.on('pageerror',e=>errors.push(e.message));
        await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
        await page.evaluate(()=>window.qaMatrixEmit('voice-status',{installed:true,listening:true,device:'System default microphone'}));
        const l=labels[lang];
        for(const [width,height] of [[1280,800],[900,600],[720,480]])for(const zoom of [1,2]){
          const name=`${lang}-${theme}-${width}x${height}-${zoom*100}`;
          if(chosen&&!chosen.test(name))continue;
          const row={name,lang,theme,width,height,zoom,passed:false};rows.push(row);
          try {
            if(await page.getByRole('dialog').count())await page.keyboard.press('Escape');
            await page.setViewportSize({width,height});
            const actualZoom=await worker.evaluate(async({baseUrl,zoom})=>{const tabs=await chrome.tabs.query({url:`${baseUrl}/*`});await chrome.tabs.setZoom(tabs[0].id,zoom);return chrome.tabs.getZoom(tabs[0].id)},{baseUrl,zoom});
            await page.waitForFunction(expected=>innerWidth===expected,Math.round(width/zoom));
            // The zoom API can report its new viewport before responsive layout commits.
            await page.evaluate(()=>document.fonts.ready);
            await page.waitForFunction(()=>{
              const board=document.querySelector('main').getBoundingClientRect(),footer=document.querySelector('.meeting-footer').getBoundingClientRect(),stop=document.querySelector('.dock .mic').getBoundingClientRect();
              return board.height>=32&&footer.bottom<=innerHeight+1&&stop.x>=-1&&stop.y>=-1&&stop.right<=innerWidth+1&&stop.bottom<=innerHeight+1;
            },null,{timeout:1500});
            await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))));
            row.metrics=await page.evaluate(()=>({innerWidth,innerHeight,outerWidth,outerHeight,dpr:devicePixelRatio,visualScale:visualViewport.scale,scrollWidth:document.documentElement.scrollWidth,boardHeight:document.querySelector('main').getBoundingClientRect().height,footer:document.querySelector('.meeting-footer').getBoundingClientRect().toJSON()}));
            assert.equal(actualZoom,zoom);assert.equal(row.metrics.dpr,zoom);assert.equal(row.metrics.visualScale,1);
            assert(row.metrics.scrollWidth<=row.metrics.innerWidth,'application shell overflows horizontally');
            assert(row.metrics.boardHeight>=32,'content viewport collapsed');
            await inside(page.getByRole('button',{name:l.stop,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight);
            assert(row.metrics.footer.bottom<=row.metrics.innerHeight+1,'footer is outside the viewport');
            const compact=row.metrics.innerWidth<=900;
            const draft=page.locator('.dock textarea');
            await draft.fill((lang==='zh'?'保留草稿，暂不发送。':'Keep this draft without sending it. ').repeat(12));
            await draft.press('Shift+Enter');
            await inside(draft,row.metrics.innerWidth,row.metrics.innerHeight,true);
            assert((await draft.inputValue()).endsWith('\n'),'Shift+Enter should retain a multiline draft');
            await draft.fill('');
            await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
            await inside(page.getByRole('button',{name:l.stop,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight,true);
            await inside(page.locator('.save-state'),row.metrics.innerWidth,row.metrics.innerHeight);
            assert(await page.locator('.save-state').evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))}),'save feedback is covered');
            const speakers=page.getByRole('button',{name:l.speakers,exact:true});
            await inside(speakers,row.metrics.innerWidth,row.metrics.innerHeight,true);await speakers.click();
            const speakerDialog=page.getByRole('dialog',{name:l.speakers,exact:true});await speakerDialog.waitFor();
            await inside(speakerDialog.getByRole('button',{name:l.cancel,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight,true);
            await page.keyboard.press('Escape');await speakerDialog.waitFor({state:'detached'});
            await inside(page.getByRole('button',{name:l.stop,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight,true);
            if(compact){await page.getByRole('tab',{name:l.board,exact:true}).click();await page.keyboard.press('End');assert.equal(await page.getByRole('tab',{name:l.details,exact:true}).getAttribute('aria-selected'),'true')}
            const tabs=page.locator('.sidebar-tabs');
            await tabs.getByRole('tab',{name:l.transcript,exact:true}).focus();
            await page.keyboard.press('ArrowRight');assert.equal(await page.locator('#sidebar-summary-panel').isVisible(),true);
            await page.keyboard.press('End');assert.equal(await page.locator('#sidebar-contents-panel').isVisible(),true);
            const contentPage=page.locator('#sidebar-contents-panel select');
            await contentPage.focus();await inside(contentPage,row.metrics.innerWidth,row.metrics.innerHeight,true);
            await page.screenshot({path:`${evidence}/${name}-contents.png`,animations:'disabled'});
            await tabs.getByRole('tab',{name:l.contents,exact:true}).focus();
            await page.keyboard.press('Home');assert.equal(await page.locator('#sidebar-transcript-panel').isVisible(),true);
            if(compact)await page.getByRole('tab',{name:l.board,exact:true}).click();
            await page.screenshot({path:`${evidence}/${name}-board.png`,animations:'disabled'});
            const importAction=await meetingAction(page,l.import);const trigger=compact?page.locator('.meeting-more'):importAction;await importAction.click();
            await page.keyboard.press('End');assert.equal(await page.evaluate(()=>document.activeElement.getAttribute('role')),'menuitem');
            await page.keyboard.press('Home');assert.equal(await page.evaluate(()=>document.activeElement===document.querySelector('[role="menu"] [role="menuitem"]')),true);
            await page.keyboard.press('Escape');assert.equal(await trigger.evaluate(el=>document.activeElement===el),true);
            const settingsAction=await meetingAction(page,l.settings);const settingsTrigger=compact?page.locator('.meeting-more'):settingsAction;await settingsAction.click();
            const dialog=page.getByRole('dialog',{name:l.settings,exact:true});await dialog.waitFor();
            await inside(dialog.getByRole('button',{name:l.close,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight,true);
            const mobile=dialog.locator('.settings-mobile-nav select');
            if(await mobile.isVisible()){
              for(const section of ['general','audio','models','privacy','glossary','maintenance']){await mobile.selectOption(section);assert.equal(await dialog.locator('.settings-content').evaluate(el=>el.scrollWidth<=el.clientWidth+1),true,`settings section ${section} horizontally overflows`)}
              await mobile.selectOption('general');
            }else{
              for(const tab of await dialog.locator('.settings-nav [role=tab]').all()){await tab.click();assert.equal(await dialog.locator('.settings-content').evaluate(el=>el.scrollWidth<=el.clientWidth+1),true,'settings section horizontally overflows')}
              await dialog.locator('.settings-nav [role=tab]').first().click();
            }
            await dialog.getByRole('button',{name:l.close,exact:true}).focus();await page.keyboard.press('Shift+Tab');
            assert.equal(await page.evaluate(()=>!!document.activeElement.closest('dialog')),true,'backward Tab escapes dialog');
            await page.keyboard.press('Tab');assert.equal(await page.evaluate(()=>!!document.activeElement.closest('dialog')),true,'forward Tab escapes dialog');
            await page.screenshot({path:`${evidence}/${name}-settings.png`,animations:'disabled'});
            await page.keyboard.press('Escape');await dialog.waitFor({state:'detached'});
            assert.equal(await settingsTrigger.evaluate(el=>document.activeElement===el),true,'dialog does not restore trigger focus');
            const reviewTrigger=await meetingAction(page,l.review);
            await inside(reviewTrigger,row.metrics.innerWidth,row.metrics.innerHeight,true);await reviewTrigger.click();
            const review=page.getByRole('dialog',{name:l.review,exact:true});await review.waitFor();
            await inside(review.getByRole('button',{name:l.cancel,exact:true}),row.metrics.innerWidth,row.metrics.innerHeight,true);
            await page.screenshot({path:`${evidence}/${name}-review.png`,animations:'disabled'});
            await page.keyboard.press('Escape');await review.waitFor({state:'detached'});
            assert.deepEqual(errors,[]);row.passed=true;await fs.rm(`${evidence}/${name}-failure.png`,{force:true});console.log(`PASS ${name}`);
          }catch(error){row.error=error.stack || String(error);row.overflow=await page.evaluate(()=>[...document.querySelectorAll('header,nav,footer,button,input,select,textarea,[role=tablist],.meeting-bar,.board-toolbar,.meeting-actions,.dock,.bar')].map(el=>({tag:el.tagName,class:el.className,text:el.textContent?.trim().slice(0,60),rect:el.getBoundingClientRect().toJSON()})).filter(x=>x.rect.width&&x.rect.height&&(x.rect.right>innerWidth+1||x.rect.bottom>innerHeight+1)).slice(0,30));await page.screenshot({path:`${evidence}/${name}-failure.png`,animations:'disabled'}).catch(()=>{});console.error(`FAIL ${name}: ${error.message}`)}
          await fs.writeFile(`${evidence}/matrix.in-progress.json`,JSON.stringify({passed:rows.every(x=>x.passed),rows},null,2));
        }
      }finally{await context.close()}
    }
  }finally{await fs.rm(temporary,{recursive:true,force:true})}
  await fs.rename(`${evidence}/matrix.in-progress.json`,`${evidence}/matrix.json`);
  if(rows.some(x=>!x.passed))process.exitCode=1;
})().catch(error=>{console.error(error);process.exitCode=1});
