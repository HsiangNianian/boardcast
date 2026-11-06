const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

// Real production downloads and Chromium's accessibility tree; no native screen-reader claim.
module.exports = async function checkSvgTitle(page, baseUrl, evidence) {
  await fs.mkdir(evidence,{recursive:true});
  const report={baseUrl,checks:[],failures:[],files:[],pageErrors:[]};
  const check=(ok,label)=>{report[ok?'checks':'failures'].push(label);console.log(`${ok?'PASS':'FAIL'}: ${label}`);};
  const cases=[
    {id:'literal',lang:'en',title:'Review <&> "</title><script>window.injected=true</script>" $& 日本語 😀'},
    {id:'chinese',lang:'zh',title:'计划与评审\n中文・日本語 & "🔭"'},
    {id:'controls',lang:'en',title:'Review\u0000\u000b\ud800\ufffe → 整理 😀',expected:'Review\ufffd\ufffd\ufffd\ufffd → 整理 😀'},
    {id:'blank-en',lang:'en',title:'',expected:'Untitled meeting'},
    {id:'blank-zh',lang:'zh',title:' \t\n ',expected:'未命名会议'},
  ];
  const viewer=await page.context().browser().newContext({reducedMotion:'reduce'});
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    report.scripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    for(const c of cases) {
      await page.evaluate(async c=>{
        const graph={nodes:[{id:'n1',label:'NODE_LABEL',kind:'node',aliases:[],marks:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1};
        const scene={elements:[{id:'free-text',type:'text',x:30,y:180,width:260,height:30,angle:0,strokeColor:'#111111',backgroundColor:'transparent',fillStyle:'solid',strokeWidth:2,strokeStyle:'solid',roughness:0,opacity:100,groupIds:[],frameId:null,roundness:null,index:null,seed:1,version:1,versionNonce:1,updated:0,isDeleted:false,boundElements:null,link:null,locked:false,text:'EDITABLE_LABEL',originalText:'EDITABLE_LABEL',fontSize:20,fontFamily:1,textAlign:'left',verticalAlign:'top',containerId:null,autoResize:true,lineHeight:1.25}],generated:[],files:{}};
        const steps=[{t:1,line:0,d:{nodes:{set:graph.nodes},nextNode:2},scene}];
        const row={version:2,storageRevision:1,id:`qa-svg-${c.id}`,title:c.title,titled:true,createdAt:1,updatedAt:2,graph,scene,steps,history:[],lines:[],pages:[{graph,scene,steps}],page:0};
        const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
        await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(row);tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();
        localStorage.setItem('boardcast.meeting',row.id);localStorage.setItem('boardcast.lang',c.lang);localStorage.setItem('boardcast.mode','ink');
      },c);
      await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
      for(const kind of ['ink','anim','pencil']) {
        if(kind==='pencil') {
          await page.getByRole('radio',{name:c.lang==='zh'?'自由编辑':'Free edit',exact:true}).click();
          await page.locator('canvas.excalidraw__canvas.interactive').waitFor();
        }
        await page.getByRole('button',{name:c.lang==='zh'?'导出':'Export',exact:true}).click();
        await page.getByRole('menuitemradio').last().click();
        const pending=page.waitForEvent('download');
        const label=kind==='anim'?(c.lang==='zh'?'动态 SVG':'Animated SVG'):(c.lang==='zh'?'SVG 矢量图':'SVG vector');
        await page.getByRole('menuitem',{name:new RegExp(`^${label}`)}).click();
        const item=await pending,file=path.join(evidence,`${c.id}-${kind}.svg`);await item.saveAs(file);
        const svg=await fs.readFile(file,'utf8');
        const metadata=await page.evaluate(svg=>{
          const doc=new DOMParser().parseFromString(svg,'image/svg+xml'),root=doc.documentElement;
          return {valid:!doc.querySelector('parsererror'),title:root.querySelector(':scope > title')?.textContent??null,titles:root.querySelectorAll(':scope > title').length,scripts:doc.querySelectorAll('script').length,paths:doc.querySelectorAll('path').length,images:doc.querySelectorAll('image').length,texts:[...doc.querySelectorAll('text')].map(t=>t.textContent)};
        },svg);
        const expected=c.expected??c.title.trim(),record={case:c.id,kind,expected,metadata};report.files.push(record);
        check(metadata.valid&&metadata.title===expected&&metadata.titles===1&&metadata.scripts===0,`${c.id}/${kind}: exported SVG has one literal, valid meeting title`);
        const doc=await viewer.newPage();
        try {
          await doc.goto(pathToFileURL(file).href);
          const session=await doc.context().newCDPSession(doc),ax=await session.send('Accessibility.getFullAXTree');
          record.ax=ax.nodes.filter(n=>!n.ignored).map(n=>({role:n.role?.value,name:n.name?.value}));
          check(record.ax.some(n=>['image','SvgRoot','graphics-document'].includes(n.role)&&n.name===expected.replace(/\s+/g,' ')),`${c.id}/${kind}: the whole SVG has an accessible name`);
          if(kind==='pencil')check(record.ax.some(n=>n.role==='StaticText'&&n.name==='EDITABLE_LABEL'),`${c.id}: free-edit text remains available to assistive technology`);
        } finally {await doc.close();}
      }
    }
    check(report.pageErrors.length===0,'no uncaught browser errors');
  } finally {await viewer.close();await fs.writeFile(path.join(evidence,'report.json'),JSON.stringify(report,null,2));}
  assert.equal(report.failures.length,0,report.failures.join('\n'));
};
