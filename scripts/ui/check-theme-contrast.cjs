// Selected rendered token pairs; not a full WCAG or assistive-technology audit.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
module.exports = async function checkThemeContrast(page, baseUrl, evidence = 'docs/ui-ux-audit-2026-11-03/evidence/implementation') {
  await page.goto(baseUrl);
  const results=[];
  for(const theme of ['light','dark']) {
    await page.evaluate(theme=>localStorage.setItem('boardcast.theme',theme),theme);
    await page.reload();await page.locator('.splash').waitFor({state:'hidden'});
    await page.waitForFunction(theme => document.documentElement.dataset.theme === theme && getComputedStyle(document.documentElement).getPropertyValue('--ink').trim(), theme);
    const pairs=await page.evaluate(()=>{
      // Measure the requested color, not the previous color during a reduced-motion transition.
      const probe=document.createElement('span');probe.style.transitionProperty='none';document.body.append(probe);
      const luminance=color=>{
        const rgb=color.match(/[\d.]+/g).slice(0,3).map(Number).map(x=>{x/=255;return x<=.04045?x/12.92:((x+.055)/1.055)**2.4});
        return .2126*rgb[0]+.7152*rgb[1]+.0722*rgb[2];
      };
      const token=name=>{probe.style.color=`var(--${name})`;return luminance(getComputedStyle(probe).color);};
      const pairs=[['ink','surface',4.5],['muted','surface',4.5],['muted','surface-2',4.5],['ok','surface',4.5],['warn','surface',4.5],['bad','surface',4.5],['accent','surface',4.5],['control-border','surface',3],['focus','surface',3]].map(([foreground,background,minimum])=>{
        const a=token(foreground),b=token(background);
        return {foreground,background,minimum,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)};
      });
      probe.style.removeProperty('color');
      for(let i=0;i<6;i++){
        probe.className=`who c${i}`;
        const css=getComputedStyle(probe),a=luminance(css.color),b=luminance(css.backgroundColor);
        pairs.push({foreground:`speaker-${i}`,background:css.backgroundColor,minimum:4.5,ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)});
      }
      probe.remove();return pairs;
    });
    for(const pair of pairs)assert(pair.ratio>=pair.minimum,`${theme} ${pair.foreground}/${pair.background}: ${pair.ratio}`);
    results.push({theme,pairs});
  }
  await fs.mkdir(evidence,{recursive:true});
  await fs.writeFile(`${evidence}/rendered-theme-contrast.json`,JSON.stringify({passed:true,scope:'Selected rendered CSS token pairs only',results},null,2));
};
