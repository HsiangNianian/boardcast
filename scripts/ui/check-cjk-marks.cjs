const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkCjkMarks(page, baseUrl, evidence = '/tmp/boardcast-cjk-marks', zoom) {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],pageErrors:[],requests:[],files:{},zoom:'not run'};
  const check=(ok,name)=>report.checks.push({name,ok:!!ok});
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.addInitScript(()=>{
    for(const [key,value] of Object.entries({lang:'en',mode:'ink',log:'closed','mode-hint':'dismissed',localLlm:'http://127.0.0.1:19879'}))localStorage.setItem('boardcast.'+key,value);
  });
  let reply='';
  await page.route('http://127.0.0.1:19879/**',route=>{
    const post=route.request().method()==='POST';if(post)report.requests.push(route.request().postDataJSON());
    return route.fulfill({contentType:'application/json',headers:{'access-control-allow-origin':'*'},body:JSON.stringify(post?{choices:[{message:{content:reply}}]}:{status:'ok'})});
  });
  const fixture=await page.context().newPage();
  await fixture.setContent(`<!doctype html><style>@page{size:900px 680px;margin:0}body{margin:0;font:20px/30px 'Noto Sans CJK SC',sans-serif}h1{position:absolute;left:60px;top:18px;font-size:28px}section{position:absolute;left:60px;width:760px}p{margin:0;white-space:nowrap}</style>
    <h1>中文原文标注验证</h1>
    <section style="top:100px"><p>本地系统需要及时<span id="save">保存</span>每场<span id="meetingFirst">会</span></p><p><span id="meetingSecond">议</span>记录，方便大家复盘。下一句。</p></section>
    <section style="top:230px"><p>「系统需要<span id="quotedSave">保存</span>会议记录。<span id="close">」</span><span id="afterQuote">下一句测试导出。</span></p></section>
    <section style="top:320px"><p>使用<span id="api">API</span>解析PDF并<span id="mixedSave">保存</span>记录。</p></section>
    <section style="top:410px"><p>扩展字<span id="astral">𠮷</span>后面仍要<span id="astralSave">保存</span>记录。</p></section>
    <section style="top:500px"><p>他说「<span id="question">保存记录</span>？！<span id="questionClose">」</span><span id="afterQuestion">下一句。</span></p></section>
    <section style="top:580px"><p>ASCII：<span id="hyphen">foo-bar</span> 与 <span id="snake">source_text</span>。</p></section>`);
  await fixture.evaluate(()=>document.fonts.ready);
  report.points=await fixture.locator('[id]').evaluateAll(elements=>Object.fromEntries(elements.map(e=>{const b=e.getBoundingClientRect();return [e.id,{x:b.x+b.width/2,y:b.y+b.height/2,left:b.left,right:b.right}];})));
  const pdf=await fixture.pdf({path:`${evidence}/cjk-source.pdf`,preferCSSPageSize:true});await fixture.close();
  const read=()=>page.evaluate(async()=>{
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return row;
  });
  const saved=async predicate=>{for(let i=0;i<160;i++){const m=await read();if(m&&predicate(m))return m;await page.waitForTimeout(50);}throw Error('CJK mark state was not saved');};
  const toolbar=()=>page.locator('.ink-toolbar');
  const clickAt=async point=>{
    await page.locator('.ink').click({trial:true});
    await page.waitForFunction(()=>{const s=document.querySelector('.ink > svg'),b=s.getBoundingClientRect(),v=s.viewBox.baseVal;return Math.abs(b.width/b.height-v.width/v.height)<0.002;});
    const at=await page.locator('.ink > svg').evaluate((svg,p)=>{const q=new DOMPoint(p.x,p.y).matrixTransform(svg.getScreenCTM());return {x:q.x,y:q.y};},point);
    const b=await page.locator('.ink').boundingBox();assert(at.x>=b.x&&at.x<=b.x+b.width&&at.y>=b.y&&at.y<=b.y+b.height,'original PDF point is inside the canvas');
    await page.mouse.click(at.x,at.y);await toolbar().waitFor();
  };
  const manual=async(point,unit,text,boxes=1)=>{
    await clickAt(report.points[point]);await toolbar().getByRole('button',{name:unit,exact:true}).click();await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();
    const m=await saved(m=>m.graph.nodes.some(n=>n.spans?.length));const n=m.graph.nodes.find(n=>n.spans?.length),span=n.spans[0];
    report.ranges??=[];report.ranges.push({point,unit,expected:text,span});
    // Some PDF fonts map visible Han characters to compatibility radicals; preserve those bytes.
    check(span.text.normalize('NFKC')===text.normalize('NFKC')&&span.boxes.length===boxes,`${point}: ${unit.toLowerCase()} selects ${text} in ${boxes} source row(s)`);
    await page.locator(`.ink [data-key^="${n.id}|高亮|"]`).waitFor();
    await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();await saved(m=>m.graph.nodes.every(n=>!n.spans?.length));
  };
  const download=async(label,name,extension)=>{
    await page.getByRole('button',{name:'Export',exact:true}).click();await page.getByRole('menuitemradio',{name:['Markdown','Replay page'].includes(label)?'Whole meeting':'Current board',exact:true}).click();
    const pending=page.waitForEvent('download',{timeout:60000});await page.getByRole('menuitem',{name:new RegExp('^'+label)}).click();
    const file=path.join(evidence,`${name}.${extension}`);await(await pending).saveAs(file);const bytes=await fs.readFile(file);report.files[`${name}.${extension}`]={file,bytes:bytes.length};return {file,bytes,text:bytes.toString('utf8')};
  };
  try{
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    await page.locator('input[type=file]').setInputFiles({name:'cjk-source.pdf',mimeType:'application/pdf',buffer:pdf});
    const dialog=page.getByRole('dialog',{name:'Import materials',exact:true});await dialog.getByRole('button',{name:'Add to this meeting',exact:true}).click({timeout:90000});await dialog.waitFor({state:'hidden'});await page.locator('.save-state.saved').waitFor();
    const original=await read();report.imported=original.graph;
    const containing=text=>original.graph.nodes.find(n=>n.rows?.some(r=>r.segs.map(s=>s.t).join('').normalize('NFKC').includes(text)));
    const source=original.graph.nodes.find(n=>(n.text??n.label).startsWith('本地系统'));
    check(source?.rows.length===2,'real Chinese PDF lines form one source paragraph');
    const astral=original.graph.nodes.find(n=>(n.text??n.label).includes('𠮷'));
    assert(astral,'the actual PDF text layer must retain the supplementary Han character');
    for(const [point,unit,text,boxes] of [
      ['save','Word','保存',1],['meetingFirst','Word','会议',2],['meetingSecond','Word','会议',2],
      ['save','Sentence','本地系统需要及时保存每场会议记录，方便大家复盘。',2],
      ['quotedSave','Sentence','「系统需要保存会议记录。」',1],['close','Sentence','「系统需要保存会议记录。」',1],['afterQuote','Sentence','下一句测试导出。',1],
      ['api','Word','API',1],['mixedSave','Word','保存',1],['astralSave','Word','保存',1],
      ['question','Sentence','他说「保存记录？！」',1],['questionClose','Sentence','他说「保存记录？！」',1],['afterQuestion','Sentence','下一句。',1],
      ['hyphen','Word','foo-bar',1],['snake','Word','source_text',1],
    ])await manual(point,unit,text,boxes);
    const quoted=containing('「系统'),mixed=containing('使用API');
    check(new Set([quoted.id,mixed.id,astral.id]).size===3,'widely separated Chinese paragraphs keep independent source identities');
    reply=`M ${astral.id} 高亮 「保存」\nM ${astral.id} 星标 「𠮷」\nM ${quoted.id} 下划线 句 「保存」`;
    await page.getByRole('textbox',{name:'What was said',exact:true}).fill('请高亮扩展字后面的保存，给𠮷加星标，并给引号内整句话加下划线。');await page.getByRole('button',{name:'Put on the board',exact:true}).click();
    const marked=await saved(m=>m.graph.nodes.flatMap(n=>n.spans??[]).length===3);
    report.modelRanges=marked.graph.nodes.flatMap(n=>(n.spans??[]).map(s=>({id:n.id,...s})));
    check(report.modelRanges.some(s=>s.id===astral.id&&s.mark==='高亮'&&s.text==='保存'&&s.boxes.some(b=>report.points.astralSave.y>=b.y&&report.points.astralSave.y<=b.y+b.h)),'model quote after a supplementary Han character maps to 保存 at its original row');
    check(report.modelRanges.some(s=>s.id===astral.id&&s.mark==='星标'&&s.text==='𠮷'),'model quote of a supplementary Han character selects exactly that character');
    check(report.modelRanges.some(s=>s.mark==='下划线'&&s.text==='「系统需要保存会议记录。」'),'quoted sentence from the model retains its closing quote');
    const beforeReload=JSON.stringify(marked.graph);await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.locator('.ink [data-key]').first().waitFor();
    check(JSON.stringify((await read()).graph)===beforeReload,'reloading preserves saved source, spans and exact Unicode text');
    await clickAt(report.points.astralSave);await toolbar().getByRole('button',{name:'Word',exact:true}).click();
    const on=await toolbar().getByRole('button',{name:'Highlight',exact:true}).getAttribute('aria-pressed')==='true';
    check(on,'manual word selection recognizes the previously saved model quote');
    if(on){await toolbar().getByRole('button',{name:'Highlight',exact:true}).click();await saved(m=>m.graph.nodes.flatMap(n=>n.spans??[]).length===2);await page.getByRole('button',{name:'Undo',exact:true}).click();await saved(m=>m.graph.nodes.flatMap(n=>n.spans??[]).length===3);check(true,'removing the model quote is one reversible edit');}
    await page.screenshot({path:`${evidence}/cjk-marked.png`});
    const markdown=await download('Markdown','cjk-marked','md');check(markdown.text.includes('Highlight: 保存')&&markdown.text.includes('Star: 𠮷')&&markdown.text.includes('Underline: 「系统需要保存会议记录。」'),'Markdown exports exact Chinese and supplementary-character annotations');
    const portable=JSON.parse((await download('Excalidraw','cjk-marked','excalidraw')).text);
    check(JSON.stringify(portable.boardcast.graph)===beforeReload,'portable drawing retains all source characters and annotation ranges');
    await download('PNG image','cjk-marked','png');
    const html=await download('Replay page','cjk-replay','html');
    const context=await page.context().browser().newContext({viewport:{width:1280,height:800},reducedMotion:'reduce'});
    try{
      await context.setOffline(true);const offline=await context.newPage();offline.on('pageerror',e=>report.pageErrors.push(e.message));await offline.goto(pathToFileURL(path.resolve(html.file)).href);await offline.locator('.ink > svg').waitFor();
      await offline.getByRole('slider').first().focus();await offline.keyboard.press('End');await offline.locator(`.ink [data-key^="${astral.id}|高亮|"]`).waitFor();
      check(await offline.locator('.ink image').count()>0,'actual downloaded replay shows the Chinese PDF and saved marks offline');await offline.screenshot({path:`${evidence}/cjk-offline.png`});
    }finally{await context.close();}
    if(zoom){
      await zoom(2);await page.waitForFunction(()=>devicePixelRatio===2&&innerWidth===640&&visualViewport.scale===1);
      await page.getByRole('button',{name:'Show full board',exact:true}).click();await clickAt(report.points.mixedSave);
      await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByRole('dialog').getByRole('radio',{name:'中文',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'关闭',exact:true}).click();
      await toolbar().getByRole('button',{name:'词',exact:true}).click();await toolbar().getByRole('button',{name:'高亮',exact:true}).click();
      const current=await saved(m=>m.graph.nodes.find(n=>n.id===mixed.id).spans?.some(s=>s.mark==='高亮'));
      check(current.graph.nodes.find(n=>n.id===mixed.id).spans[0].text==='保存','Chinese controls at actual 200% browser zoom still select the exact word');
      report.zoom=await toolbar().evaluate(el=>({dpr:devicePixelRatio,width:innerWidth,height:innerHeight,scale:visualViewport.scale,rect:el.getBoundingClientRect().toJSON(),buttons:[...el.querySelectorAll('button')].map(b=>({name:b.textContent,rect:b.getBoundingClientRect().toJSON()}))}));
      check(report.zoom.buttons.every(b=>b.rect.left>=0&&b.rect.right<=report.zoom.width&&b.rect.top>=0&&b.rect.bottom<=report.zoom.height),'all source scope and mark buttons remain visible at actual 200% zoom');
      await page.screenshot({path:`${evidence}/cjk-200-percent.png`});await toolbar().getByRole('button',{name:'高亮',exact:true}).focus();await page.keyboard.press('Enter');await saved(m=>!m.graph.nodes.find(n=>n.id===mixed.id).spans?.length);
      check(true,'keyboard toggling works at actual 200% zoom');await zoom(1);
    }
    check(report.pageErrors.length===0,'CJK annotation flow has no uncaught browser errors');report.passed=report.checks.every(c=>c.ok);assert(report.passed,JSON.stringify(report.checks.filter(c=>!c.ok)));return report;
  }catch(error){report.error=String(error);report.aria=await page.locator('body').ariaSnapshot();await page.screenshot({path:`${evidence}/cjk-failure.png`}).catch(()=>{});throw error;}
  finally{await fs.writeFile(`${evidence}/cjk-marks-check.json`,JSON.stringify(report,null,2)+'\n');}
};
