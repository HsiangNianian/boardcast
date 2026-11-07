# Shared by the package scripts: which speech models a variant carries, and what its package is called.
#   full        SenseVoice, Parakeet (English) and ReazonSpeech (Japanese)
#   ja          ReazonSpeech only
#   en          Parakeet only
#   sensevoice  SenseVoice only
#   apple       none: Apple's own recognizer, on a Mac only

variant_models() {
  case "$1" in
    full) echo "sense-voice parakeet-en reazon-ja" ;;
    ja) echo "reazon-ja" ;;
    en) echo "parakeet-en" ;;
    sensevoice) echo "sense-voice" ;;
    apple) echo "" ;;
    *) echo "unknown variant: $1" >&2; return 1 ;;
  esac
}

# The full package keeps the plain name, which is what installs from before variants look for.
variant_suffix() {
  if [ "$1" = full ]; then echo ""; else echo "-$1"; fi
}

# copy_models <variant> <voice dir in the package>
copy_models() {
  for model in $(variant_models "$1"); do
    mkdir -p "$2/$model"
    case "$model" in
      sense-voice) cp resources/voice/sense-voice/model.int8.onnx resources/voice/sense-voice/tokens.txt "$2/$model/" ;;
      parakeet-en) cp resources/voice/parakeet-en/encoder.int8.onnx resources/voice/parakeet-en/decoder.int8.onnx resources/voice/parakeet-en/joiner.int8.onnx resources/voice/parakeet-en/tokens.txt "$2/$model/" ;;
      reazon-ja) cp resources/voice/reazon-ja/encoder-epoch-99-avg-1.int8.onnx resources/voice/reazon-ja/decoder-epoch-99-avg-1.int8.onnx resources/voice/reazon-ja/joiner-epoch-99-avg-1.int8.onnx resources/voice/reazon-ja/tokens.txt "$2/$model/" ;;
    esac
  done
}
