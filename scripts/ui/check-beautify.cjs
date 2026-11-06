const {click: clickMeetingAction} = require('./meeting-action.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

module.exports = async function checkBeautify(page, baseUrl, evidence = '/tmp/boardcast-beautify') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {checks: [], pageErrors: [], requests: [], states: {}, files: {}, workerRuns: []};
  const check = (ok, name) => {report.checks.push({name, ok: !!ok}); console.log(ok ? 'PASS' : 'FAIL', name);};
  const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const count = m => m.lines.filter(l => l.text === 'Beautified the board layout' || l.text === '已美化板书排版').length;
  page.on('pageerror', e => report.pageErrors.push(e.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.addInitScript(() => {
    for (const [key,value] of Object.entries({lang:'en', mode:'ink', log:'closed', 'mode-hint':'dismissed', theme:'light', localLlm:'http://127.0.0.1:19878'})) localStorage.setItem('boardcast.' + key, value);
    const q = window.qaBeautify = {mode:'pass', held:[], compiles:0, starts:0, terminated:0};
    q.release = () => { q.mode='pass'; q.held.splice(0).forEach(run => run()); };
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      postMessage(data, options) {
        if (data?.type === 'init' && data.data?.wasm) {
          this.d2 = true; q.starts++;
          if (q.mode === 'hold-init') { q.held.push(() => super.postMessage(data, options)); return; }
        }
        if (this.d2 && data?.type === 'compile') {
          q.compiles++;
          if (q.mode === 'hold') { q.held.push(() => super.postMessage(data, options)); return; }
          // The real D2 parser rejects this input; successful results are never replaced.
          if (q.mode === 'fail') data = {...data, data:{...data.data, fs:{index:'broken: {'}}};
        }
        return super.postMessage(data, options);
      }
      terminate() { if (this.d2 && !this.stopped) {this.stopped=true; q.terminated++;} return super.terminate(); }
    };
  });
  let reply='', holdModel=false, pendingModel;
  const respond = route => route.fulfill({contentType:'application/json', headers:{'access-control-allow-origin':'*'}, body:JSON.stringify({choices:[{message:{content:reply}}]})});
  await page.route('http://127.0.0.1:19878/**', async route => {
    if (route.request().method() !== 'POST') return route.fulfill({contentType:'application/json',body:'{"status":"ok"}',headers:{'access-control-allow-origin':'*'}});
    report.requests.push(route.request().postDataJSON());
    if (holdModel) {pendingModel=route; return;}
    return respond(route);
  });
  const read = () => page.evaluate(async () => {
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const row=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get(localStorage.getItem('boardcast.meeting'));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);}); db.close(); return row;
  });
  const saved = async predicate => { for(let i=0;i<200;i++){const m=await read();if(m&&predicate(m))return m;await page.waitForTimeout(50);} throw Error('Beautify state was not saved'); };
  const button = () => page.getByRole('button',{name:/^(Beautify board|Arranging the board…|自动美化|正在整理板书…)$/});
  const workers = () => page.evaluate(()=>({starts:window.qaBeautify.starts,terminated:window.qaBeautify.terminated,compiles:window.qaBeautify.compiles}));
  const idle = () => page.waitForFunction(() => {const b=document.querySelector('header button[aria-busy]');return b && b.getAttribute('aria-busy')==='false';}, null, {timeout:60000});
  const send = async (text, ops) => {
    reply=ops; await page.getByRole('textbox',{name:'What was said',exact:true}).fill(text); await page.getByRole('button',{name:'Put on the board',exact:true}).click();
    await page.locator('.save-state.saved').waitFor(); return saved(m=>m.lines.some(l=>l.text===text&&l.ms!==undefined));
  };
  const tidy = async () => {const before=await read();await button().click();await idle();return saved(m=>count(m)===count(before)+1);};
  const free = m => m.scene?.elements.filter(e=>!e.customData?.boardcast&&!e.isDeleted)??[];
  const geometry = e => e && ({id:e.id,type:e.type,x:e.x,y:e.y,width:e.width,height:e.height,strokeColor:e.strokeColor,backgroundColor:e.backgroundColor});
  const draw = async (dx=0) => {
    const before=free(await read()).map(e=>e.id);const canvas=page.locator('canvas.excalidraw__canvas.interactive');const b=await canvas.boundingBox();
    await page.getByRole('radio',{name:'Rectangle',exact:true}).locator('..').click();
    await page.mouse.move(b.x+b.width*.62+dx,b.y+b.height*.7);await page.mouse.down();await page.mouse.move(b.x+b.width*.62+dx+70,b.y+b.height*.7+45,{steps:8});await page.mouse.up();
    const m=await saved(m=>free(m).some(e=>!before.includes(e.id)));await page.keyboard.press('Escape');return free(m).find(e=>!before.includes(e.id));
  };
  const download = async (label,name,extension) => {
    await clickMeetingAction(page, 'Export');await page.getByRole('menuitemradio',{name:label==='Replay page'?'Whole meeting':'Current board',exact:true}).click();
    const promise=page.waitForEvent('download',{timeout:60000});await page.getByRole('menuitem',{name:new RegExp('^'+label)}).click();
    const file=path.join(evidence,`${name}.${extension}`);await(await promise).saveAs(file);const bytes=await fs.readFile(file);report.files[path.basename(file)]={bytes:bytes.length};return {file,bytes};
  };
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    check(await button().isDisabled(),'an empty board cannot be beautified');
    const labels=Array.from({length:18},(_,i)=>`Stage ${String(i+1).padStart(2,'0')} ${i%3===0?'Collect and validate long meeting requirements':'Preserve source and decisions'}`);
    const ops=['T 「Release review」',...['Keep existing fallback','Confirm source ownership','Review every decision'].map(s=>`L 「Release review」 「${s}」`),
      ...labels.slice(1).map((s,i)=>`E 「${labels[i]}」 「${s}」`),...labels.slice(3,10).map((s,i)=>`E 「${labels[i]}」 「${s}」`),
      'T 「Risks and follow-up」',...['Do not lose user edits','Keep all source references','Recover from interrupted work'].map(s=>`L 「Risks and follow-up」 「${s}」`),`F 「${labels[4]}」 「latency <= 3 s」`,'M n1 星标'].join('\n');
    let m=await send('Prepare the complete long flowchart and review topics.',ops);
    await page.locator('.ink [data-key]').first().waitFor();
    const id=m.graph.nodes.find(n=>n.label===labels[0]).id;
    const at=new Map(m.layout.nodeAt).get(id), origin=new Map(m.layout.regionAt).get('__diagram');
    const p=await page.locator('.ink > svg').evaluate((svg,{at,origin})=>{const p=new DOMPoint(at.x+origin.x+at.w/2,at.y+origin.y+at.h/2).matrixTransform(svg.getScreenCTM());return {x:p.x,y:p.y};},{at,origin});
    report.gesture={id,point:p};await page.mouse.click(p.x,p.y);await page.getByRole('button',{name:'Rename',exact:true}).waitFor();
    await page.mouse.move(p.x,p.y);await page.mouse.down();await page.mouse.move(p.x+65,p.y+35,{steps:10});await page.mouse.up();
    const moved=await saved(n=>!equal(n.layout,m.layout));report.states.before=moved;
    check(!equal(moved.layout,m.layout),'a real hand-drawn gesture establishes the original user layout');
    const after=await tidy();report.states.arranged=after;
    check(equal(after.graph,moved.graph),'beautifying a 27-node, 24-edge board preserves all content, IDs, marks and relationships');
    check(!equal(after.layout,moved.layout),'the real local D2/TALA compiler changes the long board layout');
    const boxes=after.layout.nodeAt.map(([,b])=>b);
    check(boxes.length===18&&boxes.every(b=>[b.x,b.y,b.w,b.h].every(Number.isFinite)),'all eighteen flowchart nodes have finite saved geometry');
    check(boxes.every((a,i)=>boxes.slice(i+1).every(b=>a.x+a.w<=b.x||b.x+b.w<=a.x||a.y+a.h<=b.y||b.y+b.h<=a.y)),'long flowchart node boxes do not overlap');
    check(await page.locator('.ink [data-key]').evaluateAll((els,ids)=>ids.every(id=>els.some(el=>el.dataset.key.startsWith(id+'|'))),after.graph.nodes.map(n=>n.id)),'every semantic node remains rendered after arranging');
    await page.screenshot({path:`${evidence}/long-arranged.png`});
    await page.getByRole('banner').getByRole('button',{name:'Undo',exact:true}).click();const undone=await saved(n=>equal(n.layout,moved.layout));
    check(equal(undone.graph,moved.graph),'one undo restores every original hand-adjusted coordinate and semantic object');
    await page.getByRole('banner').getByRole('button',{name:'Redo',exact:true}).click();await saved(n=>equal(n.layout,after.layout));check(true,'one redo restores the complete arranged layout');
    const repeated=await tidy();check(equal(repeated.layout,after.layout),'a repeated beautify is stable rather than shuffling an already arranged board');
    await page.evaluate(()=>window.qaBeautify.mode='hold');await button().click();await page.waitForFunction(()=>window.qaBeautify.held.length>0);
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready="true"]').waitFor();await page.evaluate(()=>window.qaBeautify.release());await idle();
    for(let i=0;i<200;i++){m=await read();if(count(m)>count(repeated)||await page.getByText('The board changed while arranging. Try beautifying again.',{exact:true}).count())break;await page.waitForTimeout(50);}
    check(count(m)===count(repeated)+1,'switching views without editing does not invalidate the pending beautify');await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
    report.workerRuns.push(await workers());
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.locator('.ink [data-key]').first().waitFor();
    check(equal((await read()).layout,repeated.layout),'the arranged layout survives a real reload');
    await page.getByRole('button',{name:'Replay',exact:true}).click();await page.locator('.replay-dock').waitFor();check(await button().isDisabled(),'replay cannot beautify historical content');await page.getByRole('button',{name:'Replay',exact:true}).click();
    const failedBefore=await read();await page.evaluate(()=>window.qaBeautify.mode='fail');await button().click();await idle();await page.getByText(/Could not beautify the board; your layout is unchanged/).waitFor();
    m=await read();check(equal(m.graph,failedBefore.graph)&&equal(m.layout,failedBefore.layout)&&count(m)===count(failedBefore),'a real D2 parse failure preserves layout and content without recording success');
    check(await button().isEnabled(),'compiler failure restores the retry action');await page.screenshot({path:`${evidence}/compiler-failure.png`});
    await page.evaluate(()=>window.qaBeautify.mode='pass');await tidy();check(true,'retry after the compiler error succeeds without reload');
    check(await page.getByText(/Could not beautify the board; your layout is unchanged/).count()===0,'successful retry clears the previous compiler failure message');
    holdModel=true;reply='N 「Decision received after the pending request」';await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Keep the model request pending while checking the layout action.');await page.getByRole('button',{name:'Put on the board',exact:true}).click();
    for(let i=0;i<100&&!pendingModel;i++)await page.waitForTimeout(50);assert(pendingModel,'the real model request reached the controlled endpoint');
    check(await button().isDisabled(),'beautify is disabled while the model is still drawing');
    check(/drawing|generat/i.test(await button().getAttribute('title')??''),'the disabled action explains that board generation must finish');
    holdModel=false;await respond(pendingModel);pendingModel=null;await saved(n=>n.graph.nodes.length===failedBefore.graph.nodes.length+1);
    await page.getByRole('button',{name:'New board',exact:true}).click();await send('One node to customize in free edit.','N 「User positioned node」');
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('.pencil-stage[data-ready="true"]').waitFor();await page.waitForTimeout(300);
    const canvas=await page.locator('canvas.excalidraw__canvas.interactive').boundingBox();
    await page.mouse.move(canvas.x+canvas.width/2,canvas.y+canvas.height/2+15);await page.mouse.down();await page.mouse.move(canvas.x+canvas.width/2+90,canvas.y+canvas.height/2+55,{steps:10});await page.mouse.up();
    m=await saved(n=>Object.values(n.scene?.overrides??{}).some(o=>'x' in o||'y' in o));
    const body=m.scene.elements.find(e=>e.customData?.boardcast?.role==='body'&&!e.isDeleted);assert(body,'the customized generated node is saved');
    const first=await draw();const raceBefore=await read();await page.evaluate(()=>window.qaBeautify.mode='hold');await button().click();await page.waitForFunction(()=>window.qaBeautify.held.length>0);
    check(await button().isDisabled()&&await button().getAttribute('aria-busy')==='true','arranging has a disabled busy action while the real worker is pending');
    const second=await draw(-180);const editedDuring=await read();report.states.editedDuring=editedDuring;
    await page.evaluate(()=>window.qaBeautify.release());await idle();
    for(let i=0;i<200;i++){m=await read();if(count(m)>count(raceBefore)||await page.getByText('The board changed while arranging. Try beautifying again.',{exact:true}).count())break;await page.waitForTimeout(50);}
    check(count(m)===count(raceBefore),'a result computed before a new free shape is discarded without another undo entry');
    check(await page.getByText('The board changed while arranging. Try beautifying again.',{exact:true}).count()===1,'concurrent free editing receives the existing board-changed explanation');
    if(count(m)>count(raceBefore)){
      await page.getByRole('banner').getByRole('button',{name:'Undo',exact:true}).click();await page.waitForTimeout(800);const undoRace=await read();report.states.undoRace=undoRace;
      check(free(undoRace).some(e=>e.id===second.id),'undoing a stale beautify must not remove a shape drawn while it was running');
      await page.getByRole('banner').getByRole('button',{name:'Redo',exact:true}).click();await saved(n=>free(n).some(e=>e.id===second.id));
    }
    const beforeFree=await read(), afterFree=await tidy();report.states.freeArranged=afterFree;
    check(equal(afterFree.graph,beforeFree.graph),'free-edit beautification preserves semantic text and source identity');
    check(equal(geometry(afterFree.scene.elements.find(e=>e.id===body.id)),geometry(body)),'the explicit user position of a generated free-edit node remains unchanged');
    check([first,second].every(e=>equal(geometry(free(afterFree).find(n=>n.id===e.id)),geometry(e))),'both user-drawn shapes keep their positions and styles');
    await page.getByRole('banner').getByRole('button',{name:'Undo',exact:true}).click();await saved(n=>n.lines.length===afterFree.lines.length+1&&equal(n.layout,beforeFree.layout)&&free(n).some(e=>e.id===second.id));check(true,'one undo of the fresh beautify preserves both independent drawing edits');
    await page.getByRole('banner').getByRole('button',{name:'Redo',exact:true}).click();await saved(n=>n.lines.length===afterFree.lines.length+2&&equal(n.layout,afterFree.layout));
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();await page.locator('image.free-scene').waitFor();
    const portable=JSON.parse((await download('Excalidraw','beautified-free','excalidraw')).bytes.toString());
    check([first,second].every(e=>portable.elements.some(n=>n.id===e.id&&!n.isDeleted)),'the actual portable drawing contains both user shapes');
    const html=await download('Replay page','beautified-replay','html');
    const context=await page.context().browser().newContext({viewport:{width:1280,height:800}});
    try{await context.setOffline(true);const offline=await context.newPage();offline.on('pageerror',e=>report.pageErrors.push(e.message));await offline.goto(pathToFileURL(html.file).href);await offline.locator('.player-pages button').nth(1).click();await offline.getByRole('slider').first().focus();await offline.keyboard.press('End');await offline.locator('image.free-scene').waitFor();check(true,'downloaded offline replay renders the preserved free-edit layer');await offline.screenshot({path:`${evidence}/offline-arranged.png`});}finally{await context.close();}
    report.workerRuns.push(await workers());await page.reload();await page.locator('.splash').waitFor({state:'hidden'});await page.locator('image.free-scene').waitFor();
    m=await read();check([first,second].every(e=>equal(geometry(free(m).find(n=>n.id===e.id)),geometry(e))),'both independent free shapes survive save and reload after beautifying');
    await page.evaluate(()=>window.qaBeautify.mode='hold');await button().click();await page.waitForFunction(()=>window.qaBeautify.held.length>0);const oldPage=await read();
    await page.getByRole('button',{name:'New board',exact:true}).click();await send('Keep this new page independent.','N 「New page stays unchanged」');const newPage=await read();
    await page.evaluate(()=>window.qaBeautify.release());await idle();await page.waitForTimeout(800);m=await read();
    check(equal(m.graph,newPage.graph)&&equal(m.layout,newPage.layout)&&count(m)===count(oldPage),'an old result cannot beautify the newly selected board');
    await page.evaluate(()=>window.qaBeautify.mode='hold-init');await button().click();await page.waitForFunction(()=>window.qaBeautify.held.length>0);const started=Date.now();
    await page.waitForTimeout(21000);report.startupTimeoutMs=Date.now()-started;
    check(await button().getAttribute('aria-busy')==='false','the twenty-second timeout also releases an unready compiler worker');
    report.startupWorkers=await workers();check(report.startupWorkers.starts===report.startupWorkers.terminated,'the unready worker is terminated before the held startup is released');
    await page.evaluate(()=>window.qaBeautify.release());await idle();await page.getByText(/Could not beautify the board/).waitFor();
    const compileBefore=await read();await page.evaluate(()=>window.qaBeautify.mode='hold');await button().click();await page.waitForFunction(()=>window.qaBeautify.held.length>0);await idle();
    m=await read();check(equal(m.graph,compileBefore.graph)&&equal(m.layout,compileBefore.layout)&&count(m)===count(compileBefore),'a timeout during compilation also keeps the original board and restores the action');
    report.compileTimeoutWorkers=await workers();check(report.compileTimeoutWorkers.starts===report.compileTimeoutWorkers.terminated,'the ready compiler worker is terminated on timeout');await page.evaluate(()=>window.qaBeautify.release());
    await page.evaluate(()=>window.qaBeautify.mode='pass');await tidy();check(true,'the timeout leaves a working retry action');
    await page.setViewportSize({width:640,height:400});await clickMeetingAction(page, 'Settings');await page.getByRole('dialog').getByRole('radio',{name:'中文',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'关闭',exact:true}).click();
    holdModel=true;reply='N 「中文生成恢复」';await page.getByRole('textbox',{name:'说的话',exact:true}).fill('等待中文板书生成时检查自动美化。');await page.getByRole('button',{name:'写上白板',exact:true}).click();
    for(let i=0;i<100&&!pendingModel;i++)await page.waitForTimeout(50);assert(pendingModel);check(await button().isDisabled()&&/模型还在生成/.test(await button().getAttribute('title')??''),'compact Chinese controls explain why beautify waits for the model');
    holdModel=false;await respond(pendingModel);pendingModel=null;const chinese=await saved(n=>n.graph.nodes.some(n=>n.label==='中文生成恢复'));
    await page.evaluate(()=>window.qaBeautify.mode='fail');await button().click();await idle();await page.getByText(/美化失败，已保留原来的排版/).waitFor();m=await read();
    check(equal(m.graph,chinese.graph)&&equal(m.layout,chinese.layout),'Chinese compiler failure keeps the same original board');
    const b=await button().boundingBox();check(b.x>=0&&b.y>=0&&b.x+b.width<=640&&b.y+b.height<=400,'the retry action remains visible in the compact Chinese viewport');await page.screenshot({path:`${evidence}/chinese-failure.png`});
    await page.evaluate(()=>window.qaBeautify.mode='pass');await button().focus();await page.keyboard.press('Enter');await idle();await saved(n=>count(n)===count(chinese)+1);check(true,'keyboard retry works in the compact Chinese interface');
    check(await page.getByText(/美化失败，已保留原来的排版/).count()===0,'successful Chinese retry no longer reports the previous failure');await page.screenshot({path:`${evidence}/chinese-recovered.png`});
    report.workerRuns.push(await workers());check(report.workerRuns.every(w=>w.starts===w.terminated),'all measured compiler workers terminate across success, failure, stale work and both timeouts');
    check(report.pageErrors.length===0,'the beautify flow has no uncaught browser errors');report.passed=report.checks.every(c=>c.ok);assert(report.passed,JSON.stringify(report.checks.filter(c=>!c.ok)));return report;
  } catch(error) {report.error=String(error);report.current=await read().catch(()=>undefined);report.aria=await page.locator('body').ariaSnapshot();await page.screenshot({path:`${evidence}/beautify-failure.png`}).catch(()=>{});throw error;}
  finally {await fs.writeFile(`${evidence}/beautify-check.json`,JSON.stringify(report,null,2)+'\n');}
};
