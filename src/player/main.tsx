// 回放 HTML 的入口：单独打包成一个 IIFE 脚本（vite.player.config.ts），导出时整个塞进 HTML。
// 样式用应用自己的那一份，看起来和应用里一样。

import { createRoot } from 'react-dom/client'
import appCss from '../style.css?inline'
import playerCss from './player.css?inline'
import Player from './Player.tsx'
import type { ReplayData } from './data.ts'
import { msg, setLangChoice } from '../i18n/index.ts'
import handwritingLicense from '../../public/licenses/handwritten-OFL.txt?raw'
import mathLicense from '../../public/licenses/handwritten-cascadia.txt?raw'

const style = document.createElement('style')
style.textContent = appCss + playerCss
document.head.append(style)
// 导出的单文件也带着字形数据的许可，不依赖旁边另放文件。
document.head.append(document.createComment(handwritingLicense + '\n' + mathLicense))

const data = (window as { __boardcastData?: ReplayData }).__boardcastData
// A read-only export must not overwrite the viewer's saved language preference.
if (data?.lang === 'en' || data?.lang === 'zh') setLangChoice(data.lang, false)
const root = document.getElementById('root')!
if (data) createRoot(root).render(<Player data={data} />)
else root.textContent = msg().doc.noData
