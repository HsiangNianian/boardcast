import '@fontsource-variable/dm-sans'
import '@fontsource-variable/jetbrains-mono'
import './style.css'
import { initCards } from './cards'
import { initDots } from './dots'
import { initHero } from './hero'
import { initLasso } from './lasso'
import { initPictograms } from './pict'
import { initSheet } from './sheet'
import { initSignals } from './signals'
import { initStory } from './story'
import { $$ } from './util'

for (const el of $$('[data-version]')) el.textContent = __VERSION__

initPictograms()
initSheet()
initLasso()
initSignals()
initCards()
initDots()
// Board words are measured, so they wait for the handwriting font.
document.fonts
  .load('22px Excalifont', 'Release checklist')
  .catch(() => {})
  .then(() => {
    initHero()
    initStory()
  })
