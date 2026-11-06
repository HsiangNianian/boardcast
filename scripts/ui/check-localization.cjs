const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkLocalization(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/localization/browser') {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],pageErrors:[],downloads:[],offline:[]};
  const check=(condition,label)=>{assert(condition,label);report.checks.push(label);};
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.addInitScript(()=>{
    Object.defineProperty(navigator,'languages',{get:()=>window.qaLanguages||['ja-JP'],configurable:true});
  });
  const chinese=()=>page.evaluate(()=>document.documentElement.lang==='zh-CN');
  const language=async choice=>{
    await page.getByRole('button',{name:await chinese()?'设置':'Settings',exact:true}).click();
    const name=choice==='system'?(await chinese()?'跟随系统':'System default'):choice==='zh'?'中文':'English';
    await page.getByRole('dialog').getByRole('radio',{name,exact:true}).click();
    await page.getByRole('dialog').getByRole('button',{name:await chinese()?'关闭':'Close',exact:true}).click();
  };
  const saved=()=>page.evaluate(async()=>{
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get('qa-localization');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return row;
  });
  const exportHtml=async key=>{
    const zh=await chinese();await page.getByRole('button',{name:zh?'导出':'Export',exact:true}).click();
    await page.getByRole('menuitemradio',{name:zh?'整场会议':'Whole meeting',exact:true}).click();
    const pending=page.waitForEvent('download',{timeout:60000});
    await page.getByRole('menuitem',{name:zh?/^回放网页/:/^Replay page/}).click();
    const dl=await pending,file=path.join(evidence,key+'.html');await dl.saveAs(file);
    const html=await fs.readFile(file,'utf8');const data=JSON.parse(/window\.__boardcastData = (.*);\n/.exec(html)[1]);
    report.downloads.push({key,file,bytes:Buffer.byteLength(html),lang:data.lang,rawHtmlLang:/<html lang="([^"]+)"/.exec(html)[1]});
    return {file,html,data};
  };
  const offline=async (file,expected,{legacy=false,blocked=false}={})=>{
    const pref=legacy?expected:expected==='en'?'zh':'en';const requests=[],errors=[];
    const context=await page.context().browser().newContext({locale:pref==='zh'?'zh-CN':'en-US',viewport:{width:1280,height:800},reducedMotion:'reduce'});
    try {
      await context.addInitScript(({pref,blocked})=>{
        window.qaLangWrites=[];
        const set=Storage.prototype.setItem;
        if(!blocked)set.call(localStorage,'boardcast.lang',pref);
        Storage.prototype.setItem=function(k,v){if(k==='boardcast.lang')window.qaLangWrites.push(v);return set.call(this,k,v);};
        if(blocked)Object.defineProperty(window,'localStorage',{get(){throw new DOMException('Blocked fixture','SecurityError');}});
      },{pref,blocked});
      await context.route(/^https?:\/\//,r=>{requests.push(r.request().url());return r.abort();});await context.setOffline(true);
      const p=await context.newPage();p.on('pageerror',e=>errors.push(e.message));
      await p.goto(pathToFileURL(path.resolve(file)).href);await p.locator('.player .ink > svg').waitFor();
      const actual=await p.evaluate(()=>({lang:document.documentElement.lang,title:document.title,meta:document.querySelector('.player-meta').textContent,heading:document.querySelector('.log h2').textContent,summary:document.querySelector('.summary .md').innerText,transcript:document.querySelector('.lines').innerText,stored:(()=>{try{return localStorage.getItem('boardcast.lang');}catch{return 'blocked';}})(),writes:window.qaLangWrites,source:window.__boardcastData}));
      const suffix=expected==='en'?'boardcast replay':'boardcast 回放';
      const key=path.basename(file)+(blocked?'-blocked':'');
      check(actual.lang===(expected==='en'?'en':'zh-CN')&&actual.meta.endsWith(suffix)&&actual.heading===(expected==='en'?'Transcript':'会议记录'),`${key}: document and player controls use ${expected}`);
      if(!legacy)check(actual.title.endsWith(suffix)&&actual.source.lang===expected,`${key}: title and embedded language agree`);
      check(actual.summary.includes('USER_SUMMARY 中文 日本語')&&actual.transcript.includes('USER_TRANSCRIPT 中文 日本語'),`${key}: user content is not translated`);
      check(actual.stored===(blocked?'blocked':pref)&&actual.writes.length===0,`${key}: read-only player does not change receiver language storage`);
      check(await p.getByRole('button',{name:expected==='en'?'Play':'播放',exact:true}).isVisible(),`${key}: replay button has the localized accessible name`);
      if(!legacy){await p.evaluate(()=>dispatchEvent(new Event('languagechange')));check(await p.locator('html').getAttribute('lang')===actual.lang,`${key}: later system language events cannot replace exported language`);}
      const layouts=[];
      for(const viewport of [{width:1280,height:800},{width:640,height:400}]){
        await p.setViewportSize(viewport);
        layouts.push(await p.evaluate(()=>{const t=document.querySelector('.player-title').getBoundingClientRect(),bar=document.querySelector('.replay-dock').getBoundingClientRect();return {width:innerWidth,height:innerHeight,overflow:document.documentElement.scrollWidth>innerWidth,titleRight:t.right,controlsBottom:bar.bottom};}));
      }
      check(layouts.every(x=>!x.overflow&&x.titleRight<=x.width&&x.controlsBottom<=x.height),`${key}: long title and localized controls fit desktop and 640×400`);
      await p.screenshot({path:path.join(evidence,key+'.png')});
      check(!requests.length&&!errors.length,`${key}: actual file opens offline without HTTP or script errors`);
      report.offline.push({file,expected,legacy,blocked,pref,requests,errors,layouts,...actual,source:undefined});
    }finally{await context.close();}
  };
  try {
    await page.goto(baseUrl);await page.getByRole('button',{name:'Settings',exact:true}).waitFor();
    check(await page.locator('html').getAttribute('lang')==='en','unsupported Japanese system language falls back to English');
    const fixture=await page.evaluate(async()=>{
      const graph={nodes:[{id:'n1',label:'USER_BOARD 中文 日本語',kind:'node',aliases:[],marks:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1};
      const steps=[{t:Date.now()-1000,line:1,d:{nodes:{set:graph.nodes},nextNode:2}}];
      const row={version:2,storageRevision:1,id:'qa-localization',title:'Language fixture <EN 中文 日本語> '.repeat(10),titled:true,createdAt:Date.now()-1000,updatedAt:Date.now(),graph,history:[],summary:'USER_SUMMARY 中文 日本語',lines:[{text:'USER_TRANSCRIPT 中文 日本語',ms:1}],steps,pages:[{graph,steps}],page:0};
      const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
      await new Promise((yes,no)=>{const t=db.transaction('meetings','readwrite');t.objectStore('meetings').put(row);t.oncomplete=yes;t.onerror=()=>no(t.error);});db.close();localStorage.setItem('boardcast.meeting',row.id);return row;
    });
    await fs.writeFile(path.join(evidence,'fixture.json'),JSON.stringify(fixture,null,2));await page.reload();await page.locator('.ink g[data-key]').first().waitFor();
    report.productionScripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready="true"]').waitFor();
    const rectangle=page.getByRole('radio',{name:'Rectangle',exact:true}).locator('..');await rectangle.waitFor();
    check(await rectangle.isVisible(),'English free-edit rectangle control is translated and visible');await rectangle.click();
    const box=await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    check(await page.evaluate(({x,y})=>document.elementFromPoint(x,y)?.tagName==='CANVAS',{x:box.x+300,y:box.y+120}),'drawing starts on the canvas, outside the style panel');
    await page.mouse.move(box.x+300,box.y+120);await page.mouse.down();await page.mouse.move(box.x+440,box.y+220,{steps:8});await page.mouse.up();
    let free;
    for(let i=0;i<50&&!free;i++){const row=await saved();free=row.scene?.elements.find(e=>e.type==='rectangle'&&!e.isDeleted&&!e.customData?.boardcast);if(!free)await page.waitForTimeout(100);}
    check(!!free,'a real free-edit rectangle is saved before switching language');
    await language('zh');await page.getByRole('radio',{name:'矩形',exact:true}).locator('..').waitFor();
    check(await page.locator('html').getAttribute('lang')==='zh-CN','Chinese free-edit controls update without remounting the meeting');
    await language('en');await rectangle.waitFor();
    check(JSON.stringify((await saved()).scene.elements.find(e=>e.id===free.id))===JSON.stringify(free),'language changes preserve free-object identity, geometry and style');
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
    await language('system');await page.evaluate(()=>{window.qaLanguages=['zh-Hant-TW'];dispatchEvent(new Event('languagechange'));});
    await page.getByRole('button',{name:'设置',exact:true}).waitFor();check(await chinese(),'system language change updates the existing App to Chinese');
    await language('en');await page.evaluate(()=>{window.qaLanguages=['zh-CN'];dispatchEvent(new Event('languagechange'));});check(!await chinese(),'pinned English ignores later system language changes');
    let enFile;
    for(const choice of ['en','zh','system']){
      await page.evaluate(()=>{window.qaLanguages=['ja-JP'];dispatchEvent(new Event('languagechange'));});await language(choice);
      const expected=choice==='zh'?'zh':'en',download=await exportHtml(choice);
      check(download.data.lang===expected&&report.downloads.at(-1).rawHtmlLang===(expected==='en'?'en':'zh-CN'),`${choice}: actual export embeds the resolved interface language`);
      check(download.data.title===fixture.title&&download.data.lines[0].text===fixture.lines[0].text,`${choice}: export preserves multilingual source text`);
      await offline(download.file,expected);if(choice==='en')enFile=download;
    }
    await language('en');let release,entered;const gate=new Promise(resolve=>release=resolve),started=new Promise(resolve=>entered=resolve);
    await page.route('**/player/player.js',async route=>{entered();await gate;await route.continue();});
    const delayed=exportHtml('delayed-en');await Promise.race([started,delayed.then(()=>{throw Error('Player request was not intercepted');})]);
    try{await language('zh');}finally{release();}
    const frozen=await delayed;await page.unroute('**/player/player.js');
    check(frozen.data.lang==='en'&&await chinese(),'HTML language is frozen before asynchronous generation while the App can switch to Chinese');await offline(frozen.file,'en');
    await offline(enFile.file,'en',{blocked:true});
    for(const invalid of [undefined,'unsupported']){
      const data={...enFile.data,lang:invalid};const text=enFile.html.replace(/window\.__boardcastData = (.*);\n/,`window.__boardcastData = ${JSON.stringify(data).replace(/</g,'\\u003c')};\n`);
      const file=path.join(evidence,invalid?'unknown-language.html':'legacy-no-language.html');await fs.writeFile(file,text);
      // Legacy data has no author language; retain the receiver's ordinary English preference.
      await offline(file,'en',{legacy:true});
    }
    const row=await saved();check(row.title===fixture.title&&row.summary===fixture.summary&&JSON.stringify(row.lines)===JSON.stringify(fixture.lines)&&JSON.stringify(row.graph)===JSON.stringify(fixture.graph),'UI and export language changes preserve the source meeting');
    check(report.pageErrors.length===0,'no application script errors');
  }catch(e){report.error=String(e);await page.screenshot({path:path.join(evidence,'failure.png')});throw e;}
  finally{await fs.writeFile(path.join(evidence,'localization-check.json'),JSON.stringify(report,null,2)+'\n');}
};
