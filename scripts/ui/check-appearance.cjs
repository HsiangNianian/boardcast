const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {createHash} = require('node:crypto');
const {execFileSync} = require('node:child_process');
const {pathToFileURL} = require('node:url');

// Compare visible outlines, excluding animation masks and the screen-only chalk/marker filters.
function outlines(svg) {
  const root = svg ? new DOMParser().parseFromString(svg, 'image/svg+xml') : document.querySelector('.ink');
  return Array.from(root.querySelectorAll('g[fill][opacity]')).map(g => ({
    fill: g.getAttribute('fill').replace('#eeeee6', '#1d1f24'), opacity: Number(g.getAttribute('opacity')),
    paths: Array.from(g.querySelectorAll('path')).filter(p => !p.closest('mask')).map(p => p.getAttribute('d')),
  })).filter(g => g.paths.length).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}

module.exports = async function checkAppearance(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/appearance/browser') {
  await fs.mkdir(evidence, {recursive:true});
  const report = {baseUrl, checks:[], pageErrors:[], preferences:[], themes:[], exports:[]};
  const check = (condition, description) => { assert(condition, description); report.checks.push(description); };
  const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
  const keys = ['boardcast.messy', 'boardcast.weight', 'boardcast.englishWeight'];
  const labels = ['Messiness', 'Chinese, Japanese & line weight', 'English stroke weight'];
  const settings = () => page.getByRole('dialog', {name:'Settings', exact:true});
  const open = () => clickMeetingAction(page, 'Settings');
  const close = () => settings().getByRole('button', {name:'Close', exact:true}).click();
  const reload = async () => { await page.reload(); await page.locator('.splash').waitFor({state:'hidden'}); await page.locator('.ink g[data-key]').first().waitFor({state:'attached'}); };
  const readSliders = () => settings().getByRole('slider').evaluateAll(xs => xs.map(x => ({value:Number(x.value), label:x.closest('label').querySelector('b').textContent})));
  const stored = () => page.evaluate(keys => keys.map(k => Number(localStorage.getItem(k))), keys);
  const stableInk = async () => {
    let previous = '', same = 0;
    for (let i = 0; i < 50; i++) {
      const ink = await page.evaluate(outlines); const next = JSON.stringify(ink);
      same = next === previous && ink.length ? same + 1 : 0; previous = next;
      if (same === 3) return ink;
      await page.waitForTimeout(100);
    }
    throw Error('Ink did not settle');
  };
  const download = async (label, key) => {
    await clickMeetingAction(page, 'Export');
    await page.getByRole('menuitemradio', {name:['Whole meeting PDF', 'Replay page'].includes(label) ? 'Whole meeting' : 'Current board', exact:true}).click();
    const pending = page.waitForEvent('download', {timeout:60000});
    await page.getByRole('menuitem', {name:new RegExp(`^${label}`)}).click();
    const item = await pending; const file = path.join(evidence, `${key}-${item.suggestedFilename()}`);
    await item.saveAs(file); const bytes = await fs.readFile(file);
    return {file, bytes, text:bytes.toString('utf8')};
  };
  page.on('pageerror', e => report.pageErrors.push(e.message));
  try {
    await page.emulateMedia({colorScheme:'light', reducedMotion:'reduce'});
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state:'hidden'});
    const fixture = await page.evaluate(async () => {
      const node = (id, label) => ({id, label, kind:'node', aliases:[], marks:[], ink:'ink'});
      const base = {nodes:[node('n1','Old abc 中ア')], edges:[], groups:[], nextNode:2, nextGroup:1};
      const graph = {...base, nodes:[...base.nodes, node('n2','New xyz 中文')], edges:[{from:'n1',to:'n2',label:'Flow',ink:'ink'}], nextNode:3};
      const steps = [{t:1,line:1,d:{nodes:{set:graph.nodes},edges:{set:graph.edges},nextNode:3}}];
      const row = {version:2,storageRevision:1,id:'qa-appearance',title:'Appearance fixture',titled:true,createdAt:1,updatedAt:2,graph,history:[],lines:[{text:'Synthetic appearance fixture',ms:1}],steps,pages:[{graph,base,steps}],page:0};
      const db = await new Promise((yes,no) => { const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error); });
      await new Promise((yes,no) => {const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(row);tx.oncomplete=yes;tx.onerror=()=>no(tx.error);}); db.close();
      for (const [k,v] of Object.entries({meeting:row.id,lang:'en',mode:'ink',theme:'system'})) localStorage.setItem('boardcast.'+k,v);
      return row;
    });
    await fs.writeFile(path.join(evidence,'fixture.json'), JSON.stringify(fixture,null,2));
    for (const [name, input, expected] of [
      ['invalid',['NaN','Infinity','bad'],[0,65,100]], ['minimum',[-10,0,-200],[0,50,50]],
      ['maximum',[101,300,1000],[100,200,200]], ['fractional',[12.3,63.7,149.2],[12,65,150]],
    ]) {
      await page.evaluate(({keys,input}) => keys.forEach((key,i) => localStorage.setItem(key,String(input[i]))), {keys,input});
      await reload(); await open(); const controls=await readSliders(), saved=await stored();
      report.preferences.push({name,input,expected,controls,saved});
      check(controls.every((c,i) => c.value === expected[i] && Number(c.label.replace('%','')) === expected[i]) && JSON.stringify(saved) === JSON.stringify(expected), `${name}: slider, label and persisted preference agree`);
      if (name === 'fractional') await page.screenshot({path:path.join(evidence,'fractional-normalized.png')});
      await close();
    }
    report.productionScripts = await page.locator('script[src]').evaluateAll(xs => xs.map(x => x.src));
    const originalInk = await stableInk();
    for (const [choice, os, expected] of [
      ['system','light','light'], ['system','dark','dark'], ['system','light','light'],
      ['light','dark','light'], ['dark','light','dark'], ['system','light','light'], ['system','dark','dark'],
    ]) {
      await open(); await settings().getByRole('combobox',{name:'Appearance',exact:true}).selectOption(choice); await close();
      await page.emulateMedia({colorScheme:os});
      const paper = expected === 'dark' ? 'rgb(34, 48, 42)' : 'rgb(251, 251, 248)';
      await page.waitForFunction(paper => getComputedStyle(document.querySelector('.ink')).backgroundColor === paper, paper);
      const ink = await stableInk();
      const state = await page.evaluate(() => ({theme:document.documentElement.dataset.theme ?? null,preference:localStorage.getItem('boardcast.theme'),surface:getComputedStyle(document.documentElement).getPropertyValue('--surface').trim(),paper:getComputedStyle(document.querySelector('.ink')).backgroundColor,fill:document.querySelector('.ink g[data-key]').getAttribute('fill'),filter:document.querySelector('.ink g[filter]').getAttribute('filter')}));
      report.themes.push({choice,os,expected,...state});
      check(state.preference === choice && state.theme === (choice === 'system' ? null : choice) && state.fill === (expected === 'dark' ? '#eeeee6' : '#1d1f24') && state.filter.includes(expected === 'dark' ? 'chalk' : 'marker'), `${choice}/${os}: live board follows effective theme`);
      check(JSON.stringify(ink) === JSON.stringify(originalInk), `${choice}/${os}: theme preserves outline geometry and inherited opacity`);
    }
    await page.screenshot({path:path.join(evidence,'system-dark.png')});
    for (const theme of ['light','dark','system']) {
      await open(); await settings().getByRole('combobox',{name:'Appearance',exact:true}).selectOption(theme); await close(); await reload(); await open();
      check(await settings().getByRole('combobox',{name:'Appearance',exact:true}).inputValue() === theme, `${theme}: theme selection survives reload`); await close();
    }
    const combinations = [[0,65,100]];
    for (const messy of [0,100]) for (const weight of [50,200]) for (const english of [50,200]) combinations.push([messy,weight,english]);
    for (const values of combinations) {
      const key = values.join('-'); await open();
      for (let i=0;i<3;i++) {
        const slider=settings().getByRole('slider',{name:labels[i],exact:true}); await slider.focus(); await slider.press('Home');
        const min=i ? 50 : 0, step=i ? 5 : 1;
        if (values[i] === (i ? 200 : 100)) await slider.press('End');
        else for (let n=min;n<values[i];n+=step) await slider.press('ArrowRight');
      }
      check(JSON.stringify(await stored()) === JSON.stringify(values), `${key}: keyboard range controls persist exact values`); await close();
      await page.waitForFunction(messy => Number(document.querySelector('.ink > svg').dataset.messy) === messy/100,values[0]);
      const live=await stableInk(); check(live.filter(g=>g.opacity===0.68).length === 2 && live.filter(g=>g.opacity===1).length === 4, `${key}: inherited and new strokes remain distinct`);
      const svg=await download('SVG vector',key+'-svg'), anim=await download('Animated SVG',key+'-anim'), png=await download('PNG image',key+'-png');
      const svgInk=await page.evaluate(outlines,svg.text), animInk=await page.evaluate(outlines,anim.text);
      check(JSON.stringify(live) === JSON.stringify(svgInk), `${key}: downloaded SVG matches all live outlines and opacity on light paper`);
      check(JSON.stringify(live) === JSON.stringify(animInk), `${key}: downloaded animation matches all live outlines and opacity`);
      const pixels=await page.evaluate(async ({svg,png}) => {
        const image=new Image();image.src='data:image/png;base64,'+png;await image.decode();
        const doc=new DOMParser().parseFromString(svg,'image/svg+xml');doc.documentElement.setAttribute('width',image.width);doc.documentElement.setAttribute('height',image.height);
        const vector=new Image();vector.src='data:image/svg+xml;base64,'+btoa(new XMLSerializer().serializeToString(doc));await vector.decode();
        const c=document.createElement('canvas');c.width=image.width;c.height=image.height;const x=c.getContext('2d');x.drawImage(image,0,0);const a=x.getImageData(0,0,c.width,c.height).data;x.clearRect(0,0,c.width,c.height);x.drawImage(vector,0,0);const b=x.getImageData(0,0,c.width,c.height).data;
        let differences=0;for(let i=0;i<a.length;i++)if(a[i]!==b[i])differences++;return {width:c.width,height:c.height,differences,channels:a.length};
      },{svg:svg.text,png:png.bytes.toString('base64')});
      check(pixels.differences===0,`${key}: downloaded PNG equals downloaded SVG raster pixel for pixel`);
      const result={values,outlineHash:hash(live),groups:live.map(g=>({opacity:g.opacity,fill:g.fill,strokes:g.paths.length})),pixels,files:[svg.file,anim.file,png.file]};report.exports.push(result);
      if (key !== '0-50-50' && key !== '100-200-200') continue;
      await page.screenshot({path:path.join(evidence,key+'-board.png')});
      const html=await download('Replay page',key+'-html'), pdf=await download('Whole meeting PDF',key+'-pdf');result.files.push(html.file,pdf.file);
      const offlineContext=await page.context().browser().newContext({viewport:{width:1280,height:800},reducedMotion:'reduce',colorScheme:'dark'});
      const requests=[],errors=[];
      try {
        await offlineContext.route(/^https?:\/\//,r=>{requests.push(r.request().url());return r.abort();});await offlineContext.setOffline(true);
        const offline=await offlineContext.newPage();offline.on('pageerror',e=>errors.push(e.message));
        await offline.goto(pathToFileURL(path.resolve(html.file)).href);await offline.waitForFunction(()=>document.querySelectorAll('.ink g[data-key]').length===6);
        const data=await offline.evaluate(()=>({values:[window.__boardcastData.messy,window.__boardcastData.weight,window.__boardcastData.englishWeight],base:window.__boardcastData.pages[0].base,paper:getComputedStyle(document.querySelector('.ink')).backgroundColor}));
        check(JSON.stringify(data.values)===JSON.stringify(values.map(v=>v/100)) && JSON.stringify(data.base)===JSON.stringify(fixture.pages[0].base),`${key}: HTML embeds exact handwriting parameters and inherited base`);
        check(JSON.stringify(await offline.evaluate(outlines))===JSON.stringify(live) && data.paper==='rgb(251, 251, 248)',`${key}: offline HTML renders matching outlines and opacity on light paper`);
        check(!requests.length&&!errors.length,`${key}: actual HTML works in a fresh offline context without HTTP or script errors`);
        result.offline={...data,requests,errors};await offline.screenshot({path:path.join(evidence,key+'-offline.png')});
      } finally {await offlineContext.close();}
      execFileSync('pdfimages',['-f','1','-l','1','-j',pdf.file,path.join(evidence,key+'-pdf')]);
      const jpeg=await fs.readFile(path.join(evidence,key+'-pdf-000.jpg'));
      result.pdf=await page.evaluate(async ({svg,jpeg})=>{
        const doc=new DOMParser().parseFromString(svg,'image/svg+xml'),root=doc.documentElement;
        const [x0,y0,w0,h0]=root.getAttribute('viewBox').split(' ').map(Number),w=w0-16,h=h0-16;
        root.setAttribute('viewBox',`${x0+8} ${y0+8} ${w} ${h}`);root.setAttribute('width',w);root.setAttribute('height',h);root.querySelector('rect').remove();
        const image=new Image();image.src='data:image/svg+xml;base64,'+btoa(new XMLSerializer().serializeToString(doc));await image.decode();
        const actual=new Image();actual.src='data:image/jpeg;base64,'+jpeg;await actual.decode();
        const c=document.createElement('canvas');c.width=actual.width;c.height=actual.height;const x=c.getContext('2d');x.fillStyle='#fbfbf8';x.fillRect(0,0,c.width,c.height);
        const k=Math.min((c.width-110)/w,(c.height-160)/h);x.drawImage(image,(c.width-w*k)/2,110+(c.height-160-h*k)/2,w*k,h*k);
        const expected=new Image();expected.src=c.toDataURL('image/jpeg',0.88);await expected.decode();x.drawImage(expected,0,0);const a=x.getImageData(0,128,c.width,c.height-128).data;x.drawImage(actual,0,0);const b=x.getImageData(0,128,c.width,c.height-128).data;
        let differences=0,dark=0;for(let i=0;i<a.length;i++){if(a[i]!==b[i])differences++;if(i%4===0&&b[i]<100&&b[i+1]<100&&b[i+2]<100)dark++;}
        return {width:c.width,height:c.height,excludedHeaderRows:128,differences,channels:a.length,dark};
      },{svg:svg.text,jpeg:jpeg.toString('base64')});
      check(result.pdf.differences===0&&result.pdf.dark>100,`${key}: PDF board image equals matching SVG geometry and opacity after JPEG encoding (header excluded)`);
    }
    check(new Set(report.exports.map(r=>r.outlineHash)).size===9,'each independent boundary combination changes the rendered handwriting');
    const saved=await page.evaluate(async()=>{const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});const row=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get('qa-appearance');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return row;});
    check(JSON.stringify(saved.graph)===JSON.stringify(fixture.graph)&&JSON.stringify(saved.lines)===JSON.stringify(fixture.lines)&&JSON.stringify(saved.pages[0].base)===JSON.stringify(fixture.pages[0].base),'theme, handwriting and exports preserve saved graph, transcript and base');
    await page.evaluate(()=>localStorage.setItem('boardcast.lang','zh'));await page.setViewportSize({width:640,height:400});await reload();await clickMeetingAction(page, '设置');
    await page.waitForFunction(()=>document.activeElement.textContent==='关闭');
    report.narrowKeyboard=[];
    for(let i=0;i<14&&report.narrowKeyboard.length<3;i++) {
      await page.keyboard.press('Tab');
      if(!await page.evaluate(()=>document.activeElement.type==='range')) continue;
      await page.keyboard.press('Home');
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
      const state=await page.evaluate(()=>{const e=document.activeElement,r=e.getBoundingClientRect(),c=e.closest('.settings-content').getBoundingClientRect();return {name:e.getAttribute('aria-label'),value:Number(e.value),min:Number(e.min),top:r.top,bottom:r.bottom,visible:r.top>=c.top&&r.bottom<=Math.min(c.bottom,innerHeight)};});
      report.narrowKeyboard.push(state);
      check(state.value===state.min&&state.visible,`Chinese narrow settings: Tab then Home keeps ${state.name} visible at its minimum`);
    }
    check(report.narrowKeyboard.length===3,'Chinese narrow settings: Tab reaches all three sliders');
    check(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'Chinese 640×400 settings do not overflow horizontally');await page.screenshot({path:path.join(evidence,'chinese-narrow.png')});
    check(report.pageErrors.length===0,'no application script errors');
  } catch(e) {report.error=String(e);await page.screenshot({path:path.join(evidence,'failure.png')});throw e;}
  finally {await fs.writeFile(path.join(evidence,'appearance-check.json'),JSON.stringify(report,null,2)+'\n');}
};
