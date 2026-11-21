## Which package to download

Every package has the same app; they differ in the speech models inside.

| Package | Recognizes | Size of models |
| --- | --- | --- |
| `…-macos.zip`, `…-linux.tar.gz` | Japanese, English and Chinese, each with its best model | about 1.1 GB |
| `…-ja` | Japanese, with ReazonSpeech | about 160 MB |
| `…-en` | English, with Parakeet | about 660 MB |
| `…-windows.zip`, `…-windows-ja.zip`, `…-windows-en.zip` | The same three, for Windows; unpack and run `bin\alcyo.exe` | as above |
| `…-macos-none.zip` | Whatever your Mac's own recognizer supports (Mac only) | none |

On a Mac, any package can also use Apple's recognizer instead, from Settings › Speech: SpeechAnalyzer on
macOS 26, and the older recognizer before that.

On a Mac, open the `.dmg` and drag Alcyo to Applications; the app is signed and notarized. Each Mac package also
comes as a `.zip` holding the same app.
On Linux, unpack the archive and run `install.sh`; later versions then install themselves.
