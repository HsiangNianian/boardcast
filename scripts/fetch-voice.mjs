// 下载语音模型（SenseVoice int8 + tokens + Silero VAD + CAM++ 声纹，约 268 MB）到 src-tauri/voice/，
// 打包时作为资源带进安装包，用户装完就能直接收音，不用再到「本地模型」里下载。
//   node scripts/fetch-voice.mjs
// 已经下好、sha256 也对得上的文件直接跳过。设置了 BOARDCAST_VOICE_DIR 且那里的文件校验通过时，直接从那里复制。
// 下载顺序和应用内的下载器一样：ModelScope（直连）→ hf-mirror → Hugging Face。
// 文件清单和 sha256 要和 src-tauri/src/models.rs 里「语音识别」那一项保持一致；
// 声纹模型（区分说话人）只随安装包带，不在应用内的下载清单里。ModelScope 上没有它，从 hf-mirror、HF、GitHub 取。

import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { copyFileSync, createReadStream, existsSync, mkdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const FILES = [
  {
    name: 'voice/sense-voice/model.int8.onnx',
    sha256: 'c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51',
    ms: 'pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/model.int8.onnx',
    hf: 'csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx',
  },
  {
    name: 'voice/sense-voice/tokens.txt',
    sha256: 'f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc',
    ms: 'pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master/tokens.txt',
    hf: 'csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt',
  },
  {
    name: 'voice/silero_vad.onnx',
    sha256: 'a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28',
    ms: 'pengzhendong/silero-vad/resolve/master/v4/silero_vad.onnx',
    hf: 'csukuangfj/vad/resolve/main/silero_vad.onnx',
  },
  {
    name: 'voice/speaker/campplus_zh_en.onnx',
    sha256: 'aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2',
    hf: 'csukuangfj/speaker-embedding-models/resolve/main/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx',
    gh: 'https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx',
  },
]

const root = fileURLToPath(new URL('..', import.meta.url))
const out = join(root, 'src-tauri')

function sha256(path) {
  return new Promise((ok, fail) => {
    const h = createHash('sha256')
    createReadStream(path)
      .on('data', (d) => h.update(d))
      .on('end', () => ok(h.digest('hex')))
      .on('error', fail)
  })
}

const good = async (path, want) => existsSync(path) && (await sha256(path)) === want

let fetched = 0
for (const f of FILES) {
  const dest = join(out, f.name)
  if (await good(dest, f.sha256)) continue
  mkdirSync(dirname(dest), { recursive: true })

  const local = process.env.BOARDCAST_VOICE_DIR && join(process.env.BOARDCAST_VOICE_DIR, f.name)
  if (local && (await good(local, f.sha256))) {
    copyFileSync(local, dest)
    fetched++
    continue
  }

  const sources = [
    // ModelScope 在国内直连最快，走代理反而慢
    ...(f.ms ? [{ url: `https://modelscope.cn/models/${f.ms}`, direct: true }] : []),
    { url: `https://hf-mirror.com/${f.hf}`, direct: false },
    { url: `https://huggingface.co/${f.hf}`, direct: false },
    ...(f.gh ? [{ url: f.gh, direct: false }] : []),
  ]
  const part = `${dest}.part`
  let ok = false
  for (const s of sources) {
    console.log(`fetch-voice: 下载 ${s.url}`)
    try {
      // 用 curl：会走系统代理、能断点续传，Windows 10 以上也自带
      const args = ['-fL', '--retry', '3', '-C', '-', '-o', part, s.url]
      if (s.direct) args.unshift('--noproxy', '*')
      execFileSync('curl', args, { stdio: 'inherit' })
      if ((await sha256(part)) === f.sha256) {
        renameSync(part, dest)
        ok = true
        break
      }
      console.error(`fetch-voice: ${f.name} 校验不对，换下一个源`)
      rmSync(part, { force: true })
    } catch {
      console.error(`fetch-voice: 从 ${s.url} 下载失败，换下一个源`)
    }
  }
  if (!ok) {
    console.error(`fetch-voice: ${f.name} 所有源都下载失败`)
    process.exit(1)
  }
  fetched++
}
if (fetched) console.log(`fetch-voice: 语音模型已就位（新取了 ${fetched} 个文件）→ src-tauri/voice/`)
