// Production App with controlled native replies; real filesystem cases are separate G2 evidence.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {createHash} = require('node:crypto');

module.exports = async function checkNativeStorageLanguage(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/native-error-localization/browser') {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],messages:[],pageErrors:[]};
  const check=(ok,label)=>{assert(ok,label);report.checks.push(label);};
  const image='data:image/svg+xml;base64,'+Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><rect width="100" height="60" fill="#b0d0ff"/></svg>').toString('base64');
  const key=createHash('sha256').update(image).digest('hex').slice(0,32)+'.svg';
  await page.addInitScript(({image,key})=>{
    if(!/^https?:$/.test(location.protocol))return;
    const query=new URL(location.href).searchParams;
    localStorage.clear();localStorage.setItem('boardcast.lang',query.get('storageLang')||'en');localStorage.setItem('boardcast.log','open');localStorage.setItem('boardcast.meeting','storage-language');
    const graph={nodes:[{id:'n1',label:'Original board 中文',kind:'node',marks:[],aliases:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1,...(query.has('asset')?{underlay:{w:100,h:60}}:{})};
    const now=Date.now()-1000;
    const seed={version:2,storageRevision:1,id:'storage-language',title:'Storage language 中文',titled:true,createdAt:now,updatedAt:now,graph,history:[],lines:[{text:'ORIGINAL 中文',ms:1,page:0}],page:0,pages:[{graph,...(query.has('asset')?{underlay:'asset:'+key}:{})}]};
    const q=window.qaStorageLanguage={seed,original:JSON.stringify(seed),calls:[],callbacks:{},events:{},nativeFailure:null,assetMode:query.get('asset')};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>{q.events[event]=(q.events[event]||[]).filter(value=>value!==id);}};
    let callback=0;
    window.__TAURI_INTERNALS__={
      transformCallback:fn=>{q.callbacks[++callback]=fn;return callback;},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:value=>value,
      invoke:async(command,args={})=>{
        q.calls.push({command,args});
        if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler;}
        if(command==='llm_status'||command==='llm_ensure')return{state:'ready',url:'http://127.0.0.1:19881',model:'Fixture',device:'Fixture',gpus:[]};
        if (command === 'remote_prepare') return crypto.randomUUID();
        if (command === 'llm_chat') return {choices: [{message: {content: 'N 「New board 中文」'}}]};
        if(command==='voice_status')return{installed:true,listening:false};
        if(command==='voice_models')return{selection:{provider:'sensevoice',locale:'auto'},senseVoiceInstalled:true,listening:false,preparing:false,autoLocales:[],activeLocale:null,apple:null};
        if(command==='voice_devices')return[];
        if(command==='remote_get')return{baseUrl:'',model:'',enabled:false};
        if(command==='meeting_load')return args.storageVersion===2&&args.id===seed.id?JSON.stringify(q.seed):null;
        if(command==='meeting_asset_load'){
          if(q.assetMode==='missing')return null;
          if(q.assetMode==='hash-mismatch')throw `ASSET_HASH_MISMATCH: ${key}. The original image is preserved.`;
          return image.split(',')[1];
        }
        if(command==='meeting_save'){if(q.nativeFailure)throw q.nativeFailure;q.seed=JSON.parse(args.json);return;}
        if(command==='meetings_list')return[];
        if(command==='models_list')return{models:[],freeBytes:1000000};
        if(command==='plugin:app|version')return'0.21.0-fixture';
        if(command==='session_wayland')return false;
        if(command==='export_save'){q.exported=args;return '/fixture/recovered.md';}
        return null;
      }
    };
  },{image,key});
  page.on('pageerror',error=>report.pageErrors.push(error.message));
  const failures=[
    ['deleted','This meeting has been deleted. Your current edits are still here.','This meeting has been deleted. Your current edits are still here.','这场会议已被删除，当前修改仍在这里。'],
    ['revision','This meeting changed in another window. Reopen it before saving; current edits are preserved.','This meeting changed in another window. Reopen it before saving; your current edits are still here.','这场会议已在其他窗口更改，无法覆盖保存。当前修改仍在这里，可先导出副本，再重新打开会议。'],
    ['future','Cannot overwrite a newer meeting format','Cannot overwrite a meeting from a newer Boardcast version.','无法覆盖由新版 Boardcast 保存的会议。'],
    ['unknown','Cannot overwrite a newer meeting format: EIO /fixture/source.json','Cannot overwrite a newer meeting format: EIO /fixture/source.json','Cannot overwrite a newer meeting format: EIO /fixture/source.json'],
  ];
  try {
    for(const lang of ['zh','en']){
      const zh=lang==='zh';
      for(const [kind,raw,en,cn] of [...failures,['missing'],['hash-mismatch']]){
        const asset=kind==='missing'||kind==='hash-mismatch';
        await page.goto(`${baseUrl}/?storageLang=${lang}${asset?'&asset='+kind:''}`);
        await page.locator('.meeting-title').filter({hasText:'Storage language 中文'}).waitFor();await page.locator('.splash').waitFor({state:'hidden'});
        if(asset)await page.locator('.asset-warning').waitFor();
        else await page.evaluate(error=>window.qaStorageLanguage.nativeFailure=error,raw);
        const draft=`UNSAVED ${lang} ${kind} 中文`;
        await page.locator('textarea[aria-label]').fill(draft);await page.getByRole('button',{name:zh?'写上白板':'Put on the board',exact:true}).click();
        await page.locator('.lines .pending').waitFor({state:'hidden'});await page.locator('.save-error').waitFor();
        const actual=await page.locator('.save-error p').innerText();
        const expected=asset?(zh
          ?`会议图片 ${key}${kind==='missing'?'缺失':'与保存的校验值不符'}。现有文件和引用均已保留，请恢复原图片后再保存。`
          :`Meeting image ${key} ${kind==='missing'?'is missing':'does not match its content hash'}. Existing files and references are preserved. Restore the original image before saving.`):(zh?cn:en);
        report.messages.push({lang,kind,raw,actual,expected,matched:actual===expected});
        await page.locator('.save-error').getByRole('button',{name:zh?'重试保存':'Retry save',exact:true}).click();await page.locator('.save-error').waitFor();
        await page.getByRole('button',{name:zh?'新会议':'New meeting',exact:true}).click();await page.locator('.transition-state').waitFor({state:'hidden'});
        check(await page.evaluate(()=>JSON.stringify(window.qaStorageLanguage.seed)===window.qaStorageLanguage.original),`${lang}/${kind}: rejected writes and retry preserve the original record`);
        check((await page.locator('.meeting-title').innerText()).includes('Storage language 中文')&&await page.locator('.lines').getByText(draft,{exact:true}).count()===1,`${lang}/${kind}: failed flush retains the meeting and unsaved transcript`);
        check(await page.evaluate(asset=>{const writes=window.qaStorageLanguage.calls.filter(c=>c.command==='meeting_save').length;return asset?writes===0:writes>0;},asset),`${lang}/${kind}: failure is reached at the intended native or asset boundary`);
        await page.screenshot({path:path.join(evidence,`${lang}-${kind}.png`)});
        await page.getByRole('button',{name:zh?'导出':'Export',exact:true}).click();await page.getByRole('menuitemradio',{name:zh?'整场会议':'Whole meeting',exact:true}).click();
        await page.getByRole('menuitem',{name:/^Markdown/}).click();await page.waitForFunction(()=>!!window.qaStorageLanguage.exported);
        const exported=await page.evaluate(()=>window.qaStorageLanguage.exported),text=Buffer.from(exported.dataBase64,'base64').toString('utf8');
        check(text.includes(draft)&&text.includes('ORIGINAL 中文'),`${lang}/${kind}: rescue Markdown includes original and unsaved transcript`);
        await fs.writeFile(path.join(evidence,`${lang}-${kind}.md`),text);
        if(asset||kind==='unknown'){
          // Restoring the dependency can complete a pending autosave before a pointer click becomes stable.
          await page.locator('.save-error').getByRole('button',{name:zh?'重试保存':'Retry save',exact:true}).focus();
          await page.evaluate(()=>{window.qaStorageLanguage.assetMode=null;window.qaStorageLanguage.nativeFailure=null;});
          await page.keyboard.press('Enter');await page.locator('.save-state.saved').waitFor();
          check(await page.evaluate(draft=>window.qaStorageLanguage.seed.lines.some(l=>l.text===draft),draft),`${lang}/${kind}: restoring the dependency allows the same edits to be saved`);
          if(asset)check(await page.evaluate(key=>window.qaStorageLanguage.seed.pages[0].underlay==='asset:'+key,key),`${lang}/${kind}: repaired save retains the original image reference`);
        }
      }
    }
    report.productionScripts=await page.locator('script[src]').evaluateAll(s=>s.map(s=>s.src));
    check(report.pageErrors.length===0,'no browser script errors');
    check(report.messages.every(m=>m.matched),'all known errors are localized and unknown errors remain exact');
    report.passed=true;return report;
  }catch(error){report.error=String(error);await page.screenshot({path:path.join(evidence,'failure.png')}).catch(()=>{});throw error;}
  finally{await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2)+'\n');}
};
