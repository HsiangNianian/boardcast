const assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path');
const {promisify}=require('node:util'),execFile=promisify(require('node:child_process').execFile);
const {scannedPdf}=require('./check-import-task.cjs');

module.exports=async function checkPdfImportErrors(page,baseUrl,evidence='docs/ui-ux-audit-2026-11-03/evidence/implementation/pdf-import-errors'){
 await fs.mkdir(evidence,{recursive:true});const fixtures=path.join(evidence,'fixtures');await fs.mkdir(fixtures,{recursive:true});
 const report={checks:[],cases:[],pageErrors:[]},check=(name,ok)=>report.checks.push({name,ok:!!ok});
 for(const [name,bytes] of Object.entries({normal:scannedPdf(1),many:scannedPdf(41),tall:scannedPdf(1,100,50000),flat:scannedPdf(1,1000000,1),large:scannedPdf(1,300,220,64*1024*1024),broken:Buffer.from('%PDF-1.4\nnot a document\n%%EOF')}))await fs.writeFile(path.join(fixtures,name+'.pdf'),bytes);
 await execFile('qpdf',['--encrypt','fixture-password','fixture-owner','256','--',path.join(fixtures,'normal.pdf'),path.join(fixtures,'encrypted.pdf')]);
 await execFile('qpdf',['--empty','--pages',path.join(fixtures,'normal.pdf'),'1',path.join(fixtures,'tall.pdf'),'1','--',path.join(fixtures,'partial.pdf')]);
 const browser=page.context().browser();
 try{for(const lang of ['en','zh']){
  const context=await browser.newContext({viewport:{width:1280,height:800}}),p=await context.newPage();p.on('pageerror',e=>report.pageErrors.push({lang,error:e.message}));
  try{
   const t=lang==='zh'?{panel:'导入材料',cancel:'取消导入',retry:'重新导入',fresh:'新开一场会',append:'加到这场会',password:'这个 PDF 有密码，先去掉密码再导入',broken:'PDF 打不开：',size:n=>`第 ${n} 页无法生成有效的图片。请裁剪页面或调整页面尺寸后重新导入。`}:{panel:'Import materials',cancel:'Cancel import',retry:'Try importing again',fresh:'New meeting',append:'Add to this meeting',password:'This PDF is password-protected; remove the password and import it again',broken:"Can't open the PDF:",size:n=>`Page ${n} could not produce a valid image. Crop the page or adjust its dimensions, then import the PDF again.`};
   await p.addInitScript(lang=>{
    localStorage.setItem('boardcast.lang',lang);const qa=window.qaPdf={workers:[],reads:[],images:[],unsafeReads:[]};
    const NativeWorker=window.Worker;window.Worker=class extends NativeWorker{constructor(...args){super(...args);const state={url:String(args[0]),terminated:false};qa.workers.push(state);const stop=this.terminate.bind(this);this.terminate=()=>{state.terminated=true;return stop()}}};
    const encode=HTMLCanvasElement.prototype.toDataURL;HTMLCanvasElement.prototype.toDataURL=function(...args){const url=encode.apply(this,args);qa.images.push({width:this.width,height:this.height,valid:url.startsWith('data:image/')});return url};
    const pixels=CanvasRenderingContext2D.prototype.getImageData;CanvasRenderingContext2D.prototype.getImageData=function(...args){if(this.canvas.height>100000||!this.canvas.height)qa.unsafeReads.push({width:this.canvas.width,height:this.canvas.height});return pixels.apply(this,args)};
    const buffer=File.prototype.arrayBuffer;File.prototype.arrayBuffer=function(){if(this.name.startsWith('slow-'))return new Promise((resolve,reject)=>qa.reads.push({file:this,resolve,reject}));return buffer.call(this)};
    qa.release=async(index,fail)=>{const item=qa.reads[index];if(fail)item.reject(Error('Delayed PDF read failure'));else item.resolve(await buffer.call(item.file))};
   },lang);
   await p.goto(baseUrl);await p.getByRole('textbox',{name:lang==='zh'?'说的话':'What was said',exact:true}).waitFor();
   const panel=()=>p.getByRole('dialog',{name:t.panel,exact:true});
   const rows=()=>p.evaluate(async()=>{const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});try{return await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)})}finally{db.close()}});
   const settled=async()=>{await p.locator('.save-state.saved').waitFor();for(let i=0;i<100;i++){const id=await p.evaluate(()=>localStorage.getItem('boardcast.meeting'));const m=(await rows()).find(m=>m.id===id);if(m?.scene?.overlay&&!m.scene.pendingImport)return m;await p.waitForTimeout(100)}throw Error('Initial meeting did not settle')};
   const seed=async name=>{await p.locator('input[type=file]').setInputFiles({name:name+'.excalidraw',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({elements:[{id:name,type:'rectangle',x:20,y:20,width:120,height:80}],files:{}}))});await panel().getByRole('button',{name:t.fresh,exact:true}).click();await panel().waitFor({state:'hidden'});return settled()};
   const upload=name=>p.locator('input[type=file]').setInputFiles(path.join(fixtures,name+'.pdf'));
   const idle=()=>p.waitForFunction(()=>window.qaPdf.workers.every(w=>w.terminated),null,{timeout:10000});
   await seed('original');
   for(const [name,message] of [['encrypted',t.password],['broken',t.broken],['tall',t.size(1)],['flat',t.size(1)],['partial',t.size(2)]]){
    const before=await rows(),readCount=await p.evaluate(()=>window.qaPdf.unsafeReads.length);await upload(name);await panel().getByRole('alert').waitFor();await idle();const error=await panel().getByRole('alert').innerText();
    check(`${lang}/${name}: actionable localized error`,error.includes(message));check(`${lang}/${name}: no partial saved boards`,JSON.stringify(await rows())===JSON.stringify(before));
    check(`${lang}/${name}: invalid image does not allocate pixel data`,await p.evaluate(n=>window.qaPdf.unsafeReads.length===n,readCount));
    const workers=await p.evaluate(()=>window.qaPdf.workers.length);await panel().getByRole('button',{name:t.retry,exact:true}).click();await p.waitForFunction(n=>window.qaPdf.workers.length>n,workers);await panel().getByRole('alert').waitFor();await idle();
    check(`${lang}/${name}: same-file retry retains the error and original records`,(await panel().getByRole('alert').innerText())===error&&JSON.stringify(await rows())===JSON.stringify(before));
    if(name==='tall'){await p.setViewportSize({width:640,height:400});for(const label of [t.cancel,t.retry]){const b=panel().getByRole('button',{name:label,exact:true});await b.focus();const box=await b.boundingBox();check(`${lang}: compact ${label} stays reachable`,box&&box.y>=0&&box.y+box.height<=400)}await p.screenshot({path:path.join(evidence,lang+'-compact-error.png')});await p.setViewportSize({width:1280,height:800})}
    report.cases.push({lang,name,error});await panel().getByRole('button',{name:t.cancel,exact:true}).click();
   }
   for(const fail of [false,true]){
    const before=await rows(),index=await p.evaluate(()=>window.qaPdf.reads.length);await p.locator('input[type=file]').setInputFiles({name:fail?'slow-error.pdf':'slow-read.pdf',mimeType:'application/pdf',buffer:scannedPdf(1)});await p.waitForFunction(n=>window.qaPdf.reads.length>n,index);
    await p.keyboard.press('Escape');await panel().waitFor({state:'hidden'});const target=await seed(fail?'after-failed-read':'after-slow-read'),saved=await rows(),workers=await p.evaluate(()=>window.qaPdf.workers.length);await p.evaluate(({index,fail})=>window.qaPdf.release(index,fail),{index,fail});await p.waitForTimeout(200);
    check(`${lang}: canceled late ${fail?'failure':'success'} cannot write into the new meeting`,JSON.stringify(await rows())===JSON.stringify(saved)&&await p.evaluate(id=>localStorage.getItem('boardcast.meeting')===id,target.id)&&await panel().count()===0);
    check(`${lang}: canceled file read starts no PDF worker and preserves older records`,await p.evaluate(n=>window.qaPdf.workers.length===n,workers)&&before.every(m=>JSON.stringify(saved.find(x=>x.id===m.id))===JSON.stringify(m)));
   }
   const before=await rows();await upload('many');await panel().getByText(lang==='zh'?/正在读 many.pdf：\d+\/41 页/:/Reading many.pdf: page \d+\/41/).waitFor();check(`${lang}: cancellation reaches a live PDF worker`,await p.evaluate(()=>window.qaPdf.workers.some(w=>!w.terminated)));await panel().getByRole('button',{name:t.cancel,exact:true}).click();await idle();await p.waitForTimeout(200);
   check(`${lang}: real extraction cancellation leaves no partial boards or active workers`,JSON.stringify(await rows())===JSON.stringify(before)&&await panel().count()===0);
   if(lang==='en'){
    const size=(await fs.stat(path.join(fixtures,'large.pdf'))).size;await upload('large');await panel().getByRole('button',{name:t.append,exact:true}).waitFor({timeout:90000});await idle();check('64 MiB PDF: ready preview preserves the existing meetings',size>64*1024*1024&&JSON.stringify(await rows())===JSON.stringify(before));await panel().getByRole('button',{name:t.cancel,exact:true}).click();
   }
   await upload('normal');await panel().getByRole('button',{name:t.append,exact:true}).click();await panel().waitFor({state:'hidden'});await p.locator('.save-state.saved').waitFor();await idle();const after=await rows(),id=await p.evaluate(()=>localStorage.getItem('boardcast.meeting')),m=after.find(m=>m.id===id);check(`${lang}: a valid replacement PDF still appends and saves after failures`,m.pages?.length===2&&m.pages[1].graph.underlay?.w===900);
   report.cases.push({lang,workers:await p.evaluate(()=>window.qaPdf.workers),images:await p.evaluate(()=>window.qaPdf.images),unsafeReads:await p.evaluate(()=>window.qaPdf.unsafeReads)});await p.screenshot({path:path.join(evidence,lang+'-recovered.png')});
  }finally{await context.close()}
 }
 check('no unhandled production JavaScript errors',report.pageErrors.length===0);assert.ok(report.checks.every(c=>c.ok),JSON.stringify(report.checks.filter(c=>!c.ok)));report.passed=true;
 }catch(error){report.error=String(error);throw error}
 finally{await fs.writeFile(path.join(evidence,'check.json'),JSON.stringify(report,null,2)+'\n')}
};
