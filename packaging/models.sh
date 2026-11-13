# Shared by the package scripts: which speech models a variant carries, and what its package is called.
#   all    SenseVoice, Parakeet (English) and ReazonSpeech (Japanese); "full" is its older name
#   ja     ReazonSpeech only
#   en     Parakeet only
#   none   no models: Apple's own recognizer, on a Mac only

variant_models() {
  case "$1" in
    all | full) echo "sense-voice parakeet-en reazon-ja" ;;
    ja) echo "reazon-ja" ;;
    en) echo "parakeet-en" ;;
    none) echo "" ;;
    *) echo "unknown variant: $1" >&2; return 1 ;;
  esac
}

# The package with every model keeps the plain name, which is what installs from before variants look for.
variant_suffix() {
  case "$1" in all | full) echo "" ;; *) printf '%s\n' "-$1" ;; esac
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

# copy_search <search dir in the package>: the meeting search model, which every variant carries, with the
# notice the Gemma Terms of Use ask to go with it (the terms themselves are among the licenses).
copy_search() {
  mkdir -p "$1"
  cp resources/search/*.gguf "$1/"
  cp assets/licenses/gemma-NOTICE.txt "$1/NOTICE.txt"
}
