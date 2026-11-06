const assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path');
const png='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=';
const element=(id,type,x,y,width,height,extra={})=>({id,type,x,y,width,height,angle:0,strokeColor:'#264d3a',backgroundColor:'transparent',fillStyle:'solid',strokeWidth:2,strokeStyle:'solid',roughness:1,opacity:100,roundness:null,index:null,groupIds:[],frameId:null,seed:3,version:1,versionNonce:1,updated:0,isDeleted:false,boundElements:null,link:null,locked:false,customData:{fixtureId:id,extension:{version:7,note:'preserve imported metadata'}},...extra});
const text=(id,label,x,y,containerId=null)=>element(id,'text',x,y,150,28,{text:label,originalText:label,fontSize:20,fontFamily:5,textAlign:'left',verticalAlign:'top',lineHeight:1.25,autoResize:false,containerId});
const nodes=(names,frames=false)=>names.flatMap((name,i)=>[element('box'+i,'rectangle',40+i*300,80,200,110,{frameId:frames?'frame'+i:null,boundElements:[{id:'label'+i,type:'text'},...(i<2?[{id:'arrow0',type:'arrow'},{id:'arrow1',type:'arrow'}]:[])]}),text('label'+i,name,60+i*300,115,'box'+i)]);
const notes=['Review','Review','RE VIEW'].map((s,i)=>text('note'+i,s,60+i*240,430));
const frames=['Team','TE AM',null].map((name,i)=>element('frame'+i,'frame',20+i*300,40,240,200,{name}));
const arrows=['first','second'].flatMap((label,i)=>[element('arrow'+i,'arrow',240,140+i*80,100,80,{points:[[0,0],[50,i?100:-50],[100,0]],lastCommittedPoint:null,startBinding:{elementId:'box0',focus:0,gap:1},endBinding:{elementId:'box1',focus:0,gap:1},startArrowhead:null,endArrowhead:'arrow',elbowed:false,boundElements:[{id:'arrow-label'+i,type:'text'}]}),text('arrow-label'+i,label,250,190+i*60,'arrow'+i)]);
const free=[element('free-box','rectangle',60,550,200,80,{backgroundColor:'#ecf3ff',groupIds:['external-group'],customData:{fixtureId:'free-box',extension:{pinned:true,language:'ja'}}}),element('free-ink','freedraw',300,550,100,60,{points:[[0,0],[30,55],[100,0]],pressures:[0.5,0.7,0.4],simulatePressure:false,lastCommittedPoint:null,groupIds:['external-group']}),element('free-image','image',500,550,120,80,{fileId:'asset1',status:'saved',scale:[1,1],crop:null})];
const drawing=elements=>({type:'excalidraw',version:2,source:'synthetic-import-validation',elements,files:elements.some(e=>e.type==='image')?{asset1:{id:'asset1',dataURL:png,mimeType:'image/png',created:1}}:{}});
const fixtures={
 repeated:drawing([...nodes(['Service','SERVICE','Service（2）']),...notes]),
 frames:drawing([...frames,...nodes(['Task 1','Task 2','Task 3'],true)]),
 parallel:drawing([...nodes(['A','B']),...arrows]),
 metadata:drawing([...nodes(['Metadata']),...free]),
 rich:drawing([...frames,...nodes(['Service','SERVICE','Service（2）'],true),...arrows,...notes,...free]),
};
const expected={repeated:{node:3,item:3,groups:0,edges:0},frames:{node:3,item:0,groups:3,edges:0},parallel:{node:2,item:0,groups:0,edges:1},metadata:{node:1,item:0,groups:0,edges:0},rich:{node:3,item:3,groups:3,edges:1}};
module.exports=async function checkDrawingImport(page,baseUrl,evidence='docs/ui-ux-audit-2026-11-03/evidence/implementation/drawing-import'){
 await fs.mkdir(evidence,{recursive:true});const report={checks:[],cases:[],pageErrors:[]};page.on('pageerror',e=>report.pageErrors.push(e.message));
 const check=(ok,name)=>report.checks.push({name,ok});
 await page.addInitScript(()=>{localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.mode-intro','1')});
 const panel=()=>page.getByRole('dialog',{name:'Import materials',exact:true});
 const upload=(name,data)=>page.locator('input[type=file]').setInputFiles({name:name+'.excalidraw',mimeType:'application/json',buffer:Buffer.from(typeof data==='string'?data:JSON.stringify(data))});
 const read=()=>page.evaluate(async()=>{if(!(await indexedDB.databases()).some(db=>db.name==='boardcast-v2'))return [];const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});try{return await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)})}finally{db.close()}});
 const current=async()=>{const id=await page.evaluate(()=>localStorage.getItem('boardcast.meeting'));return(await read()).find(m=>m.id===id)};
 const settled=async()=>{await page.locator('.save-state.saved').waitFor();for(let i=0;i<100;i++){const m=await current();if(m?.scene&&!m.scene.pendingImport&&m.scene.overlay)return m;await page.waitForTimeout(100)}throw Error('Imported scene was not saved with its projection')};
 const fidelity=(m,source)=>{
  const elements=m.scene?.elements??m.elements;const found=new Map(elements.filter(e=>!e.isDeleted&&e.customData?.fixtureId).map(e=>[e.customData.fixtureId,e]));
  return source.elements.every(old=>{const e=found.get(old.id);return e&&['type','x','y','width','height','strokeColor','backgroundColor','fillStyle','strokeWidth','strokeStyle','roughness','seed','opacity'].every(k=>e[k]===old[k])&&JSON.stringify(e.customData.extension)===JSON.stringify(old.customData.extension)&&(!old.text||e.originalText===old.originalText)&&(!old.points||JSON.stringify(e.points)===JSON.stringify(old.points))})&&found.size===source.elements.length;
 };
 try{
  await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
  for(const [name,source] of Object.entries(fixtures)){
   const before=await read();await upload(name,source);await page.waitForFunction(()=>!!document.querySelector('dialog[open] [role="alert"]')||[...document.querySelectorAll('dialog[open] button')].some(b=>b.textContent==='New meeting'));
   if(await panel().getByRole('alert').count()){
    report.cases.push({name,error:await panel().getByRole('alert').innerText()});check(false,name+': valid drawing must import');await page.screenshot({path:path.join(evidence,name+'-error.png')});await panel().getByRole('button',{name:'Cancel import',exact:true}).click();continue;
   }
   check(JSON.stringify(await read())===JSON.stringify(before),name+': preview preserves existing meetings');
   await panel().getByRole('button',{name:'New meeting',exact:true}).click();await panel().waitFor({state:'hidden'});let m=await settled();
   const counts={node:m.graph.nodes.filter(n=>n.kind==='node').length,item:m.graph.nodes.filter(n=>n.kind==='item').length,groups:m.graph.groups.length,edges:m.graph.edges.length};
   check(JSON.stringify(counts)===JSON.stringify(expected[name]),name+': distinct semantic objects and graph edge limit');
   check(fidelity(m,source),name+': every source object keeps its visual fields and custom data');
   const after=await read();check(before.every(old=>JSON.stringify(after.find(row=>row.id===old.id))===JSON.stringify(old)),name+': new meeting preserves preceding records');
   await fs.writeFile(path.join(evidence,name+'-meeting.json'),JSON.stringify(m,null,2)+'\n');
   await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.excalidraw').waitFor();
   await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();m=await settled();check(fidelity(m,source),name+': Free edit round trip keeps source objects');
   await page.getByRole('button',{name:'Export',exact:true}).click();await page.getByRole('menuitemradio',{name:'Current board',exact:true}).click();
   const pending=page.waitForEvent('download');await page.getByRole('menuitem',{name:/^Excalidraw/}).click();const download=await pending;const file=path.join(evidence,name+'-export.excalidraw');await download.saveAs(file);const exported=JSON.parse(await fs.readFile(file,'utf8'));
   check(fidelity(exported,source),name+': exported drawing retains source geometry, labels and custom data');
   await upload(name+'-roundtrip',exported);await panel().getByRole('button',{name:'New meeting',exact:true}).waitFor();await panel().getByRole('button',{name:'New meeting',exact:true}).click();await panel().waitFor({state:'hidden'});m=await settled();check(fidelity(m,source),name+': exported drawing reimports without losing source objects');
   await page.reload();await page.locator('.splash').waitFor({state:'hidden'});m=await settled();check(fidelity(m,source),name+': stored drawing survives reload');
   await page.locator('image.free-scene').waitFor();await page.screenshot({path:path.join(evidence,name+'-restored.png')});
   if(name==='parallel'||name==='rich'){const edge=m.graph.edges[0];const keys=await page.locator('[data-key]').evaluateAll(els=>els.map(e=>e.getAttribute('data-key')));check(!['>','~'].some(sep=>keys.includes(`${edge.from}${sep}${edge.to}l|first`)||keys.includes(`${edge.from}${sep}${edge.to}l|bg`)),name+': hand-drawn view does not duplicate the imported edge label');}
   report.cases.push({name,counts,sourceObjects:source.elements.length});
  }
  for(const [name,data] of [['broken','{broken'],['unknown-shape',drawing([element('unsupported','future-shape',0,0,10,10)])],['unsafe-image',{...fixtures.metadata,files:{asset1:{id:'asset1',dataURL:'https://image.invalid/tracker.png'}}}],['duplicate-id',drawing([element('same','rectangle',0,0,10,10),element('same','rectangle',20,0,10,10)])]]){
   const before=await read();await upload(name,data);await panel().getByRole('alert').waitFor();await panel().getByRole('button',{name:'Cancel import',exact:true}).click();check(JSON.stringify(await read())===JSON.stringify(before),name+': invalid input is rejected without changing saved meetings');
  }
  check(report.pageErrors.length===0,'no production JavaScript errors');assert.ok(report.checks.every(c=>c.ok),JSON.stringify(report.checks.filter(c=>!c.ok)));report.passed=true;
 }catch(e){report.error=String(e);await page.screenshot({path:path.join(evidence,'failure.png')}).catch(()=>{});throw e}
 finally{await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2)+'\n')}
};
module.exports.fixtures=fixtures;
