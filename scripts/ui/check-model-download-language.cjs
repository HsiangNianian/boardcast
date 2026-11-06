// Production App; native catalog, IPC failures and progress events are controlled here.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async function checkModelDownloadLanguage(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/model-download-language/browser') {
  await fs.mkdir(evidence, {recursive: true});
  const native = await fs.readFile(path.resolve(__dirname, '../../src-tauri/src/models.rs'), 'utf8');
  const catalog = [...native.matchAll(/id: "([^"]+)",\s+label: "([^"]+)",\s+note: "([^"]+)"/g)].map(([,id,label,note]) => ({id,label,note,kind:id==='voice'?'voice':id.endsWith('-vision')?'vision':'llm',size:2e9,installed:['voice','qwen3.5-4b','qwen3.5-4b-vision'].includes(id),partial:id==='qwen3.5-9b-vision'?2e8:0,selected:id==='qwen3.5-4b',recommended:false}));
  const chinese = [
    ['Qwen3.5-4B','轻量：可在笔记本、集成显卡或纯 CPU 环境运行'],
    ['Qwen3.5-9B','更准确：建议使用至少有 6 GB 显存的独立显卡'],
    ['视觉组件 · Qwen3.5-4B','让内置模型识别导入图片中的文字，用于会议批注。图片只在本机处理。'],
    ['视觉组件 · Qwen3.5-9B','让内置模型识别导入图片中的文字，用于会议批注。图片只在本机处理。'],
    ['语音识别模型','实时转写：SenseVoice（支持中文、英语、日语、韩语和粤语，含标点）与 Silero 语音检测'],
  ];
  const report = {checks: [], messages: [], layouts: [], meetings: [], pageErrors: []};
  const check = (ok,label) => {assert(ok,label);report.checks.push(label);};
  const copy = async (locator, expected, label) => {const actual=(await locator.innerText()).trim(); const flat=s=>s.replace(/\s+/g,' ').trim(); report.messages.push({label,actual,expected,ok:flat(actual)===flat(expected)});};
  await page.addInitScript(({catalog}) => {
    if (!/^https?:$/.test(location.protocol)) return;
    const lang = new URL(location.href).searchParams.get('downloadLang');
    localStorage.clear();localStorage.setItem('boardcast.lang','system');localStorage.setItem('boardcast.meeting','model-download-language');
    const graph={nodes:[{id:'n1',label:'Keep original 中文',kind:'node',marks:[],aliases:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1},now=Date.now()-1000;
    const seed={version:2,storageRevision:1,id:'model-download-language',title:'Download language 中文',titled:true,createdAt:now,updatedAt:now,graph,history:[],lines:[{text:'ORIGINAL 中文',ms:1,page:0}],page:0,pages:[{graph}]};
    catalog.push({id:'future-model',label:'Custom label 自定义',note:'Preserve custom metadata 原文',kind:'llm',size:1e9,partial:0,installed:false,selected:false,recommended:false});
    const q=window.qaDownloadLanguage={catalog:{models:catalog,freeBytes:50e9,dir:'/fixture/models'},languages:[lang],seed,original:JSON.stringify(seed),calls:[],failures:{},events:{},callbacks:{}};
    Object.defineProperty(navigator,'languages',{get:()=>q.languages});
    q.emit=payload=>{
      const model=catalog.find(m=>m.id===payload.id);model.partial=payload.downloaded;
      if(['error','canceled','done'].includes(payload.state))delete q.catalog.downloading;
      if(payload.state==='done')model.installed=true;
      q.lastProgress=structuredClone(payload);
      (q.events['model-progress']||[]).forEach(id=>q.callbacks[id]?.({event:'model-progress',payload}));
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>q.events[event]=(q.events[event]||[]).filter(v=>v!==id)};
    let next=0;
    window.__TAURI_INTERNALS__={transformCallback:fn=>{q.callbacks[++next]=fn;return next;},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:value=>value,invoke:async(command,args={})=>{
      q.calls.push({command,args});if(q.failures[command])throw q.failures[command];
      if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler;}
      if(command==='models_list')return structuredClone(q.catalog);
      if(command==='model_download'){q.catalog.downloading=args.id;return;}
      if(command==='model_cancel'){const id=q.catalog.downloading;q.emit({id,state:'canceled',downloaded:catalog.find(m=>m.id===id).partial,total:2e9,speed:0});return;}
      if(command==='model_delete'){catalog.find(m=>m.id===args.id).installed=false;return;}
      if(command==='llm_status'||command==='llm_ensure')return{state:'idle',model:'Qwen3.5-4B.gguf',gpus:[]};
      if(command==='remote_get')return{baseUrl:'',model:'',enabled:false};
      if(command==='voice_status')return{installed:true,listening:false};
      if(command==='voice_models')return{selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:true,listening:false,preparing:false,autoLocales:[],activeLocale:null,apple:null,appleError:null};
      if(command==='voice_devices')return[];
      if(command==='meeting_load')return args.storageVersion===2&&args.id===seed.id?JSON.stringify(q.seed):null;
      if(command==='meeting_save'){q.seed=JSON.parse(args.json);return;}
      if(command==='meetings_list')return[];
      if(command==='plugin:app|version')return'0.21.0-fixture';
      if(command==='session_wayland')return false;
      return null;
    }};
  },{catalog});
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  const panel=page.locator('.model-panel');
  const item=id=>panel.locator('.model').filter({has:page.locator('code').getByText(id,{exact:true})});
  const language=async lang=>page.evaluate(lang=>{window.qaDownloadLanguage.languages=[lang];window.dispatchEvent(new Event('languagechange'));},lang);
  const emit=async payload=>page.evaluate(p=>window.qaDownloadLanguage.emit({id:'qwen3.5-9b',total:2e9,speed:0,...p}),payload);
  const hash='a1'.repeat(32),rawHash=`Verification failed (sha256 ${hash}); file deleted. Please download it again`;
  const failure=(zh,reason)=>(zh?'下载未完成，请在下方重试或继续下载。':'Download failed. Retry or resume the download below.')+' '+reason;
  const hashFailure=zh=>failure(zh,zh?`文件校验失败（sha256 ${hash}），已删除该文件，请重新下载。`:rawHash);
  try {
    for(const lang of ['zh','en']){
      const zh=lang==='zh';await page.goto(`${baseUrl}/?downloadLang=${lang}`);await page.locator('.splash').waitFor({state:'hidden'});
      await page.getByRole('button',{name:zh?'设置':'Settings',exact:true}).click();
      await page.getByRole('tab',{name:zh?'板书与模型':'Board & models',exact:true}).click();
      await page.getByRole('button',{name:zh?'管理模型':'Manage models',exact:true}).click();
      await item('voice').waitFor();
      for(const [i,model] of catalog.entries()){
        await item(model.id).locator('summary').click();
        await copy(item(model.id).locator('b').first(),zh?chinese[i][0]:model.label,`${lang}/${model.id}/name`);
        await copy(item(model.id).locator('details p'),zh?chinese[i][1]:model.note,`${lang}/${model.id}/details`);
      }
      check((await item('future-model').innerText()).includes('Custom label 自定义'),'unknown model label is preserved');
      await item('future-model').locator('summary').click();
      check((await item('future-model').innerText()).includes('Preserve custom metadata 原文'),'unknown model detail is preserved');
      await copy(item('qwen3.5-4b').locator('p.note').nth(1),zh?'配套模型：视觉组件 · Qwen3.5-4B · 已安装':'Paired with Vision add-on · Qwen3.5-4B · installed',`${lang}/paired-name`);
      report.messages.push({label:`${lang}/progress-name`,actual:await item('qwen3.5-9b-vision').getByRole('progressbar').getAttribute('aria-label'),expected:(zh?'下载进度: 视觉组件 · Qwen3.5-9B':'Download progress: Vision add-on · Qwen3.5-9B'),ok:(await item('qwen3.5-9b-vision').getByRole('progressbar').getAttribute('aria-label'))===(zh?'下载进度: 视觉组件 · Qwen3.5-9B':'Download progress: Vision add-on · Qwen3.5-9B')});
      const board=item('qwen3.5-9b');
      await board.getByRole('button',{name:zh?'下载':'Download',exact:true}).click();
      await board.getByRole('button',{name:zh?'暂停':'Pause',exact:true}).waitFor();
      for(const [message,cn] of [['Verifying the downloaded portion…','正在校验已下载的部分…'],['Verifying file…','正在校验文件…']]){
        await emit({state:'verifying',downloaded:5e8,message});
        await board.locator('.meta p').waitFor();await copy(board.locator('.meta p'),zh?cn:message,`${lang}/${message}`);
      }
      await emit({state:'error',downloaded:0,message:rawHash});await board.getByRole('button',{name:zh?'下载':'Download',exact:true}).waitFor();
      await copy(board.getByRole('alert'),hashFailure(zh),`${lang}/checksum-failure`);
      const before=await page.evaluate(()=>JSON.stringify(window.qaDownloadLanguage.lastProgress));
      await language(zh?'en':'zh');await copy(board.getByRole('alert'),hashFailure(!zh),`${lang}/live-error-language`);await language(lang);
      check(await page.evaluate(()=>JSON.stringify(window.qaDownloadLanguage.lastProgress))===before,`${lang}: language change preserves native hash and progress`);
      await page.setViewportSize({width:720,height:480});await board.getByRole('button',{name:zh?'下载':'Download',exact:true}).scrollIntoViewIfNeeded();
      report.layouts.push({lang,...await panel.evaluate(el=>({width:el.clientWidth,scrollWidth:el.scrollWidth,overflow:[...el.querySelectorAll('*')].filter(child=>child.getBoundingClientRect().right>el.getBoundingClientRect().right).map(child=>({tag:child.tagName,class:child.className,text:child.textContent.slice(0,100),right:child.getBoundingClientRect().right})).slice(0,12)}))});
      await page.screenshot({path:path.join(evidence,`${lang}-checksum-compact.png`)});await page.setViewportSize({width:1280,height:800});
      await board.getByRole('button',{name:zh?'下载':'Download',exact:true}).click();await board.getByRole('button',{name:zh?'暂停':'Pause',exact:true}).waitFor();await emit({state:'downloading',downloaded:5e8,source:'ModelScope'});
      await board.getByRole('button',{name:zh?'暂停':'Pause',exact:true}).click();
      const resume=()=>board.getByRole('button',{name:zh?/^继续下载/:/^Resume download/});await resume().waitFor();
      check(await page.evaluate(()=>window.qaDownloadLanguage.catalog.models.find(m=>m.id==='qwen3.5-9b').partial)===5e8,`${lang}: pausing retains the partial file size and offers resume`);
      const space="Not enough disk space: Qwen3.5-9B needs 1.5 GB more (plus 0.3 GB spare); only 0.1 GB is free on the model folder's disk";
      await page.evaluate(space=>window.qaDownloadLanguage.failures.model_download=space,space);await resume().click();
      const globalError=panel.locator(':scope > [role="alert"]');await globalError.waitFor();
      await copy(globalError.locator('p'),zh?'磁盘空间不足：Qwen3.5-9B还需 1.5 GB，另需保留 0.3 GB 空间；模型目录所在磁盘仅剩 0.1 GB。':space,`${lang}/space-preflight`);
      const calls=await page.evaluate(()=>window.qaDownloadLanguage.calls.filter(c=>c.command==='model_download').length);
      await globalError.getByRole('button',{name:zh?'重新读取模型':'Reload models',exact:true}).click();
      check(await page.evaluate(()=>window.qaDownloadLanguage.calls.filter(c=>c.command==='model_download').length)===calls,`${lang}: reloading after a preflight error does not restart a download`);
      await page.evaluate(()=>delete window.qaDownloadLanguage.failures.model_download);await resume().click();await board.getByRole('button',{name:zh?'暂停':'Pause',exact:true}).waitFor();
      await emit({state:'error',downloaded:5e8,message:'Mirror: HTTP 503 /fixture/日志'});await board.getByRole('alert').waitFor();
      await copy(board.getByRole('alert'),failure(zh,'Mirror: HTTP 503 /fixture/日志'),`${lang}/unknown-network-detail`);
      check(await page.evaluate(()=>JSON.stringify(window.qaDownloadLanguage.seed)===window.qaDownloadLanguage.original),`${lang}: catalog, progress, recovery and language changes do not write the meeting`);
      await page.evaluate(()=>window.qaDownloadLanguage.failures.model_delete='Download in progress; cancel it first');
      await item('voice').getByRole('button',{name:zh?'删除':'Delete',exact:true}).click();const confirm=page.locator('.model-remove');await confirm.waitFor();
      await copy(confirm.getByRole('heading'),zh?'删除 语音识别模型？':'Delete Speech recognition model?',`${lang}/delete-name`);
      await confirm.getByRole('button',{name:zh?'停止、保存并删除':'Stop, save and delete',exact:true}).click();await confirm.getByRole('alert').waitFor();
      await copy(confirm.getByRole('alert'),zh?'模型正在下载，请先暂停下载再删除。':'Download in progress; cancel it first',`${lang}/delete-failure`);
      check(await page.evaluate(()=>window.qaDownloadLanguage.catalog.models.find(m=>m.id==='voice').installed),`${lang}: failed deletion keeps the installed model and confirmation`);
      await confirm.getByRole('button',{name:zh?'取消':'Cancel',exact:true}).click();
      const meeting=await page.evaluate(()=>({before:JSON.parse(window.qaDownloadLanguage.original),after:window.qaDownloadLanguage.seed}));report.meetings.push({lang,...meeting});
      const pageGraphs=m=>(m.pages||[{graph:m.graph}]).map(p=>p.graph);
      check(['id','title','graph','lines'].every(key=>JSON.stringify(meeting.before[key])===JSON.stringify(meeting.after[key]))&&JSON.stringify(pageGraphs(meeting.before))===JSON.stringify(pageGraphs(meeting.after))&&(meeting.before.page||0)===(meeting.after.page||0),`${lang}: the pre-delete save preserves original meeting content with the existing compact single-page encoding`);
      check(await page.evaluate(()=>window.qaDownloadLanguage.calls.filter(c=>c.command==='model_download').every(c=>c.args.id==='qwen3.5-9b')&&window.qaDownloadLanguage.calls.filter(c=>c.command==='model_delete').every(c=>c.args.id==='voice')),`${lang}: native actions use the unchanged model ids`);
    }
    check(!report.pageErrors.length,'no production JavaScript errors');
    check(report.messages.every(m=>m.ok),'all catalog, progress, recovery and confirmation text uses the selected language');
    check(report.layouts.every(l=>l.scrollWidth<=l.width+1),'long checksum and recovery controls fit the compact panel in both languages');
  } finally {await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2));}
  console.log(`model-download-language: ${report.checks.length} checks; ${report.messages.length} text comparisons`);
};
