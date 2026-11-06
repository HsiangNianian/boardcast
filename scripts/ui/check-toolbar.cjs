const assert=require('node:assert/strict');
module.exports=async function checkToolbar(page,baseUrl){
 await page.goto(`${baseUrl}/scripts/ui/fixtures/toolbar.html`);
 await page.locator('.ink-toolbar').waitFor();
 for(const [width,height] of [[1280,800],[720,480],[320,240]]){
  await page.setViewportSize({width,height});
  for(const [x,y] of [[0,0],[width,0],[0,height],[width,height]]){
   await page.evaluate(({x,y})=>window.qaAnchor({x,top:y,bottom:y+40}),{x,y});
   await page.waitForFunction(({width,height})=>{const r=document.querySelector('.ink-toolbar').getBoundingClientRect();return r.x>=7&&r.y>=7&&r.right<=width-7&&r.bottom<=height-7},{width,height});
   for(const name of ['Rename','Delete']){
    const button=page.getByRole('button',{name,exact:true});await button.focus();
    const r=await button.boundingBox();assert.ok(r.x>=0&&r.y>=0&&r.x+r.width<=width&&r.y+r.height<=height,JSON.stringify({name,width,height,r}));
   }
  }
 }
 return {measuredToolbarEdges:true,renameAndDeleteKeyboardReachable:true};
};
