import { defineConfig } from 'vite'

// 官网是独立的静态页：依赖直接用仓库根目录装好的（perfect-freehand、hanzi-writer-data、hersheytext、字体）。
// base 用相对路径，丢到任何子路径下都能打开。
export default defineConfig({
  root: import.meta.dirname,
  base: process.env.LANDING_BASE || './',
  clearScreen: false,
  server: { port: 1430, allowedHosts: true },
  preview: { port: 1431, allowedHosts: true },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
})
