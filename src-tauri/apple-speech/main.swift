import AVFoundation
import Foundation
import Speech

// Only JSON events go to stdout; transcription input is 16 kHz mono float32 little endian.
private let outputLock = NSLock()
func emit(_ event: [String: Any]) {
    guard var data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) else { return }
    data.append(10)
    outputLock.lock()
    defer { outputLock.unlock() }
    FileHandle.standardOutput.write(data)
}

struct Failure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

// Pull-based input keeps file imports and a slow recognizer from accumulating unbounded audio.
final class PCMInput {
    let format: AVAudioFormat
    let converter: AVAudioConverter
    let read: () throws -> Data
    var pending = Data()
    var ended = false

    init(format: AVAudioFormat, read: @escaping () throws -> Data) throws {
        let source = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000,
                                   channels: 1, interleaved: false)!
        guard let converter = AVAudioConverter(from: source, to: format) else {
            throw Failure(message: "Cannot convert audio to Apple's input format")
        }
        converter.primeMethod = .none
        self.format = format
        self.converter = converter
        self.read = read
    }

    func next() throws -> AVAudioPCMBuffer? {
        while true {
            if !ended {
                let bytes = try read()
                ended = bytes.isEmpty
                pending.append(bytes)
            }
            if ended && pending.count % 4 != 0 {
                throw Failure(message: "Incomplete float32 audio sample")
            }
            let frames = pending.count / 4
            if frames == 0 && !ended { continue }
            let input = AVAudioPCMBuffer(pcmFormat: converter.inputFormat,
                                          frameCapacity: AVAudioFrameCount(max(1, frames)))!
            input.frameLength = AVAudioFrameCount(frames)
            if frames > 0 {
                _ = pending.copyBytes(to: UnsafeMutableRawBufferPointer(start: input.floatChannelData![0], count: frames * 4))
                pending.removeFirst(frames * 4)
            }
            let capacity = AVAudioFrameCount(ceil(Double(max(frames, 1)) * format.sampleRate / 16_000)) + 64
            let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity)!
            var supplied = false
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, state in
                if !supplied && frames > 0 {
                    supplied = true
                    state.pointee = .haveData
                    return input
                }
                state.pointee = self.ended ? .endOfStream : .noDataNow
                return nil
            }
            if let error { throw error }
            if status == .error { throw Failure(message: "Audio conversion failed") }
            if output.frameLength > 0 { return output }
            if status == .endOfStream { return nil }
        }
    }
}

@available(macOS 26.0, *)
func assetState(_ status: AssetInventory.Status) -> String {
    switch status {
    case .installed: return "installed"
    case .downloading: return "downloading"
    case .supported: return "supported"
    case .unsupported: return "unsupported"
    @unknown default: return "unknown"
    }
}

@available(macOS 26.0, *)
func transcriber(_ locale: Locale) -> SpeechTranscriber {
    SpeechTranscriber(locale: locale, transcriptionOptions: [],
                      reportingOptions: [.volatileResults], attributeOptions: [.audioTimeRange])
}

@available(macOS 26.0, *)
func requireAssets(_ module: SpeechTranscriber, locale: String) async throws {
    let status = await AssetInventory.status(forModules: [module])
    guard status == .installed else {
        throw Failure(message: "APPLE_SPEECH_ASSETS_NOT_READY:\(locale):\(assetState(status))")
    }
}

@available(macOS 26.0, *)
func availability() async -> [String: Any] {
    guard SpeechTranscriber.isAvailable else {
        return ["type": "status", "isApple": true, "available": false, "reason": "hardware", "locales": []]
    }
    let installed = await SpeechTranscriber.installedLocales.map(\.identifier)
    let supported = await SpeechTranscriber.supportedLocales
    var locales: [[String: Any]] = []
    for locale in supported.sorted(by: { $0.identifier < $1.identifier }) {
        // Check the same module configuration as recording, not only the system's locale list.
        let module = transcriber(locale)
        let status = assetState(await AssetInventory.status(forModules: [module]))
        locales.append(["id": locale.identifier,
                        "name": locale.localizedString(forIdentifier: locale.identifier) ?? locale.identifier,
                        "status": status])
    }
    let preferred = await SpeechTranscriber.supportedLocale(equivalentTo: Locale.current)
    return ["type": "status", "isApple": true, "available": true, "locales": locales,
            "defaultLocale": preferred?.identifier ?? "", "installedLocales": installed]
}

@available(macOS 26.0, *)
func transcriber(_ identifier: String) async throws -> SpeechTranscriber {
    guard SpeechTranscriber.isAvailable else { throw Failure(message: "Apple transcription is unavailable on this device") }
    guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: identifier)) else {
        throw Failure(message: "Apple transcription does not support this language")
    }
    return transcriber(locale)
}

