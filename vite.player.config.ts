import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// 回放播放器：单独打成一个 IIFE 脚本（React 和手写渲染都在里面），导出回放 HTML 时整个塞进去。
// 输出到 public/player/，主程序打包时随静态文件一起带上，导出时按 /player/player.js 取。
export default defineConfig({
  plugins: [react()],
  define: { 'process.env.NODE_ENV': '"production"' },
  // 不带 ELK（1.5 MB）：播放器先放回每张板最终的排版，基本用不上；万一用上，换成一个简单的分层排版
  resolve: { alias: { 'elkjs/lib/elk.bundled.js': new URL('./src/player/elk-stub.ts', import.meta.url).pathname } },
  publicDir: false,
  build: {
    target: 'es2022',
    outDir: 'public/player',
    emptyOutDir: true,
    lib: {
      entry: 'src/player/main.tsx',
      formats: ['iife'],
      name: 'BoardcastPlayer',
      fileName: () => 'player.js',
    },
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
})
