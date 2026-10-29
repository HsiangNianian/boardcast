// 铅笔模式的白板（Excalidraw）。单独成一个模块，App 里按需加载：
// Excalidraw 连同它带的 mermaid、katex 有好几 MB，手写模式和启动时都用不着。

import { Excalidraw } from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'

export default function PencilBoard({
  onApi,
  dark,
  paper,
}: {
  onApi: (api: ExcalidrawImperativeAPI) => void
  dark: boolean
  paper: string
}) {
  return (
    <Excalidraw
      excalidrawAPI={onApi}
      theme={dark ? 'dark' : 'light'}
      initialData={{ appState: { viewBackgroundColor: paper, currentItemFontFamily: 1 } }}
      UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false } }}
    />
  )
}
