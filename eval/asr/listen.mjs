import { createReadStream, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { join } from 'node:path'
import { corpus, directory, prepareAudio } from './download.mjs'

await prepareAudio()
const allowed = new Set(['index.html', 'LICENSES.md', ...corpus.samples.map((s) => s.file)])
const server = createServer((request, response) => {
  const file = new URL(request.url, 'http://localhost').pathname.slice(1) || 'index.html'
  if (!allowed.has(file) || !['GET', 'HEAD'].includes(request.method)) { response.writeHead(404).end(); return }
  const path = join(directory, file)
  const size = statSync(path).size
  let start = 0, end = size - 1
  if (request.headers.range) {
    const match = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range)
    if (!match || Number(match[1]) >= size || (match[2] && Number(match[2]) < Number(match[1]))) {
      response.writeHead(416, { 'Content-Range': `bytes */${size}` }).end(); return
    }
    start = Number(match[1]); end = match[2] ? Math.min(Number(match[2]), end) : end
  }
  response.writeHead(request.headers.range ? 206 : 200, {
    'Content-Type': file.endsWith('.wav') ? 'audio/wav' : file.endsWith('.html') ? 'text/html; charset=utf-8' : 'text/plain; charset=utf-8',
    'Content-Length': end - start + 1, 'Accept-Ranges': 'bytes',
    ...(request.headers.range ? { 'Content-Range': `bytes ${start}-${end}/${size}` } : {}),
  })
  if (request.method === 'HEAD') response.end()
  else createReadStream(path, { start, end }).on('error', () => response.destroy()).pipe(response)
})
server.on('error', (error) => { console.error(error.message); process.exitCode = 1 })
server.listen(4175, '127.0.0.1', () => console.log('Listen: http://127.0.0.1:4175 · Ctrl+C to stop'))
