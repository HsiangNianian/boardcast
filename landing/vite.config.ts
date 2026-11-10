import { readFileSync } from 'node:fs'
import { defineConfig } from 'vite'

// The website is a standalone static page. Its fonts come from the root's dependencies, and a relative base
// lets it open under any subpath. The version shown is the desktop app's, read from the workspace.
const cargo = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8')
const version = /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/.exec(cargo)?.[1] ?? 'dev'

export default defineConfig({
  root: import.meta.dirname,
  base: process.env.LANDING_BASE || './',
  clearScreen: false,
  define: { __VERSION__: JSON.stringify(version) },
  server: { port: 1430, allowedHosts: true },
  preview: { port: 1431, allowedHosts: true },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
})
