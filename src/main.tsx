import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App.tsx'
import './style.css'
import { tryLockLandscape } from './ui/landscape.ts'

// 手机上第一次点屏幕时试着真的锁成横屏；锁不了的话 style.css 会把界面转过来
addEventListener('pointerdown', tryLockLandscape, { once: true })

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
