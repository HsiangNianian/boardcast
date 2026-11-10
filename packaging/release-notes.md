## Which package to download

Every package has the same app; they differ in the speech models inside.

| Package | Recognizes | Size of models |
| --- | --- | --- |
| `…-macos.zip`, `…-linux.tar.gz` | Japanese, English and Chinese, each with its best model | about 1.1 GB |
| `…-ja` | Japanese, with ReazonSpeech | about 160 MB |
| `…-en` | English, with Parakeet | about 660 MB |
| `…-windows.zip`, `…-windows-ja.zip`, `…-windows-en.zip` | The same three, for Windows; unpack and run `bin\boardcast.exe` | as above |
| `…-macos-none.zip` | Whatever your Mac's own recognizer supports (Mac only) | none |

On a Mac, any package can also use Apple's recognizer instead, from Settings › Speech: SpeechAnalyzer on
macOS 26, and the older recognizer before that.

The Mac app is signed only ad hoc, so open it with Control-click › Open the first time.
On Linux, unpack the archive and run `install.sh`; later versions then install themselves.
