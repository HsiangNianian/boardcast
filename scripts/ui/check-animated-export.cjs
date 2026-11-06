// Actual App downloads, animated/image/static hosts; synthetic meetings and no model requests.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const {existsSync} = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {execFileSync} = require('node:child_process');
const {click: clickMeetingAction} = require('./meeting-action.cjs');

module.exports = async function checkAnimatedExport(page, baseUrl, evidence) {
  await fs.mkdir(evidence, {recursive:true});
  const report = {baseUrl, checks:[], failures:[], pageErrors:[], boards:[]};
  const check = (ok, label) => {report[ok ? 'checks' : 'failures'].push(label); console.log(`${ok ? 'PASS' : 'FAIL'}: ${label}`);};
  const browser = page.context().browser();
  const normal = await browser.newContext({viewport:{width:1600,height:1200}, reducedMotion:'no-preference'});
  // Chromium's page-level media emulation alone does not reach an SVG loaded as an image.
  const reducedBrowser = await browser.browserType().launch({headless:true, executablePath:process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined), args:['--no-sandbox','--force-prefers-reduced-motion']});
  const reduced = await reducedBrowser.newContext({viewport:{width:1600,height:1200}, reducedMotion:'reduce'});
  const compare = async (context, actual, expected, mime, waitMs) => {
    const host = await context.newPage();
    host.on('pageerror', error => report.pageErrors.push(error.message));
    try {
      return await host.evaluate(async ({actual,expected,mime,waitMs}) => {
        const load = async (bytes, mount) => {const img=new Image();img.src=`data:${mime};base64,${bytes}`;if(mount)document.body.append(img);await img.decode();return img;};
        const [a,b] = await Promise.all([load(actual,true),load(expected,false)]);
        if (a.width !== b.width || a.height !== b.height) throw Error('Export dimensions differ');
        const canvas=document.createElement('canvas');canvas.width=a.width;canvas.height=a.height;
        const ctx=canvas.getContext('2d');
        const pixels=img=>{ctx.clearRect(0,0,a.width,a.height);ctx.drawImage(img,0,0);return ctx.getImageData(0,0,a.width,a.height).data;};
        const diff=(x,y)=>{let changed=0,max=0;for(let i=0;i<x.length;i+=4){const d=Math.max(...[0,1,2,3].map(j=>Math.abs(x[i+j]-y[i+j])));if(d>10)changed++;max=Math.max(max,d);}return {changed,max};};
        const reference=pixels(b),first=pixels(a);
        await new Promise(resolve=>setTimeout(resolve,waitMs));
        const last=pixels(a);
        return {width:a.width,height:a.height,reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,first:diff(first,reference),last:diff(last,reference),motion:diff(first,last)};
      }, {actual:actual.toString('base64'),expected:expected.toString('base64'),mime,waitMs});
    } finally {await host.close();}
  };
  const download = async (label, name) => {
    await page.getByRole('button',{name:'Export',exact:true}).click();
    await page.getByRole('menuitemradio',{name:'Current board',exact:true}).click();
    const pending=page.waitForEvent('download');
    await page.getByRole('menuitem',{name:new RegExp(`^${label}`)}).click();
    const item=await pending, file=path.join(evidence,`${name}-${item.suggestedFilename()}`);
    await item.saveAs(file);
    return {file,bytes:await fs.readFile(file)};
  };
  const stored = () => page.evaluate(async () => {
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get('qa-animated-export');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    db.close();return JSON.stringify(row);
  });
  page.on('pageerror', error=>report.pageErrors.push(error.message));
  await page.addInitScript(()=>{if(!localStorage.getItem('boardcast.lang'))localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.mode','ink');localStorage.setItem('boardcast.theme','light');});
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    report.scripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    await page.evaluate(async () => {
      const empty=()=>({nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1});
      const node=(id,label,extra={})=>({id,label,kind:'node',aliases:[],marks:[],ink:'ink',...extra});
      const png=color=>{const c=document.createElement('canvas');c.width=640;c.height=360;const x=c.getContext('2d');x.fillStyle=color;x.fillRect(0,0,640,360);return c.toDataURL();};
      const shape=(id,type,extra)=>({id,type,x:100,y:240,width:100,height:60,angle:0,strokeColor:'#ff00ff',backgroundColor:'#ff00ff',fillStyle:'solid',strokeWidth:2,strokeStyle:'solid',roughness:0,opacity:100,groupIds:[],frameId:null,roundness:null,index:null,seed:1,version:1,versionNonce:1,updated:0,isDeleted:false,boundElements:null,link:null,locked:false,...extra});
      const scene={elements:[shape('free-block','rectangle'),shape('free-image','image',{x:220,fileId:'red',status:'saved',scale:[1,1],crop:null})],generated:[],files:{red:{id:'red',mimeType:'image/png',dataURL:png('#ee2222'),created:1}}};
      const first=node('n1','Highlighted note',{marks:['高亮']}),second=node('n2','Earlier node');
      const graph={...empty(),nodes:[first,second],edges:[{from:'n1',to:'n2',label:'Link',ink:'ink'}],nextNode:3,underlay:{w:640,h:360}};
      const steps=[{t:1,line:0,d:{nodes:{set:[second]},nextNode:3,underlay:graph.underlay}},{t:2,line:1,d:{nodes:{set:[{...first,marks:[]}]},edges:{set:graph.edges}}},{t:3,line:2,d:{nodes:{set:[first]}},scene}];
      const source=(id,y)=>node(id,`Source ${id}`,{kind:'region',marks:['高亮'],text:`Source ${id}`,at:{x:40,y,w:240,h:24},rows:[{y,h:24,segs:[{t:`Source ${id}`,x:40,w:240}]}]});
      const marked={...empty(),nodes:[source('n1',60),source('n2',150)],nextNode:3,underlay:{w:640,h:360}};
      const marks=[{t:1,line:0,d:{nodes:{set:marked.nodes.map(n=>({...n,marks:[]}))},nextNode:3,underlay:marked.underlay}},{t:2,line:1,d:{nodes:{set:[marked.nodes[0]]}}},{t:3,line:2,d:{nodes:{set:[marked.nodes[1]]}}}];
      const pages=[{graph,steps,scene,underlay:png('#daeaf8')},{graph:marked,steps:marks,underlay:png('#daeaf8')},{graph:empty(),steps:[{t:1,line:0,d:{},scene}],scene}];
      const row={version:2,storageRevision:1,id:'qa-animated-export',title:'Animated export fixture',titled:true,createdAt:1,updatedAt:2,...pages[0],pages,page:0,history:[],lines:[]};
      const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
      await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(row);tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();localStorage.setItem('boardcast.meeting',row.id);
    });
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    for (let n=1;n<=3;n++) {
      await page.locator('.pages').getByRole('button',{name:`Board ${n}`,exact:true}).click();
      await page.waitForTimeout(700);
      const before=await stored();
      const still=await download('SVG vector',`board${n}-static`), anim=await download('Animated SVG',`board${n}-animated`);
      const text=anim.bytes.toString('utf8');
      const metadata=await page.evaluate(svg=>{
        const doc=new DOMParser().parseFromString(svg,'image/svg+xml');
        const delays=[...doc.querySelectorAll('.w,.pop')].map(p=>({delay:parseFloat(p.style.animationDelay)||0,duration:parseFloat(p.style.animationDuration)||1}));
        return {invalid:!!doc.querySelector('parsererror'),scripts:doc.querySelectorAll('script').length,end:Math.max(0,...delays.map(d=>d.delay+d.duration)),popDelays:[...doc.querySelectorAll('.pop')].map(p=>parseFloat(p.style.animationDelay)),images:doc.querySelectorAll('image').length};
      },text);
      const board={board:n,metadata};report.boards.push(board);
      check(!metadata.invalid&&!metadata.scripts,`board ${n}: standalone XML contains no scripts`);
      check(metadata.end<=12001,`board ${n}: animation stays within twelve seconds`);
      if(n===2) check(metadata.popDelays.some(d=>d>=300), 'source-only highlights wait for their own round');
      if(n!==2) check(metadata.images>0,`board ${n}: free objects remain embedded`);
      board.normal=await compare(normal,anim.bytes,still.bytes,'image/svg+xml',metadata.end+200);
      check(board.normal.last.changed===0,`board ${n}: natural image playback ends at the static SVG pixels`);
      check(n===3 ? board.normal.motion.changed===0 : board.normal.motion.changed>20,`board ${n}: ${n===3?'free objects stay static':'the saved rounds visibly progress'}`);
      board.reduced=await compare(reduced,anim.bytes,still.bytes,'image/svg+xml',350);
      check(board.reduced.reduced&&board.reduced.first.changed===0&&board.reduced.last.changed===0&&board.reduced.motion.changed===0,`board ${n}: reduced-motion image immediately shows the full unchanged board`);
      const doc=await reduced.newPage();
      try {
        await doc.goto(pathToFileURL(anim.file).href);
        board.document=await doc.evaluate(()=>({reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,animations:document.getAnimations().length,hidden:[...document.querySelectorAll('.w')].filter(p=>parseFloat(getComputedStyle(p).strokeDashoffset)!==0).length}));
        check(board.document.reduced&&board.document.animations===0&&board.document.hidden===0,`board ${n}: reduced-motion standalone document has no hidden strokes or animation`);
        await doc.locator('svg').screenshot({path:path.join(evidence,`board${n}-reduced.png`)});
      } finally {await doc.close();}
      for(const file of [anim.file,still.file]) execFileSync('rsvg-convert',[file,'-o',`${file}.png`]);
      board.staticRenderer=await compare(normal,await fs.readFile(`${anim.file}.png`),await fs.readFile(`${still.file}.png`),'image/png',0);
      check(board.staticRenderer.first.changed===0,`board ${n}: librsvg shows the complete board without CSS animation`);
      check(before===await stored(),`board ${n}: export leaves the stored meeting unchanged`);
    }
    await page.setViewportSize({width:640,height:400});
    for (const [lang,label,hint] of [['en','Animated SVG','free drawings stay still'],['zh','动态 SVG','自由绘图保持静态']]) {
      await page.evaluate(lang=>localStorage.setItem('boardcast.lang',lang),lang);
      await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
      await clickMeetingAction(page,lang==='en'?'Export':'导出');
      await page.getByRole('menuitemradio').last().click();
      const item=page.getByRole('menuitem',{name:new RegExp(`^${label}`)});
      const bounds=await item.boundingBox();
      check((await item.innerText()).includes(hint)&&bounds&&bounds.x>=0&&bounds.y>=0&&bounds.x+bounds.width<=640&&bounds.y+bounds.height<=400,`${lang}: compact export explains static free drawings before download`);
      await page.screenshot({path:path.join(evidence,`menu-${lang}.png`)});
      await page.keyboard.press('Escape');
    }
    check(report.pageErrors.length===0,'no uncaught browser errors');
  } finally {
    await normal.close();await reduced.close();await reducedBrowser.close();
    await fs.writeFile(path.join(evidence,'report.json'),JSON.stringify(report,null,2));
  }
  assert.equal(report.failures.length,0,report.failures.join('\n'));
};
