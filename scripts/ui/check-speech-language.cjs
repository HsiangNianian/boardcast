// Production UI with controlled native audio replies; this script never opens a host audio device.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');

module.exports = async function checkSpeechLanguage(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/speech-language/browser') {
  await fs.mkdir(evidence,{recursive:true});
  const {en}=await import('../../src/i18n/en.ts'),{zh}=await import('../../src/i18n/zh.ts');
  const dictionaries={en,zh},report={checks:[],messages:[],pageErrors:[]};
  const check=(ok,label)=>{assert(ok,label);report.checks.push(label);};
  const copy=async(locator,expected,label)=>{const actual=(await locator.innerText()).trim();report.messages.push({label,actual,expected,ok:actual===expected});};
  await page.addInitScript(()=>{
    if(!/^https?:$/.test(location.protocol))return;
    const query=new URL(location.href).searchParams,lang=query.get('speechLang'),empty=query.has('empty');
    localStorage.clear();localStorage.setItem('boardcast.lang','system');localStorage.setItem('boardcast.meeting','speech-language');localStorage.setItem('boardcast.source','mic');
    const graph={nodes:empty?[]:[{id:'n1',label:'Original board 中文',kind:'node',marks:[],aliases:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1},now=Date.now()-1000;
    const seed={version:2,storageRevision:1,id:'speech-language',title:'Speech language 中文',titled:true,createdAt:now,updatedAt:now,graph,history:[],lines:empty?[]:[{text:'ORIGINAL 中文',ms:1,page:0}],page:0,pages:[{graph}]};
    const q=window.qaSpeechLanguage={languages:[lang],seed,original:JSON.stringify(seed),calls:[],callbacks:{},events:{},failures:{},devices:['Fixture microphone','Disconnected USB microphone'],listening:false,startFailure:'No microphone found',checkFailure:'No microphone found',provider:'sensevoice',appleError:null,boardReady:false,speechInstalled:!query.has('missingSpeech')};
    Object.defineProperty(navigator,'languages',{get:()=>q.languages});
    q.emit=(event,payload)=>(q.events[event]||[]).forEach(id=>q.callbacks[id]?.({event,payload}));
    q.listen=listening=>{q.listening=listening;q.emit('voice-status',{installed:true,listening,device:'Fixture microphone'});};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(event,id)=>q.events[event]=(q.events[event]||[]).filter(v=>v!==id)};
    let next=0;
    window.__TAURI_INTERNALS__={transformCallback:fn=>{q.callbacks[++next]=fn;return next;},unregisterCallback:id=>delete q.callbacks[id],convertFileSrc:value=>value,invoke:async(command,args={})=>{
      q.calls.push({command,args});if(q.failures[command])throw q.failures[command];
      if(command==='plugin:event|listen'){(q.events[args.event]||=[]).push(args.handler);return args.handler;}
      if(command==='voice_status')return{installed:q.speechInstalled,listening:q.listening};
      if(command==='voice_models')return{selection:{provider:q.provider,locale:'en-US'},senseVoiceInstalled:q.speechInstalled,listening:q.listening,preparing:false,autoLocales:['en-US','zh-CN','ja-JP'],activeLocale:null,apple:{isApple:true,available:true,defaultLocale:'en-US',installedLocales:['en-US'],locales:[{id:'en-US',name:'English',status:'installed'}]},appleError:q.appleError};
      if(command==='voice_devices')return q.devices;
      if(command==='voice_start'){if(q.startFailure)throw q.startFailure;q.listen(true);return;}
      if(command==='voice_stop'){q.listen(false);return;}
      if(command==='voice_input_check_start'){q.input=args.id;q.emit('voice-input-check',{id:args.id,state:q.checkFailure?'error':'active',device:'Fixture microphone',level:0.2,...(q.checkFailure?{message:q.checkFailure}:{})});if(q.checkFailure)q.input=null;return;}
      if(command==='voice_input_check_stop'){if(q.input){q.emit('voice-input-check',{id:q.input,state:'stopped',reason:'cancelled'});q.input=null;}return;}
      if(command==='llm_status'||command==='llm_ensure')return q.boardReady?{state:'ready',url:'http://127.0.0.1:19888',model:'Fixture',gpus:[]}:{state:'no-model',model:'Fixture',gpus:[]};
      if(command==='remote_prepare')return String(++next);
      if(command==='llm_chat')return{choices:[{message:{content:'N 「Recovered spoken point」'}}]};
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
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  const count=command=>page.evaluate(command=>window.qaSpeechLanguage.calls.filter(c=>c.command===command).length,command);
  const language=lang=>page.evaluate(lang=>{window.qaSpeechLanguage.languages=[lang];window.dispatchEvent(new Event('languagechange'));},lang);
  const error=page.locator('.dock .caption.err');
  try {
    for(const lang of ['zh','en']){
      const t=dictionaries[lang],other=lang==='zh'?'en':'zh',alternate=dictionaries[other];
      await page.goto(`${baseUrl}/?speechLang=${lang}`);await page.locator('.meeting-title').filter({hasText:'Speech language 中文'}).waitFor();await page.locator('.splash').waitFor({state:'hidden'});
      const cases=[
        ['No microphone found',t.speechErrors.fixed['No microphone found'],false],
        ['SYSTEM_AUDIO_PERMISSION',t.settings.systemAudioPermission,false],
        ['Microphone could not be opened: EACCES /fixture/会议/设备',t.speechErrors.details['Microphone could not be opened']('EACCES /fixture/会议/设备'),false],
        ['Vendor fault EIO /fixture/原始诊断','Vendor fault EIO /fixture/原始诊断',false],
        ['APPLE_SPEECH_ASSETS_NOT_READY:en-US:custom','APPLE_SPEECH_ASSETS_NOT_READY:en-US:custom',false],
        ['APPLE_SPEECH_ASSETS_NOT_READY:en-US:supported',t.models.speechSetupHint,true],
        ['APPLE_AUTO_LANGUAGE_UNCERTAIN',t.models.appleAutoUncertain,true],
      ];
      for(const [raw,expected,setup] of cases){
        await page.evaluate(raw=>window.qaSpeechLanguage.startFailure=raw,raw);const starts=await count('voice_start');
        await page.locator('.dock .mic').click();await error.waitFor();await copy(error,expected,`${lang}/start/${raw}`);
        if(setup){const models=page.getByRole('dialog',{name:t.models.speechSetupTitle,exact:true});await models.waitFor();await models.getByRole('button',{name:t.models.close,exact:true}).click();}
        else check(await page.getByRole('dialog').count()===0,`${lang}/${raw}: ordinary or unknown failures do not open model setup`);
        check(await count('voice_start')===starts+1&&!await page.evaluate(()=>window.qaSpeechLanguage.listening),`${lang}/${raw}: one explicit attempt fails without leaving capture active`);
        if(raw==='SYSTEM_AUDIO_PERMISSION'||setup){
          await language(other);await copy(error,raw==='SYSTEM_AUDIO_PERMISSION'?alternate.settings.systemAudioPermission:raw==='APPLE_AUTO_LANGUAGE_UNCERTAIN'?alternate.models.appleAutoUncertain:alternate.models.speechSetupHint,`${lang}/existing-code-language/${raw}`);await language(lang);
        }
      }
      await page.evaluate(()=>window.qaSpeechLanguage.startFailure=null);await page.locator('.dock .mic').click();await page.getByRole('button',{name:t.composer.stopListening,exact:true}).waitFor();
      const runtime='Apple transcription could not keep up with audio; recording stopped and the last phrase may be incomplete';
      await page.evaluate(message=>{window.qaSpeechLanguage.listen(false);window.qaSpeechLanguage.emit('voice-error',message);},runtime);
      await copy(error,t.speechErrors.fixed[runtime],`${lang}/runtime-last-phrase`);
      check(await page.getByRole('button',{name:t.composer.startListening,exact:true}).isEnabled(),`${lang}: runtime failure restores the explicit retry action`);
      await page.locator('.dock .mic').click();await page.getByRole('button',{name:t.composer.stopListening,exact:true}).waitFor();
      await page.getByRole('button',{name:t.app.settings,exact:true}).click();await page.getByRole('tab',{name:t.settings.sections.audio,exact:true}).click();
      const microphone=page.locator('.settings').getByRole('combobox',{name:t.settings.micAria,exact:true});
      await microphone.selectOption('Fixture microphone');
      await page.waitForFunction(()=>window.qaSpeechLanguage.listening&&window.qaSpeechLanguage.calls.filter(c=>c.command==='voice_start').at(-1).args.device==='Fixture microphone');
      const startsBeforeRemoval=await count('voice_start');
      await page.evaluate(()=>{window.qaSpeechLanguage.devices=['Fixture microphone'];window.qaSpeechLanguage.startFailure='The selected microphone is no longer available';});
      await microphone.selectOption('Disconnected USB microphone');
      await page.waitForFunction(starts=>!window.qaSpeechLanguage.listening&&window.qaSpeechLanguage.calls.filter(c=>c.command==='voice_start').length===starts+1,startsBeforeRemoval);
      check(await microphone.inputValue()==='Disconnected USB microphone',`${lang}: failed hot switch keeps the requested microphone`);
      const switching=await page.evaluate(()=>window.qaSpeechLanguage.calls.filter(c=>c.command==='voice_start'||c.command==='voice_stop').slice(-2));
      check(switching[0].command==='voice_stop'&&switching[1].command==='voice_start'&&switching[1].args.device==='Disconnected USB microphone',`${lang}: hot switch releases the old input before attempting the selected one`);
      await page.locator('.settings').getByRole('button',{name:t.settings.close,exact:true}).click();await copy(error,t.speechErrors.fixed['The selected microphone is no longer available'],`${lang}/missing-microphone-error`);
      await page.getByRole('button',{name:t.app.settings,exact:true}).click();await page.getByRole('tab',{name:t.settings.sections.audio,exact:true}).click();
      await copy(microphone.locator('option:checked'),t.settings.micMissing('Disconnected USB microphone'),`${lang}/missing-microphone-option`);
      await page.screenshot({path:path.join(evidence,`${lang}-missing-microphone.png`)});
      await page.evaluate(()=>window.qaSpeechLanguage.startFailure=null);await microphone.selectOption('');
      check(await count('voice_start')===startsBeforeRemoval+1,`${lang}: choosing the default after a failure waits for an explicit start`);
      await page.locator('.settings').getByRole('button',{name:t.settings.close,exact:true}).click();await page.locator('.dock .mic').click();await page.getByRole('button',{name:t.composer.stopListening,exact:true}).waitFor();
      check(await page.evaluate(()=>window.qaSpeechLanguage.calls.filter(c=>c.command==='voice_start').at(-1).args.device)===null,`${lang}: explicit default selection sends no named microphone`);
      await page.getByRole('button',{name:t.composer.stopListening,exact:true}).click();await page.getByRole('button',{name:t.composer.startListening,exact:true}).waitFor();
      await page.getByRole('button',{name:t.app.settings,exact:true}).click();await page.getByRole('tab',{name:t.settings.sections.audio,exact:true}).click();
      const speech=page.locator('.settings .speech-models');
      await page.evaluate(()=>{window.qaSpeechLanguage.provider='apple';window.qaSpeechLanguage.appleError='Apple returned no capability information';});
      await speech.getByRole('button',{name:t.models.detect,exact:true}).click();await copy(speech.getByRole('alert'),t.speechErrors.fixed['Apple returned no capability information'],`${lang}/speech-capability`);
      await page.evaluate(()=>{window.qaSpeechLanguage.appleError=null;window.qaSpeechLanguage.failures.voice_select='Audio input is shutting down';});
      await speech.getByRole('button',{name:t.models.detect,exact:true}).click();
      await speech.locator('.model').filter({has:page.getByText('SenseVoice',{exact:true})}).getByRole('button',{name:t.models.use,exact:true}).click();
      await copy(speech.getByRole('alert'),t.speechErrors.fixed['Audio input is shutting down'],`${lang}/speech-selection`);
      check(await page.evaluate(()=>window.qaSpeechLanguage.provider)==='apple',`${lang}: failed selection preserves the selected provider`);
      await page.screenshot({path:path.join(evidence,`${lang}-speech-selection.png`)});
      await page.locator('.settings').getByRole('button',{name:t.settings.close,exact:true}).click();
      check(await page.evaluate(()=>JSON.stringify(window.qaSpeechLanguage.seed)===window.qaSpeechLanguage.original),`${lang}: all failed starts and model actions preserve the original meeting`);
      check(await count('model_download')===0&&await count('voice_prepare')===0,`${lang}: displaying errors does not download models`);

      await page.goto(`${baseUrl}/?speechLang=${lang}&empty=1`);await page.locator('.splash').waitFor({state:'hidden'});await page.getByRole('button',{name:t.preparation.startMeeting,exact:true}).click();
      const preparation=page.getByRole('dialog',{name:t.preparation.title,exact:true}),inputError=page.locator('.preparation .input-check [role="alert"]');
      check(await count('voice_input_check_start')===0&&await count('voice_start')===0,`${lang}: opening preparation does not acquire audio`);
      await preparation.getByRole('button',{name:t.preparation.checkInput,exact:true}).click();await inputError.waitFor();await copy(inputError,t.speechErrors.fixed['No microphone found'],`${lang}/input-no-device`);
      await language(other);await copy(inputError,alternate.speechErrors.fixed['No microphone found'],`${lang}/input-existing-language`);await language(lang);
      await preparation.getByRole('combobox',{name:t.settings.sourceAria,exact:true}).selectOption('system');await page.evaluate(()=>window.qaSpeechLanguage.checkFailure='SYSTEM_AUDIO_PERMISSION');
      await preparation.getByRole('button',{name:t.preparation.checkInput,exact:true}).click();await inputError.waitFor();await copy(inputError,t.settings.systemAudioPermission,`${lang}/input-permission`);
      check(await preparation.getByRole('combobox',{name:t.settings.sourceAria,exact:true}).inputValue()==='system'&&await count('voice_start')===0,`${lang}: failed preview keeps the chosen source and never starts recording`);
      await page.evaluate(()=>window.qaSpeechLanguage.checkFailure=null);await preparation.getByRole('button',{name:t.preparation.checkInput,exact:true}).click();await preparation.getByRole('meter').waitFor();
      await preparation.getByRole('button',{name:t.preparation.stopInputCheck,exact:true}).click();await preparation.getByRole('button',{name:t.preparation.checkInput,exact:true}).waitFor();
      check(!await page.evaluate(()=>window.qaSpeechLanguage.input),`${lang}: preview can be retried and explicitly released after an error`);
      await preparation.getByRole('button',{name:t.preparation.recordFirst,exact:true}).click();await preparation.getByRole('alert').waitFor();
      await copy(preparation.getByRole('alert'),t.speechErrors.fixed['No microphone found'],`${lang}/preparation-start-error`);
      await page.screenshot({path:path.join(evidence,`${lang}-preparation-error.png`)});
      await page.evaluate(()=>{window.qaSpeechLanguage.provider='apple';window.qaSpeechLanguage.appleError='Apple returned no capability information';});await preparation.getByRole('button',{name:t.preparation.recheck,exact:true}).click();
      await preparation.getByRole('button',{name:t.preparation.recheck,exact:true}).and(page.locator(':enabled')).waitFor();
      await copy(preparation.locator('ol > li').nth(1).locator('p.hint').first(),t.speechErrors.fixed['Apple returned no capability information'],`${lang}/preparation-capability`);
      check(await preparation.getByRole('button',{name:t.preparation.recordFirst,exact:true}).isDisabled(),`${lang}: capability errors block recording until recovery`);
      await page.evaluate(()=>{window.qaSpeechLanguage.provider='sensevoice';window.qaSpeechLanguage.appleError=null;window.qaSpeechLanguage.startFailure=null;});await preparation.getByRole('button',{name:t.preparation.recheck,exact:true}).click();
      await preparation.getByRole('button',{name:t.preparation.recordFirst,exact:true}).click();await preparation.waitFor({state:'hidden'});
      await page.evaluate(()=>window.qaSpeechLanguage.emit('voice-final',{id:'record-first-sentence',revision:0,sources:['mic'],text:'Keep this spoken point until the board model is ready.',start:0,end:2}));
      await page.waitForFunction(()=>window.qaSpeechLanguage.seed.lines.some(line=>line.id==='record-first-sentence'&&line.boardFailure==='unavailable'));
      check(await count('llm_chat')===0,`${lang}: missing board model still saves the actual speech event without an inference request`);
      await page.getByRole('button',{name:t.app.configureBoard,exact:true}).click();
      const boardSettings=page.getByRole('dialog',{name:t.settings.title,exact:true});
      check(await boardSettings.getByRole('tab',{name:t.settings.sections.models,exact:true}).getAttribute('aria-selected')==='true',`${lang}: transcript recovery opens the board model section directly`);
      await page.evaluate(()=>{window.qaSpeechLanguage.boardReady=true;});
      await page.waitForFunction(()=>window.qaSpeechLanguage.calls.some(call=>call.command==='llm_chat'));
      await boardSettings.getByRole('button',{name:t.settings.close,exact:true}).click();
      await page.getByRole('button',{name:t.app.retryLine,exact:true}).click();
      await page.waitForFunction(()=>window.qaSpeechLanguage.seed.graph.nodes.some(node=>node.label==='Recovered spoken point')&&window.qaSpeechLanguage.seed.lines.every(line=>!line.failed));
      check(await page.evaluate(()=>window.qaSpeechLanguage.seed.lines.length===1&&window.qaSpeechLanguage.seed.lines[0].id==='record-first-sentence'&&window.qaSpeechLanguage.seed.lines[0].text==='Keep this spoken point until the board model is ready.'),`${lang}: retry draws the saved sentence without duplicating or replacing its transcript`);
      await page.getByRole('button',{name:t.composer.stopListening,exact:true}).click();
      await page.getByRole('button',{name:t.composer.startListening,exact:true}).waitFor();
      check(!await page.evaluate(()=>window.qaSpeechLanguage.listening),`${lang}: explicit retry starts and stop releases capture`);
      await page.goto(`${baseUrl}/?speechLang=${lang}&empty=1&missingSpeech=1`);await page.locator('.splash').waitFor({state:'hidden'});
      await page.getByRole('button',{name:t.preparation.startMeeting,exact:true}).click();await preparation.getByText(t.preparation.speechMissing,{exact:true}).waitFor();
      check(await preparation.getByRole('button',{name:t.preparation.recordFirst,exact:true}).isDisabled(),`${lang}: missing transcription blocks capture while typing remains available`);
      await preparation.getByRole('button',{name:t.preparation.typeFirst,exact:true}).click();await preparation.waitFor({state:'hidden'});
      const typed=page.locator('.dock textarea');await typed.fill('Keep typed notes with no models installed.');await typed.press('Enter');
      await page.waitForFunction(()=>window.qaSpeechLanguage.seed.lines.some(line=>line.text==='Keep typed notes with no models installed.'&&line.boardFailure==='unavailable'));
      check(await count('voice_start')===0&&await count('llm_chat')===0&&await count('model_download')===0,`${lang}: typing with both models missing saves the note without capture, inference or downloads`);
    }
    check(!report.pageErrors.length,'no production JavaScript errors');
    check(report.messages.every(m=>m.ok),'all audio errors and existing recovery codes follow the current interface language');
  } finally {await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2));}
  console.log(`speech-language: ${report.checks.length} checks; ${report.messages.length} text comparisons`);
};
