// 桌面版（Tauri）才有的能力。网页版里这些函数都返回 null，界面照旧用手填的模型地址。

export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface LlmStatus {
  state: 'starting' | 'ready' | 'no-model' | 'error'
  url?: string
  model?: string
  device?: string
  message?: string
  modelsDir?: string
}

export async function llmStatus(): Promise<LlmStatus | null> {
  if (!isDesktop) return null
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<LlmStatus>('llm_status')
}
