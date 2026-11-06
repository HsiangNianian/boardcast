import assert from 'node:assert/strict'
import { test } from 'node:test'
import { llmProblem, type LlmStatus } from '../desktop.ts'
import { langChoice, setLangChoice } from '../i18n/index.ts'

test('existing model status changes language without changing its diagnostic data', () => {
  const previous = langChoice()
  const log = 'C:\\Users\\会议 notes (old); log: keep\\llama-server.log\nsecond line'
  const status: LlmStatus = { state: 'error', gpus: [], message: `llama-server exited unexpectedly (signal: 11 (core dumped)); log: ${log}` }
  const original = structuredClone(status)
  try {
    setLangChoice('zh')
    assert.equal(llmProblem(status), `内置板书服务意外退出（signal: 11 (core dumped)），请在「管理模型」中重试。日志：${log}`)
    setLangChoice('en')
    assert.equal(llmProblem(status), `The built-in board service stopped unexpectedly (signal: 11 (core dumped)). Retry it in Manage models. Log: ${log}`)
    assert.deepEqual(status, original)
  } finally { setLangChoice(previous) }
})

for (const language of ['zh', 'en'] as const) {
  test(`${language}: model availability and known startup failures retain actionable details`, () => {
    const previous = langChoice()
    setLangChoice(language)
    const zh = language === 'zh', log = '/fixture/user notes/日志 (one).log'
    const state = (message?: string): LlmStatus => ({ state: 'error', message, gpus: [] })
    try {
      assert.equal(llmProblem({ state: 'no-model', gpus: [] }), zh
        ? '尚未安装内置板书模型，请打开「管理模型」下载。'
        : 'No built-in board model is installed. Open Manage models to download one.')
      assert.equal(llmProblem(state('llama-server was not found in the app installation')), zh
        ? '应用安装中缺少内置板书服务，请重新安装 Boardcast 后重试。'
        : 'The built-in board service is missing from the app installation. Reinstall Boardcast, then try again.')
      assert.equal(llmProblem(state(`llama-server failed to start; log: ${log}`)), zh
        ? `内置板书模型未能启动，请在「管理模型」中重试。日志：${log}`
        : `The built-in board model could not start. Retry it in Manage models. Log: ${log}`)
      for (const message of [undefined, '']) assert.equal(llmProblem(state(message)), zh ? '内置模型不可用' : 'Built-in model unavailable')
      for (const message of ['GPU fault EIO /fixture/raw.log', 'llama-server was not found in the app installation: custom detail', 'llama-server failed to start; log: ', 'llama-server exited unexpectedly (exit status: 1); custom log: keep']) {
        assert.equal(llmProblem(state(message)), message, 'unknown or incomplete diagnostics must remain verbatim')
      }
    } finally { setLangChoice(previous) }
  })
}
