const assert = require('node:assert/strict');
const fs = require('node:fs/promises');

function scannedPdf(count, width = 300, height = 220, paddingBytes = 0) {
  const objects = ['<< /Type /Catalog /Pages 2 0 R >>', `<< /Type /Pages /Count ${count} /Kids [${Array.from({length:count},(_,i)=>`${3+i*2} 0 R`).join(' ')}] >>`];
  for (let i=0;i<count;i++) {
    const paint='0 0 0 rg 30 30 150 80 re f\n'+' '.repeat(paddingBytes);
    objects.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${width} ${height}] /Resources << >> /Contents ${4+i*2} 0 R >>`, `<< /Length ${paint.length} >>\nstream\n${paint}endstream`);
  }
  let pdf='%PDF-1.4\n', offsets=[0];
  for(let i=0;i<objects.length;i++){offsets.push(Buffer.byteLength(pdf));pdf+=`${i+1} 0 obj\n${objects[i]}\nendobj\n`;}
  const xref=Buffer.byteLength(pdf);
  pdf+=`xref\n0 ${objects.length+1}\n0000000000 65535 f \n${offsets.slice(1).map(n=>`${String(n).padStart(10,'0')} 00000 n \n`).join('')}trailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
  return Buffer.from(pdf);
}
const drawing=JSON.stringify({type:'excalidraw',elements:[{id:'preview-rectangle',type:'rectangle',x:50,y:50,width:140,height:90,isDeleted:false}],files:{}});
module.exports=async function checkImportTask(page,baseUrl,evidence='docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await fs.mkdir(evidence,{recursive:true});
  const report={checks:[],pageErrors:[]};
  page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.addInitScript(()=>{
    localStorage.setItem('boardcast.lang','en');localStorage.setItem('boardcast.localLlm','http://127.0.0.1:19881');
    window.qaImport={reads:[],retry:0,failWrites:false,vision:[],visionMode:'hold'};
    const originalText=File.prototype.text;
    File.prototype.text=function(){
      if(this.name==='delayed.excalidraw')return new Promise((resolve,reject)=>window.qaImport.reads.push({resolve,reject}));
      if(this.name==='retry.excalidraw'&&window.qaImport.retry++===0)return Promise.resolve('invalid-json');
      return originalText.call(this);
    };
    const put=IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put=function(...args){if(this.name==='meetings'&&window.qaImport.failWrites)throw new DOMException('Injected import save failure','QuotaExceededError');return put.apply(this,args);};
    const fetch=window.fetch.bind(window);
    window.fetch=function(url,init){
      if(String(url).startsWith('http://127.0.0.1:19881')&&init?.method==='POST'){
        const body=JSON.parse(init.body);
        if(Array.isArray(body.messages?.[0]?.content)){
          const state={aborted:false};window.qaImport.vision.push(state);
          init.signal?.addEventListener('abort',()=>state.aborted=true);
          // Deliberately uncancellable transport proves the stale result guard too.
          if(window.qaImport.visionMode==='hold')return new Promise((resolve,reject)=>{state.resolve=resolve;state.reject=reject});
          if(window.qaImport.visionMode==='fail')return Promise.resolve(new Response('Injected vision failure',{status:503}));
          return Promise.resolve(new Response(JSON.stringify({choices:[{message:{content:'无'}}]}),{status:200}));
        }
        return Promise.resolve(new Response(JSON.stringify({choices:[{message:{content:'N 「Original content」'}}]}),{status:200}));
      }
      if(String(url).startsWith('http://127.0.0.1:19881'))return Promise.resolve(new Response('{}',{status:200}));
      return fetch(url,init);
    };
  });
  const dialog=()=>page.getByRole('dialog',{name:'Import materials',exact:true});
  const cancel=()=>dialog().getByRole('button',{name:'Cancel import',exact:true}).click();
  const ready=()=>dialog().getByRole('button',{name:'Add to this meeting',exact:true}).waitFor({timeout:90000});
  const upload=(name,buffer,mimeType='application/json')=>page.locator('input[type=file]').setInputFiles({name,mimeType,buffer});
  const read=()=>page.evaluate(async()=>{
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
    const rows=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').getAll();r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});db.close();return {id:localStorage.getItem('boardcast.meeting'),rows};
  });
  const current=async()=>{const s=await read();const m=s.rows.find(m=>m.id===s.id);return m&&{...m,pages:m.pages??[{graph:m.graph,scene:m.scene,layout:m.layout,steps:m.steps}]}};
  const saved=async pred=>{for(let i=0;i<100;i++){const m=await current();if(m&&pred(m))return m;await page.waitForTimeout(100);}throw Error('Import state was not saved')};
  const check=(name)=>report.checks.push(name);
  try {
    await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Keep original content.');await page.getByRole('button',{name:'Put on the board',exact:true}).click();
    await page.locator('.save-state.saved').waitFor();
    const original=await saved(m=>m.graph.nodes.length===1);
    const drop=await page.evaluateHandle(()=>{const d=new DataTransfer();d.items.add(new File(['{}'],'first.json',{type:'application/json'}));d.items.add(new File(['{}'],'second.json',{type:'application/json'}));return d});
    await page.locator('main').dispatchEvent('drop',{dataTransfer:drop});await drop.dispose();
    assert.ok((await page.locator('body').innerText()).includes('one file at a time'));assert.equal(await dialog().count(),0);check('multi-file drop is explicit and imports none');
    await upload('drawing-without-extension',Buffer.from(drawing));await ready();await cancel();check('JSON MIME import works without filename extension');
    for(const fail of [false,true]){
      const n=await page.evaluate(()=>window.qaImport.reads.length);
      await upload('delayed.excalidraw',Buffer.from(drawing));await page.waitForFunction(n=>window.qaImport.reads.length>n,n);
      await cancel();await dialog().waitFor({state:'hidden'});
      await page.evaluate(({n,fail,drawing})=>{const r=window.qaImport.reads[n];if(fail)r.reject(Error('Canceled late import error'));else r.resolve(drawing);},{n,fail,drawing});
      await page.waitForTimeout(200);assert.equal(await dialog().count(),0);assert.ok(!(await page.locator('body').innerText()).includes('Canceled late import error'));
      assert.deepEqual((await current()).pages,original.pages);check(`cancel ignores late ${fail?'failure':'success'}`);
    }
    await upload('retry.excalidraw',Buffer.from(drawing));await dialog().getByRole('alert').waitFor();
    await dialog().getByRole('button',{name:'Try importing again',exact:true}).click();await ready();
    assert.equal(await page.evaluate(()=>window.qaImport.retry),2);assert.deepEqual((await current()).pages,original.pages);
    await cancel();check('same file retry prepares preview without mutation; cancel retains original');
    await upload('append.excalidraw',Buffer.from(drawing));await ready();
    await dialog().getByRole('button',{name:'Add to this meeting',exact:true}).click();await dialog().waitFor({state:'hidden'});
    const appended=await saved(m=>m.pages.length===2&&m.pages[1].scene?.elements.some(e=>e.id==='import:preview-rectangle'));
    assert.equal(appended.id,original.id);assert.ok(appended.pages[0].graph.nodes.some(n=>n.label==='Original content'));check('explicit append keeps existing board and adds free objects');
    // Current meeting has a real unsaved edit before choosing a new import destination.
    await page.evaluate(()=>window.qaImport.failWrites=true);
    await page.getByRole('textbox',{name:'What was said',exact:true}).fill('Retain this unsaved statement.');await page.getByRole('button',{name:'Put on the board',exact:true}).click();await page.locator('.save-error').waitFor();
    await upload('new.excalidraw',Buffer.from(drawing));await ready();
    await dialog().getByRole('button',{name:'New meeting',exact:true}).click();await dialog().getByRole('alert').waitFor();
    assert.equal((await read()).id,original.id);assert.ok((await dialog().innerText()).includes('preview are retained'));check('save failure blocks new meeting and retains ready preview');
    await page.evaluate(()=>window.qaImport.failWrites=false);
    await dialog().getByRole('button',{name:'New meeting',exact:true}).click();await dialog().waitFor({state:'hidden'});
    const fresh=await saved(m=>m.id!==original.id&&m.pages[0].scene?.elements.some(e=>e.id==='import:preview-rectangle'));
    assert.ok((await read()).rows.find(m=>m.id===original.id).lines.some(l=>l.text==='Retain this unsaved statement.'));check('retry commit saves original before creating new imported meeting');
    await upload('scanned-limit.pdf',scannedPdf(41),'application/pdf');await ready();
    const details=await dialog().innerText();assert.ok(details.includes('40 boards'));assert.ok(details.includes('40 pages without a text layer'));assert.ok(details.includes('last 1 page not imported'));
    await page.screenshot({path:`${evidence}/import-pdf-preview.png`});await cancel();assert.equal((await current()).id,fresh.id);assert.equal((await current()).pages.length,1);check('PDF preview exposes 40-board limit and scan fallback before commit');
    await upload('cancel-active.pdf',scannedPdf(41),'application/pdf');
    await dialog().getByText(/Reading cancel-active.pdf: page/).waitFor();await cancel();await page.waitForTimeout(500);
    assert.equal(await dialog().count(),0);assert.equal((await current()).pages.length,1);check('active PDF extraction can be canceled without partial boards');
    // A PNG with real segmented glyph-like regions exercises vision request cancellation.
    const png=Buffer.from(await page.evaluate(()=>{const c=document.createElement('canvas');c.width=600;c.height=300;const x=c.getContext('2d');x.fillStyle='white';x.fillRect(0,0,600,300);x.fillStyle='black';for(let i=20;i<250;i+=18)x.fillRect(i,30,14,14);return c.toDataURL('image/png').split(',')[1];}),'base64');
    await upload('vision.png',png,'image/png');await page.waitForFunction(()=>window.qaImport.vision.length>0);await cancel();
    assert.equal(await page.evaluate(()=>window.qaImport.vision[0].aborted),true);
    await page.evaluate(()=>window.qaImport.vision[0].resolve(new Response(JSON.stringify({choices:[{message:{content:'Canceled vision text'}}]}),{status:200})));
    await page.waitForTimeout(200);assert.equal(await dialog().count(),0);assert.equal((await current()).pages.length,1);check('cancel aborts vision request and discards uncancellable late response');
    await page.evaluate(()=>window.qaImport.visionMode='fail');await upload('vision.png',png,'image/png');await dialog().getByRole('alert').waitFor();
    await page.evaluate(()=>window.qaImport.visionMode='success');await dialog().getByRole('button',{name:'Try importing again',exact:true}).click();await ready();
    assert.ok((await dialog().innerText()).includes('No text was recognized'));check('vision failure retries same image and previews no-text fallback');
    await dialog().getByRole('button',{name:'Add to this meeting',exact:true}).click();await dialog().waitFor({state:'hidden'});
    await saved(m=>m.pages.length===2&&!!m.pages[1].underlay);check('no-text image commits original underlay after explicit choice');
    await page.getByRole('button',{name:'Settings',exact:true}).click();
    await page.getByRole('tab',{name:'Board & models',exact:true}).click();
    await page.getByPlaceholder('http://127.0.0.1:8080').fill('https://image-test.example.invalid');
    await page.getByRole('dialog').getByRole('button',{name:'Close',exact:true}).click();
    let remoteImages=0;page.on('request',r=>{if(r.url().startsWith('https://image-test.example.invalid')&&r.method()==='POST')remoteImages++});
    await upload('local-only.png',png,'image/png');await dialog().getByRole('alert').waitFor();assert.ok((await dialog().innerText()).includes('not sent to the remote address'));assert.equal(remoteImages,0);
    await dialog().getByRole('button',{name:'Configure local image model',exact:true}).click();
    assert.equal(await page.getByRole('dialog',{name:'Settings',exact:true}).getByRole('tab',{name:'Board & models',exact:true}).getAttribute('aria-selected'),'true');
    assert.ok(await page.getByPlaceholder('http://127.0.0.1:8080').isVisible());
    await page.getByPlaceholder('http://127.0.0.1:8080').fill('http://127.0.0.1:19881');
    await page.getByRole('dialog',{name:'Settings',exact:true}).getByRole('button',{name:'Close',exact:true}).click();
    await dialog().getByRole('button',{name:'Try importing again',exact:true}).click();await ready();await cancel();check('remote image transfer refused; local configuration recovers same file');
    const beforeBroken=await current(), beforeRequests=await page.evaluate(()=>window.qaImport.vision.length);
    await upload('broken.png',Buffer.from('not a PNG'),'image/png');await dialog().getByRole('alert').waitFor();
    assert.ok((await dialog().innerText()).includes("Can't open the image (PNG, JPEG, WebP and GIF are supported)"));
    assert.equal(await page.evaluate(()=>window.qaImport.vision.length),beforeRequests);await cancel();
    assert.deepEqual((await current()).pages,beforeBroken.pages);check('corrupt image fails before recognition and retains existing boards');
    const images=await page.evaluate(()=>{
      const make=(width,height,regions,type)=>{
        const c=document.createElement('canvas');c.width=width;c.height=height;const x=c.getContext('2d');
        x.fillStyle='white';x.fillRect(0,0,width,height);x.fillStyle='black';
        for(let i=0;i<regions;i++)for(let j=0;j<3;j++)x.fillRect(30+(i%8)*190+j*18,30+Math.floor(i/8)*95,14,18);
        return c.toDataURL(type).split(',')[1];
      };
      return [...['png','jpeg','webp'].map(ext=>({name:`format.${ext}`,type:`image/${ext}`,data:make(600,300,2,`image/${ext}`),width:600,height:300})),
        {name:'large.png',type:'image/png',data:make(3600,2400,2,'image/png'),width:1800,height:1200},
        {name:'limit.png',type:'image/png',data:make(1600,1200,96,'image/png'),width:1600,height:1200}];
    });
    images.push({name:'blank.gif',type:'image/gif',data:'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7',width:1,height:1});
    report.images=[];
    for(const image of images){
      const before=await current(), requests=await page.evaluate(()=>window.qaImport.vision.length);
      await upload(image.name,Buffer.from(image.data,'base64'),image.type);await ready();
      const calls=await page.evaluate(()=>window.qaImport.vision.length)-requests;
      if(image.name==='blank.gif')assert.equal(calls,0);else assert.ok(calls>0);
      if(image.name==='limit.png'){
        assert.equal(calls,60);assert.ok((await dialog().innerText()).includes('Only the first 60 regions were read; the full image is retained.'));
        await page.screenshot({path:`${evidence}/image-limit-preview.png`});
      }
      assert.deepEqual((await current()).pages,before.pages);
      await dialog().getByRole('button',{name:'Add to this meeting',exact:true}).click();await dialog().waitFor({state:'hidden'});
      const after=await saved(m=>m.pages.length===before.pages.length+1&&!!m.pages.at(-1).underlay);
      assert.deepEqual(after.pages.slice(0,-1),before.pages);
      assert.ok(after.pages.at(-1).underlay.startsWith('asset:'));
      const dimensions=await page.evaluate(async({id,source})=>{
        const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
        try{
          const image=await new Promise((yes,no)=>{const r=db.transaction('assets').objectStore('assets').get(id+'/'+source.slice(6));r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
          const b=await createImageBitmap(await(await fetch(image)).blob());const d={width:b.width,height:b.height};b.close();return d;
        }finally{db.close();}
      },{id:after.id,source:after.pages.at(-1).underlay});
      assert.deepEqual(dimensions,{width:image.width,height:image.height});
      report.images.push({name:image.name,calls,dimensions});check(`${image.name}: decoded image preview, explicit append and expected retained resolution`);
    }
    assert.deepEqual(report.pageErrors,[]);report.passed=true;
    await page.screenshot({path:`${evidence}/import-task-complete.png`});return report;
  }catch(error){report.error=String(error);report.dialogs=await page.locator('dialog').evaluateAll(ds=>ds.map(d=>({open:d.open,label:d.getAttribute('aria-label'),inert:d.inert,html:d.outerHTML.slice(0,500)})));report.aria=await page.locator('body').ariaSnapshot();await page.screenshot({path:`${evidence}/import-task-failure.png`}).catch(()=>{});throw error;}
  finally{await fs.writeFile(`${evidence}/import-task-check.json`,JSON.stringify(report,null,2)+'\n');}
};
module.exports.scannedPdf=scannedPdf;
