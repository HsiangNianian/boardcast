import '@fontsource-variable/dm-sans'
import '@fontsource-variable/jetbrains-mono'
import './style.css'
import { initFinale } from './finale'
import { initHero } from './hero'
import { initStage } from './stage'
import { initSwap } from './swap'

async function boot() {
  // 巨字的位置要等字体到了再量
  await Promise.all([document.fonts.load('900 100px "BC Display"', '说话自己'), document.fonts.ready]).catch(() => {})
  const hero = initHero()
  initStage(hero)
  initSwap()
  initFinale()
}
boot()
