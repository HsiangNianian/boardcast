const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const {pathToFileURL} = require('node:url');

module.exports = async function checkPdfDirections(page, baseUrl, evidence = '/tmp/boardcast-pdf-directions') {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],errors:[],ranges:[],files:{}};
  const check=(ok,name)=>report.checks.push({name,ok:!!ok});
  page.on('pageerror',e=>report.errors.push(e.message));
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.addInitScript(()=>{for(const[k,v]of Object.entries({lang:'en',mode:'ink',log:'closed','mode-hint':'dismissed'}))localStorage.setItem('boardcast.'+k,v)});
  const fixture=await page.context().newPage();
  await fixture.setContent(`<!doctype html><title>PDF direction checks</title><style>@page{size:900px 760px;margin:0}body{margin:0;font:20px/30px 'Noto Sans CJK SC',sans-serif}p{position:absolute;margin:0;white-space:nowrap;transform-origin:0 0}</style>
    <p style="left:60px;top:30px">Horizontal <span id="horizontal">selection</span> stays readable.</p>
    <p style="left:260px;top:100px;transform:rotate(90deg)">Rotated <span id="clockwise">selection</span> keeps its direction.</p>
    <p style="left:800px;top:650px;transform:rotate(180deg)">Inverted <span id="inverted">selection</span> stays readable.</p>
    <p style="left:650px;top:570px;transform:rotate(-90deg)">Upright <span id="counterclockwise">selection</span> follows the text.</p>
    <p style="left:70px;top:410px;transform:rotate(25deg)">Slanted <span id="slanted">selection</span> is preserved.</p>
    <p style="left:770px;top:90px;writing-mode:vertical-rl;height:500px">竖排会议需要<span id="vertical">保存</span>记录。第二句话。</p>
    <p style="left:8px;top:650px;font-size:10px;transform:rotate(-90deg)">arXiv: <b>1234.56789v1</b> [cs.AI] 1 Jan 2024</p>`);
  await fixture.evaluate(()=>document.fonts.ready);
  report.points=await fixture.locator('[id]').evaluateAll(es=>Object.fromEntries(es.map(e=>{const r=e.getBoundingClientRect();return[e.id,{x:r.x+r.width/2,y:r.y+r.height/2,rect:r.toJSON()}]})));
  const sourceImage=await fixture.screenshot({path:evidence+'/source.png',clip:{x:0,y:0,width:900,height:760}});
  const pdf=await fixture.pdf({path:evidence+'/source.pdf',preferCSSPageSize:true});
  await fixture.setContent(`<style>@page{size:900px 760px;margin:0}body{margin:0}img{width:900px;height:760px}</style><img src="data:image/png;base64,${sourceImage.toString('base64')}">`);
  await fixture.locator('img').evaluate(i=>i.decode());
  const scan=await fixture.pdf({path:evidence+'/scan.pdf',preferCSSPageSize:true});await fixture.close();
  const read=()=>page.evaluate(async()=>{const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});const m=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});db.close();return m});
  const saved=async predicate=>{for(let i=0;i<160;i++){const m=await read();if(m&&predicate(m))return m;await page.waitForTimeout(50)}throw Error('PDF direction state did not save')};
  const toolbar=()=>page.locator('.ink-toolbar');
  const clickAt=async point=>{
    await page.locator('.ink').click({trial:true});
    await page.waitForFunction(()=>{const s=document.querySelector('.ink > svg'),b=s.getBoundingClientRect(),v=s.viewBox.baseVal;return Math.abs(b.width/b.height-v.width/v.height)<0.002});
    const at=await page.locator('.ink > svg').evaluate((svg,p)=>{const q=new DOMPoint(p.x,p.y).matrixTransform(svg.getScreenCTM());return{x:q.x,y:q.y}},point);
    await page.mouse.click(at.x,at.y);await page.waitForTimeout(100);return toolbar().isVisible();
  };
  const importFile=async(buffer,name,first=false)=>{
    await page.locator('input[type=file]').setInputFiles({name,mimeType:'application/pdf',buffer});
    const dialog=page.getByRole('dialog',{name:'Import materials',exact:true});
    await dialog.getByRole('button',{name:first?'Add to this meeting':'New meeting',exact:true}).waitFor({timeout:90000});
    const text=await dialog.innerText();await dialog.getByRole('button',{name:first?'Add to this meeting':'New meeting',exact:true}).click();await dialog.waitFor({state:'hidden'});await page.locator('.save-state.saved').waitFor();return text;
  };
  const download=async(label,name,extension)=>{
    await page.getByRole('button',{name:'Export',exact:true}).click();await page.getByRole('menuitemradio',{name:['Markdown','Replay page','Whole meeting PDF'].includes(label)?'Whole meeting':'Current board',exact:true}).click();
    const pending=page.waitForEvent('download',{timeout:60000});await page.getByRole('menuitem',{name:new RegExp('^'+label)}).click();
    const file=path.join(evidence,`${name}.${extension}`);await(await pending).saveAs(file);const bytes=await fs.readFile(file);report.files[`${name}.${extension}`]={bytes:bytes.length};return{file,bytes,text:bytes.toString('utf8')};
  };
  const labels={horizontal:'Horizontal',clockwise:'Rotated',inverted:'Inverted',counterclockwise:'Upright',slanted:'Slanted',vertical:'竖排'};
  const angles={horizontal:0,clockwise:Math.PI/2,inverted:Math.PI,counterclockwise:-Math.PI/2,slanted:25*Math.PI/180,vertical:0};
  const inside=(point,b)=>{const a=b.angle||0,c=Math.cos(a),s=Math.sin(a),x=point.x*c+point.y*s,y=-point.x*s+point.y*c;return x>=b.x-5&&x<=b.x+b.w+5&&y>=b.y-5&&y<=b.y+b.h+5};
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});await importFile(pdf,'directions.pdf',true);
    const original=await read();report.original=original.graph;
    check(!original.graph.nodes.some(n=>(n.text??n.label).includes('arXiv:')),'the marginal arXiv watermark is excluded');
    check(original.graph.nodes.length===6,'all six source regions remain without fragments of the split-font watermark');
    for(const[name,point]of Object.entries(report.points)){
      const n=original.graph.nodes.find(n=>(n.text??n.label).startsWith(labels[name]));
      check(n?.rows?.length>0,`${name}: PDF text remains an addressable source region`);
      if(!n?.rows?.length)continue;
      const visible=await clickAt(point);check(visible,`${name}: clicking the independently measured PDF word opens its toolbar`);if(!visible)continue;
      await toolbar().getByRole('button',{name:'Word',exact:true}).click();await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
      const m=await saved(m=>m.graph.nodes.find(x=>x.id===n.id).spans?.some(s=>s.mark==='高亮'));
      const current=m.graph.nodes.find(x=>x.id===n.id),span=current.spans?.find(s=>s.mark==='高亮');report.ranges.push({name,id:n.id,span});
      check(span?.text===(name==='vertical'?'保存':'selection')&&span.boxes.some(b=>inside(point,b))&&span.boxes.every(b=>Math.abs(Math.atan2(Math.sin((b.angle||0)-angles[name]),Math.cos((b.angle||0)-angles[name])))<1e-5),`${name}: word text and oriented bounds agree with the source PDF`);
      if(span)await page.locator(`.ink [data-key^="${n.id}|高亮|"]`).waitFor();
      await page.screenshot({path:evidence+`/word-${name}.png`});
    }
    const clockwise=report.ranges.find(r=>r.name==='clockwise');
    if(clockwise?.span){
      await clickAt(report.points.clockwise);await toolbar().getByRole('button',{name:'Word',exact:true}).click();
      for(const[label,mark]of [['Circle','圈出'],['Underline','下划线'],['Strike','划掉'],['Star','星标']]){
        await toolbar().getByRole('button',{name:label,exact:true}).click();const m=await saved(m=>m.graph.nodes.find(n=>n.id===clockwise.id).spans?.some(s=>s.mark===mark));
        check(m.graph.nodes.find(n=>n.id===clockwise.id).spans.find(s=>s.mark===mark).text==='selection',`rotated ${label.toLowerCase()} retains the selected word`);
      }
      await page.getByRole('button',{name:'Undo',exact:true}).click();await saved(m=>!m.graph.nodes.find(n=>n.id===clockwise.id).spans.some(s=>s.mark==='星标'));
      await page.getByRole('button',{name:'Redo',exact:true}).click();await saved(m=>m.graph.nodes.find(n=>n.id===clockwise.id).spans.some(s=>s.mark==='星标'));check(true,'a rotated mark remains one undo/redo step');
    }
    const vertical=report.ranges.find(r=>r.name==='vertical');
    if(vertical?.span){await clickAt(report.points.vertical);await toolbar().getByRole('button',{name:'Sentence',exact:true}).click();await toolbar().getByRole('button',{name:'Star',exact:true}).click();const m=await saved(m=>m.graph.nodes.find(n=>n.id===vertical.id).spans.some(s=>s.mark==='星标'));const span=m.graph.nodes.find(n=>n.id===vertical.id).spans.find(s=>s.mark==='星标');check(span.text.normalize('NFKC')==='竖排会议需要保存记录。','vertical punctuation keeps the next sentence outside the selected range');}
    report.finalGraph=(await read()).graph;await page.screenshot({path:evidence+'/hand-marks.png'});
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready=true]').waitFor({timeout:60000});
    const portable=await download('Excalidraw','directions','excalidraw'),free=JSON.parse(portable.text);report.freeElements=free.elements;
    check(JSON.stringify(free.boardcast.graph)===JSON.stringify(report.finalGraph),'portable drawing retains exact source rows, directions and marks');
    for(const range of report.ranges.filter(r=>r.span?.boxes.some(b=>b.angle))){const elements=free.elements.filter(e=>e.id.startsWith(`${range.id}:span:高亮:`));const point=report.points[range.name];check(elements.length===range.span.boxes.length&&elements.every(e=>Math.abs(e.angle-range.span.boxes[0].angle)<1e-5)&&elements.some(e=>{const dx=point.x-e.x-e.width/2,dy=point.y-e.y-e.height/2,c=Math.cos(e.angle),s=Math.sin(e.angle);return Math.abs(dx*c+dy*s)<e.width/2+5&&Math.abs(-dx*s+dy*c)<e.height/2+5}),`${range.name}: Free edit projects the same rotated highlight at the original word`);}
    await page.screenshot({path:evidence+'/free-marks.png'});
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.locator('.ink > svg').waitFor();
    check(JSON.stringify((await read()).graph)===JSON.stringify(report.finalGraph),'reopening preserves every source direction and mark');
    await download('SVG vector','directions','svg');await download('Animated SVG','directions-animated','svg');await download('PNG image','directions','png');await download('Whole meeting PDF','directions-export','pdf');
    const html=await download('Replay page','directions-replay','html');
    const offline=await page.context().browser().newContext({offline:true,viewport:{width:1280,height:800},reducedMotion:'reduce'});
    try{const replay=await offline.newPage();const requests=[];replay.on('pageerror',e=>report.errors.push(e.message));replay.on('request',r=>{if(/^https?:/.test(r.url()))requests.push(r.url())});await replay.goto(pathToFileURL(path.resolve(html.file)).href);await replay.getByRole('slider').first().focus();await replay.keyboard.press('End');await replay.locator('.ink [data-key]').first().waitFor();
      for(const r of report.ranges.filter(r=>r.span)){check(await replay.locator(`.ink [data-key^="${r.id}|高亮|"]`).count()>0,`${r.name}: the actual offline HTML replays its saved highlight`)}
      check(!requests.length,'offline replay has no HTTP requests');await replay.screenshot({path:evidence+'/offline.png'});
    }finally{await offline.close();}
    await page.locator('input[type=file]').setInputFiles({name:'directions.excalidraw',mimeType:'application/json',buffer:portable.bytes});const restore=page.getByRole('dialog',{name:'Import materials',exact:true});await restore.getByRole('button',{name:'New meeting',exact:true}).click();await restore.waitFor({state:'hidden'});await saved(m=>m.id!==original.id);
    check(JSON.stringify((await read()).graph)===JSON.stringify(report.finalGraph),'reimporting the portable drawing keeps source coordinates and directions');
    execFileSync('qpdf',[evidence+'/source.pdf',evidence+'/rotated-page.pdf','--rotate=+90']);
    await importFile(await fs.readFile(evidence+'/rotated-page.pdf'),'rotated-page.pdf');
    const rotated=await read();report.rotatedGraph=rotated.graph;
    for(const name of ['horizontal','clockwise','inverted','slanted']){const p=report.points[name],point={x:(760-p.y)*900/760,y:p.x*900/760};const n=rotated.graph.nodes.find(n=>(n.text??n.label).startsWith(labels[name]));const visible=await clickAt(point);check(n?.rows?.length&&visible,`${name}: PDF page /Rotate keeps source text and pointer alignment`);if(n?.rows?.length&&visible){await toolbar().getByRole('button',{name:'Word',exact:true}).click();await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();const m=await saved(m=>m.graph.nodes.find(x=>x.id===n.id).spans?.length);check(m.graph.nodes.find(x=>x.id===n.id).spans[0].text==='selection',`${name}: page rotation still selects the exact word`);}}
    await page.screenshot({path:evidence+'/page-rotation.png'});
    report.scanSummary=await importFile(scan,'scan.pdf');const scanned=await read();report.scanGraph=scanned.graph;
    const scanImage=await page.locator('.ink image').first().evaluate(async e=>{const image=new Image();image.src=e.getAttribute('href');await image.decode();return{width:image.naturalWidth,height:image.naturalHeight}});
    report.scanImage=scanImage;
    check(scanned.graph.nodes.length>0&&scanned.graph.nodes.every(n=>!n.rows)&&scanImage.width>0&&scanImage.height>0,'image-only PDF preserves the visible page without inventing OCR text');
    await clickAt(report.points.horizontal);check(!await toolbar().getByRole('button',{name:'Word',exact:true}).count(),'scanned-page region does not offer an unavailable word selection');
    await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();await saved(m=>m.graph.nodes.some(n=>n.marks.includes('高亮')));check(true,'a scanned page still supports a persistent region mark');
    await page.screenshot({path:evidence+'/scan-marked.png'});await download('Replay page','scan-replay','html');
    check(report.errors.length===0,'direction/import/export flows have no uncaught browser errors');report.passed=report.checks.every(c=>c.ok);assert(report.passed,JSON.stringify(report.checks.filter(c=>!c.ok)));return report;
  }catch(error){report.error=String(error);await page.screenshot({path:evidence+'/failure.png'}).catch(()=>{});report.aria=await page.locator('body').ariaSnapshot();throw error;}
  finally{await fs.writeFile(evidence+'/pdf-directions-check.json',JSON.stringify(report,null,2)+'\n');}
};
