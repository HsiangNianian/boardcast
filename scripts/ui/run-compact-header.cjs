const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const meetingAction = require('./meeting-action.cjs');
const baseUrl = process.argv[2] || 'http://127.0.0.1:1441';
const evidence = process.argv[3] || '/tmp/boardcast-compact-header';

(async () => {
  await fs.mkdir(evidence, {recursive:true});
  const fixture = (await fs.readFile(path.join(__dirname,'run-visual-matrix.cjs'),'utf8')).match(/function nativeFixture\([\s\S]*?\n}\n/)?.[0];
  assert(fixture);
  const browser = await chromium.launch({headless:true, executablePath:process.env.CHROMIUM_PATH || '/usr/bin/chromium'});
  const rows = [];
  try {
    for (const lang of ['en','zh']) for (const theme of ['light','dark']) for (const [width,height,touch] of [[1280,800,false],[900,600,false],[720,480,false],[360,240,false],[720,480,true],[480,720,true]]) {
      const name = `${lang}-${theme}-${width}x${height}-${touch?'touch':'mouse'}`;
      if (process.env.HEADER_CASE && !new RegExp(process.env.HEADER_CASE).test(name)) continue;
      const row = {name, checks:[], errors:[]};rows.push(row);
      const context = await browser.newContext({viewport:{width,height},hasTouch:touch,reducedMotion:'reduce'});
      let page;
      try {
        await context.addInitScript({content:`(${fixture})(${JSON.stringify({lang,theme})})`});

        page=await context.newPage();page.setDefaultTimeout(10000);page.on('pageerror',e=>row.errors.push(e.message));
        await page.goto(baseUrl);await page.locator('.splash').waitFor({state:'hidden'});await page.evaluate(()=>document.fonts.ready);
        await page.evaluate(() => {
          const invoke = window.__TAURI_INTERNALS__.invoke;
          const q = window.qaHeader = {mode:'normal', pending:[]};
          window.__TAURI_INTERNALS__.invoke = async (cmd,args) => {
            if (cmd === 'meeting_save') {
              if (q.mode === 'hold') await new Promise((resolve,reject) => q.pending.push({resolve,reject}));
              if (q.mode === 'fail') throw Error('Header fixture save failed');
            }
            return invoke(cmd,args);
          };
        });
        async function visibleStatus(state) {
          const result = await page.locator('.save-state').evaluate(el=>{
            const r=el.getBoundingClientRect(),hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
            return {state:el.className,text:el.textContent,box:r.toJSON(),visible:r.y>=0 && r.bottom<=innerHeight && el.contains(hit),shellWidth:document.documentElement.scrollWidth,viewport:innerWidth};
          });
          row.checks.push({phase:state,...result});
          assert(result.visible,`${state}: save feedback is clipped or covered`);
          assert(result.shellWidth<=result.viewport,`${state}: horizontal shell overflow`);
        }
        const mode=page.getByRole('radio',{name:lang==='zh'?'手绘':'Hand-drawn',exact:true});
        await mode.focus();await visibleStatus('saved after board-tool focus');
        await page.evaluate(()=>window.qaHeader.mode='hold');
        await page.locator('.write textarea').fill('Keep the pending save visible');await page.locator('.write textarea').press('Enter');
        await page.locator('.save-state.saving').waitFor();await mode.focus();await visibleStatus('saving after board-tool focus');
        await page.screenshot({path:path.join(evidence,name+'-saving.png')});
        await page.evaluate(()=>{const q=window.qaHeader;q.mode='fail';q.pending.splice(0).forEach(p=>p.resolve());});
        await page.locator('.save-state.error').waitFor();await mode.focus();await visibleStatus('failed after board-tool focus');
        const retry=page.locator('.save-error button').first();await retry.focus();await visibleStatus('failed while reaching retry');
        assert(await retry.evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))}),'retry is reachable');
        await page.evaluate(()=>window.qaHeader.mode='normal');await retry.press('Enter');await page.locator('.save-state.saved').waitFor();
        await mode.focus();await visibleStatus('saved after retry');
        for (const label of (lang==='zh'?['导入','导出']:['Import','Export'])) {
          const action=await meetingAction(page,label);await action.click();await page.getByRole('menu',{name:label,exact:true}).waitFor();
          if(width<=900) assert.equal(await page.locator('.meeting-more').getAttribute('aria-expanded'),'true',`${label}: persistent trigger announces the open menu`);
          await page.keyboard.press('End');await page.keyboard.press('Home');await page.keyboard.press('Escape');
          const trigger=width<=900?page.locator('.meeting-more'):page.locator('.meeting-actions').getByRole('button',{name:label,exact:true});
          assert(await trigger.evaluate(el=>document.activeElement===el),`${label}: return focus`);await visibleStatus(label+' closed');
          assert.equal(await trigger.getAttribute('aria-expanded'),'false',`${label}: closed menu state`);
        }
        for (const label of (lang==='zh'?['设置','结束并复盘']:['Settings','End and review'])) {
          const action=await meetingAction(page,label);await action.click();await page.getByRole('dialog').waitFor();
          await page.keyboard.press('Escape');await page.getByRole('dialog').waitFor({state:'detached'});
          const trigger=width<=900?page.locator('.meeting-more'):page.locator('.meeting-actions').getByRole('button',{name:label,exact:true});
          assert(await trigger.evaluate(el=>document.activeElement===el),`${label}: return focus`);await visibleStatus(label+' closed');
        }
        await (await meetingAction(page,lang==='zh'?'导入':'Import')).click();
        const pendingChooser=page.waitForEvent('filechooser');
        await page.locator('.import-menu [role=menuitem]').click();
        await (await pendingChooser).setFiles([]);
        await page.locator('.import-menu').waitFor({state:'detached'});
        const fileTrigger=width<=900?page.locator('.meeting-more'):page.locator('.meeting-actions').getByRole('button',{name:lang==='zh'?'导入':'Import',exact:true});
        assert(await fileTrigger.evaluate(el=>document.activeElement===el),'canceling file selection restores its persistent trigger');
        await visibleStatus('file selection canceled');
        if(width<=900){
          await page.locator('.meeting-more').press('Enter');await page.locator('.meeting-menu').waitFor();
          await page.keyboard.press('End');await page.keyboard.press('ArrowDown');
          assert(await page.locator('.meeting-menu [role=menuitem]').first().evaluate(el=>document.activeElement===el),'More wraps keyboard navigation');
          await page.setViewportSize({width:1280,height:800});await page.locator('.meeting-menu').waitFor({state:'detached'});
          await page.setViewportSize({width,height});assert.equal(await page.locator('.meeting-menu').count(),0,'resizing does not reopen More');
        }
        assert.deepEqual(row.errors,[]);row.ok=true;
      } catch(error) {row.error=String(error);console.error(name,row.error);if(page)await page.screenshot({path:path.join(evidence,name+'-failure.png')}).catch(()=>{});}
      finally {await context.close();}
      console.log(name,row.ok?'PASS':'FAIL');
    }
  } finally {await browser.close();await fs.writeFile(path.join(evidence,'report.json'),JSON.stringify(rows,null,2)+'\n');}
  assert(rows.length && rows.every(row=>row.ok),'all header cases pass');
})().catch(error=>{console.error(error);process.exitCode=1;});
