// Runs the actual theme function in Chromium and compares rasterized output pixels.
// Only its TypeScript function signature is removed; no image/color logic is duplicated.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
module.exports = async function checkOverlay(page) {
 const source=fs.readFileSync(path.resolve(__dirname, '../../src/board/overlayTheme.ts'),'utf8').replace('export function darkOverlayUrl(url: string): string','function darkOverlayUrl(url)');
 const result=await page.evaluate(async(source)=>{
  const darkOverlayUrl=new Function(source+';return darkOverlayUrl')();
  const encode=s=>'data:image/svg+xml;base64,'+btoa(s);
  const c=document.createElement('canvas');c.width=30;c.height=30;const ctx=c.getContext('2d');ctx.fillStyle='#ff0000';ctx.fillRect(0,0,30,30);
  const png=c.toDataURL();const vector=encode('<svg xmlns="http://www.w3.org/2000/svg" width="30" height="30"><rect width="30" height="30" fill="rgb(0,180,70)"/></svg>');
  const light=encode(`<svg xmlns="http://www.w3.org/2000/svg" width="90" height="30"><defs><symbol id="photo"><image href="${png}" width="30" height="30"/></symbol></defs><rect width="30" height="30" fill="black"/><g><use href="#photo" x="30" width="30" height="30"/></g><image href="${vector}" x="60" width="30" height="30"/></svg>`);
  const dark=darkOverlayUrl(light);
  async function pixels(url){ const i=new Image();i.src=url;await i.decode();const c=document.createElement('canvas');c.width=90;c.height=30;const ctx=c.getContext('2d');ctx.drawImage(i,0,0);return [15,45,75].map(x=>[...ctx.getImageData(x,15,1,1).data]); }
  return {light:await pixels(light),dark:await pixels(dark),darkSvg:atob(dark.split(',')[1])};
 },source);
 assert.ok(result.dark[0].slice(0,3).every(v=>v>225));
 assert.deepEqual(result.dark[1],result.light[1]);
 assert.deepEqual(result.dark[2],result.light[2]);
 assert.equal((result.darkSvg.match(/filter=/g)||[]).length,1);
 console.log(JSON.stringify({light:result.light,dark:result.dark},null,2));
};
