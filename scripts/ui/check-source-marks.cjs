const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkSourceMarks(page, baseUrl, evidence = '/tmp/boardcast-source-marks') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: [], files: {}};
  const check = (ok, name) => report.checks.push({name, ok: !!ok});
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {
    for (const [key, value] of Object.entries({lang: 'en', mode: 'ink', log: 'closed', 'mode-hint': 'dismissed'})) localStorage.setItem('boardcast.' + key, value);
  });
  // Chromium produces a real text-layer PDF; the independent HTML positions are the pointer oracle.
  const fixture = await page.context().newPage();
  await fixture.setContent(`<!doctype html><style>@page{size:900px 680px;margin:0}body{margin:0;font:15px/24px monospace}h1{position:absolute;left:56px;top:24px;font-size:24px}section{position:absolute;left:56px;top:120px;width:360px}section.right{left:490px}p{margin:0;white-space:nowrap}.hyphen{top:300px}</style>
    <h1>Source annotation fixture</h1>
    <section><p><span id="left">Alpha</span> system keeps the original words</p><p>together across lines. Next sentence.</p></section>
    <section class="right"><p>Right column repeats <span id="right">Alpha</span> safely.</p><p>Independent text stays on this side.</p></section>
    <section class="hyphen"><p>We retain a hyphenated <span id="hyphen">micro-</span></p><p>service across multiple lines.</p></section>
    <section style="top:450px"><p><span id="long">pneumonoultramicroscopicsilicovolcanoconiosis</span></p></section>`);
  report.points = await fixture.locator('[id]').evaluateAll(elements => Object.fromEntries(elements.map(e => {const b = e.getBoundingClientRect();return [e.id, {x:b.x+b.width/2,y:b.y+b.height/2,left:b.left,right:b.right}];})));
  const pdf = await fixture.pdf({path: `${evidence}/two-column.pdf`, preferCSSPageSize: true});
  await fixture.close();
  const read = () => page.evaluate(async () => {
    const db = await new Promise((yes, no) => {const r = indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row = await new Promise((yes, no) => {const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    db.close();return row;
  });
  const saved = async predicate => {
    for (let i=0;i<150;i++) {const row=await read();if(row&&predicate(row))return row;await page.waitForTimeout(50);}
    throw Error('Source mark state was not saved');
  };
  const clickAt = async point => {
    await page.locator('.ink').click({trial:true});
    await page.waitForFunction(()=>{const svg=document.querySelector('.ink > svg'),b=svg.getBoundingClientRect(),v=svg.viewBox.baseVal;return Math.abs(b.width/b.height-v.width/v.height)<0.002;});
    const at = await page.locator('.ink > svg').evaluate((svg, point) => {const p=new DOMPoint(point.x,point.y).matrixTransform(svg.getScreenCTM());return {x:p.x,y:p.y};}, point);
    const canvas=await page.locator('.ink').boundingBox();
    assert(at.x>=canvas.x&&at.x<=canvas.x+canvas.width&&at.y>=canvas.y&&at.y<=canvas.y+canvas.height,'the PDF target must be inside the actual canvas before clicking');
    await page.mouse.click(at.x,at.y);
    await page.locator('.ink-toolbar').waitFor();
  };
  const toolbar = () => page.locator('.ink-toolbar');
  const download = async (label, key, ext) => {
    await clickMeetingAction(page, 'Export');
    await page.getByRole('menuitemradio',{name:['Markdown','Whole meeting PDF','Replay page'].includes(label)?'Whole meeting':'Current board',exact:true}).click();
    const pending=page.waitForEvent('download',{timeout:60000});
    await page.getByRole('menuitem',{name:new RegExp(`^${label}`)}).click();
    const item=await pending,file=path.join(evidence,`${key}.${ext}`);await item.saveAs(file);
    const bytes=await fs.readFile(file);report.files[key]={file,bytes:bytes.length};return {file,bytes,text:bytes.toString('utf8')};
  };
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    await page.locator('input[type=file]').setInputFiles({name:'two-column.pdf',mimeType:'application/pdf',buffer:pdf});
    const dialog=page.getByRole('dialog',{name:'Import materials',exact:true});
    await dialog.getByRole('button',{name:'Add to this meeting',exact:true}).click({timeout:90000});
    await dialog.waitFor({state:'hidden'});
    await page.locator('.save-state.saved').waitFor();
    const original = await saved(m=>m.graph.underlay&&m.graph.nodes.some(n=>n.rows?.length));
    report.imported = original.graph;
    check(original.graph.nodes.filter(n=>n.rows?.length).length>=4,'actual text-layer PDF imports separate source blocks');
    await clickAt(report.points.left);
    await toolbar().getByRole('button',{name:'Word',exact:true}).click();
    await toolbar().getByRole('button',{name:'Underline',exact:true}).click();
    const marked=await saved(m=>m.graph.nodes.some(n=>n.spans?.some(s=>s.mark==='下划线')));
    report.marked=marked.graph;
    const left=marked.graph.nodes.find(n=>n.spans?.some(s=>s.mark==='下划线'));
    check(left.spans[0].text==='Alpha'&&left.spans[0].boxes[0].x<450,'pointer uses original PDF position and hits left-column Alpha only');
    check(await toolbar().getByRole('button',{name:'Underline',exact:true}).getAttribute('aria-pressed')==='true','mark button exposes pressed state');
    const hasHighlight=await toolbar().getByRole('button',{name:'Highlight',exact:true}).count()===1;
    check(hasHighlight,'persistent Highlight is available with the other marks');
    await page.screenshot({path:`${evidence}/source-word.png`});
    if (hasHighlight) {
      const node = m => m.graph.nodes.find(n=>n.id===left.id);
      await toolbar().getByRole('button',{name:'Underline',exact:true}).click();
      await saved(m=>!node(m).spans?.length);
      const marks=[['Strike','划掉','x'],['Circle','圈出','o'],['Underline','下划线','u'],['Star','星标','s'],['Highlight','高亮','h']];
      for (const unit of ['Word','Sentence','Block']) for (const [name,mark,suffix] of marks) {
        await clickAt(report.points.left);
        await toolbar().getByRole('button',{name:unit,exact:true}).click();
        const button=toolbar().getByRole('button',{name,exact:true});await button.click();
        const m=await saved(m=>unit==='Block'?node(m).marks.includes(mark):node(m).spans?.some(s=>s.mark===mark));
        const n=node(m),span=n.spans?.find(s=>s.mark===mark);
        const scope=unit==='Block'?n.marks.length===1&&!n.spans?.length:span?.text===(unit==='Word'?'Alpha':'Alpha system keeps the original words together across lines.')&&span.boxes.length===(unit==='Word'?1:2)&&!n.marks.length;
        const key=unit==='Block'?`${left.id}|${suffix}`:`${left.id}|${mark}|`;
        await page.locator(`.ink [data-key${unit==='Block'?'=':'^='}"${key}"]`).waitFor();
        check(scope&&await button.getAttribute('aria-pressed')==='true',`${name}: ${unit.toLowerCase()} hits the exact PDF range and reports on`);
        await button.click();await saved(m=>!node(m).marks.length&&!node(m).spans?.length);
        check(await button.getAttribute('aria-pressed')==='false',`${name}: second ${unit.toLowerCase()} action removes the same mark`);
      }
      await clickAt({x:report.points.left.left+1,y:report.points.left.y});await toolbar().getByRole('button',{name:'Word',exact:true}).click();
      await toolbar().getByRole('button',{name:'Circle',exact:true}).click();await saved(m=>node(m).spans?.some(s=>s.text==='Alpha'));
      await clickAt({x:report.points.left.right-1,y:report.points.left.y});await toolbar().getByRole('button',{name:'Circle',exact:true}).click();await saved(m=>!node(m).spans?.length);
      check(true,'both visual edges of the PDF word toggle the same exact annotation');
      await clickAt(report.points.right);await toolbar().getByRole('button',{name:'Word',exact:true}).click();
      await toolbar().getByRole('button',{name:'Star',exact:true}).click();
      const right=await saved(m=>m.graph.nodes.some(n=>n.spans?.some(s=>s.mark==='星标')));
      const rightNode=right.graph.nodes.find(n=>n.spans?.some(s=>s.mark==='星标'));
      check(rightNode.id!==left.id&&rightNode.spans[0].text==='Alpha'&&rightNode.spans[0].boxes[0].x>450,'identical word in the right column keeps its own source location');
      await toolbar().getByRole('button',{name:'Star',exact:true}).click();
      await clickAt(report.points.hyphen);await toolbar().getByRole('button',{name:'Circle',exact:true}).click();
      const hyphen=await saved(m=>m.graph.nodes.some(n=>n.spans?.some(s=>s.mark==='圈出')));
      const joined=hyphen.graph.nodes.flatMap(n=>n.spans??[]).find(s=>s.mark==='圈出');
      check(joined.text==='microservice'&&joined.boxes.length===2,'hyphenated word is one annotation across both original lines');
      await clickAt(report.points.long);await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
      const long=await saved(m=>m.graph.nodes.some(n=>n.spans?.some(s=>s.mark==='高亮')));
      check(long.graph.nodes.flatMap(n=>n.spans??[]).some(s=>s.mark==='高亮'&&s.text==='pneumonoultramicroscopicsilicovolcanoconiosis'),'long English word retains its full source range');
      await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
      await clickAt(report.points.left);await toolbar().getByRole('button',{name:'Sentence',exact:true}).click();
      await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
      await saved(m=>node(m).spans?.some(s=>s.mark==='高亮'));
      await page.getByRole('button',{name:'Undo',exact:true}).click();await saved(m=>!node(m).spans?.some(s=>s.mark==='高亮'));
      await page.getByRole('button',{name:'Redo',exact:true}).click();await saved(m=>node(m).spans?.some(s=>s.mark==='高亮'));
      check(true,'persistent sentence highlight is one undo/redo step');
      await clickAt(report.points.left);
      await toolbar().getByRole('button',{name:'Word',exact:true}).click();await toolbar().getByRole('button',{name:'Underline',exact:true}).click();
      await toolbar().getByRole('button',{name:'Block',exact:true}).click();await toolbar().getByRole('button',{name:'Star',exact:true}).click();
      await clickAt(report.points.right);await toolbar().getByRole('button',{name:'Strike',exact:true}).click();
      await saved(m=>m.graph.nodes.some(n=>n.marks.includes('划掉')));
      await page.getByRole('button',{name:'Transcript',exact:true}).click();await page.getByRole('tab',{name:'Contents',exact:true}).click();
      const contents=page.locator('#sidebar-contents-panel');
      check((await contents.innerText()).includes('Highlight: Alpha system keeps the original words together across lines.'),'content list names the highlight and exact sentence without relying on color');
      const sourceItem=contents.locator('li').filter({has:page.locator('strong').filter({hasText:left.label})}).first();
      await sourceItem.getByRole('button',{name:'Locate',exact:true}).focus();await page.keyboard.press('Enter');
      await toolbar().waitFor();
      check(await toolbar().getByRole('button',{name:'Block',exact:true}).getAttribute('aria-pressed')==='true'&&await toolbar().getByRole('button',{name:'Word',exact:true}).isDisabled(),'keyboard Locate selects the block and cannot silently apply a word mark to the whole block');
      await page.getByRole('button',{name:'Transcript',exact:true}).click();
      await page.setViewportSize({width:640,height:400});
      await page.getByRole('button',{name:'Show full board',exact:true}).click();await clickAt(report.points.left);
      const boxes=await toolbar().evaluate(el=>{const b=el.getBoundingClientRect();return {x:b.x,right:b.right,top:b.top,bottom:b.bottom,w:innerWidth,h:innerHeight};});
      check(boxes.x>=0&&boxes.right<=boxes.w&&boxes.top>=0&&boxes.bottom<=boxes.h,'all five marks and scope controls fit a 640px window');
      await page.screenshot({path:`${evidence}/source-compact.png`});
      await toolbar().getByRole('button',{name:'Sentence',exact:true}).click();
      await clickMeetingAction(page, 'Settings');await page.getByRole('dialog').getByRole('radio',{name:'中文',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'关闭',exact:true}).click();
      check(await toolbar().getByRole('button',{name:'高亮',exact:true}).getAttribute('aria-pressed')==='true'&&await toolbar().getByRole('button',{name:'句',exact:true}).getAttribute('aria-pressed')==='true','Chinese narrow-window controls retain the selected sentence and highlight state');
      await page.screenshot({path:`${evidence}/source-compact-zh.png`});
      await clickMeetingAction(page, '设置');await page.getByRole('dialog').getByRole('radio',{name:'English',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Close',exact:true}).click();
      await page.setViewportSize({width:1280,height:800});
      report.finalGraph=(await read()).graph;
      await page.screenshot({path:`${evidence}/source-all-marks.png`});
    }
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();
    await page.locator('.pencil-stage[data-ready=true]').waitFor({timeout:60000});
    await page.waitForTimeout(1000);
    const portable=await download('Excalidraw','source-free','excalidraw'),free=JSON.parse(portable.text);
    report.freeScene={elements:free.elements,files:Object.keys(free.files)};
    report.freePixels=await page.locator('canvas').evaluateAll(canvases=>canvases.map(c=>{const {data}=c.getContext('2d').getImageData(0,0,c.width,c.height);let dark=0;for(let i=0;i<data.length;i+=4)if(data[i+3]>200&&data[i]<150&&data[i+1]<150&&data[i+2]<150)dark++;return {class:c.className,dark};}));
    check(report.freePixels.some(c=>c.dark>500),'source PDF text stays visible in Free edit');
    check(free?.elements.some(e=>e.customData?.boardcast?.id===left.id),'source mark has a Free edit projection');
    await page.screenshot({path:`${evidence}/source-free.png`});
    if (hasHighlight) {
      const scoped=free.elements.filter(e=>e.customData?.boardcast?.id===left.id);
      check(scoped.filter(e=>e.id.includes(':span:高亮:')).length===2&&scoped.some(e=>e.id.includes(':mark:star')),'Free edit export preserves both highlighted lines and the block star');
      check(free.elements.filter(e=>e.id.includes(':mark:strike')).length===2,'Free edit crosses both lines of a short source block instead of striking the line gap');
      await page.locator('label').filter({has:page.getByRole('radio',{name:'Rectangle',exact:true})}).click();
      const canvas=await page.locator('.excalidraw__canvas.interactive').boundingBox();
      await page.mouse.move(canvas.x+canvas.width-150,canvas.y+canvas.height-150);await page.mouse.down();await page.mouse.move(canvas.x+canvas.width-70,canvas.y+canvas.height-100,{steps:8});await page.mouse.up();
      const edit=await saved(m=>m.scene?.elements.some(e=>!e.customData?.boardcast&&!e.isDeleted));
      check(!edit.scene.elements.some(e=>e.customData?.boardcastUnderlay)&&!Object.keys(edit.scene.files).length,'free drawing saves without duplicating the locked PDF image');
      await page.getByRole('banner').getByRole('button',{name:'Undo',exact:true}).click();
      await saved(m=>!m.scene?.elements.some(e=>!e.customData?.boardcast&&!e.isDeleted));
      await page.getByRole('banner').getByRole('button',{name:'Redo',exact:true}).click();
      await saved(m=>m.scene?.elements.some(e=>!e.customData?.boardcast&&!e.isDeleted));
      check(true,'a free drawing over the source page remains one undo/redo step');
      await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
      const beforeReload=(await read()).graph;await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
      await page.locator(`.ink [data-key^="${left.id}|高亮|"]`).waitFor();
      check(JSON.stringify((await read()).graph)===JSON.stringify(beforeReload),'reload retains all PDF text, exact ranges and five mark types');
      const markdown=await download('Markdown','source','md');
      check(markdown.text.includes('Alpha system keeps the original words together across lines. Next sentence.')&&markdown.text.includes('Highlight: Alpha system keeps the original words together across lines.')&&markdown.text.includes('microservice'),'Markdown retains full source and named exact-range annotations');
      for (const [label,key] of [['SVG vector','source'],['Animated SVG','source-animated']]) {
        const file=await download(label,key,'svg');check(file.text.includes('data:image/jpeg;base64,')&&file.text.includes('mix-blend-mode:multiply'),'original page and persistent highlights are included in '+label);
      }
      const pdf=await download('Whole meeting PDF','source-export','pdf');check(pdf.bytes.subarray(0,5).toString()==='%PDF-','whole meeting PDF is produced from the annotated page');
      const html=await download('Replay page','source-replay','html');
      const context=await page.context().browser().newContext({viewport:{width:1280,height:800},reducedMotion:'reduce'});
      try {
        await context.setOffline(true);const offline=await context.newPage();offline.on('pageerror',e=>report.pageErrors.push(e.message));
        await offline.goto(pathToFileURL(path.resolve(html.file)).href);await offline.locator('.ink > svg').waitFor();
        const slider=offline.getByRole('slider').first();await slider.focus();await offline.keyboard.press('End');
        await offline.locator(`.ink [data-key^="${left.id}|高亮|"]`).waitFor();
        check(await offline.locator('.ink image').count()>=1,'downloaded replay renders the original PDF and persistent highlight offline');
        await offline.screenshot({path:`${evidence}/source-offline.png`});
      } finally {await context.close();}
      await page.locator('input[type=file]').setInputFiles({name:'source.excalidraw',mimeType:'application/json',buffer:portable.bytes});
      const imported=page.getByRole('dialog',{name:'Import materials',exact:true});await imported.getByRole('button',{name:'New meeting',exact:true}).click();await imported.waitFor({state:'hidden'});
      await saved(m=>m.id!==original.id&&m.graph.nodes.some(n=>n.spans?.some(s=>s.mark==='高亮')));
      const restored=await read();check(JSON.stringify(restored.graph)===JSON.stringify(free.boardcast.graph)&&!restored.scene.elements.some(e=>e.customData?.boardcastUnderlay),'portable Excalidraw reimport retains graph/ranges and removes only the display underlay');
      const normal={type:'excalidraw',elements:[],files:{},boardcast:{version:2,generated:[],graph:{nodes:[{id:'n1',label:'Decision',aliases:[],kind:'node',marks:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1}}};
      await page.locator('input[type=file]').setInputFiles({name:'normal-node.excalidraw',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(normal))});
      await imported.getByRole('button',{name:'New meeting',exact:true}).click();await imported.waitFor({state:'hidden'});await saved(m=>m.id!==restored.id&&m.graph.nodes[0]?.label==='Decision');
      const label=page.locator('.ink [data-key="n1|Decision"]');await label.waitFor();await page.locator('.ink').click({trial:true});
      const box=await label.boundingBox();await page.mouse.click(box.x+box.width/2,box.y+box.height/2);
      await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();await saved(m=>m.graph.nodes[0].marks.includes('高亮'));
      await page.locator('.ink [data-key="n1|Decision|h"]').waitFor();
      check(await toolbar().getByRole('button',{name:'Highlight',exact:true}).getAttribute('aria-pressed')==='true','normal diagram node has a persistent highlight with no source-scope controls');
      await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready=true]').waitFor();
      const normalFree=JSON.parse((await download('Excalidraw','normal-free','excalidraw')).text);
      check(normalFree.elements.some(e=>e.id==='n1:mark:highlight')&&normalFree.boardcast.graph.nodes[0].marks.includes('高亮'),'normal node highlight also survives Free edit export');
    }
    check(report.pageErrors.length===0,'no browser page errors');
    report.passed=report.checks.every(c=>c.ok);
    assert.equal(report.passed,true,JSON.stringify(report.checks.filter(c=>!c.ok)));
    return report;
  } catch(error) {
    report.saved=await read().catch(()=>null);
    report.error=String(error);report.aria=await page.locator('body').ariaSnapshot();
    await page.screenshot({path:`${evidence}/source-failure.png`}).catch(()=>{});throw error;
  } finally {await fs.writeFile(`${evidence}/source-marks-check.json`,JSON.stringify(report,null,2)+'\n');}
};
