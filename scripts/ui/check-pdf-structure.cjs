const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const {pathToFileURL}=require('node:url');

module.exports=async function checkPdfStructure(page,baseUrl,evidence='/tmp/boardcast-pdf-structure'){
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],ranges:[],errors:[],files:{}};
  const check=(ok,name)=>report.checks.push({name,ok:!!ok});
  page.on('pageerror',e=>report.errors.push(e.message));await page.emulateMedia({reducedMotion:'reduce'});
  await page.addInitScript(()=>{for(const[k,v]of Object.entries({lang:'en',mode:'ink',log:'closed','mode-hint':'dismissed'}))localStorage.setItem('boardcast.'+k,v)});
  const fixture=await page.context().newPage();
  await fixture.setContent(`<!doctype html><title>PDF structure checks</title><style>@page{size:900px 900px;margin:0}body{margin:0;font:20px/30px 'Noto Sans CJK SC',sans-serif}.sheet{position:relative;width:900px;height:900px;break-after:page}.sheet:last-child{break-after:auto}p{position:absolute;margin:0}h1{position:absolute;left:60px;top:20px;font-size:24px}.vertical{writing-mode:vertical-rl;height:180px;width:100px;line-height:36px;top:100px}.cell{position:absolute;font-size:16px;border:1px solid #888;width:210px;height:40px;padding:5px;box-sizing:border-box}.body{left:60px;width:760px;font-size:18px;line-height:28px}</style>
    <div class="sheet"><h1>Vertical reading order</h1>
      <p class="vertical" style="left:140px">本地系统需要<span id="save">保存</span><span id="meetingFirst">会</span><span id="meetingSecond">议</span>记录。下一句。</p>
      <p class="vertical" style="left:550px">右列需要<span id="rightSave">保存</span>甲项。左列需要<span id="leftSave">保存</span>乙项。</p>
    </div>
    <div class="sheet"><p style="left:60px;top:50px;font-size:18px">Table 1. Status of the two independent systems.</p>
      <div class="cell" style="left:60px;top:100px"><span id="tableLeft">Alpha ready</span></div><div class="cell" style="left:270px;top:100px"><span id="tableRight">Beta waiting</span></div>
      <div class="cell" style="left:60px;top:140px">Gamma ready</div><div class="cell" style="left:270px;top:140px">Delta waiting</div>
      ${['甲乙丙','丁戊己'].flatMap((text,col)=>[...text].map((t,row)=>`<p ${col===0&&row===0?'id="tableCjk"':''} style="left:${550+col*36}px;top:${100+row*20}px;width:20px;line-height:20px">${t}</p>`)).join('')}
      <p class="body" style="top:250px">This paragraph explains the table without becoming part of its selected region. The original source and the formula below must remain separate.</p>
      <p style="left:130px;top:400px;font-size:22px">E = <span id="term">mc</span><sup id="exponent" style="font-size:14px">2</sup></p><p style="left:760px;top:400px;font-size:22px">(<span id="equationNumber">7</span>)</p>
      <p class="body" style="top:520px">This independent paragraph explains the equation and establishes a boundary before the diagram. It must remain separate from the caption and its labels.</p>
      <p style="left:140px;top:650px;font-size:16px">Input</p><p style="left:360px;top:650px;font-size:16px">Output</p>
      <p style="left:60px;top:720px;font-size:18px">Figure 2. A simple diagram with two labels.</p>
    </div>`);
  await fixture.evaluate(()=>document.fonts.ready);
  report.points=await fixture.locator('[id]').evaluateAll(es=>Object.fromEntries(es.map(e=>{const r=e.getBoundingClientRect(),sheet=e.closest('.sheet').getBoundingClientRect();return[e.id,{x:r.x+r.width/2,y:r.y-sheet.y+r.height/2,rect:{x:r.x,y:r.y-sheet.y,w:r.width,h:r.height}}]})));
  await fixture.screenshot({path:evidence+'/source.png',fullPage:true});const pdf=await fixture.pdf({path:evidence+'/source.pdf',preferCSSPageSize:true});await fixture.close();
  const read=()=>page.evaluate(async()=>{const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});const m=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});db.close();return m});
  const saved=async predicate=>{for(let i=0;i<160;i++){const m=await read();if(m&&predicate(m))return m;await page.waitForTimeout(50)}throw Error('Structure annotation did not save')};
  const toolbar=()=>page.locator('.ink-toolbar');
  const clickAt=async point=>{await page.locator('.ink').click({trial:true});await page.waitForFunction(()=>{const s=document.querySelector('.ink > svg'),b=s.getBoundingClientRect(),v=s.viewBox.baseVal;return Math.abs(b.width/b.height-v.width/v.height)<0.002});const at=await page.locator('.ink > svg').evaluate((svg,p)=>{const q=new DOMPoint(p.x,p.y).matrixTransform(svg.getScreenCTM());return{x:q.x,y:q.y}},point);await page.mouse.click(at.x,at.y);await page.waitForTimeout(100);return toolbar().isVisible()};
  const manual=async(name,unit,expected)=>{
    const visible=await clickAt(report.points[name]);check(visible,`${name}: source point opens the annotation toolbar`);if(!visible)return;
    const scope=toolbar().getByRole('button',{name:unit,exact:true});check(await scope.count()>0,`${name}: ${unit} is available for the text layer`);if(!await scope.count())return;
    await scope.click();await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
    const m=await saved(m=>m.graph.nodes.some(n=>n.spans?.length));const n=m.graph.nodes.find(n=>n.spans?.length),span=n.spans[0];
    report.ranges.push({name,unit,id:n.id,expected,span});
    check(span.text.normalize('NFKC')===expected,`${name}: ${unit} follows the source reading order`);
    const p=report.points[name];check(span.boxes.some(b=>p.x>=b.x-5&&p.x<=b.x+b.w+5&&p.y>=b.y-5&&p.y<=b.y+b.h+5),`${name}: mark covers the actual clicked glyph`);
    if(unit==='Word'&&['save','rightSave','leftSave','meetingFirst','meetingSecond'].includes(name)){
      const rects=(name.startsWith('meeting')?['meetingFirst','meetingSecond']:[name]).map(k=>report.points[k].rect);
      check(span.boxes.length===2&&span.boxes.every(b=>rects.some(r=>b.x>=r.x-5&&b.x+b.w<=r.x+r.w+5&&b.y>=r.y-5&&b.y+b.h<=r.y+r.h+5)),`${name}: both selected glyphs stay inside the independently measured source word`);
    }
    await page.screenshot({path:evidence+`/${name}-${unit}.png`});
    await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();await saved(m=>m.graph.nodes.every(n=>!n.spans?.length));
  };
  const download=async(label,name,extension)=>{
    await page.getByRole('button',{name:'Export',exact:true}).click();await page.getByRole('menuitemradio',{name:['Replay page','Whole meeting PDF'].includes(label)?'Whole meeting':'Current board',exact:true}).click();
    const pending=page.waitForEvent('download',{timeout:60000});await page.getByRole('menuitem',{name:new RegExp('^'+label)}).click();
    const file=path.join(evidence,`${name}.${extension}`);await(await pending).saveAs(file);const bytes=await fs.readFile(file);report.files[`${name}.${extension}`]={bytes:bytes.length};check(bytes.length>100,`${name}.${extension}: actual download is nonempty`);return{file,bytes,text:bytes.toString('utf8')};
  };
  try{
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});await page.locator('input[type=file]').setInputFiles({name:'structure.pdf',mimeType:'application/pdf',buffer:pdf});
    const dialog=page.getByRole('dialog',{name:'Import materials',exact:true});await dialog.getByRole('button',{name:'Add to this meeting',exact:true}).click({timeout:90000});await dialog.waitFor({state:'hidden'});await page.locator('.save-state.saved').waitFor();
    const original=await read();report.original=original;check(original.pages.length===2,'real source PDF creates two separate boards');
    for(const args of [['save','Word','保存'],['meetingFirst','Word','会议'],['meetingSecond','Word','会议'],['save','Sentence','本地系统需要保存会议记录。'],['rightSave','Word','保存'],['leftSave','Word','保存'],['rightSave','Sentence','右列需要保存甲项。'],['leftSave','Sentence','左列需要保存乙项。']])await manual(...args);
    await page.getByRole('button',{name:'Board 2',exact:true}).click();await page.locator('.pages button[aria-current=page][aria-label="Board 2"]').waitFor();
    await manual('term','Word','mc2');await manual('equationNumber','Word','7');
    await manual('term','Sentence','E = mc2 (7)');
    for(const point of ['tableLeft','tableRight','tableCjk']){
      const visible=await clickAt(report.points[point]);check(visible,`${point}: the recognized table is selectable`);if(!visible)continue;
      check(!await toolbar().getByRole('button',{name:'Word',exact:true}).count(),`${point}: table annotation clearly uses the whole-region capability`);
      await toolbar().getByRole('button',{name:'Circle',exact:true}).click();const m=await saved(m=>m.graph.nodes.some(n=>n.marks.includes('圈出')));const n=m.graph.nodes.find(n=>n.marks.includes('圈出'));report.tableMarks??=[];report.tableMarks.push(n);
      check(n.at.y<100&&n.at.y+n.at.h>=180&&n.at.y+n.at.h<400,`${point}: table mark includes its caption and cells but not the formula`);
      await page.screenshot({path:evidence+`/${point}.png`});await toolbar().getByRole('button',{name:'Circle',exact:true}).click();await saved(m=>m.graph.nodes.every(n=>!n.marks.length));
    }
    check(report.tableMarks.length===3&&new Set(report.tableMarks.map(n=>n.id)).size===1,'Latin and single-glyph CJK table cells address the same recognized table');
    await clickAt(report.points.tableLeft);await toolbar().getByRole('button',{name:'Circle',exact:true}).click();await saved(m=>m.graph.nodes.some(n=>n.marks.includes('圈出')));
    await clickAt(report.points.equationNumber);await toolbar().getByRole('button',{name:'Word',exact:true}).click();await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
    const page2=await saved(m=>m.graph.nodes.some(n=>n.spans?.length));report.finalPage2=page2.graph;report.equationId=page2.graph.nodes.find(n=>n.spans?.length).id;
    await page.getByRole('button',{name:'Board 1',exact:true}).click();await page.locator('.pages button[aria-current=page][aria-label="Board 1"]').waitFor();
    await clickAt(report.points.meetingFirst);await toolbar().getByRole('button',{name:'Word',exact:true}).click();
    for(const[label,mark]of [['Highlight','高亮'],['Circle','圈出'],['Underline','下划线'],['Strike','划掉'],['Star','星标']]){
      await toolbar().getByRole('button',{name:label,exact:true}).click();const m=await saved(m=>m.page===0&&m.graph.nodes.some(n=>n.spans?.some(s=>s.mark===mark)));const n=m.graph.nodes.find(n=>n.spans?.some(s=>s.mark===mark)),span=n.spans.find(s=>s.mark===mark);report.wordId=n.id;
      check(span.text==='会议'&&span.boxes.length===2,`${label}: the wrapped word remains one mark with two source boxes`);
    }
    const selector=`.ink [data-key^="${report.wordId}|圈出|"]`;
    await page.waitForFunction(selector=>document.querySelector(selector)?.querySelectorAll(':scope > path').length===2,selector);
    report.circlePaths=await page.locator(selector).locator(':scope > path').evaluateAll(es=>es.map(e=>({d:e.getAttribute('d'),box:{x:e.getBBox().x,y:e.getBBox().y,w:e.getBBox().width,h:e.getBBox().height}})));
    check(report.circlePaths.every((p,i)=>Math.abs(p.box.y+p.box.h/2-report.points[i?'meetingSecond':'meetingFirst'].y)<10&&p.box.h<45),'hand-drawn circle makes two local loops without a diagonal bridge across the columns');
    await page.getByRole('button',{name:'Undo',exact:true}).click();await saved(m=>m.graph.nodes.find(n=>n.id===report.wordId).spans.length===4);
    await page.getByRole('button',{name:'Redo',exact:true}).click();await saved(m=>m.graph.nodes.find(n=>n.id===report.wordId).spans.length===5);check(true,'a wrapped word mark is one undo/redo step');
    report.finalGraph=(await read()).graph;await page.screenshot({path:evidence+'/hand-marks.png'});
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready=true]').waitFor({timeout:60000});
    const portable=await download('Excalidraw','structure','excalidraw'),free=JSON.parse(portable.text);report.freeElements=free.elements;
    check(JSON.stringify(free.boardcast.graph)===JSON.stringify(report.finalGraph),'portable drawing keeps exact source column order and all five marks');
    const circles=free.elements.filter(e=>e.id.startsWith(`${report.wordId}:span:圈出:`));check(circles.length===2&&circles.every((e,i)=>Math.abs(e.x+e.width/2-report.points[i?'meetingSecond':'meetingFirst'].x)<5&&Math.abs(e.y+e.height/2-report.points[i?'meetingSecond':'meetingFirst'].y)<5),'Free edit circles both halves at the original PDF word positions');
    await page.screenshot({path:evidence+'/free-marks.png'});
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.locator('.ink > svg').waitFor();
    check(JSON.stringify((await read()).graph)===JSON.stringify(report.finalGraph),'refresh restores vertical source rows and all five annotations');
    const svg=await download('SVG vector','structure','svg');check(report.circlePaths.every(p=>svg.text.includes(p.d)),'downloaded SVG contains the two actual local circle paths');
    const animated=await download('Animated SVG','structure-animated','svg');check(report.circlePaths.every(p=>animated.text.includes(p.d)),'animated SVG contains the same separate circle paths');
    await download('Whole meeting PDF','structure-export','pdf');
    const html=await download('Replay page','structure-replay','html');
    const offline=await page.context().browser().newContext({offline:true,viewport:{width:1280,height:800},reducedMotion:'reduce'});
    try{
      const replay=await offline.newPage(),requests=[];replay.on('pageerror',e=>report.errors.push(e.message));replay.on('request',r=>{if(/^https?:/.test(r.url()))requests.push(r.url())});await replay.goto(pathToFileURL(path.resolve(html.file)).href);
      await replay.getByRole('slider').first().focus();await replay.keyboard.press('End');await replay.waitForFunction(selector=>document.querySelector(selector)?.querySelectorAll(':scope > path').length===2,selector);
      check(JSON.stringify(await replay.evaluate(()=>window.__boardcastData.pages[0].graph))===JSON.stringify(report.finalGraph),'offline HTML retains the exact multi-column source and annotations');
      check(await replay.locator(selector).locator(':scope > path').count()===2,'offline HTML renders two local circles at the column break');await replay.screenshot({path:evidence+'/offline-vertical.png'});
      await replay.locator('.player-pages').getByRole('button',{name:'2',exact:true}).click();await replay.locator(`.ink [data-key^="${report.equationId}|高亮|"]`).waitFor();
      check(JSON.stringify(await replay.evaluate(()=>window.__boardcastData.pages[1].graph))===JSON.stringify(report.finalPage2),'offline HTML preserves the independent table region and formula number highlight');
      await replay.screenshot({path:evidence+'/offline-table-formula.png'});check(!requests.length,'offline replay uses no HTTP requests');
    }finally{await offline.close();}
    await page.locator('input[type=file]').setInputFiles({name:'structure.excalidraw',mimeType:'application/json',buffer:portable.bytes});const restore=page.getByRole('dialog',{name:'Import materials',exact:true});await restore.getByRole('button',{name:'New meeting',exact:true}).click();await restore.waitFor({state:'hidden'});await saved(m=>m.id!==original.id);
    check(JSON.stringify((await read()).graph)===JSON.stringify(report.finalGraph),'reimporting the portable drawing preserves column order and all annotations');
    check(!report.errors.length,'complex source interactions have no uncaught browser errors');report.passed=report.checks.every(c=>c.ok);assert(report.passed,JSON.stringify(report.checks.filter(c=>!c.ok)));return report;
  }catch(error){report.error=String(error);await page.screenshot({path:evidence+'/failure.png'}).catch(()=>{});report.aria=await page.locator('body').ariaSnapshot();throw error;}
  finally{await fs.writeFile(evidence+'/pdf-structure-check.json',JSON.stringify(report,null,2)+'\n');}
};
