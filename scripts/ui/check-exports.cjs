const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const {pathToFileURL} = require('node:url');

module.exports = async function checkExports(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation/exports') {
  await fs.mkdir(evidence, {recursive: true});
  const report = {baseUrl, files: {}, checks: [], failures: [], pageErrors: [], offline: {requests: [], failures: [], pageErrors: []}};
  const check = (condition, description) => { (condition ? report.checks : report.failures).push(description); };
  page.on('pageerror', e => report.pageErrors.push(e.message));
  await page.addInitScript(() => { localStorage.setItem('boardcast.lang', 'en'); localStorage.setItem('boardcast.theme', 'light'); localStorage.setItem('boardcast.mode', 'ink'); });
  const download = async (label, key) => {
    await page.getByRole('button', {name: 'Export', exact: true}).click();
    await page.getByRole('menuitemradio', {name: ['Markdown','Whole meeting PDF','Replay page'].includes(label) ? 'Whole meeting' : 'Current board', exact: true}).click();
    const pending = page.waitForEvent('download', {timeout: 60000});
    await page.getByRole('menuitem', {name: new RegExp(`^${label}`)}).click();
    const item = await pending;
    const file = path.join(evidence, `${key}-${item.suggestedFilename()}`);
    await item.saveAs(file);
    const bytes = await fs.readFile(file);
    report.files[key] = {path: file, bytes: bytes.length};
    check(bytes.length > 100, `${key}: nonempty download`);
    return {file, bytes, text: bytes.toString('utf8')};
  };
  const rasterStats = async (bytes, mime = 'image/png') => page.evaluate(async ({b64, mime}) => {
    const image = new Image(); image.src = `data:${mime};base64,${b64}`; await image.decode();
    const scale = Math.min(1, 1600 / Math.max(image.width, image.height));
    const canvas = document.createElement('canvas'); canvas.width = Math.ceil(image.width * scale); canvas.height = Math.ceil(image.height * scale);
    const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    const counts = {cyan: 0, magenta: 0, red: 0, green: 0, dark: 0};
    for (let i = 0; i < pixels.length; i += 4) {
      const [r,g,b,a] = pixels.slice(i, i + 4); if (a < 200) continue;
      if (r < 90 && g > 130 && g < 225 && b > 170) counts.cyan++;
      if (r > 200 && g < 80 && b > 170) counts.magenta++;
      if (r > 200 && g < 80 && b < 80) counts.red++;
      if (r < 90 && g > 120 && b < 120) counts.green++;
      if (r < 100 && g < 100 && b < 100) counts.dark++;
    }
    return {width: image.width, height: image.height, counts};
  }, {b64: bytes.toString('base64'), mime});
  let offlineContext;
  try {
    await page.goto(baseUrl); await page.locator('.splash').waitFor({state: 'hidden'});
    report.productionScripts = await page.locator('script[src]').evaluateAll(els => els.map(e => e.src));
    const fixture = await page.evaluate(async () => {
      const png = color => { const c = document.createElement('canvas'); c.width = 80; c.height = 60; const x = c.getContext('2d'); x.fillStyle = color; x.fillRect(0,0,80,60); return c.toDataURL(); };
      const underlay = png('rgb(40,180,220)'), red = png('#ff0000');
      const vector = 'data:image/svg+xml;base64,' + btoa('<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60"><rect width="60" height="60" fill="#00b446"/></svg>');
      const empty = () => ({nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1});
      const node = (id,label) => ({id,label,aliases:[],kind:'node',marks:[],ink:'ink'});
      const graph = {nodes:[node('n1','导出节点 EXPORT_NODE_A'),node('n2','EXPORT_NODE_B')],edges:[{from:'n1',to:'n2',label:'EXPORT_EDGE',ink:'ink'}],groups:[],nextNode:3,nextGroup:1,underlay:{w:640,h:360}};
      const el = (id,type,x,y,width,height,extra = {}) => ({id,type,x,y,width,height,angle:0,strokeColor:'#111111',backgroundColor:'transparent',fillStyle:'solid',strokeWidth:2,strokeStyle:'solid',roughness:0,opacity:100,groupIds:[],frameId:null,roundness:null,index:null,seed:1,version:1,versionNonce:1,updated:0,isDeleted:false,boundElements:null,link:null,locked:false,...extra});
      const text = (id,value,x,y) => el(id,'text',x,y,240,27,{text:value,originalText:value,fontSize:22,fontFamily:1,textAlign:'left',verticalAlign:'top',containerId:null,autoResize:true,lineHeight:1.25});
      const elements = [
        el('free-magenta','rectangle',420,50,100,80,{strokeColor:'#ff00ff',backgroundColor:'#ff00ff'}),
        el('free-red-image','image',50,240,80,60,{fileId:'red',status:'saved',scale:[1,1],crop:null}),
        el('free-green-svg','image',160,240,60,60,{fileId:'green',status:'saved',scale:[1,1],crop:null}),
        text('free-text','FREE_EXPORT_TOKEN',350,190),
        el('free-stroke','freedraw',390,280,130,30,{strokeColor:'#00b446',points:[[0,0],[35,30],[90,0],[130,25]],pressures:[0.5,0.5,0.5,0.5],simulatePressure:false,lastCommittedPoint:null}),
      ];
      const files = {red:{id:'red',mimeType:'image/png',dataURL:red,created:1},green:{id:'green',mimeType:'image/svg+xml',dataURL:vector,created:1}};
      const scene = {elements,generated:[],files};
      const freeScene = {elements:[el('only-free-magenta','rectangle',20,20,150,100,{strokeColor:'#ff00ff',backgroundColor:'#ff00ff'}),text('only-free-text','ONLY_FREE_TOKEN',20,150)],generated:[],files:{}};
      const pages = [
        {graph,underlay,scene,steps:[{t:1000,line:1,d:{nodes:{set:graph.nodes},edges:{set:graph.edges},nextNode:3,underlay:graph.underlay},scene:null},{t:2000,line:2,d:{},scene}]},
        {graph:empty(),scene:freeScene,steps:[{t:3000,line:2,d:{},scene:freeScene}]},
        {graph:empty(),steps:[]},
      ];
      const row = {version:2,storageRevision:1,id:'qa-complete-exports',title:'EXPORT_MEETING_TITLE',titled:true,createdAt:1,updatedAt:2,graph:empty(),history:[],lines:[{text:'TRANSCRIPT_EXPORT_TOKEN first board',speaker:'Alice',ms:1,page:0},{text:'TRANSCRIPT_SECOND_TOKEN free board',speaker:'Bob',ms:1,page:1}],summary:'SUMMARY_EXPORT_TOKEN\n\n- Confirm complete exported content.',pages,page:2,steps:[]};
      const db = await new Promise((yes,no) => { const r = indexedDB.open('boardcast-v2',1); r.onupgradeneeded=()=>{r.result.createObjectStore('meetings',{keyPath:'id'});r.result.createObjectStore('assets');};r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error); });
      await new Promise((yes,no)=>{const t=db.transaction('meetings','readwrite');t.objectStore('meetings').put(row);t.oncomplete=yes;t.onerror=()=>no(t.error);});db.close();localStorage.setItem('boardcast.meeting',row.id);
      return {underlay,red,vector,graph,scene,freeScene};
    });
    await page.reload(); await page.locator('.splash').waitFor({state:'hidden'});
    await page.locator('.pages').getByRole('button',{name:'Board 3',exact:true}).waitFor();
    await page.getByRole('button',{name:'Export',exact:true}).click();
    await page.getByRole('menuitemradio',{name:'Current board',exact:true}).click();
    for (const label of ['PNG image','SVG vector','Animated SVG','Excalidraw']) check(await page.getByRole('menuitem',{name:new RegExp(`^${label}`)}).isDisabled(),`empty current board disables ${label}`);
    await page.getByRole('menuitemradio',{name:'Whole meeting',exact:true}).click();
    for (const label of ['Markdown','Whole meeting PDF','Replay page']) check(await page.getByRole('menuitem',{name:new RegExp(`^${label}`)}).isEnabled(),`empty current board retains ${label}`);
    await page.keyboard.press('Escape');
    const md = await download('Markdown','meeting-md');
    check((await page.locator('.toast').innerText()).startsWith('Sent to your browser for download:') && await page.getByRole('button',{name:'Open file',exact:true}).count() === 0,'browser export claims a download handoff and offers no native file action');
    const [{default: Markdown}, {createElement}, {renderToStaticMarkup}] = await Promise.all([import('react-markdown'), import('react'), import('react-dom/server')]);
    const renderedMarkdown = renderToStaticMarkup(createElement(Markdown, {children: md.text}));
    for (const token of ['EXPORT_MEETING_TITLE','SUMMARY_EXPORT_TOKEN','TRANSCRIPT_EXPORT_TOKEN','TRANSCRIPT_SECOND_TOKEN','EXPORT_NODE_A','EXPORT_NODE_B','Board 1','Board 2','Board 3']) check(renderedMarkdown.includes(token),`Markdown contains ${token}`);
    check(md.text.includes('omits free drawings'),'Markdown explicitly identifies free visuals on other boards');
    const pdf = await download('Whole meeting PDF','meeting-pdf');
    const pdfInfo = execFileSync('pdfinfo',[pdf.file],{encoding:'utf8'});report.pdfInfo=pdfInfo;
    check(/Pages:\s+3\b/.test(pdfInfo),'PDF has two nonempty boards plus summary/transcript page');
    execFileSync('pdftoppm',['-png','-scale-to','1400',pdf.file,path.join(evidence,'pdf-page')]);
    report.pdfPages={};
    for(let n=1;n<=3;n++) { const image=await fs.readFile(path.join(evidence,`pdf-page-${n}.png`)); report.pdfPages[n]=await rasterStats(image); }
    check(report.pdfPages[1].counts.cyan>2000 && report.pdfPages[1].counts.magenta>100 && report.pdfPages[1].counts.red>100 && report.pdfPages[1].counts.green>100,'PDF first board includes underlay, free shape, PNG and SVG image colors');
    check(report.pdfPages[2].counts.magenta>100,'PDF includes the scene-only board');
    check(report.pdfPages[3].counts.dark>500,'PDF summary/transcript page contains rendered text');
    const html = await download('Replay page','meeting-html');
    check(html.text.includes('SUMMARY_EXPORT_TOKEN')&&html.text.includes('TRANSCRIPT_EXPORT_TOKEN'),'HTML retains summary and transcript');
    await page.locator('.pages').getByRole('button',{name:'Board 1',exact:true}).click();
    await page.locator('image.free-scene').waitFor();
    const svg = await download('SVG vector','board1-svg');
    const svgStats = await rasterStats(svg.bytes,'image/svg+xml'); report.files['board1-svg'].raster=svgStats;
    check(svg.text.includes(fixture.underlay),'SVG embeds the page background');
    check(svgStats.counts.cyan>2000 && svgStats.counts.magenta>100 && svgStats.counts.red>100 && svgStats.counts.green>100,'SVG pixels include underlay, free shape and both image types');
    const png = await download('PNG image','board1-png');
    const pngStats=await rasterStats(png.bytes);report.files['board1-png'].raster=pngStats;
    check(pngStats.counts.cyan>2000 && pngStats.counts.magenta>100 && pngStats.counts.red>100 && pngStats.counts.green>100,'PNG pixels include underlay, free shape and both image types');
    const anim = await download('Animated SVG','board1-animated');
    check(anim.text.includes('@keyframes')&&anim.text.includes(fixture.underlay),'Animated SVG contains timeline and page background');
    const finalAnim=Buffer.from(anim.text.replace('</svg>','<style>*{animation:none!important;stroke-dashoffset:0!important;opacity:1!important}</style></svg>'));
    report.files['board1-animated'].finalRaster=await rasterStats(finalAnim,'image/svg+xml');
    check(report.files['board1-animated'].finalRaster.counts.magenta>100 && report.files['board1-animated'].finalRaster.counts.red>100,'Animated SVG final frame retains free shapes and image');
    const animationPage=await page.context().newPage();
    try {
      report.files['board1-animated'].playback=await animationPage.evaluate(async b64=>{
        const image=new Image();image.src='data:image/svg+xml;base64,'+b64;document.body.append(image);await image.decode();
        const c=document.createElement('canvas');const scale=Math.min(1,1600/Math.max(image.width,image.height));c.width=Math.ceil(image.width*scale);c.height=Math.ceil(image.height*scale);const x=c.getContext('2d');
        const dark=()=>{x.clearRect(0,0,c.width,c.height);x.drawImage(image,0,0,c.width,c.height);const p=x.getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<p.length;i+=4)if(p[i]<100&&p[i+1]<100&&p[i+2]<100&&p[i+3]>200)n++;return n;};
        const initial=dark();await new Promise(resolve=>setTimeout(resolve,14000));return {initialDark:initial,finalDark:dark(),waitMs:14000};
      },anim.bytes.toString('base64'));
      check(report.files['board1-animated'].playback.finalDark>report.files['board1-animated'].playback.initialDark+100,'Animated SVG naturally reveals semantic strokes over its timeline');
    } finally {await animationPage.close();}

    const ex = await download('Excalidraw','board1-excalidraw');
    const drawing=JSON.parse(ex.text);
    check(drawing.elements.some(e=>e.id==='free-magenta')&&drawing.elements.some(e=>e.id==='free-stroke')&&drawing.elements.some(e=>e.originalText==='FREE_EXPORT_TOKEN'),'Excalidraw retains free shape, stroke and text elements');
    check(drawing.files.red.dataURL===fixture.red&&drawing.files.green.dataURL===fixture.vector,'Excalidraw retains inserted PNG and SVG bytes');
    check(drawing.boardcast.graph.nodes.some(n=>n.label.includes('EXPORT_NODE_A'))&&drawing.boardcast.underlay===fixture.underlay,'Excalidraw recovery metadata retains semantic graph and underlay');
    check(drawing.elements.some(e=>e.customData?.boardcastUnderlay&&drawing.files[e.fileId]?.dataURL===fixture.underlay),'Excalidraw underlay is also a portable visible image');
    await page.getByRole('radio',{name:'Free edit',exact:true}).click();await page.locator('canvas.excalidraw__canvas.interactive').waitFor();
    for(const [label,key,mime] of [['PNG image','pc-png','image/png'],['SVG vector','pc-svg','image/svg+xml']]) {const file=await download(label,key);const stats=await rasterStats(file.bytes,mime);report.files[key].raster=stats;check(stats.counts.cyan>2000&&stats.counts.magenta>100&&stats.counts.red>100&&stats.counts.green>100,`${key}: complete underlay/free/image pixels`);}
    await page.getByRole('radio',{name:'Hand-drawn',exact:true}).click();
    await page.locator('.pages').getByRole('button',{name:'Board 2',exact:true}).click();await page.locator('image.free-scene').waitFor();
    for(const [label,key,mime] of [['PNG image','free-only-png','image/png'],['SVG vector','free-only-svg','image/svg+xml']]) {const file=await download(label,key);const stats=await rasterStats(file.bytes,mime);report.files[key].raster=stats;check(stats.counts.magenta>100,`${key}: scene-only board exports visible content`);}
    const only=JSON.parse((await download('Excalidraw','free-only-excalidraw')).text);check(only.boardcast.graph.nodes.length===0&&only.elements.some(e=>e.id==='only-free-magenta'),'scene-only Excalidraw remains editable with an empty semantic graph');
    offlineContext=await page.context().browser().newContext({viewport:{width:1280,height:800},reducedMotion:'reduce'});
    await offlineContext.route(/^https?:\/\//, route=>{report.offline.requests.push(route.request().url());return route.abort();});
    await offlineContext.setOffline(true);
    const offline=await offlineContext.newPage();offline.on('pageerror',e=>report.offline.pageErrors.push(e.message));offline.on('requestfailed',r=>report.offline.failures.push({url:r.url(),error:r.failure()?.errorText}));
    await offline.goto(pathToFileURL(path.resolve(html.file)).href);await offline.locator('.player .ink > svg').waitFor();await offline.locator('image.free-scene').waitFor();
    const originalData=await offline.evaluate(()=>JSON.stringify(window.__boardcastData));
    await offline.waitForFunction(()=>document.querySelectorAll('.ink path').length>50);
    const geometry=await offline.evaluate(()=>Object.fromEntries(['.player .board','.player .log','.player .topbar','.player-title','.player-pages'].map(key=>{const r=document.querySelector(key).getBoundingClientRect();return [key,{x:r.x,y:r.y,width:r.width,height:r.height}]})));report.offline.geometry=geometry;
    check(geometry['.player .board'].x===0&&geometry['.player .board'].width>=900,'offline player uses the full intended canvas column');
    check(geometry['.player-title'].y>=0&&geometry['.player-title'].y+geometry['.player-title'].height<=56&&geometry['.player-pages'].y+geometry['.player-pages'].height<=56,'offline title and page controls fit the header');
    const data=await offline.evaluate(()=>({pages:window.__boardcastData.pages.map(p=>({nodes:p.graph.nodes.length,scene:p.scene?.elements.map(e=>e.id),underlay:p.underlay,steps:p.steps?.map(s=>!!s.scene),layout:p.layout})),hanzi:Object.keys(window.__boardcastHanzi)}));report.offline.data=data;
    check(data.pages.length===3&&data.pages[0].scene.includes('free-magenta')&&data.pages[1].scene.includes('only-free-magenta'),'offline HTML retains all pages including scene-only and empty');
    check(data.pages[0].layout?.nodeAt?.length>0,'offline HTML stores the completed layout for a board never opened before export');
    check(data.pages[0].underlay===fixture.underlay&&data.hanzi.includes('导')&&data.hanzi.includes('出'),'offline HTML embeds background and required Chinese stroke data');
    check(await offline.locator('.summary').innerText().then(t=>t.includes('SUMMARY_EXPORT_TOKEN')),'offline HTML renders summary');
    check((await offline.locator('.lines').innerText()).includes('TRANSCRIPT_EXPORT_TOKEN'),'offline HTML renders transcript');
    await offline.screenshot({path:path.join(evidence,'offline-final-board1.png')});
    const range=offline.locator('.replay-dock input[type=range]');await range.focus();await offline.keyboard.press('Home');await offline.locator('image.free-scene').waitFor({state:'hidden'});check(await range.inputValue()==='0','offline playback can seek to the initial empty scene');
    await range.press('ArrowRight');check(await range.inputValue()==='1','offline slider ArrowRight advances exactly one step');
    await range.focus();await offline.keyboard.press('End');await offline.locator('image.free-scene').waitFor();check(await range.inputValue()==='2','offline playback restores the final scene at the correct step');
    await range.press('ArrowLeft');check(await range.inputValue()==='1','offline slider ArrowLeft retreats exactly one step');
    const exit=offline.getByRole('button',{name:'Exit replay',exact:true});
    const paused=()=>offline.getByRole('button',{name:'Play',exact:true}).count();
    const reset=async()=>{await exit.click();await range.press('Home');await offline.evaluate(()=>document.activeElement.blur());};
    await exit.click();await offline.locator('.replay-dock .speed').press(' ');
    check(await offline.locator('.replay-dock .speed').textContent()==='2×'&&await paused()===1&&await range.inputValue()==='2','Space activates the speed button without starting playback');
    await exit.click();await offline.getByRole('button',{name:'Play',exact:true}).press(' ');
    check(await paused()===0,'Space on the play button starts playback once');
    await offline.keyboard.press(' ');check(await paused()===1,'Space on the pause button pauses playback once');
    await reset();await offline.getByRole('button',{name:'Play',exact:true}).click();await exit.press(' ');
    check(await paused()===1&&await range.inputValue()==='2','Space activates the exit button and restores the final board');
    for(const focus of ['body','slider']) {
      await reset();await offline.getByRole('button',{name:'Play',exact:true}).click();
      if(focus==='slider')await range.focus();else await offline.evaluate(()=>document.activeElement.blur());
      await offline.keyboard.press('Escape');
      check(await paused()===1&&await range.inputValue()==='2',`Escape exits offline replay from the ${focus}`);
    }
    await reset();await range.focus();await range.press(' ');
    check(await paused()===1&&await range.inputValue()==='0','Space on the progress slider does not start playback');
    await reset();await offline.keyboard.press('ArrowRight');check(await range.inputValue()==='1','bare ArrowRight advances offline replay');
    await offline.keyboard.press('ArrowLeft');check(await range.inputValue()==='0','bare ArrowLeft retreats offline replay');
    await offline.keyboard.down('Space');await offline.getByRole('button',{name:'Pause',exact:true}).waitFor();
    await offline.keyboard.down('Space');await offline.keyboard.up('Space');
    check(await paused()===0,'holding bare Space toggles playback only once');
    for(const modifier of ['ctrlKey','metaKey','altKey','shiftKey']) {
      await reset();
      const prevented=await offline.evaluate(modifier=>{const e=new KeyboardEvent('keydown',{key:' ',[modifier]:true,bubbles:true,cancelable:true});document.body.dispatchEvent(e);return e.defaultPrevented;},modifier);
      check(!prevented&&await paused()===1&&await range.inputValue()==='0',`offline playback leaves ${modifier}+Space alone`);
    }
    for(const key of [' ','ArrowRight','Escape']) {
      await reset();await offline.locator('body').dispatchEvent('keydown',{key,isComposing:true});
      check(await paused()===1&&await range.inputValue()==='0',`offline playback ignores the composing ${JSON.stringify(key)} key`);
    }
    await exit.click();
    const pageButtons=offline.locator('.player-pages button');
    await pageButtons.nth(0).focus();await offline.keyboard.press('Tab');
    check(await pageButtons.nth(1).evaluate(button=>button===document.activeElement),'Tab reaches the next board button');
    await offline.keyboard.press(' ');
    check(await pageButtons.nth(1).getAttribute('aria-current')==='page'&&await pageButtons.nth(1).evaluate(button=>button.classList.contains('on')),'Space selects the next board and exposes its current-page state');
    await pageButtons.nth(1).click();await offline.locator('image.free-scene').waitFor();await offline.screenshot({path:path.join(evidence,'offline-final-board2.png')});
    check((await offline.locator('.ink image.free-scene').getAttribute('href')).startsWith('data:image/svg+xml;base64,'),'offline scene-only board uses an embedded projection');
    const imageStats=await offline.evaluate(async()=>{const img=new Image();img.src=document.querySelector('.ink image.free-scene').getAttribute('href');await img.decode();return {width:img.width,height:img.height};});check(imageStats.width>100&&imageStats.height>100,'offline embedded scene image decodes successfully');
    await offline.setViewportSize({width:720,height:480});
    const compact=await offline.evaluate(()=>{const b=document.querySelector('.player .board').getBoundingClientRect();const l=document.querySelector('.player .log').getBoundingClientRect();return {board:{x:b.x,y:b.y,width:b.width,height:b.height},log:{x:l.x,y:l.y,width:l.width},scrollWidth:document.documentElement.scrollWidth}});report.offline.compact=compact;
    check(compact.board.x===0&&compact.board.width===720&&compact.log.x===0&&compact.log.y>=compact.board.y+compact.board.height,'compact offline player stacks board and transcript without an implicit grid column');
    check(compact.scrollWidth<=720,'compact offline player has no horizontal overflow');
    await offline.screenshot({path:path.join(evidence,'offline-compact.png')});
    await pageButtons.nth(1).focus();await offline.keyboard.press('Tab');await offline.keyboard.press(' ');
    check(await pageButtons.nth(2).getAttribute('aria-current')==='page'&&await range.count()===0,'keyboard navigation reaches a board with no replay history');
    await pageButtons.nth(2).click();await offline.evaluate(()=>document.activeElement.blur());
    const emptySpace=await offline.evaluate(()=>{const e=new KeyboardEvent('keydown',{key:' ',bubbles:true,cancelable:true});document.body.dispatchEvent(e);return e.defaultPrevented;});
    check(!emptySpace&&await range.count()===0,'an empty board preserves the browser Space behavior');
    check(await offline.evaluate(()=>JSON.stringify(window.__boardcastData))===originalData,'keyboard playback and page changes preserve the exported meeting data');
    check(report.offline.requests.length===0,'offline player makes no HTTP requests for board content or fonts');
    check(report.offline.pageErrors.length===0,'offline player has no script errors');

    // A background-only page also needs a truthful Markdown omission notice.
    await page.evaluate(async underlay=>{
      const db=await new Promise((yes,no)=>{const r=indexedDB.open('boardcast-v2',1);r.onsuccess=()=>yes(r.result);r.onerror=()=>no(r.error)});
      const graph={nodes:[],edges:[],groups:[],nextNode:1,nextGroup:1,underlay:{w:640,h:360}};
      const row={version:2,storageRevision:1,id:'qa-underlay-only-export',title:'BACKGROUND_ONLY',titled:true,createdAt:1,updatedAt:2,graph,history:[],lines:[],pages:[{graph,underlay}],page:0};
      await new Promise((yes,no)=>{const t=db.transaction('meetings','readwrite');t.objectStore('meetings').put(row);t.oncomplete=yes;t.onerror=()=>no(t.error)});db.close();localStorage.setItem('boardcast.meeting',row.id);
    },fixture.underlay);
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    const backgroundMd=await download('Markdown','underlay-only-md');
    check(backgroundMd.text.includes('omits free drawings')&&backgroundMd.text.includes('images'),'background-only Markdown explicitly identifies omitted image content');
    check(report.pageErrors.length===0,'production export flow has no script errors');
    report.passed=report.failures.length===0;
    assert.deepEqual(report.failures,[]);
    await fs.rm(path.join(evidence,'failure.png'),{force:true});
    return report;
  } catch(error) {report.error=String(error);await page.screenshot({path:path.join(evidence,'failure.png')});throw error;}
  finally {if(offlineContext) await offlineContext.close();await fs.writeFile(path.join(evidence,'export-check.json'),JSON.stringify(report,null,2)+'\n');}
};
