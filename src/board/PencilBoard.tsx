// 铅笔模式的白板（Excalidraw）。单独成一个模块，App 里按需加载：
// Excalidraw 连同它带的 mermaid、katex 有好几 MB，手写模式和启动时都用不着。

import { Excalidraw } from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type { ExcalidrawImperativeAPI, ExcalidrawProps } from '@excalidraw/excalidraw/types'
import { currentLang, useT } from '../i18n/index.ts'
import './pencil-board.css'

export default function PencilBoard({
  onApi,
  ready,
  readOnly,
  dark,
  paper,
  onChange,
  onPointerUp,
  onInteract,
}: {
  onApi: (api: ExcalidrawImperativeAPI) => void
  ready: boolean
  readOnly: boolean
  dark: boolean
  paper: string
  onChange?: ExcalidrawProps['onChange']
  onPointerUp?: ExcalidrawProps['onPointerUp']
  onInteract: () => void
}) {
  const t = useT()
  return (
    <div className="pencil-stage" data-ready={ready} aria-busy={!ready}
      onPointerDownCapture={() => { if (ready) onInteract() }}
      onWheelCapture={() => { if (ready) onInteract() }}
      onKeyDownCapture={() => { if (ready) onInteract() }}>
      <div className="pencil-editor" inert={!ready}>
    <Excalidraw
      excalidrawAPI={onApi}
      langCode={currentLang() === 'zh' ? 'zh-CN' : 'en'}
      onChange={onChange}
      onPointerUp={onPointerUp}
      viewModeEnabled={readOnly}
      theme={dark ? 'dark' : 'light'}
      initialData={{ appState: { viewBackgroundColor: paper, currentItemFontFamily: 1 } }}
      UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false } }}
    />
      </div>
      {!ready && <div className="board-loading pencil-loading" role="status">{t.app.pencilLoading}</div>}
    </div>
  )
}