@available(macOS 26.0, *)
func transcribe(_ identifier: String) async throws {
    let module = try await transcriber(identifier)
    try await requireAssets(module, locale: identifier)
    guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [module]) else {
        throw Failure(message: "Apple could not provide a compatible audio format")
    }
    let input = try PCMInput(format: format) {
        try FileHandle.standardInput.read(upToCount: 6_400) ?? Data()
    }
    let sequence = AsyncThrowingStream<AnalyzerInput, Error>(unfolding: {
        try input.next().map { AnalyzerInput(buffer: $0) }
    })
    let analyzer = SpeechAnalyzer(modules: [module])
    try await analyzer.prepareToAnalyze(in: format)
    let results = Task {
        do {
            for try await result in module.results {
                let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                let start = CMTimeGetSeconds(result.range.start)
                let end = CMTimeGetSeconds(CMTimeRangeGetEnd(result.range))
                guard start.isFinite, end.isFinite else { throw Failure(message: "Invalid Apple transcript timestamps") }
                emit(["type": result.isFinal ? "final" : "partial", "text": text, "start": start, "end": end])
            }
        } catch {
            emit(["type": "error", "message": error.localizedDescription])
            await analyzer.cancelAndFinishNow()
            throw error
        }
    }
    do {
        emit(["type": "ready"])
        if let last = try await analyzer.analyzeSequence(sequence) {
            try await analyzer.finalizeAndFinish(through: last)
        } else {
            await analyzer.cancelAndFinishNow()
        }
        try await results.value
    } catch {
        await analyzer.cancelAndFinishNow()
        results.cancel()
        throw error
    }
}

func selfTest() throws {
    guard Bundle.main.object(forInfoDictionaryKey: "LSUIElement") as? Bool == true else {
        throw Failure(message: "Speech helper must run as a background agent")
    }
    let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    let values: [Float] = [0.25, -0.5, 0.75, 0]
    let data = values.withUnsafeBytes { Data($0) }
    var chunks = [Data(data.prefix(3)), Data(data.dropFirst(3).prefix(7)), Data(data.dropFirst(10)), Data()]
    let input = try PCMInput(format: format) { chunks.removeFirst() }
    var actual: [Float] = []
    while let buffer = try input.next() {
        actual += Array(UnsafeBufferPointer(start: buffer.floatChannelData![0], count: Int(buffer.frameLength)))
    }
    guard actual == values else { throw Failure(message: "PCM split-read self-test failed") }
    var truncated = [Data([0, 0, 0]), Data()]
    let broken = try PCMInput(format: format) { truncated.removeFirst() }
    do {
        _ = try broken.next()
        throw Failure(message: "Truncated PCM was accepted")
    } catch let error as Failure where error.message == "Incomplete float32 audio sample" {}
    emit(["type": "done"])
}

@main
struct Main {
    static func main() async {
        // Also leave after an abrupt parent crash; normal stop closes stdin and finalizes.
        let parent = getppid()
        let watchdog = DispatchSource.makeTimerSource(queue: .global())
        watchdog.schedule(deadline: .now() + 1, repeating: 1)
        watchdog.setEventHandler { if getppid() != parent { exit(0) } }
        watchdog.resume()
        defer { watchdog.cancel() }
        do {
            let args = Array(CommandLine.arguments.dropFirst())
            if args == ["self-test"] { try systemAudioSelfTest(); try selfTest(); return }
            if args == ["capture-system"] {
                guard #available(macOS 13.0, *) else {
                    throw Failure(message: "System audio capture requires macOS 13 or later")
                }
                try await captureSystemAudio()
                emit(["type": "done"])
                return
            }
            guard #available(macOS 26.0, *) else {
                if args == ["status"] {
                    emit(["type": "status", "isApple": true, "available": false, "reason": "os", "locales": [],
                          "osVersion": ProcessInfo.processInfo.operatingSystemVersionString])
                    return
                }
                throw Failure(message: "Apple SpeechAnalyzer requires macOS 26 or later")
            }
            if args == ["status"] {
                var status = await availability()
                status["osVersion"] = ProcessInfo.processInfo.operatingSystemVersionString
                emit(status)
                return
            }
            guard args.count == 2 && !args[1].isEmpty else { throw Failure(message: "Expected prepare or transcribe and a locale") }
            switch args[0] {
            case "prepare":
                let module = try await transcriber(args[1])
                if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
                    try await request.downloadAndInstall()
                }
                try await requireAssets(module, locale: args[1])
            case "transcribe": try await transcribe(args[1])
            default: throw Failure(message: "Unknown command")
            }
            emit(["type": "done"])
        } catch {
            emit(["type": "error", "message": error.localizedDescription])
            exit(1)
        }
    }
}
