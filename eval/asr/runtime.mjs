export function nativeErrors(stderr) {
  const count = (stderr.match(/\[E:onnxruntime:|Caught exception:/g) || []).length
  return count ? [`Native decoder emitted ${count} error markers; see inference.stderr.log (multiple markers can describe one exception)`] : []
}
