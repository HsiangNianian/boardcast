import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Tauri 开发时固定端口；TAURI_ENV_* 由 tauri cli 注入
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
})
