// Evaluation candidates only; the desktop still uses SenseVoice.
const multilingual = ['zh', 'ja', 'en', 'zh-en']
const transducer = (name, languages, suffix = '.int8.onnx') => ({
  languages, package: name, languageMode: 'model-native',
  config: { family: 'transducer', encoder: `${name}/encoder${suffix}`, decoder: `${name}/decoder${suffix}`, joiner: `${name}/joiner${suffix}`, tokens: `${name}/tokens.txt` },
})

export const models = {
  sensevoice: { languages: multilingual, languageMode: 'auto', config: { family: 'sensevoice' } },
  ...Object.fromEntries(['tiny', 'base', 'small', 'medium', 'large-v3', 'turbo'].map((size) => [`whisper-${size}`, {
    languages: multilingual, package: `whisper-${size}`, languageMode: 'auto',
    config: { family: 'whisper', encoder: `${size}-encoder.int8.onnx`, decoder: `${size}-decoder.int8.onnx`, tokens: `${size}-tokens.txt` },
  }])),
  ...Object.fromEntries(['0.6b', '1.7b'].map((size) => {
    const name = `qwen3-asr-${size}`
    return [name, { languages: multilingual, package: name, languageMode: 'auto', config: {
      family: 'qwen', conv_frontend: `${name}/conv_frontend.onnx`, encoder: `${name}/encoder.int8.onnx`,
      decoder: `${name}/decoder.int8.onnx`, tokenizer: `${name}/tokenizer`,
    } }]
  })),
  'paraformer-zh-en': { languages: ['zh', 'en', 'zh-en'], package: 'paraformer-zh-en', languageMode: 'model-native',
    config: { family: 'paraformer', model: 'paraformer-zh-en/model.int8.onnx', tokens: 'paraformer-zh-en/tokens.txt' } },
  'parakeet-v3': transducer('parakeet-v3', ['en']),
  'parakeet-unified-en': transducer('parakeet-unified-en', ['en']),
  reazonspeech: transducer('reazonspeech', ['ja'], '-epoch-99-avg-1.int8.onnx'),
  // These are explicitly language-selected controls, not automatic language routing.
  ...Object.fromEntries(['zh', 'ja', 'en'].map((language) => [`cohere-${language}`, {
    languages: [language], package: 'cohere-transcribe', languageMode: `fixed:${language}`,
    config: { family: 'cohere', language, encoder: 'cohere-transcribe/encoder.int8.onnx', decoder: 'cohere-transcribe/decoder.int8.onnx', tokens: 'cohere-transcribe/tokens.txt' },
  }])),
  ...Object.fromEntries(['zh', 'ja', 'en'].map((language) => [`wav2vec2-${language}`, {
    languages: [language], package: `wav2vec2-${language}`, languageMode: 'model-native', backend: 'transformers-ctc',
    config: { family: 'ctc', model: `wav2vec2-${language}` },
  }])),
}

export function supportedSamples(samples, model) {
  if (!models[model]) throw new Error(`Unknown model: ${model}`)
  return samples.filter((s) => s.scoring === 'silence' || models[model].languages.includes(s.language))
}
