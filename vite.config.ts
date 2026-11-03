import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join, normalize } from 'node:path'
import react from '@vitejs/plugin-react'
import { type Plugin, defineConfig } from 'vite'

/**
 * pdf.js 运行时要另外取的文件：字体映射（中文 PDF 离不开）、标准字体、解码图片用的 wasm、颜色配置。
 * 开发时直接从 node_modules 里给，打包时复制到 dist/pdfjs/ 下，导入 PDF 时按 /pdfjs/... 去取。
 */
function pdfjsAssets(): Plugin {
  const root = join(import.meta.dirname, 'node_modules/pdfjs-dist')
  const dirs = ['cmaps', 'standard_fonts', 'wasm', 'iccs']
  return {
    name: 'pdfjs-assets',
    configureServer(server) {
      server.middlewares.use('/pdfjs', (req, res, next) => {
        const rel = normalize(decodeURIComponent((req.url ?? '').split('?')[0])).replace(/^[/\\]+/, '')
        const file = join(root, rel)
        if (!dirs.some((d) => rel.startsWith(`${d}/`)) || !file.startsWith(root) || !existsSync(file)) return next()
        res.setHeader('content-type', file.endsWith('.wasm') ? 'application/wasm' : 'application/octet-stream')
        res.end(readFileSync(file))
      })
    },
    generateBundle() {
      for (const d of dirs) {
        const dir = join(root, d)
        if (!existsSync(dir)) continue
        for (const f of readdirSync(dir)) this.emitFile({ type: 'asset', fileName: `pdfjs/${d}/${f}`, source: readFileSync(join(dir, f)) })
      }
    },
  }
}

/** Local, lazy Japanese dictionary assets; no CDN or Python runtime. */
function pronunciationAssets(): Plugin {
  const root = join(import.meta.dirname, 'node_modules/kuromoji/dict')
  const files = readdirSync(root).filter((f) => f.endsWith('.dat.gz'))
  return {
    name: 'pronunciation-assets',
    configureServer(server) {
      server.middlewares.use('/pronunciation/ja', (req, res, next) => {
        const name = (req.url ?? '').split('?')[0].replace(/^\//, '')
        if (!files.includes(name)) return next()
        res.setHeader('content-type', 'application/octet-stream')
        res.end(readFileSync(join(root, name)))
      })
    },
    generateBundle() {
      for (const name of files) this.emitFile({ type: 'asset', fileName: `pronunciation/ja/${name}`, source: readFileSync(join(root, name)) })
    },
  }
}

// Tauri 开发时固定端口；TAURI_ENV_* 由 tauri cli 注入
export default defineConfig({
  // The web demo on GitHub Pages lives under /boardcast/ (BOARDCAST_BASE=/boardcast/); the desktop app uses /
  base: process.env.BOARDCAST_BASE || '/',
  plugins: [react(), pdfjsAssets(), pronunciationAssets()],
  worker: { format: 'es' },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
})
