const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {click: meetingAction} = require('./meeting-action.cjs');

// Actual large PNG/SVG downloads; only the canvas encoder failure is injected.
module.exports = async function checkPngExport(page, baseUrl, evidence) {
  const report={baseUrl,checks:[],failures:[],boards:[],pageErrors:[]};
  const check=(ok,label)=>{report[ok?'checks':'failures'].push(label);console.log(`${ok?'PASS':'FAIL'}: ${label}`);};
  page.on('pageerror',error=>report.pageErrors.push(error.message));
  const choose=async (format,zh)=>{
    await meetingAction(page,zh?'导出':'Export');
    await page.getByRole('menuitemradio',{name:zh?'当前白板':'Current board',exact:true}).click();
    await page.getByRole('menuitem',{name:new RegExp(`^${format}`)}).click();
  };
  const download=async (format,zh,name)=>{
    const pending=page.waitForEvent('download');await choose(format,zh);
    const item=await pending,file=path.join(evidence,name);await item.saveAs(file);return fs.readFile(file);
  };
  const stored=()=>page.evaluate(async()=>{
    const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
    const value=await new Promise((yes,no)=>{const r=db.transaction('meetings').objectStore('meetings').get('qa-large-png');r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});db.close();return JSON.stringify(value);
  });
  try {
    await fs.mkdir(evidence,{recursive:true});await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});
    report.scripts=await page.locator('script[src]').evaluateAll(xs=>xs.map(x=>x.src));
    for(const tall of [false,true]) {
      const zh=tall;
      await page.evaluate(async tall=>{
        const w=tall?720:12000,h=tall?12000:720;
        const c=document.createElement('canvas');c.width=80;c.height=60;const x=c.getContext('2d');x.fillStyle='#00b4dc';x.fillRect(0,0,80,60);
        const graph={nodes:[{id:'n1',label:'LARGE_BOARD',kind:'node',aliases:[],marks:[],ink:'ink'}],edges:[],groups:[],nextNode:2,nextGroup:1,underlay:{w,h}};
        const rect=(id,x,y,color)=>({id,type:'rectangle',x,y,width:120,height:120,angle:0,strokeColor:color,backgroundColor:color,fillStyle:'solid',strokeWidth:2,strokeStyle:'solid',roughness:0,opacity:100,groupIds:[],frameId:null,roundness:null,index:null,seed:1,version:1,versionNonce:1,updated:0,isDeleted:false,boundElements:null,link:null,locked:false});
        const scene={elements:[rect('negative',-200,-180,'#ff0000'),rect('positive',w+200,h+180,'#00b400')],generated:[],files:{}};
        const steps=[{t:1,line:0,d:{nodes:{set:graph.nodes},nextNode:2,underlay:graph.underlay},scene}];
        const board={graph,scene,steps,underlay:c.toDataURL()};
        const row={version:2,storageRevision:1,id:'qa-large-png',title:'Large PNG fixture',titled:true,createdAt:1,updatedAt:2,...board,pages:[board],page:0,history:[],lines:[]};
        const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error);});
        await new Promise((yes,no)=>{const tx=db.transaction('meetings','readwrite');tx.objectStore('meetings').put(row);tx.oncomplete=yes;tx.onerror=()=>no(tx.error);});db.close();
        localStorage.setItem('boardcast.meeting',row.id);localStorage.setItem('boardcast.lang',tall?'zh':'en');localStorage.setItem('boardcast.theme',tall?'dark':'light');localStorage.setItem('boardcast.mode','ink');
      },tall);
      await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
      for(const mode of ['ink','pencil']) {
        if(mode==='pencil'){await page.getByRole('radio',{name:zh?'自由编辑':'Free edit',exact:true}).click();await page.locator('canvas.excalidraw__canvas.interactive').waitFor();}
        const id=`${tall?'tall-dark-zh':'wide-light-en'}-${mode}`,before=await stored();
        const png=await download('PNG',zh,`${id}.png`);
        await page.evaluate(()=>{
          window.qaPngBlob=HTMLCanvasElement.prototype.toBlob;
          HTMLCanvasElement.prototype.toBlob=function(callback,type,...args){if(type==='image/png')return queueMicrotask(()=>callback(null));return window.qaPngBlob.call(this,callback,type,...args);};
        });
        try {
          await page.setViewportSize({width:640,height:400});await choose('PNG',zh);await page.locator('.toast.err').waitFor();
          const error=await page.locator('.toast').innerText();
          check(error.includes(zh?'导出 → 当前白板 → SVG 矢量图':'Export → Current board → SVG vector'),`${id}: PNG generation failure explains the SVG route`);
          check(await page.locator('.toast').getByRole('button',{name:zh?'重新导出':'Export again',exact:true}).isEnabled(),`${id}: failed generation releases the export action`);
          const bounds=await page.locator('.toast').boundingBox();
          check(bounds&&bounds.x>=0&&bounds.y>=0&&bounds.x+bounds.width<=640&&bounds.y+bounds.height<=400,`${id}: recovery text and actions fit the compact window`);
          await page.screenshot({path:path.join(evidence,`${id}-failure.png`)});
          const svg=await download(zh?'SVG 矢量图':'SVG vector',zh,`${id}.svg`);
          const pixels=await page.evaluate(async ({png,svg})=>{
            const load=async(bytes,type)=>{const img=new Image();img.src=`data:${type};base64,${bytes}`;await img.decode();return img;};
            const [p,s]=await Promise.all([load(png,'image/png'),load(svg,'image/svg+xml')]);
            const c=document.createElement('canvas');c.width=p.width;c.height=p.height;const x=c.getContext('2d',{willReadFrequently:true});
            const stats=img=>{
              x.clearRect(0,0,c.width,c.height);x.drawImage(img,0,0,c.width,c.height);const a=x.getImageData(0,0,c.width,c.height).data;
              const colors=Object.fromEntries(['red','green','blue'].map(k=>[k,{count:0,x0:c.width,y0:c.height,x1:0,y1:0}]));
              for(let i=0;i<a.length;i+=4){const r=a[i],g=a[i+1],b=a[i+2];if(a[i+3]<200)continue;const key=r>230&&g<30&&b<30?'red':r<30&&g>150&&g<210&&b<30?'green':r<30&&g>150&&g<210&&b>200?'blue':null;if(!key)continue;const v=colors[key],j=i/4,px=j%c.width,py=Math.floor(j/c.width);v.count++;v.x0=Math.min(v.x0,px);v.y0=Math.min(v.y0,py);v.x1=Math.max(v.x1,px);v.y1=Math.max(v.y1,py);}
              return colors;
            };
            const doc=new DOMParser().parseFromString(atob(svg),'image/svg+xml');
            return {png:{width:p.width,height:p.height,colors:stats(p)},svg:{width:s.width,height:s.height,colors:stats(s)},validSvg:!doc.querySelector('parsererror'),title:doc.querySelector('svg > title')?.textContent};
          },{png:png.toString('base64'),svg:svg.toString('base64')});
          report.boards.push({id,error,...pixels});
          check(Math.max(pixels.png.width,pixels.png.height)===8000&&Math.min(pixels.png.width,pixels.png.height)>0,`${id}: actual PNG limits its longest edge to 8000 pixels`);
          check(pixels.validSvg&&pixels.title==='Large PNG fixture'&&Math.max(pixels.svg.width,pixels.svg.height)>12000,`${id}: SVG downloads successfully while the PNG encoder still fails`);
          check(Object.values(pixels.png.colors).every(c=>c.count>200&&c.x0>0&&c.y0>0&&c.x1<pixels.png.width-1&&c.y1<pixels.png.height-1),`${id}: background and both outside edge objects remain visible and uncropped`);
          check(Object.keys(pixels.png.colors).every(k=>['x0','y0','x1','y1'].every(v=>Math.abs(pixels.png.colors[k][v]-pixels.svg.colors[k][v])<=3)),`${id}: PNG and SVG preserve the same visual bounds`);
          check(await stored()===before,`${id}: successful and failed exports leave the meeting unchanged`);
        } finally {
          await page.evaluate(()=>{HTMLCanvasElement.prototype.toBlob=window.qaPngBlob;delete window.qaPngBlob;});
          await page.setViewportSize({width:1280,height:800});
        }
      }
    }
    check(report.pageErrors.length===0,'no uncaught browser errors');
  } finally {await fs.writeFile(path.join(evidence,'report.json'),JSON.stringify(report,null,2));}
  assert.equal(report.failures.length,0,report.failures.join('\n'));
};
