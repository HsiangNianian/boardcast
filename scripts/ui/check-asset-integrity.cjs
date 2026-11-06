// Real browser image decoding and IndexedDB; native file/IPC coverage is a separate G2 check.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkAssetIntegrity(page, baseUrl, evidence='docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`${baseUrl}/scripts/ui/fixtures/asset-integrity.html`);
  const result=await page.evaluate(async()=>{
    const {meetings}=await import('/src/meetings.ts');
    const {assetKey,verifyAsset}=await import('/src/engine/assetIntegrity.ts');
    const {emptyGraph}=await import('/src/engine/graph.ts');
    const checks=[];
    const check=(ok,description)=>{if(!ok)throw new Error(description);checks.push(description)};
    const failure=async action=>{try{await action();return null}catch(error){return String(error)}};
    const tx=async(store,mode,action)=>new Promise((resolve,reject)=>{
      const req=indexedDB.open('boardcast-v2',1);
      req.onerror=()=>reject(req.error);
      req.onsuccess=()=>{
        const db=req.result,t=db.transaction(store,mode),r=action(t.objectStore(store));
        t.oncomplete=()=>{db.close();resolve(r.result)};
        t.onabort=t.onerror=()=>{db.close();reject(t.error)};
      };
    });
    const canvas=document.createElement('canvas');canvas.width=32;canvas.height=16;
    const context=canvas.getContext('2d');context.fillStyle='#00ffff';context.fillRect(0,0,32,16);const cyan=canvas.toDataURL();
    context.fillStyle='#ff00ff';context.fillRect(0,0,32,16);const magenta=canvas.toDataURL();
    const svg='data:image/svg+xml;base64,'+btoa('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><rect width="64" height="32" fill="#00ff00"/></svg>');
    const m={version:2,id:'asset-integrity-browser',title:'Original',createdAt:1,updatedAt:1,graph:emptyGraph(),history:[],lines:[],page:0,pages:[{graph:{...emptyGraph(),underlay:{w:320,h:160}},underlay:cyan},{graph:emptyGraph(),underlay:svg}]};
    await meetings.save(m);
    const key=await assetKey(cyan),path=`${m.id}/${key}`;
    const raw=await meetings.raw(m.id),loaded=await meetings.load(m.id);
    check(loaded.pages[0].underlay===cyan&&loaded.pages[1].underlay===svg&&!loaded.assetIssues,'real PNG and SVG decode and round-trip through IndexedDB');
    await tx('assets','readwrite',s=>s.put(magenta,path));
    const damaged=await meetings.load(m.id);
    check(damaged.assetIssues?.[0]?.reason==='hash-mismatch'&&damaged.pages[0].underlay===`asset:${key}`,'valid different PNG is rejected under the original hash and keeps its reference');
    check(damaged.pages[1].underlay===svg,'unaffected content remains readable');
    const error=await failure(()=>meetings.save({...damaged,title:'Retained edit'}));
    check(error?.includes('content hash'),'editing refuses to commit an unresolved content mismatch');
    check(JSON.stringify(await meetings.raw(m.id))===JSON.stringify(raw),'failed commit leaves the original record unchanged');
    check(await tx('assets','readonly',s=>s.get(path))===magenta,'failed commit preserves damaged original bytes');
    await meetings.save({...m,id:'asset-integrity-clean-copy',pages:[{graph:emptyGraph(),underlay:magenta}]});
    const copy=await meetings.raw('asset-integrity-clean-copy');
    check(copy.pages[0].underlay===`asset:${await assetKey(magenta)}`,'rejected filename cannot poison the hash used by a later meeting');
    await tx('assets','readwrite',s=>s.put(cyan,path));
    await meetings.save({...damaged,title:'Retained edit'});
    const restored=await meetings.load(m.id);
    check(restored.title==='Retained edit'&&!restored.assetIssues&&!restored.missingAssets&&restored.pages[0].underlay===cyan,'restoring the original permits retry of the same in-memory edits and clears diagnostics');
    await tx('assets','readwrite',s=>s.put(magenta,path));
    check((await failure(()=>meetings.save(restored)))?.includes('content hash'),'post-load corruption is checked even when correct bytes remain in memory');
    await tx('assets','readwrite',s=>s.put(cyan,path));
    const truncated='data:image/png;base64,'+btoa(atob(cyan.split(',')[1]).slice(0,32));
    const badKey=await assetKey(truncated);
    const beforeBad=JSON.stringify(await meetings.raw(m.id));
    const badError=await failure(()=>meetings.save({...restored,pages:[{graph:emptyGraph(),underlay:truncated}]}));
    check(badError?.includes('decoded')&&JSON.stringify(await meetings.raw(m.id))===beforeBad,'truncated PNG with a correct hash is rejected by the real decoder before commit');
    const historical={...raw,id:'asset-integrity-history',pages:[{graph:emptyGraph(),steps:[{t:1,line:0,d:{},scene:{elements:[],generated:[],files:{f:{id:'f',mimeType:'image/png',created:1,dataURL:`asset:${badKey}`}}}}]}]};
    await tx('meetings','readwrite',s=>s.put(historical));
    await tx('assets','readwrite',s=>s.put(truncated,`${historical.id}/${badKey}`));
    const history=await meetings.load(historical.id);
    check(history.assetIssues?.[0]?.reason==='decode'&&history.pages[0].steps[0].scene.files.f.dataURL===`asset:${badKey}`,'historical-only corrupt image is diagnosed without losing its original reference');
    check((await failure(()=>meetings.save(history)))?.includes('decoded'),'historical-only corruption also blocks the record commit');
    const future={...raw,id:'asset-integrity-future',version:99};
    await new Promise((resolve,reject)=>{
      const req=indexedDB.open('boardcast',2);
      req.onupgradeneeded=()=>{const db=req.result;if(!db.objectStoreNames.contains('meetings'))db.createObjectStore('meetings',{keyPath:'id'});if(!db.objectStoreNames.contains('assets'))db.createObjectStore('assets')};
      req.onerror=()=>reject(req.error);req.onsuccess=()=>{const db=req.result,t=db.transaction('meetings','readwrite');t.objectStore('meetings').put(future);t.oncomplete=()=>{db.close();resolve()};t.onerror=()=>{db.close();reject(t.error)}};
    });
    check((await failure(()=>meetings.load(`legacy:${future.id}`)))?.includes('newer Boardcast'),'virtual legacy-copy loading cannot bypass the future-version guard');
    const NativeImage=window.Image;
    const stalled=svg.replace(/.$/,'A'), stalledKey=await assetKey(stalled);
    let timeoutMs;
    try {
      window.Image=class {set src(value){} removeAttribute(){} };
      const started=performance.now();
      const error=await failure(()=>verifyAsset(stalled,stalledKey));
      timeoutMs=performance.now()-started;
      check(error?.includes('decoded')&&timeoutMs>=4900&&timeoutMs<8000,'a stalled image decoder fails within a bounded interval');
    } finally {window.Image=NativeImage}
    return {passed:true,checks,timeoutMs};
  });
  assert.deepEqual(errors,[]);
  await fs.mkdir(evidence,{recursive:true});
  await fs.writeFile(`${evidence}/asset-integrity-check.json`,JSON.stringify({...result,pageErrors:errors},null,2));
};
